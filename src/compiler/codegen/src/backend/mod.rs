use askama::Template;
use idl::{
    CidlType, CloesceIdl, DurableBinding, IncludeTree, Model, ValidatedField, model_bindings,
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

    fn backing_binding(&self, model: &Model<'_>) -> String {
        model
            .backing
            .as_ref()
            .map(|b| b.binding.to_string())
            .unwrap_or_default()
    }

    fn env_durable_target_key(&self) -> &'static str {
        idl::ENV_DURABLE_TARGET_KEY
    }

    fn interpolate_key_format(&self, format: &str, params: &[ValidatedField<'_>]) -> String {
        let names = params.iter().map(|p| p.name.as_ref());
        self.mapper.interpolate_format(format, names)
    }

    fn key_prefix(&self, prefix: &str) -> String {
        self.mapper.interpolate_format(prefix, std::iter::empty())
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
