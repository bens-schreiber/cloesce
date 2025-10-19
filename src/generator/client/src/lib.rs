mod mappers;

use std::{ops::Deref, sync::Arc};

use common::{
    CidlType, CloesceAst, CrudKind, InputLanguage, NavigationProperty, NavigationPropertyKind,
};
use mappers::{ClientLanguageTypeMapper, TypeScriptMapper};

use handlebars::{Handlebars, handlebars_helper};

handlebars_helper!(is_serializable: |cidl_type: CidlType| !matches!(cidl_type.root_type(), CidlType::Inject(_)));
handlebars_helper!(is_object: |cidl_type: CidlType| match cidl_type {
    CidlType::Object(_) => true,
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Object(_)),
    _ => false,
});
handlebars_helper!(is_object_array: |cidl_type: CidlType| match cidl_type {
    CidlType::HttpResult(inner) => matches!(inner.deref(), CidlType::Array(inner2) if matches!(inner2.deref(), CidlType::Object(_))),
    CidlType::Array(inner) => matches!(inner.deref(), CidlType::Object(_)),
    _ => false,
});
handlebars_helper!(is_one_to_one: |nav: NavigationProperty| matches!(nav.kind, NavigationPropertyKind::OneToOne {..}));
handlebars_helper!(object_name: |cidl_type: CidlType| match cidl_type.root_type() {
    CidlType::Object(name) => name.clone(),
    _ => panic!("Not an object")
});
handlebars_helper!(crud_name: |crud: CrudKind, kind: str| format!("{crud:?}") == kind);
handlebars_helper!(eq: |a: str, b: str| a == b);

fn register_helpers(
    handlebars: &mut Handlebars<'_>,
    mapper: Arc<dyn ClientLanguageTypeMapper + Send + Sync>,
) {
    handlebars.register_helper("is_serializable", Box::new(is_serializable));
    handlebars.register_helper("is_object", Box::new(is_object));
    handlebars.register_helper("is_object_array", Box::new(is_object_array));
    handlebars.register_helper("is_one_to_one", Box::new(is_one_to_one));
    handlebars.register_helper("object_name", Box::new(object_name));
    handlebars.register_helper("crud_name", Box::new(crud_name));
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
                        CidlType::Object(nav.model_name)
                    }
                    common::NavigationPropertyKind::OneToMany { .. }
                    | common::NavigationPropertyKind::ManyToMany { .. } => {
                        CidlType::array(CidlType::Object(nav.model_name))
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

pub fn generate_client_api(ast: CloesceAst, domain: String) -> String {
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
    register_helpers(&mut handlebars, mapper);

    let mut context = serde_json::to_value(&ast).unwrap();
    if let serde_json::Value::Object(ref mut map) = context {
        map.insert("domain".to_string(), serde_json::Value::String(domain));
    }

    handlebars.render("models", &context).unwrap()
}
