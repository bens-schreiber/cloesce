use std::collections::{BTreeSet, HashSet};

use askama::Template;
use idl::{CidlType, CloesceIdl, IncludeTree, Model, ValidatedField};

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

    fn is_env_injected(&self, name: &str) -> bool {
        !self.idl.injects.contains(&name)
    }

    fn interpolate_key_format(&self, format: &str, params: &[ValidatedField<'_>]) -> String {
        let names = params.iter().map(|p| p.name.as_ref());
        self.mapper.interpolate_format(format, names)
    }

    /// Wrangler bindings a model's method may touch.
    ///
    /// `include` filters which nav/kv/r2 fields are traversed,
    /// where [None] means "everything"
    fn model_bindings(
        &self,
        model: &Model<'src>,
        include: Option<&IncludeTree<'src>>,
    ) -> Vec<&'src str> {
        let mut visited = HashSet::new();
        let mut bindings = BTreeSet::new();
        self.collect_model_bindings(model, include, &mut visited, &mut bindings);
        bindings.into_iter().collect()
    }

    fn collect_model_bindings(
        &self,
        model: &Model<'src>,
        include: Option<&IncludeTree<'src>>,
        visited: &mut HashSet<&'src str>,
        bindings: &mut BTreeSet<&'src str>,
    ) {
        if !visited.insert(model.name) {
            return;
        }
        let included = |name: &str| include.is_none_or(|t| t.0.contains_key(name));

        if let Some(b) = model.backing_binding {
            bindings.insert(b);
        }
        for kv in &model.kv_fields {
            if included(kv.field.name.as_ref()) {
                bindings.insert(kv.binding);
            }
        }
        for r2 in &model.r2_fields {
            if included(r2.field.name.as_ref()) {
                bindings.insert(r2.binding);
            }
        }
        for nav in &model.navigation_fields {
            let subtree = match include {
                Some(t) => match t.0.get(nav.field.name.as_ref()) {
                    Some(sub) => Some(sub),
                    None => continue,
                },
                None => None,
            };
            if let Some(referenced) = self.idl.models.get(nav.model_reference) {
                self.collect_model_bindings(referenced, subtree, visited, bindings);
            }
        }
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
