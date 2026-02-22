pub mod err;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use err::GeneratorErrorKind;
use err::Result;
use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

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

    /// A Cloudflare R2 object (HEAD object response)
    R2Object,

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

    /// A Cloudflare Workers KV object (GET value response)
    KvObject(Box<CidlType>),
}

impl CidlType {
    pub fn root_type(&self) -> &CidlType {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.root_type(),
            CidlType::Nullable(inner) => inner.root_type(),
            CidlType::KvObject(inner) => inner.root_type(),
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
    /// Symbol name of the value.
    pub name: String,

    /// Cloesce type associated with the value.
    pub cidl_type: CidlType,
}

/// The expected media type for request/response bodies.
/// An API endpoint may expect data in some format, and return data in some format.
/// Defaults to JSON.
#[derive(Serialize, Deserialize, Debug, Default)]
pub enum MediaType {
    #[default]
    Json,

    Octet,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiMethod {
    /// Symbol name of the method.
    pub name: String,

    /// If true, the method is static (instantiated on a class, not an instance).
    /// Static methods require no hydration or data source.
    pub is_static: bool,
    pub data_source: Option<String>,

    pub http_verb: HttpVerb,

    /// The media format the client should use to read the response body.
    #[serde(default)]
    pub return_media: MediaType,
    pub return_type: CidlType,

    /// The media format the client should use to send the request body.
    #[serde(default)]
    pub parameters_media: MediaType,
    pub parameters: Vec<NamedTypedValue>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

/// A tree of model symbol names to include when hydrating a data source.
#[derive(Serialize, Deserialize, Debug)]
pub struct DataSource {
    /// The symbol name of the data source, e.g., "withUserDetails"
    pub name: String,
    pub tree: IncludeTree,

    /// If true, the data source will not be generated on the client.
    pub is_private: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum NavigationPropertyKind {
    OneToOne { column_reference: String },
    OneToMany { column_reference: String },
    ManyToMany,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NavigationProperty {
    #[serde(default)]
    pub hash: u64,

    /// Symbol name of the navigation property.
    pub var_name: String,

    /// Referenced model name.
    pub model_reference: String,

    pub kind: NavigationPropertyKind,
}

impl NavigationProperty {
    pub fn many_to_many_table_name(&self, parent_model_name: &str) -> String {
        let mut names = [parent_model_name, &self.model_reference];
        names.sort();
        format!("{}{}", names[0], names[1])
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct D1Column {
    #[serde(default)]
    pub hash: u64,

    /// Symbol name and Cloesce type of the attribute.
    /// Represents both the column name and type.
    pub value: NamedTypedValue,

    /// If the attribute is a foreign key, the referenced model name.
    /// Otherwise, None.
    pub foreign_key_reference: Option<String>,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KeyValue {
    pub format: String,
    pub namespace_binding: String,
    pub value: NamedTypedValue,

    /// If true, treat the key as a prefix for listing multiple keys.
    pub list_prefix: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct R2Object {
    pub format: String,
    pub var_name: String,
    pub bucket_binding: String,

    /// If true, treat the key as a prefix for listing multiple keys.
    pub list_prefix: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Model {
    #[serde(default)]
    pub hash: u64,

    /// The symbol that defines the model in the source code.
    pub name: String,

    /// Primary key column of the model.
    // TODO: Composite primary keys
    pub primary_key: Option<NamedTypedValue>,
    pub columns: Vec<D1Column>,
    pub navigation_properties: Vec<NavigationProperty>,

    pub key_params: Vec<String>,
    pub kv_objects: Vec<KeyValue>,
    pub r2_objects: Vec<R2Object>,

    /// API definitions.
    pub methods: BTreeMap<String, ApiMethod>,

    pub data_sources: BTreeMap<String, DataSource>,

    pub cruds: Vec<CrudKind>,
    pub source_path: PathBuf,
}

impl Model {
    pub fn has_d1(&self) -> bool {
        !self.columns.is_empty() || self.primary_key.is_some()
    }

    pub fn has_kv(&self) -> bool {
        !self.kv_objects.is_empty()
    }

    pub fn has_r2(&self) -> bool {
        !self.r2_objects.is_empty()
    }

    /// Returns the data source with the symbol name "default", if it exists.
    pub fn default_data_source(&self) -> Option<&DataSource> {
        self.data_sources.get("default")
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServiceAttribute {
    /// Symbol name of the class field.
    pub var_name: String,

    /// Symbol of the injected class.
    pub inject_reference: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    /// The symbol that defines the service in the source code.
    pub name: String,

    /// Class fields which are all injected dependencies.
    pub attributes: Vec<ServiceAttribute>,

    /// Injected symbols required to initialize the service.
    pub initializer: Option<Vec<String>>,

    /// API definitions.
    pub methods: BTreeMap<String, ApiMethod>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlainOldObject {
    /// The symbol that defines the POO in the source code.
    pub name: String,

    /// Class fields of any serializable type.
    pub attributes: Vec<NamedTypedValue>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub struct WranglerEnv {
    /// Class name of the Wrangler environment.
    pub name: String,
    pub source_path: PathBuf,

    // TODO: Many database bindings
    pub d1_binding: Option<String>,

    pub kv_bindings: Vec<String>,
    pub r2_bindings: Vec<String>,
    pub vars: HashMap<String, CidlType>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct CloesceAst {
    #[serde(default)]
    pub hash: u64,

    pub project_name: String,
    pub wrangler_env: Option<WranglerEnv>,

    pub models: IndexMap<String, Model>,
    pub services: IndexMap<String, Service>,
    pub poos: BTreeMap<String, PlainOldObject>,

    pub main_source: Option<PathBuf>,
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
        let Self { hash, models, .. } = self;

        let migrations_models: IndexMap<String, MigrationsModel> = models
            .into_iter()
            .filter_map(|(name, model)| {
                let Some(pk) = model.primary_key else {
                    // Skip non-D1 models
                    return None;
                };

                let m = MigrationsModel {
                    hash: model.hash,
                    name: model.name,
                    primary_key: pk,
                    columns: model.columns,
                    navigation_properties: model.navigation_properties,
                };
                Some((name, m))
            })
            .collect();

        let migrations_ast = MigrationsAst {
            hash,
            models: migrations_models,
        };

        serde_json::to_string_pretty(&migrations_ast).expect("serialize migrations ast to work")
    }

    /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    pub fn set_merkle_hash(&mut self) {
        if self.hash != 0u64 {
            // If the root is hashed, it's safe to assume all children are hashed.
            // No work to be done.
            return;
        }

        let mut root_h = FxHasher::default();
        for model in self.models.values_mut() {
            let mut model_h = FxHasher::default();
            model_h.write(b"Model");
            model.primary_key.hash(&mut model_h);
            model.name.hash(&mut model_h);

            for col in model.columns.iter_mut() {
                let col_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelColumn");
                    col.value.hash(&mut h);
                    col.foreign_key_reference.hash(&mut h);
                    h.finish()
                };

                col.hash = col_h;
                model_h.write_u64(col_h);
            }

            for nav in model.navigation_properties.iter_mut() {
                let nav_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelNavigationProperty");
                    nav.model_reference.hash(&mut h);
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
    pub columns: Vec<D1Column>,
    pub navigation_properties: Vec<NavigationProperty>,
}

/// A subset of [CloesceAst] suited for D1 migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsAst {
    pub hash: u64,

    #[serde(deserialize_with = "skip_if_null_primary_key")]
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct KVNamespace {
    pub binding: Option<String>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct R2Bucket {
    pub binding: Option<String>,
    pub bucket_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WranglerSpec {
    pub name: Option<String>,
    pub compatibility_date: Option<String>,
    pub main: Option<String>,

    #[serde(default)]
    pub d1_databases: Vec<D1Database>,

    #[serde(default)]
    pub kv_namespaces: Vec<KVNamespace>,

    #[serde(default)]
    pub r2_buckets: Vec<R2Bucket>,

    #[serde(default)]
    pub vars: HashMap<String, Value>,
}

fn skip_if_null_primary_key<'de, D>(
    deserializer: D,
) -> std::result::Result<IndexMap<String, MigrationsModel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Temp {
        hash: u64,
        name: String,
        primary_key: Option<NamedTypedValue>,
        columns: Vec<D1Column>,
        navigation_properties: Vec<NavigationProperty>,
    }

    let temps: IndexMap<String, Temp> = Deserialize::deserialize(deserializer)?;

    Ok(temps
        .into_iter()
        .filter_map(|(key, t)| {
            t.primary_key.map(|pk| {
                (
                    key,
                    MigrationsModel {
                        hash: t.hash,
                        name: t.name,
                        primary_key: pk,
                        columns: t.columns,
                        navigation_properties: t.navigation_properties,
                    },
                )
            })
        })
        .collect())
}
