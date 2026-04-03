use askama::Template;
use ast::{CidlType, CloesceAst, IncludeTree};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'src> {
    ast: &'src CloesceAst<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl BackendTemplate<'_> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.ast)
    }

    fn is_crud_method(&self, name: &str) -> bool {
        name == "$get" || name == "$save" || name == "$list"
    }

    fn include_tree_to_js(&self, tree: &IncludeTree<'_>) -> String {
        serde_json::to_string(&tree).unwrap()
    }
}

pub struct BackendGenerator;
impl BackendGenerator {
    pub fn generate(ast: &CloesceAst, worker_url: &str) -> String {
        let tmpl = BackendTemplate {
            ast,
            worker_url,
            mapper: TypeScriptMapper::backend(),
        };
        tmpl.render().unwrap()
    }
}
