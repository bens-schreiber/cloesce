mod mappers;

use std::sync::Arc;

use ast::{
    CidlType, CloesceAst, HttpVerb, MediaType, NavigationProperty, NavigationPropertyKind,
    cidl_type_contains,
};
use mappers::{ClientLanguageTypeMapper, TypeScriptMapper};

use handlebars::{Handlebars, handlebars_helper};
use serde_json::Value;

handlebars_helper!(needs_constructor: |cidl_type: CidlType| matches!(cidl_type.root_type(),
    CidlType::Object(_)
    | CidlType::Blob
    | CidlType::DateIso
    | CidlType::Stream
));

handlebars_helper!(get_object_name: |cidl_type: CidlType| match cidl_type.root_type() {
    CidlType::Inject(name) | CidlType::Object(name) | CidlType::Partial(name) => serde_json::to_value(name).unwrap(),
    ty => serde_json::to_value(ty).unwrap()
});
handlebars_helper!(get_content_type: |media: MediaType| match media {
    MediaType::Json=>"application/json",
    MediaType::Octet => "application/octet-stream",
});

handlebars_helper!(is_blob: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob));
handlebars_helper!(is_one_to_one: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToOne {..}));
handlebars_helper!(is_get_request: |verb: HttpVerb| matches!(verb, HttpVerb::GET));
handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type.root_type(), CidlType::Inject(_)));
handlebars_helper!(is_object: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object(_) | CidlType::Partial(_)));

// TODO: This method of generating fromJson for arrays won't help for n-dimensional arrays
handlebars_helper!(has_array: |cidl_type: CidlType| cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_object_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Object(_)) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));
handlebars_helper!(is_blob_array: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Blob) && cidl_type_contains!(&cidl_type, CidlType::Array(_)));

// If a parameter should be placed in the url instead of the body.
// True for any [CidlType::DataSource] or given the verb [HttpVerb::GET]
handlebars_helper!(is_url_param: |cidl_type: CidlType, verb: HttpVerb| matches!(verb, HttpVerb::GET) || matches!(cidl_type, CidlType::DataSource(_)));
handlebars_helper!(is_stream: |cidl_type: CidlType| matches!(cidl_type.root_type(), CidlType::Stream));
handlebars_helper!(is_some: |val: Value| !val.is_null());

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");
const TEMPLATE_STRING: &str = "client_api";

pub struct ClientGenerator;
impl ClientGenerator {
    pub fn generate(ast: &CloesceAst, domain: String) -> String {
        // TODO: Hardcoded TypeScript for now
        let template = TYPESCRIPT_TEMPLATE;
        let mapper = Arc::new(TypeScriptMapper);

        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string(TEMPLATE_STRING, template)
            .unwrap();
        register_helpers(&mut handlebars, mapper, ast);

        let mut context = serde_json::to_value(ast).unwrap();

        // Manually set the "domain" field in the context
        if let serde_json::Value::Object(ref mut map) = context {
            map.insert("domain".to_string(), serde_json::Value::String(domain));
        }

        handlebars.render(TEMPLATE_STRING, &context).unwrap()
    }
}

fn register_helpers<'a>(
    hbs: &mut Handlebars<'a>,
    mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync>,
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
        ("is_some", Box::new(is_some)),
    ];

    for (name, helper) in simple_helpers {
        hbs.register_helper(name, helper);
    }

    hbs.register_helper(
        "get_nav_cidl_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn ClientLanguageTypeMapper, ast: &CloesceAst| -> String {
                let nav: NavigationProperty = serde_json::from_value(value).unwrap();

                let cidl_type = match nav.kind {
                    NavigationPropertyKind::OneToOne { .. } => {
                        CidlType::Object(nav.model_reference)
                    }
                    NavigationPropertyKind::OneToMany { .. }
                    | NavigationPropertyKind::ManyToMany => {
                        CidlType::array(CidlType::Object(nav.model_reference))
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
            |value: Value, mapper: &dyn ClientLanguageTypeMapper, ast: &CloesceAst| -> String {
                let cidl_type: CidlType = serde_json::from_value(value).unwrap();
                mapper.cidl_type(&cidl_type, ast)
            },
        ),
    );

    hbs.register_helper(
        "map_root_cidl_type",
        make_mapper_helper(
            mapper.clone(),
            ast,
            |value: Value, mapper: &dyn ClientLanguageTypeMapper, ast: &CloesceAst| -> String {
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
            |value: Value, mapper: &dyn ClientLanguageTypeMapper, _ast: &CloesceAst| -> String {
                let media_type: MediaType = serde_json::from_value(value).unwrap();
                mapper.media_type(&media_type)
            },
        ),
    );

    fn make_mapper_helper<'a, F>(
        mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync + 'a>,
        ast: &'a CloesceAst,
        f: F,
    ) -> Box<dyn handlebars::HelperDef + Send + Sync + 'a>
    where
        F: Fn(Value, &dyn ClientLanguageTypeMapper, &CloesceAst) -> String + Send + Sync + 'a,
    {
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _hb: &Handlebars<'_>,
                  _ctx: &handlebars::Context,
                  _rc: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let value = h.param(0).unwrap().value().clone();
                let rendered = f(value, mapper.as_ref(), ast);
                out.write(&rendered)?;
                Ok(())
            },
        )
    }
}
