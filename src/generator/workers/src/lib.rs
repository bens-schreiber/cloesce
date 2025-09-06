mod builders;
mod tests;
use anyhow::Result;
use common::{CidlSpec, HttpVerb, InputLanguage, Method, Model, WranglerSpec};

use crate::builders::typescript::TsWorkersApiBuilder;

/// Main trait defining the interface for generating Workers API code
pub trait WorkersApiBuilder {
    /// Creates a new builder instance
    fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self;

    fn generate_imports(&self) -> String;

    fn generate_parameter_validation(&self, method: &Method) -> String;

    fn generate_http_verb_validation(&self, verb: &HttpVerb) -> String;

    fn generate_method_handler(&self, model: &Model, method: &Method) -> String;

    fn build_router_trie(&self) -> String;

    fn generate_match_function(&self) -> String;

    fn generate_fetch_handler(&self) -> String;

    fn build(&self) -> Result<String, anyhow::Error>;
}

pub struct WorkersGenerator {
    cidl: CidlSpec,
    wrangler: WranglerSpec,
}

impl WorkersGenerator {
    pub fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self {
        Self { cidl, wrangler }
    }

    pub fn generate(&self) -> Result<String> {
        match self.cidl.language {
            InputLanguage::TypeScript => {
                let builder = TsWorkersApiBuilder::new(self.cidl.clone(), self.wrangler.clone());
                builder.build()
            }
        }
    }
}
