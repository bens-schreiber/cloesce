use askama::Template;
use ast::{ApiMethod, CidlType, CloesceAst};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'src> {
    ast: &'src CloesceAst<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl<'src> BackendTemplate<'src> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.ast)
    }

    fn is_generated_method(&self, name: &str) -> bool {
        name.starts_with('$')
    }

    /// Renders the injected `env: { name: Type, ... }` parameter for a method,
    /// or the empty string if the method has no injects.
    /// Includes a leading comma if `prepend_comma` is true and there are injects.
    fn injected_env_param(&self, api: &ApiMethod<'_>, prepend_comma: bool) -> String {
        if api.injected.is_empty() {
            return String::new();
        }
        let shape = api
            .injected
            .iter()
            .map(|name| format!("{name}: {}", self.injected_ts_type(name)))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix = if prepend_comma { ", " } else { "" };
        format!("{prefix}env: {{ {shape} }}")
    }

    /// Looks up the TypeScript type for an injected symbol name.
    fn injected_ts_type(&self, name: &str) -> String {
        if let Some(env) = &self.ast.wrangler_env {
            if env.d1_bindings.iter().any(|b| *b == name) {
                return "D1Database".to_string();
            }
            if env.kv_bindings.iter().any(|b| *b == name) {
                return "KVNamespace".to_string();
            }
            if env.r2_bindings.iter().any(|b| *b == name) {
                return "R2Bucket".to_string();
            }
            if let Some(var) = env.vars.iter().find(|v| v.name == name) {
                return self.map_type(&var.cidl_type);
            }
        }

        if self.ast.injects.iter().any(|s| *s == name) {
            return name.to_string();
        }

        // Should be unreachable: semantic analysis validates the name.
        "unknown".to_string()
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
