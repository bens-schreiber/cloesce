mod typescript;

use std::collections::HashMap;

use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, TypedValue};
use typescript::TypescriptWorkersGenerator;

struct TrieNode {
    value: String,
    children: HashMap<String, TrieNode>,
}

impl TrieNode {
    fn new(value: String) -> Self {
        Self {
            value,
            children: HashMap::default(),
        }
    }
}

struct RouterTrie {
    root: TrieNode,
}

impl RouterTrie {
    fn new(domain: &str) -> Self {
        Self {
            root: TrieNode {
                value: domain.to_string(),
                children: HashMap::default(),
            },
        }
    }
}

trait LanguageWorkerGenerator {
    /// Necessary imports for the language
    fn imports(&self, models: &[Model]) -> String;

    /// Necessary boilerplate for the language
    fn preamble(&self) -> String;

    /// Model validators
    fn validators(&self, models: &[Model]) -> String;

    /// Workers entrypoint
    fn main(&self) -> String;

    /// Adds a method onto a model in the router.
    fn router_method(
        &self,
        model_name: &str,
        method: &Method,
        proto: String,
        router: &mut RouterTrie,
    );

    /// Serializes the router into a language appropriate router trie
    fn router_serialize(&self, router: &RouterTrie) -> String;

    /// Places a function body inside of a function prototype or header
    fn proto(&self, method: &Method, body: String) -> String;

    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[TypedValue]) -> String;
    fn hydrate_model(&self, model_name: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
}

pub struct WorkersFactory;
impl WorkersFactory {
    fn model(model: &Model, lang: &dyn LanguageWorkerGenerator, router: &mut RouterTrie) {
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
            lang.router_method(&model.name, method, proto, router);
        }
    }

    pub fn create(&self, spec: CidlSpec, domain: &str) -> String {
        let generator: &mut dyn LanguageWorkerGenerator = match spec.language {
            InputLanguage::TypeScript => &mut TypescriptWorkersGenerator,
        };

        let imports = generator.imports(&spec.models);
        let preamble = generator.preamble();
        let validators = generator.validators(&spec.models);

        let router = {
            let mut router = RouterTrie::new(domain);
            for m in spec.models {
                Self::model(&m, generator, &mut router);
            }
            generator.router_serialize(&router)
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
