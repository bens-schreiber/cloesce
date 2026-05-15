use askama::Template;
use idl::{CidlType, CloesceIdl};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'src> {
    idl: &'src CloesceIdl<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl BackendTemplate<'_> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.idl)
    }

    fn is_generated_method(&self, name: &str) -> bool {
        name.starts_with('$')
    }

    fn is_env_injected(&self, name: &str) -> bool {
        !self.idl.injects.contains(&name)
    }
}

pub struct BackendGenerator;
impl BackendGenerator {
    pub fn generate(idl: &CloesceIdl, worker_url: &str) -> String {
        let tmpl = BackendTemplate {
            idl,
            worker_url,
            mapper: TypeScriptMapper::backend(),
        };
        tmpl.render().unwrap()
    }
}
