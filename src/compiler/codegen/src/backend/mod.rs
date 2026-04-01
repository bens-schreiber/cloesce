use askama::Template;
use ast::{CidlType, CloesceAst};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'a> {
    ast: &'a CloesceAst<'a>,
    mapper: TypeScriptMapper,
}

impl BackendTemplate<'_> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.ast)
    }

    fn is_crud_method(&self, name: &str) -> bool {
        name == "$get" || name == "$save" || name == "$list"
    }
}

pub struct BackendGenerator;
impl BackendGenerator {
    pub fn generate(ast: &CloesceAst) -> String {
        let tmpl = BackendTemplate {
            ast,
            mapper: TypeScriptMapper::backend(),
        };
        tmpl.render().unwrap()
    }
}
