mod typescript;
use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, TypedValue};
use typescript::TypescriptWorkersGenerator;
use std::collections::HashMap;

#[derive(Default)]
pub struct TrieNode {
    value: String,
    children: HashMap<String, TrieNode>,
}

#[derive(Default)]
pub struct RouterTrie {
    root: TrieNode,
}

impl RouterTrie {
    pub fn insert(&mut self, path: Vec<&str>, value: String) {
        let mut current = &mut self.root;
        for segment in path {
            current = current.children.entry(segment.to_string()).or_default();
        }
        current.value = value;
    }
    
    pub fn get(&self, path: Vec<&str>) -> Option<&String> {
        let mut current = &self.root;
        for segment in path {
            current = current.children.get(segment)?;
        }
        Some(&current.value)
    }
}

trait LanguageWorkerGenerator {
    fn imports(&self, models: &[Model]) -> String;
    fn preamble(&self) -> String;
    fn validators(&self, models: &[Model]) -> String;
    fn main(&self) -> String;
    
    // Router building methods - these modify internal state
    fn router_init(&mut self, root_path: &str);
    fn router_add_method(&mut self, model_name: &str, method: &Method, is_instance: bool);
    fn router_build(&self) -> String;
    
    // Method generation
    fn proto(&self, method: &Method, body: String) -> String;
    fn validate_http(&self, verb: &HttpVerb) -> String;
    fn validate_req_body(&self, params: &[TypedValue]) -> String;
    fn hydrate_model(&self, model: &Model) -> String;
    fn dispatch_method(&self, model_name: &str, method: &Method) -> String;
}

pub struct WorkersFactory;

impl WorkersFactory {
    pub fn new() -> Self {
        Self
    }
   
    fn build_routes(models: &[Model], lang: &mut dyn LanguageWorkerGenerator) {
        for model in models {
            for method in &model.methods {
                // Let the language implementation handle how to add this method to the router
                lang.router_add_method(&model.name, method, !method.is_static);
            }
        }
    }
    
    pub fn create(self, spec: CidlSpec, domain: Option<String>) -> String {
        // Parse domain first to get root_path before moving domain
        let (_, root_path) = TypescriptWorkersGenerator::parse_domain(domain.clone());
        
        let mut generator: Box<dyn LanguageWorkerGenerator> = match spec.language {
            InputLanguage::TypeScript => {
                Box::new(TypescriptWorkersGenerator::new(domain))
            }
        };
        
        // Initialize the router with domain info
        generator.router_init(&root_path);
        
        // Build all routes
        Self::build_routes(&spec.models, generator.as_mut());
        
        // Generate the final output
        let imports = generator.imports(&spec.models);
        let preamble = generator.preamble();
        let validators = generator.validators(&spec.models);
        let router = generator.router_build();
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