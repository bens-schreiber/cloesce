mod typescript;

use anyhow::Result;
use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, TypedValue};
use std::path::Path;
use typescript::TypescriptWorkersGenerator;

trait LanguageWorkerGenerator {
    fn imports(&self, models: &[Model], workers_path: &Path) -> Result<String>;
    fn preamble(&self) -> String;
    fn validators(&self, models: &[Model]) -> String;
    fn main(&self) -> String;
    fn router(&self, model: String) -> String;
    fn router_model(&self, model_name: &str, method: String) -> String;
    fn router_method(&self, method: &Method, proto: String) -> String;
    fn proto(&self, method: &Method, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn hydrate_model(&self, model: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
    fn validate_request(&self, method: &Method) -> String;
}

pub struct WorkersFactory;
impl WorkersFactory {
    fn model(model: &Model, lang: &dyn LanguageWorkerGenerator) -> String {
        let mut router_methods = vec![];
        for method in &model.methods {
            let validate_http = lang.validate_http(&method.http_verb);
            let validate_params = lang.validate_request(method);
            let hydration = if method.is_static {
                String::new()
            } else {
                lang.hydrate_model(model)
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

    pub fn create(self, spec: CidlSpec, workers_path: &Path) -> Result<String> {
        let generator: &dyn LanguageWorkerGenerator = match spec.language {
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
                .join("\n");
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
