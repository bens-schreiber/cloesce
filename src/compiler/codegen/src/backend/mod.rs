use askama::Template;
use ast::{CidlType, CloesceAst, IncludeTree};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'src> {
    ast: &'src CloesceAst<'src>,
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
        if tree.0.is_empty() {
            return "{}".to_string();
        }
        let mut parts = Vec::new();
        for (key, subtree) in &tree.0 {
            parts.push(format!("{}: {}", key, self.include_tree_to_js(subtree)));
        }
        format!("{{ {} }}", parts.join(", "))
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
