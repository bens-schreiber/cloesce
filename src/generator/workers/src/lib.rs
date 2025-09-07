use anyhow::anyhow;
use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, WranglerSpec};

use crate::builders::typescript::TsWorkersApiBuilder;
mod builders;
mod tests;

pub trait WorkersApiBuilder {
    fn imports(&mut self, models: &[Model]) -> &mut Self;

    fn parameter_validation(&mut self, methods: &[Method]) -> &mut Self;

    fn http_verb_validation(&mut self, verbs: &[HttpVerb]) -> &mut Self;

    fn method_handlers(&mut self, models: &[Model]) -> &mut Self;

    fn router_trie(&mut self, models: &[Model]) -> &mut Self;

    fn route_matcher(&mut self) -> &mut Self;

    fn fetch_handler(&mut self) -> &mut Self;

    /// Adds header/metadata to the stack
    fn header(&mut self, version: &str, project_name: &str) -> &mut Self;

    fn build(&self) -> Result<String, anyhow::Error>;
}

pub struct WorkersGenerator;

impl WorkersGenerator {
    pub fn generate(cidl: &CidlSpec, wrangler: &WranglerSpec) -> Result<String, anyhow::Error> {
        if !matches!(cidl.language, InputLanguage::TypeScript) {
            return Err(anyhow!("Only TypeScript is currently supported"));
        }

        let all_methods: Vec<Method> = cidl
            .models
            .iter()
            .flat_map(|model| model.methods.clone())
            .collect();

        let all_verbs: Vec<HttpVerb> = all_methods
            .iter()
            .map(|method| method.http_verb.clone())
            .collect();

        let mut builder = TsWorkersApiBuilder::new();

        builder
            .header(&cidl.version, &cidl.project_name)
            .imports(&cidl.models)
            .parameter_validation(&all_methods)
            .http_verb_validation(&all_verbs)
            .method_handlers(&cidl.models)
            .router_trie(&cidl.models)
            .route_matcher()
            .fetch_handler()
            .build()
    }
}
