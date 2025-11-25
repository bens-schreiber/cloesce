mod mappers;

use std::{ops::Deref, sync::Arc};

use ast::{
    CidlType, CloesceAst, InputLanguage, MediaType, NavigationProperty, NavigationPropertyKind,
};
use mappers::{ClientLanguageTypeMapper, TypeScriptMapper};

use handlebars::{Handlebars, handlebars_helper};

handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type.root_type(), CidlType::Inject(_)));
handlebars_helper!(is_object: |cidl_type: CidlType| match cidl_type {
    CidlType::Object(_) => true,
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Object(_)),
    _ => false,
});
handlebars_helper!(object_name: |cidl_type: CidlType| match cidl_type.root_type() {
    CidlType::Object(name) => name.clone(),
    _ => unreachable!()
});
handlebars_helper!(is_object_array: |cidl_type: CidlType| match cidl_type {
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Array(inner2) if matches!(inner2.deref(), CidlType::Object(_))),
    CidlType::Array(inner) => matches!(inner.deref(), CidlType::Object(_)),
    _ => false,
});
handlebars_helper!(is_blob: |cidl_type: CidlType| match cidl_type {
    CidlType::Blob => true,
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Blob),
    _ => false,
});
handlebars_helper!(is_blob_array: |cidl_type: CidlType| match cidl_type {
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Array(inner2) if matches!(inner2.deref(), CidlType::Blob)),
    CidlType::Array(inner) => matches!(inner.deref(), CidlType::Blob),
    _ => false,
});
handlebars_helper!(is_one_to_one: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToOne {..}));
handlebars_helper!(is_many_nav: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToMany {..} | NavigationPropertyKind::ManyToMany { .. }));
handlebars_helper!(eq: |a: str, b: str| a == b);
handlebars_helper!(get_content_type: |media: MediaType| match media {
    MediaType::Json=>"application/json",
    MediaType::Octet => "application/octet-stream",
});

fn register_helpers<'a>(
    handlebars: &mut Handlebars<'a>,
    mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync>,
    ast: &'a CloesceAst,
) {
    handlebars.register_helper("is_serializable", Box::new(is_serializable));
    handlebars.register_helper("is_object", Box::new(is_object));
    handlebars.register_helper("is_object_array", Box::new(is_object_array));
    handlebars.register_helper("is_blob", Box::new(is_blob));
    handlebars.register_helper("is_blob_array", Box::new(is_blob_array));
    handlebars.register_helper("is_one_to_one", Box::new(is_one_to_one));
    handlebars.register_helper("is_many_nav", Box::new(is_many_nav));
    handlebars.register_helper("object_name", Box::new(object_name));
    handlebars.register_helper("eq", Box::new(eq));
    handlebars.register_helper("get_content_type", Box::new(get_content_type));

    let mapper1 = mapper.clone();
    handlebars.register_helper(
        "get_nav_cidl_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let nav: NavigationProperty =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let cidl_type = match nav.kind {
                    NavigationPropertyKind::OneToOne { .. } => CidlType::Object(nav.model_name),
                    NavigationPropertyKind::OneToMany { .. }
                    | NavigationPropertyKind::ManyToMany { .. } => {
                        CidlType::array(CidlType::Object(nav.model_name))
                    }
                };

                let rendered = mapper1.cidl_type(&cidl_type, ast);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );

    let mapper2 = mapper.clone();
    handlebars.register_helper(
        "get_cidl_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let cidl_type: CidlType =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let rendered = mapper2.cidl_type(&cidl_type, ast);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );

    let mapper3 = mapper.clone();
    handlebars.register_helper(
        "get_media_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let media_type: MediaType =
                    serde_json::from_value(h.param(0).unwrap().value().clone()).unwrap();

                let rendered = mapper3.media_type(&media_type);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );
}

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");

pub fn generate_client_api(ast: &CloesceAst, domain: String) -> String {
    let template = match ast.language {
        InputLanguage::TypeScript => TYPESCRIPT_TEMPLATE,
    };

    let mapper = match ast.language {
        InputLanguage::TypeScript => Arc::new(TypeScriptMapper),
    };

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("models", template)
        .unwrap();
    register_helpers(&mut handlebars, mapper, ast);

    let mut context = serde_json::to_value(ast).unwrap();
    if let serde_json::Value::Object(ref mut map) = context {
        map.insert("domain".to_string(), serde_json::Value::String(domain));
    }

    handlebars.render("models", &context).unwrap()
}
