use std::sync::Arc;

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper, make_mapper_helper};
use ast::{
    CidlType, CloesceAst, CrudKind, DataSource, HttpVerb, MediaType, NavigationField,
    NavigationFieldKind,
};

use handlebars::{Handlebars, handlebars_helper};
use serde_json::Value;

macro_rules! cidl_type_contains {
    ($value:expr, $pattern:pat) => {{
        let mut cur = $value;

        loop {
            match cur {
                CidlType::Array(inner)
                | CidlType::Nullable(inner)
                | CidlType::HttpResult(inner)
                | CidlType::KvObject(inner)
                | CidlType::Paginated(inner) => {
                    if matches!(cur, $pattern) {
                        break true;
                    }
                    cur = inner;
                }

                other => break matches!(other, $pattern),
            }
        }
    }};
}

handlebars_helper!(needs_constructor: |cidl_type: CidlType| matches!(cidl_type.root_type(),
    CidlType::Object { .. }
    | CidlType::Blob
    | CidlType::DateIso
    | CidlType::Stream
));

handlebars_helper!(get_object_name: |cidl_type: CidlType| match cidl_type.root_type() {
    CidlType::Inject { name, ..} | CidlType::Object { name, ..} | CidlType::Partial { object_name: name, .. } => serde_json::to_value(name).unwrap(),
    ty => serde_json::to_value(ty).unwrap()
});
handlebars_helper!(get_content_type: |media: MediaType| match media {
    MediaType::Json=>"application/json",
    MediaType::Octet => "application/octet-stream",
});

handlebars_helper!(is_blob: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob));
handlebars_helper!(is_one_to_one: |nav: NavigationField| matches!(nav.kind, NavigationFieldKind::OneToOne {..}));
handlebars_helper!(is_get_request: |verb: HttpVerb| matches!(verb, HttpVerb::Get));
handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type.root_type(), CidlType::Inject { .. } | CidlType::Env));
handlebars_helper!(is_object: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object { .. } | CidlType::Partial { .. }));

// TODO: This method of generating fromJson for arrays won't help for n-dimensional arrays
handlebars_helper!(has_array: |cidl_type: CidlType| cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_object_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object { .. }) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_blob_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));

// If a parameter should be placed in the url instead of the body.
// True for any [CidlType::DataSource] or given the verb [HttpVerb::GET]
handlebars_helper!(is_url_param: |cidl_type: CidlType, verb: HttpVerb| matches!(verb, HttpVerb::Get) || matches!(cidl_type, CidlType::DataSource { .. }));
handlebars_helper!(is_stream: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Stream));
handlebars_helper!(is_crud_method: |name: String| name == "$get" || name == "$save" || name == "$list");
handlebars_helper!(contains_stream: |cidl_type: CidlType| cidl_type_contains!(&cidl_type, CidlType::Stream));
handlebars_helper!(is_paginated: |cidl_type: CidlType| matches!(cidl_type, CidlType::Paginated(_)));

handlebars_helper!(is_datasource: |cidl_type: CidlType| matches!(cidl_type, CidlType::DataSource { .. }));
handlebars_helper!(is_crud_kind: |crud: CrudKind, kind: str| match kind {
    "Get" => matches!(crud, CrudKind::Get),
    "List" => matches!(crud, CrudKind::List),
    "Save" => matches!(crud, CrudKind::Save),
    _ => panic!("Unknown CRUD kind: {}", kind),
});

// For KvObject or Paginated<KvObject<T>>, returns the CidlType of T (the inner type of KvObject)
handlebars_helper!(kv_inner_cidl_type: |cidl_type: CidlType| match &cidl_type {
    CidlType::KvObject(inner) => serde_json::to_value(inner.as_ref()).unwrap(),
    CidlType::Paginated(inner) => match inner.as_ref() {
        CidlType::KvObject(inner) => serde_json::to_value(inner.as_ref()).unwrap(),
        _ => serde_json::to_value(&cidl_type).unwrap(),
    },
    _ => serde_json::to_value(&cidl_type).unwrap(),
});

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");
const TEMPLATE_STRING: &str = "client_api";

pub struct ClientGenerator;
impl ClientGenerator {
    pub fn generate(ast: &CloesceAst, worker_url: &str) -> String {
        // TODO: Hardcoded TypeScript for now
        let template = TYPESCRIPT_TEMPLATE;
        let mapper = Arc::new(TypeScriptMapper::client());

        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string(TEMPLATE_STRING, template)
            .unwrap();
        register_helpers(&mut handlebars, mapper, ast);

        let mut context = serde_json::to_value(ast).unwrap();

        // Manually set the "worker_url" field in the context
        if let serde_json::Value::Object(ref mut map) = context {
            map.insert(
                "worker_url".to_string(),
                serde_json::Value::String(worker_url.to_string()),
            );
        }

        handlebars.render(TEMPLATE_STRING, &context).unwrap()
    }
}

/// Generates the args type for a CRUD method.
/// For $get/$list: `DataSources.Model.$get.DS1 | DataSources.Model.$get.DS2`
/// For $save: `DataSources.Model.$save`
fn crud_args_type_helper(
    h: &handlebars::Helper<'_>,
    _hb: &Handlebars<'_>,
    _ctx: &handlebars::Context,
    _rc: &mut handlebars::RenderContext<'_, '_>,
    out: &mut dyn handlebars::Output,
) -> Result<(), handlebars::RenderError> {
    let method_name = h.param(0).unwrap().value().as_str().unwrap();
    let model_name = h.param(1).unwrap().value().as_str().unwrap();
    let data_sources: Vec<DataSource> =
        serde_json::from_value(h.param(2).unwrap().value().clone()).unwrap();

    if method_name == "$save" {
        out.write(&format!("DataSources.{}.{}", model_name, method_name))?;
    } else {
        let parts: Vec<String> = data_sources
            .iter()
            .filter(|ds| !ds.is_internal)
            .map(|ds| format!("DataSources.{}.{}.{}", model_name, method_name, ds.name))
            .collect();
        out.write(&parts.join(" | "))?;
    }
    Ok(())
}

/// Generates `"DS1" | "DS2"` for $save kind union
fn ds_kind_union_helper(
    h: &handlebars::Helper<'_>,
    _hb: &Handlebars<'_>,
    _ctx: &handlebars::Context,
    _rc: &mut handlebars::RenderContext<'_, '_>,
    out: &mut dyn handlebars::Output,
) -> Result<(), handlebars::RenderError> {
    let data_sources: Vec<DataSource> =
        serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

    let parts: Vec<String> = data_sources
        .iter()
        .filter(|ds| !ds.is_internal)
        .map(|ds| format!("\"{}\"", ds.name))
        .collect();

    out.write(&parts.join(" | "))?;
    Ok(())
}

fn register_helpers<'a>(
    hbs: &mut Handlebars<'a>,
    mapper: Arc<dyn LanguageTypeMapper + Send + Sync>,
    ast: &'a CloesceAst,
) {
    let simple_helpers: Vec<(&str, Box<dyn handlebars::HelperDef + Send + Sync>)> = vec![
        ("is_serializable", Box::new(is_serializable)),
        ("is_blob", Box::new(is_blob)),
        ("is_one_to_one", Box::new(is_one_to_one)),
        ("get_content_type", Box::new(get_content_type)),
        ("has_array", Box::new(has_array)),
        ("needs_constructor", Box::new(needs_constructor)),
        ("get_object_name", Box::new(get_object_name)),
        ("is_object", Box::new(is_object)),
        ("is_object_array", Box::new(is_object_array)),
        ("is_blob_array", Box::new(is_blob_array)),
        ("is_url_param", Box::new(is_url_param)),
        ("is_get_request", Box::new(is_get_request)),
        ("is_stream", Box::new(is_stream)),
        ("is_crud_method", Box::new(is_crud_method)),
        ("contains_stream", Box::new(contains_stream)),
        ("is_paginated", Box::new(is_paginated)),
        ("kv_inner_cidl_type", Box::new(kv_inner_cidl_type)),
        ("is_datasource", Box::new(is_datasource)),
        ("is_crud_kind", Box::new(is_crud_kind)),
    ];

    for (name, helper) in simple_helpers {
        hbs.register_helper(name, helper);
    }

    hbs.register_helper("crud_args_type", Box::new(crud_args_type_helper));
    hbs.register_helper("ds_kind_union", Box::new(ds_kind_union_helper));

    hbs.register_helper(
        "get_nav_cidl_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn LanguageTypeMapper, ast: &CloesceAst| -> String {
                let nav: NavigationField = serde_json::from_value(value).unwrap();

                let cidl_type = match nav.kind {
                    NavigationFieldKind::OneToOne { .. } => CidlType::Object {
                        name: nav.model_reference,
                    },
                    NavigationFieldKind::OneToMany { .. } | NavigationFieldKind::ManyToMany => {
                        CidlType::array(CidlType::Object {
                            name: nav.model_reference,
                        })
                    }
                };

                mapper.cidl_type(&cidl_type, ast)
            },
        ),
    );

    hbs.register_helper(
        "map_cidl_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn LanguageTypeMapper, ast: &CloesceAst| -> String {
                let cidl_type: CidlType = serde_json::from_value(value).unwrap();
                mapper.cidl_type(&cidl_type, ast)
            },
        ),
    );

    hbs.register_helper(
        "map_kv_inner_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn LanguageTypeMapper, ast: &CloesceAst| -> String {
                let cidl_type: CidlType = serde_json::from_value(value).unwrap();
                // For KvObject(T) or Paginated(KvObject(T)), map the KvObject(T) part
                let kv_type = match &cidl_type {
                    CidlType::KvObject(_) => &cidl_type,
                    CidlType::Paginated(inner) => match inner.as_ref() {
                        kv @ CidlType::KvObject(_) => kv,
                        _ => &cidl_type,
                    },
                    _ => &cidl_type,
                };
                mapper.cidl_type(kv_type, ast)
            },
        ),
    );

    hbs.register_helper(
        "map_root_cidl_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn LanguageTypeMapper, ast: &CloesceAst| -> String {
                let cidl_type: CidlType = serde_json::from_value(value).unwrap();
                mapper.cidl_type(cidl_type.root_type(), ast)
            },
        ),
    );

    hbs.register_helper(
        "get_media_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn LanguageTypeMapper, _ast: &CloesceAst| -> String {
                let media_type: MediaType = serde_json::from_value(value).unwrap();
                mapper.media_type(&media_type)
            },
        ),
    );
}
