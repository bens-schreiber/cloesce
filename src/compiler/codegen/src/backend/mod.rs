use std::sync::Arc;

use ast::{CidlType, CloesceAst};
use handlebars::Handlebars;

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper, make_mapper_helper};

const TYPESCRIPT_TEMPLATE: &str = include_str!("./templates/ts.hbs");
const TEMPLATE_STRING: &str = "backend_types";

pub struct BackendGenerator;
impl BackendGenerator {
    pub fn generate(ast: &CloesceAst) -> String {
        // TODO: Hardcoded TypeScript for now
        let template = TYPESCRIPT_TEMPLATE;
        let mapper = Arc::new(TypeScriptMapper::with_namespaces());
        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string(TEMPLATE_STRING, template)
            .unwrap();

        let context = serde_json::to_value(ast).unwrap();

        handlebars.register_helper(
            "map_cidl_type",
            make_mapper_helper(
                mapper.clone(),
                ast,
                |value: serde_json::Value,
                 mapper: &dyn LanguageTypeMapper,
                 ast: &CloesceAst|
                 -> String {
                    let cidl_type: CidlType = serde_json::from_value(value).unwrap();
                    mapper.cidl_type(&cidl_type, ast)
                },
            ),
        );

        handlebars.render(TEMPLATE_STRING, &context).unwrap()
    }
}
