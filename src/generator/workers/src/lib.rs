mod typescript;

use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, TypedValue};
use typescript::TypescriptWorkersGenerator;

trait LanguageWorkerGenerator {
    fn imports(&self, models: &[Model]) -> String;
    fn preamble(&self) -> String;
    fn main(&self) -> String;
    fn router(&self, model: String) -> String;
    fn router_model(&self, model_name: &str, method: String) -> String;
    fn router_method(&self, method: &Method, proto: String) -> String;
    fn proto(&self, method: &Method, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[TypedValue]) -> String;
    fn instantiate_model(&self, model_name: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
}

pub struct WorkersFactory;
impl WorkersFactory {
    fn model(model: &Model, lang: &dyn LanguageWorkerGenerator) -> String {
        let mut router_methods = vec![];
        for method in &model.methods {
            let validate_http = lang.validate_http(&method.http_verb);
            let validate_params = lang.validate_req_body(&method.parameters);
            let hydration = if method.is_static {
                ""
            } else {
                &lang.instantiate_model(model)
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

    pub fn create(self, spec: CidlSpec) -> String {
        let generator: &dyn LanguageWorkerGenerator = match spec.language {
            InputLanguage::TypeScript => &TypescriptWorkersGenerator {},
        };

        let imports = generator.imports(&spec.models);
        let preamble = generator.preamble();

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

        format!(
            r#" 
{imports}
{preamble}
{router}
{main}
        "#
        )
    }
}
