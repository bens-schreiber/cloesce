mod typescript;

use std::path::Path;

use common::{CidlSpec, HttpVerb, InputLanguage, Model, ModelMethod, NamedTypedValue};
use typescript::TypescriptWorkersGenerator;

use anyhow::Result;

trait WorkersGeneratable {
    fn imports(&self, models: &[Model], workers_path: &Path) -> Result<String>;
    fn preamble(&self) -> String;
    fn validators(&self, models: &[Model]) -> String;
    fn main(&self) -> String;
    fn router(&self, model: String) -> String;
    fn router_model(&self, model_name: &str, method: String) -> String;
    fn router_method(&self, method: &ModelMethod, proto: String) -> String;
    fn proto(&self, method: &ModelMethod, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[NamedTypedValue]) -> String;
    fn hydrate_model(&self, model_name: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &ModelMethod) -> String;
}

pub struct WorkersGenerator;
impl WorkersGenerator {
    fn model(model: &Model, lang: &dyn WorkersGeneratable) -> String {
        let mut router_methods = vec![];
        for method in &model.methods {
            let validate_http = lang.validate_http(&method.http_verb);
            let validate_params = lang.validate_req_body(&method.parameters);
            let hydration = if method.is_static {
                ""
            } else {
                &lang.hydrate_model(model)
            };
            let dispatch = lang.dispatch_method(&model.name, method);

            let method_body = format!(
                r#"
                {validate_http}
                {validate_params}
                {hydration}
                {dispatch}
            "#
            );

            let proto = lang.proto(method, method_body);

            router_methods.push(lang.router_method(method, proto))
        }

        lang.router_model(&model.name, router_methods.join(",\n"))
    }

    pub fn generate(self, spec: CidlSpec, workers_path: &Path) -> Result<String> {
        let generator: &dyn WorkersGeneratable = match spec.language {
            InputLanguage::TypeScript => &TypescriptWorkersGenerator {},
        };

        let imports = generator.imports(&spec.models, workers_path)?;
        let preamble = generator.preamble();
        let validators = generator.validators(&spec.models);
        let router = {
            let router_body = spec
                .models
                .iter()
                .map(|m| Self::model(m, generator))
                .collect::<Vec<_>>()
                .join(",\n");
            generator.router(router_body)
        };
        let main = generator.main();

        Ok(format!(
            r#" 
        {imports}
        {preamble}
        {validators}
        {router}
        {main}
        "#
        ))
    }
}
