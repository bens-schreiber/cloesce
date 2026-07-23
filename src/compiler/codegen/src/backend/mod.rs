use askama::Template;
use idl::{
    ApiMethod, CidlType, CloesceIdl, DEFAULT_DATA_SOURCE_NAME, DataSource, DurableBinding, Model,
    ValidatedField,
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
        self.mapper.cidl_type(ty)
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

    /// The env-store key for a model or source name (camelCase: `Parent` -> `parent`).
    fn store_key(&self, name: &str) -> String {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }

    /// Capitalize the first character (`create` -> `Create`), for building type names.
    fn cap_first(&self, name: &str) -> String {
        let mut chars = name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        }
    }

    /// The `Env.<Route>` interface name for a route: `<Model><Method>`.
    fn route_env_name(&self, model: &Model<'src>, api: &ApiMethod<'src>) -> String {
        format!("{}{}", model.name, self.cap_first(&api.name))
    }

    /// The store-handle type prefix for a binding, keyed by its kind.
    fn binding_prefix(&self, name: &str) -> &'static str {
        if self.idl.wrangler_env.d1_bindings.contains(&name) {
            return "Db";
        }

        if self
            .idl
            .wrangler_env
            .kv_bindings
            .iter()
            .any(|b| b.name == name)
        {
            return "Kv";
        }

        if self
            .idl
            .wrangler_env
            .r2_bindings
            .iter()
            .any(|b| b.name == name)
        {
            return "R2";
        }

        "Do"
    }

    /// True if any model is backed by this binding.
    fn binding_hosts_models(&self, name: &str) -> bool {
        !self.binding_models(name).is_empty()
    }

    /// Models hosted by (backed by) the given binding, in schema order.
    fn binding_models(&self, binding: &str) -> Vec<&Model<'src>> {
        self.idl
            .models
            .values()
            .filter(|m| m.backing.as_ref().map(|b| b.binding) == Some(binding))
            .collect()
    }

    /// User-authored routes (excludes generated `$crud` methods).
    fn user_routes<'a>(&self, model: &'a Model<'src>) -> Vec<&'a ApiMethod<'src>> {
        model
            .apis
            .iter()
            .filter(|a| !a.name.starts_with('$'))
            .collect()
    }

    /// User routes that appear as their own member on the model store.
    ///
    /// - Excludes a route whose name collides with a data source's store key (e.g. a
    ///   `withoutB` route backed by a `WithoutB` source).
    /// - The source (or verb) keeps the store slot; the route stays reachable over HTTP.
    fn store_route_apis<'a>(&self, model: &'a Model<'src>) -> Vec<&'a ApiMethod<'src>> {
        let reserved: Vec<String> = model
            .data_sources
            .values()
            .filter(|ds| ds.name != DEFAULT_DATA_SOURCE_NAME)
            .map(|ds| self.store_key(ds.name))
            .collect();

        self.user_routes(model)
            .into_iter()
            .filter(|api| !reserved.contains(&api.name.to_string()))
            .collect()
    }

    /// Data sources with stubbed verbs
    fn stub_sources<'a>(&self, model: &'a Model<'src>) -> Vec<&'a DataSource<'src>> {
        model
            .data_sources
            .values()
            .filter(|ds| ds.has_stubs())
            .collect()
    }

    /// Members of a model's `Api.Of`
    fn of_members(&self, model: &Model<'src>) -> Vec<String> {
        let mut members: Vec<String> = self
            .user_routes(model)
            .iter()
            .map(|a| a.name.to_string())
            .collect();
        for ds in self.stub_sources(model) {
            members.push(ds.name.to_string());
        }
        members
    }

    /// The bindings a route's narrowed `env` carries:
    /// - `[inject ...]` bindings
    /// - The Durable Object it targets
    ///
    /// Deduped in decl order.
    fn route_env_bindings(&self, api: &ApiMethod<'src>) -> Vec<String> {
        let mut out: Vec<String> = api.injected.iter().map(|s| s.to_string()).collect();
        if let Some(dt) = &api.durable_target
            && !out.iter().any(|b| b == dt.binding)
        {
            out.push(dt.binding.to_string());
        }
        out
    }

    /// True if the route receives an `env` parameter (it injects or runs in a DO).
    fn route_has_env(&self, api: &ApiMethod<'src>) -> bool {
        !api.injected.is_empty() || api.durable_target.is_some()
    }

    /// The name used to reference an injectable's env/handle type.
    fn is_injectable(&self, name: &str) -> bool {
        self.idl.injects.contains(&name)
    }

    /// Every injectable name a model's routes or stubbed data-source verbs require.
    fn model_injectables(&self, model: &Model<'src>) -> Vec<String> {
        let route_injects = model.apis.iter().flat_map(|api| api.injected.iter());
        let source_injects = model.data_sources.values().flat_map(|ds| {
            ds.get
                .injected
                .iter()
                .chain(ds.list.injected.iter())
                .chain(ds.save.injected.iter())
        });

        let mut out: Vec<String> = Vec::new();
        for inj in route_injects.chain(source_injects) {
            if self.is_injectable(inj) && !out.iter().any(|x| x == inj) {
                out.push(inj.to_string());
            }
        }
        out
    }

    /// All injectable names referenced by any route or stubbed data-source verb in the schema.
    fn injectables_used(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for model in self.idl.models.values() {
            for inj in self.model_injectables(model) {
                if !out.iter().any(|x| x == &inj) {
                    out.push(inj);
                }
            }
        }
        out
    }

    /// The set of deployable hosts:
    /// - the Worker (owns everything)
    /// - One host per Durable Object binding (owns a subset)
    fn hosts(&self) -> Vec<HostInfo> {
        let mut hosts = Vec::new();

        let worker_models: Vec<String> = self
            .idl
            .models
            .values()
            .map(|m| m.name.to_string())
            .collect();
        hosts.push(HostInfo {
            name: "Worker".to_string(),
            models: worker_models,
            injectables: self.injectables_used(),
        });

        for binding in &self.idl.wrangler_env.durable_bindings {
            let models: Vec<String> = self
                .binding_models(binding.name)
                .iter()
                .map(|m| m.name.to_string())
                .collect();
            let mut injectables: Vec<String> = Vec::new();
            for model in self.binding_models(binding.name) {
                for inj in self.model_injectables(model) {
                    if !injectables.iter().any(|x| x == &inj) {
                        injectables.push(inj);
                    }
                }
            }
            hosts.push(HostInfo {
                name: format!("{}Host", binding.name),
                models,
                injectables,
            });
        }

        hosts
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

/// A deployable host (a Worker or one Durable Object) and everything
/// it needs to run.
struct HostInfo {
    name: String,
    models: Vec<String>,
    injectables: Vec<String>,
}

impl HostInfo {
    fn owed(&self) -> Vec<String> {
        self.models
            .iter()
            .chain(self.injectables.iter())
            .cloned()
            .collect()
    }
}
