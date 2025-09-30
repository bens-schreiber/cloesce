mod mappers;

use std::{ops::Deref, sync::Arc};

use common::{CidlSpec, CidlType, InputLanguage, NavigationProperty};
use handlebars::{Handlebars, handlebars_helper};

use mappers::TypeScriptMapper;

pub trait ClientLanguageTypeMapper {
    fn type_name(&self, ty: &CidlType) -> String;
}

handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type, CidlType::Inject(_)));
handlebars_helper!(is_model: |cidl_type: CidlType| match cidl_type {
    CidlType::Model(_) => true,
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Model(_)),
    _ => false,
});
handlebars_helper!(is_model_array: |cidl_type: CidlType| match cidl_type {
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Array(inner2) if matches!(inner2.deref(), CidlType::Model(_))),
    CidlType::Array(inner) => matches!(inner.deref(), CidlType::Model(_)),
    _ => false,
});
handlebars_helper!(eq: |a: str, b: str| a == b);

fn register_helpers(
    handlebars: &mut Handlebars<'_>,
    mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync>,
) {
    handlebars.register_helper("is_serializable", Box::new(is_serializable));
    handlebars.register_helper("is_model", Box::new(is_model));
    handlebars.register_helper("is_model_array", Box::new(is_model_array));
    handlebars.register_helper("eq", Box::new(eq));

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
                    common::NavigationPropertyKind::OneToOne { .. } => {
                        CidlType::Model(nav.model_name)
                    }
                    common::NavigationPropertyKind::OneToMany { .. }
                    | common::NavigationPropertyKind::ManyToMany { .. } => {
                        CidlType::array(CidlType::Model(nav.model_name))
                    }
                };

                let rendered = mapper1.type_name(&cidl_type);
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

                let rendered = mapper2.type_name(&cidl_type);
                out.write(&rendered)?;
                Ok(())
            },
        ),
    );
}

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");

pub fn generate_client_api(spec: CidlSpec, domain: String) -> String {
    let template = match spec.language {
        InputLanguage::TypeScript => TYPESCRIPT_TEMPLATE,
    };

    let mapper = match spec.language {
        InputLanguage::TypeScript => Arc::new(TypeScriptMapper),
    };

    let mut handlebars = Handlebars::new();
    handlebars
        .register_template_string("models", template)
        .unwrap();
    register_helpers(&mut handlebars, mapper);

    let mut context = serde_json::to_value(&spec).unwrap();
    if let serde_json::Value::Object(ref mut map) = context {
        map.insert("domain".to_string(), serde_json::Value::String(domain));
    }

    handlebars.render("models", &context).unwrap()
}
