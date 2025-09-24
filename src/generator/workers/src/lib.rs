mod typescript;
use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, TypedValue};
use typescript::TypescriptWorkersGenerator;

trait LanguageWorkerGenerator {
    fn imports(&self, models: &[Model]) -> String;
    fn preamble(&self) -> String;
    fn validators(&self, models: &[Model]) -> String;
    fn main(&self) -> String;
    fn router(&self, model: String) -> String;
    fn router_model(&self, model_name: &str, method: String) -> String;
    fn router_method(&self, method: &Method, proto: String) -> String;
    fn proto(&self, method: &Method, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[TypedValue]) -> String;
    fn hydrate_model(&self, model_name: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
}

pub struct WorkersFactory {
    domain: Option<String>,
}

impl WorkersFactory {
    pub fn new(domain: String) -> Self {
        Self { domain: Some(domain) }
    }

    // If you want builder-style, you can still add a setter:
    pub fn with_domain(mut self, domain: String) -> Self {
        self.domain = Some(domain);
        self
    }
   
    fn model(model: &Model, lang: &dyn LanguageWorkerGenerator) -> String {
        let mut static_methods = vec![];
        let mut instance_methods = vec![];
       
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
            let router_method = lang.router_method(method, proto);
           
            if method.is_static {
                static_methods.push(router_method);
            } else {
                instance_methods.push(router_method);
            }
        }
       
        // Combine static methods and instance methods under <id> if any exist
        let mut all_methods = static_methods;
        if !instance_methods.is_empty() {
            let instance_routes = instance_methods.join(",\n");
            all_methods.push(format!(r#""<id>": {{ {} }}"#, instance_routes));
        }
       
        lang.router_model(&model.name, all_methods.join(",\n"))
    }

    pub fn create(self, spec: CidlSpec) -> String {
        let generator: Box<dyn LanguageWorkerGenerator> = match spec.language {
            InputLanguage::TypeScript => {
                Box::new(TypescriptWorkersGenerator::new(self.domain))
            }
        };

        let imports = generator.imports(&spec.models);
        let preamble = generator.preamble();
        let validators = generator.validators(&spec.models);
        let router = {
            let router_body = spec
                .models
                .iter()
                .map(|m| Self::model(m, generator.as_ref()))
                .collect::<Vec<_>>()
                .join("\n");
            generator.router(router_body)
        };
        let main = generator.main();
       
        format!(
            r#"
{imports}
{preamble}
{validators}
{router}
{main}
        "#
        )
    }
}