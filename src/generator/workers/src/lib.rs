mod typescript;

use std::{collections::BTreeMap, path::Path};

use common::{CidlSpec, HttpVerb, InputLanguage, Model, ModelMethod};
use typescript::TypescriptWorkersGenerator;

use anyhow::{Result, anyhow};

struct TrieNode {
    value: String,
    children: BTreeMap<String, TrieNode>,
}

impl TrieNode {
    fn new(value: String) -> Self {
        Self {
            value,
            children: BTreeMap::default(),
        }
    }
}

struct RouterTrie {
    root: TrieNode,
}

impl RouterTrie {
    fn from_domain(domain: String) -> Self {
        Self {
            root: TrieNode {
                value: format!("\"{domain}\""),
                children: BTreeMap::default(),
            },
        }
    }
}

trait WorkersGenerateable {
    /// Necessary imports for the language
    fn imports(&self, models: &[Model], workers_path: &Path) -> Result<String>;

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
        method: &ModelMethod,
        proto: String,
        router: &mut RouterTrie,
    );

    /// Serializes the router into a language appropriate router trie
    fn router_serialize(&self, router: &RouterTrie) -> String;

    /// Places a function body inside of a function prototype or header
    fn proto(&self, method: &ModelMethod, body: String) -> String;

    /// Validates that the request matches the correct http verb
    fn validate_http(&self, verb: &HttpVerb) -> String;

    /// Validates that the request body has the correct structure and input
    fn validate_request(&self, method: &ModelMethod) -> String;

    /// Fetches the model from the database on instantiated methods
    fn hydrate_model(&self, model: &Model) -> String;

    /// Dispatches the model function
    fn dispatch_method(&self, model_name: &str, method: &ModelMethod) -> String;
}

pub struct WorkersGenerator;
impl WorkersGenerator {
    fn model(model: &Model, lang: &dyn WorkersGenerateable, router: &mut RouterTrie) {
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
            lang.router_method(&model.name, method, proto, router);
        }
    }

    /// Returns the API route
    fn validate_domain(domain: &str) -> Result<String> {
        if domain.is_empty() {
            return Err(anyhow!("Empty domain."));
        }

        match domain.split_once("://") {
            None => Err(anyhow!("Missing HTTP protocol")),
            Some((protocol, rest)) => {
                if protocol != "http" {
                    return Err(anyhow!("Unsupported protocol {}", protocol));
                }

                match rest.split_once("/") {
                    None => Err(anyhow!("Missing API route on domain")),
                    Some((_, rest)) => Ok(rest.to_string()),
                }
            }
        }
    }

    pub fn create(&self, spec: CidlSpec, domain: String, workers_path: &Path) -> Result<String> {
        let api_route = Self::validate_domain(&domain)?;

        let generator: &dyn WorkersGenerateable = match spec.language {
            InputLanguage::TypeScript => &TypescriptWorkersGenerator {},
        };

        let imports = generator.imports(&spec.models, workers_path)?;
        let preamble = generator.preamble();
        let validators = generator.validators(&spec.models);
        let router = {
            let mut router = RouterTrie::from_domain(api_route);
            for m in spec.models {
                Self::model(&m, generator, &mut router);
            }
            generator.router_serialize(&router)
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
