use askama::Template;
use idl::{
    ApiMethod, CidlType, CloesceIdl, DurableBinding, IncludeTree, Model, ValidatedField,
    model_bindings,
};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

#[derive(Template)]
#[template(path = "backend.ts.jinja", escape = "none")]
struct BackendTemplate<'src> {
    idl: &'src CloesceIdl<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl<'src> BackendTemplate<'src> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.idl)
    }

    fn is_generated_method(&self, name: &str) -> bool {
        name.starts_with('$')
    }

    fn inject_type(&self, name: &str) -> String {
        self.mapper.inject_type(self.idl, name)
    }

    fn api_injected_type(&self, api: &ApiMethod, name: &str) -> String {
        self.mapper.api_injected_type(self.idl, api, name)
    }

    fn ds_injected_type(&self, model: &Model<'_>, name: &str) -> String {
        match &model.backing {
            Some(backing) if model.is_durable_backed() && name == idl::CONTEXT_INJECT_KEY => {
                backing.binding.to_string()
            }
            _ => self.mapper.inject_type(self.idl, name),
        }
    }

    fn backing_binding(&self, model: &Model<'_>) -> String {
        model
            .backing
            .as_ref()
            .map(|b| b.binding.to_string())
            .unwrap_or_default()
    }

    fn context_inject_key(&self) -> &'static str {
        idl::CONTEXT_INJECT_KEY
    }

    fn interpolate_key_format(&self, format: &str, params: &[ValidatedField<'_>]) -> String {
        let names = params.iter().map(|p| p.name.as_ref());
        self.mapper.interpolate_format(format, names)
    }

    fn shard_template(&self, binding: &DurableBinding<'_>) -> String {
        let mut format = binding.name.to_string();
        for field in &binding.shard_fields {
            format.push_str(&format!("/{{{}}}", field.name));
        }
        let names = binding.shard_fields.iter().map(|f| f.name.as_ref());
        self.mapper.interpolate_format(&format, names)
    }

    fn model_bindings(
        &self,
        model: &Model<'src>,
        include: Option<&IncludeTree<'src>>,
    ) -> Vec<&'src str> {
        model_bindings(self.idl, model, include)
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
        tmpl.render().expect("Failed to render backend template")
    }
}

#[cfg(test)]
mod tests {
    use super::BackendGenerator;
    use compiler_test::{COMPREHENSIVE_SRC, src_to_idl};

    #[test]
    fn backend_code_generation_snapshot() {
        const WORKERS_URL: &str = "http://example.com/path/to/api";
        let idl = src_to_idl(COMPREHENSIVE_SRC);

        let backend_code = BackendGenerator::generate(&idl, WORKERS_URL);
        insta::assert_snapshot!(backend_code);
    }
}
