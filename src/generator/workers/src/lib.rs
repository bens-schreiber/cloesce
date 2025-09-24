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
    //adds all instance methods under "<id>" key
    fn router_instance_method(&self, methods: Vec<String>) -> String;
    fn proto(&self, method: &Method, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[TypedValue]) -> String;
    fn hydrate_model(&self, model: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
}

pub struct WorkersFactory;
impl WorkersFactory {
    fn model(model: &Model, lang: &dyn LanguageWorkerGenerator) -> String {
        // Separate static and instance methods
        let static_methods: Vec<_> = model.methods.iter().filter(|m| m.is_static).collect();
        let instance_methods: Vec<_> = model.methods.iter().filter(|m| !m.is_static).collect();

        let mut router_entries = vec![];

        // Add static methods directly at the model level
        for method in &static_methods {
            let validate_http = lang.validate_http(&method.http_verb);
            let validate_params = lang.validate_req_body(&method.parameters);
            let dispatch = lang.dispatch_method(&model.name, method);

            let method_body = format!(
                r#"
                {validate_http}
                {validate_params}
                {dispatch}
            "#
            );

            let proto = lang.proto(method, method_body);
            router_entries.push(proto);
        }

        // Group all instance methods under a single "<id>" key
        if !instance_methods.is_empty() {
            let mut instance_router_methods = vec![];

            for method in &instance_methods {
                let validate_http = lang.validate_http(&method.http_verb);
                let validate_params = lang.validate_req_body(&method.parameters);
                let hydration = lang.hydrate_model(model);
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
                instance_router_methods.push(proto);
            }

            // Add the grouped instance methods under "<id>"
            let instanceMethods = lang.router_instance_method(instance_router_methods);
            router_entries.push(instanceMethods);
        }

        lang.router_model(&model.name, router_entries.join(","))
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
