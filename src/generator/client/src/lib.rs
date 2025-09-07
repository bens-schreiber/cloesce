pub mod mappers;

use std::sync::Arc;

use common::{CidlSpec, CidlType, InputLanguage};
use handlebars::Handlebars;

use mappers::TypeScriptMapper;

pub trait LanguageTypeMapper {
    fn type_name(&self, ty: &CidlType, nullable: bool) -> String;
}

/// A helper that can be used inside of an `.hbs` file via `lang_type`
fn register_type_mapper(
    handlebars: &mut Handlebars<'_>,
    mapper: Arc<dyn LanguageTypeMapper + Send + Sync>,
) {
    handlebars.register_helper(
        "lang_type",
        Box::new(
            move |h: &handlebars::Helper<'_>,
                  _: &Handlebars,
                  _: &handlebars::Context,
                  _: &mut handlebars::RenderContext<'_, '_>,
                  out: &mut dyn handlebars::Output| {
                let cidl_type: CidlType =
                    serde_json::from_value(h.param(0).unwrap().value().clone())
                        .expect("Expected CidlType");

                let nullable: bool = h
                    .param(1)
                    .and_then(|v| v.value().as_bool())
                    .unwrap_or(false);

                let rendered = mapper.type_name(&cidl_type, nullable);
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
    register_type_mapper(&mut handlebars, mapper);

    // TODO: Determine where we want the domain passed in...
    let mut context = serde_json::to_value(&spec).unwrap();
    if let serde_json::Value::Object(ref mut map) = context {
        map.insert("domain".to_string(), serde_json::Value::String(domain));
    }

    handlebars.render("models", &context).unwrap()
}
