use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum CidlType {
    Void,
    Integer,
    Double,
    String,
    Blob,
    Boolean,

    /// An ISO Date string
    DateIso,

    /// A Binary Large Object that is not to be buffered in memory,
    /// but rather piped to some destination.
    Stream,

    /// Any valid JSON value
    Json,

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

    /// A paginated response containing list metadata and a page of results.
    Paginated(Box<CidlType>),

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
            CidlType::Paginated(inner) => inner.root_type(),
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

    pub fn paginated(cidl_type: CidlType) -> CidlType {
        CidlType::Paginated(Box::new(cidl_type))
    }
}

impl Default for CidlType {
    fn default() -> Self {
        CidlType::Void
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileSpan {
    pub start: usize,
    pub end: usize,
    pub file: PathBuf,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Symbol {
    pub id: usize,
    pub name: String,
    pub span: FileSpan,
    pub cidl_type: CidlType,
}

impl Hash for Symbol {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

pub struct ForeignKey {
    pub adj_model: Symbol,
    pub columns: Vec<Symbol>,
}

pub struct Model {
    pub hash: u64,
    pub symbol: Symbol,

    pub d1_binding: Option<Symbol>,
    pub columns: HashSet<Symbol>,
    pub primary_key_columns: HashSet<Symbol>,
    pub foreign_keys: Vec<ForeignKey>,
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Default)]
pub struct CloesceAst {
    pub models: HashMap<Symbol, Model>,
}

pub struct WranglerEnv {
    pub symbol: Symbol,
    pub d1_bindings: HashSet<Symbol>,
    pub kv_bindings: HashSet<Symbol>,
    pub r2_bindings: HashSet<Symbol>,
    pub vars: HashSet<Symbol>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
    pub migrations_dir: Option<String>,
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
