mod builders;
mod tests;
use anyhow::Result;
use common::{CidlSpec, InputLanguage, WranglerSpec};

use crate::builders::typescript::TsWorkersApiBuilder;

pub trait WorkersApiBuilder {
    fn build(&self) -> Result<String>;
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
