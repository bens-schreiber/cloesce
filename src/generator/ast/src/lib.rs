pub mod builder;
pub mod err;
pub mod semantic;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use err::GeneratorErrorKind;
use err::Result;
use indexmap::IndexMap;
use rustc_hash::FxHasher;
use semantic::SemanticAnalysis;
use serde::Deserialize;
use serde::Serialize;
use serde_with::{MapPreventDuplicates, serde_as};

#[macro_export]
macro_rules! cidl_type_contains {
    ($value:expr, $pattern:pat) => {{
        let mut cur = $value;

        loop {
            match cur {
                $pattern => break true,

                CidlType::Array(inner)
                | CidlType::Nullable(inner)
                | CidlType::HttpResult(inner) => {
                    cur = inner;
                }

                _ => break false,
            }
        }
    }};
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CidlType {
    Void,

    /// SQLite integer
    Integer,

    /// SQLite floating point number
    Real,

    /// SQLite string
    Text,

    /// SQLite Binary Large Object
    Blob,

    /// (SQL equivalent to Integer)
    Boolean,

    /// An ISO Date string (SQL equivalent to Text)
    DateIso,

    /// A Binary Large Object that is not to be buffered in memory,
    /// but rather piped to some destination.
    Stream,

    /// Any valid JSON value
    JsonValue,

    /// A dependency injected instance, containing a type name.
    Inject(String),

    /// A model, or plain old object, containing the name of the class.
    Object(String),

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial(String),

    /// A data source of some model
    DataSource(String),

    /// An array of any type
    Array(Box<CidlType>),

    /// A REST API response, which can contain any type or nothing.
    HttpResult(Box<CidlType>),

    /// A wrapper denoting the type within can be null.
    /// If the inner value is void, represents just null.
    Nullable(Box<CidlType>),
}

impl CidlType {
    pub fn root_type(&self) -> &CidlType {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.root_type(),
            CidlType::Nullable(inner) => inner.root_type(),
            t => t,
        }
    }

    pub fn is_nullable(&self) -> bool {
        matches!(self, CidlType::Nullable(_))
    }

    pub fn array(cidl_type: CidlType) -> CidlType {
        CidlType::Array(Box::new(cidl_type))
    }

    pub fn nullable(cidl_type: CidlType) -> CidlType {
        CidlType::Nullable(Box::new(cidl_type))
    }

    pub fn null() -> CidlType {
        CidlType::Nullable(Box::new(CidlType::Void))
    }

    pub fn http(cidl_type: CidlType) -> CidlType {
        CidlType::HttpResult(Box::new(cidl_type))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpVerb {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct NamedTypedValue {
    pub name: String,
    pub cidl_type: CidlType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct D1ModelAttribute {
    #[serde(default)]
    pub hash: u64,

    pub value: NamedTypedValue,
    pub foreign_key_reference: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub enum MediaType {
    #[default]
    Json,

    Octet,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiMethod {
    pub name: String,
    pub is_static: bool,
    pub http_verb: HttpVerb,

    #[serde(default)]
    pub return_media: MediaType,
    pub return_type: CidlType,

    #[serde(default)]
    pub parameters_media: MediaType,
    pub parameters: Vec<NamedTypedValue>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

#[derive(Serialize, Deserialize, Debug)]
pub struct DataSource {
    #[serde(default)]
    pub hash: u64,

    pub name: String,
    pub tree: IncludeTree,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum NavigationPropertyKind {
    OneToOne { reference: String },
    OneToMany { reference: String },
    ManyToMany { unique_id: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NavigationProperty {
    #[serde(default)]
    pub hash: u64,

    pub var_name: String,
    pub model_name: String,
    pub kind: NavigationPropertyKind,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct D1Model {
    #[serde(default)]
    pub hash: u64,

    pub name: String,
    pub primary_key: NamedTypedValue,
    pub attributes: Vec<D1ModelAttribute>,
    pub navigation_properties: Vec<NavigationProperty>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub methods: BTreeMap<String, ApiMethod>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub data_sources: BTreeMap<String, DataSource>,

    pub cruds: Vec<CrudKind>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServiceAttribute {
    pub var_name: String,
    pub injected: String,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    pub name: String,
    pub attributes: Vec<ServiceAttribute>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub methods: BTreeMap<String, ApiMethod>,

    pub source_path: PathBuf,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct KVModel {
    pub name: String,
    pub namespace: String,
    pub cidl_type: CidlType,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub methods: BTreeMap<String, ApiMethod>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlainOldObject {
    pub name: String,
    pub attributes: Vec<NamedTypedValue>,
    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub enum InputLanguage {
    TypeScript,
}

#[derive(Serialize, Deserialize)]
pub struct WranglerEnv {
    pub name: String,
    pub source_path: PathBuf,

    // TODO: Many database bindings?
    pub db_binding: Option<String>,
    pub kv_bindings: Vec<String>,

    pub vars: HashMap<String, CidlType>,
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct CloesceAst {
    #[serde(default)]
    pub hash: u64,

    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub wrangler_env: Option<WranglerEnv>,

    // TODO: MapPreventDuplicates is not supported for IndexMap
    pub d1_models: IndexMap<String, D1Model>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub kv_models: HashMap<String, KVModel>,

    // TODO: MapPreventDuplicates is not supported for IndexMap
    pub poos: IndexMap<String, PlainOldObject>,

    // TODO: MapPreventDuplicates is not supported for IndexMap
    pub services: IndexMap<String, Service>,

    pub app_source: Option<PathBuf>,
}

impl CloesceAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self> {
        let cidl_contents = std::fs::read_to_string(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        serde_json::from_str::<Self>(&cidl_contents).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }

    pub fn to_migrations_json(self) -> String {
        let Self {
            hash,
            d1_models: models,
            ..
        } = self;

        let migrations_models: IndexMap<String, MigrationsModel> = models
            .into_iter()
            .map(|(name, model)| {
                let m = MigrationsModel {
                    hash: model.hash,
                    name: model.name,
                    primary_key: model.primary_key,
                    attributes: model.attributes,
                    navigation_properties: model.navigation_properties,
                    data_sources: model.data_sources,
                };
                (name, m)
            })
            .collect();

        let migrations_ast = MigrationsAst {
            hash,
            models: migrations_models,
        };

        serde_json::to_string_pretty(&migrations_ast).expect("serialize migrations ast to work")
    }

    pub fn semantic_analysis(&mut self) -> Result<()> {
        SemanticAnalysis::analyze(self)
    }

    /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    pub fn set_merkle_hash(&mut self) {
        if self.hash != 0u64 {
            // If the root is hashed, it's safe to assume all children are hashed.
            // No work to be done.
            return;
        }

        let mut root_h = FxHasher::default();
        for model in self.d1_models.values_mut() {
            let mut model_h = FxHasher::default();
            model_h.write(b"Model");
            model.primary_key.hash(&mut model_h);
            model.name.hash(&mut model_h);

            for attr in model.attributes.iter_mut() {
                let attr_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelAttribute");
                    attr.value.hash(&mut h);
                    attr.foreign_key_reference.hash(&mut h);
                    h.finish()
                };

                attr.hash = attr_h;
                model_h.write_u64(attr_h);
            }

            for ds in model.data_sources.values_mut() {
                let ds_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelDataSource");
                    ds.name.hash(&mut h);

                    fn dfs(node: &IncludeTree, h: &mut FxHasher) {
                        for (k, v) in &node.0 {
                            dfs(v, h);
                            k.hash(h);
                        }
                    }

                    dfs(&ds.tree, &mut h);
                    h.finish()
                };

                ds.hash = ds_h;
                model_h.write_u64(ds_h);
            }

            for nav in model.navigation_properties.iter_mut() {
                let nav_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelNavigationProperty");
                    nav.model_name.hash(&mut h);
                    nav.var_name.hash(&mut h);
                    nav.kind.hash(&mut h);
                    h.finish()
                };

                nav.hash = nav_h;
                model_h.write_u64(nav_h);
            }

            let model_h_finished = model_h.finish();
            model.hash = model_h_finished;
            root_h.write_u64(model_h_finished);
        }

        self.hash = root_h.finish();
    }
}

/// A subset of [Model] suited for migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsModel {
    pub hash: u64,
    pub name: String,
    pub primary_key: NamedTypedValue,
    pub attributes: Vec<D1ModelAttribute>,
    pub navigation_properties: Vec<NavigationProperty>,
    pub data_sources: BTreeMap<String, DataSource>,
}

/// A subset of [CloesceAst] suited for D1 migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsAst {
    pub hash: u64,
    pub models: IndexMap<String, MigrationsModel>,
}

impl MigrationsAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        serde_json::from_str::<Self>(&contents).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }
}
