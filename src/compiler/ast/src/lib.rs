pub mod err;

use std::collections::HashMap;
use std::hash::Hash;

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

// // https://docs.rs/chumsky/latest/chumsky/span/struct.SimpleSpan.html
// #[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
// pub struct FileSpan {
//     pub start: usize,
//     pub end: usize,
// }

// #[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
// pub struct SourceLocation {
//     pub file: PathBuf,
//     pub span: FileSpan,
// }

// #[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
// pub struct Symbol {
//     pub name: String,
//     pub location: SourceLocation,
//     pub ty: Option<CidlType>,
// }

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
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

// #[derive(Serialize, Deserialize, Debug)]
// pub struct Api {
//     pub symbol: Symbol,
//     pub model_symbol: Symbol,
//     pub cruds: Vec<CrudKind>,
//     pub methods: Vec<ApiMethod>,
// }

// #[derive(Serialize, Deserialize, Debug)]
// pub struct ApiMethod {
//     pub symbol: Symbol,

//     /// If true, the method is static (instantiated on a class, not an instance).
//     /// Static methods require no hydration or data source.
//     pub is_static: bool,
//     pub data_source: Option<Symbol>,

//     pub http_verb: HttpVerb,

//     /// The media format the client should use to read the response body.
//     #[serde(default)]
//     pub return_media: MediaType,
//     pub return_type: CidlType,

//     /// The media format the client should use to send the request body.
//     #[serde(default)]
//     pub parameters_media: MediaType,
//     pub parameters: Vec<Symbol>,
// }

// #[derive(Serialize, Deserialize, Debug, Default)]
// pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

// #[derive(Serialize, Deserialize, Debug)]
// pub struct DataSourceMethod {
//     pub parameters: Vec<Symbol>,
//     pub raw_sql: String,
// }

// /// A tree of model symbol names to include when hydrating a data source.
// #[derive(Serialize, Deserialize, Debug)]
// pub struct DataSource {
//     pub symbol: Symbol,
//     pub model_symbol: Symbol,
//     pub tree: IncludeTree,

//     /// If true, the data source will not be generated on the client.
//     pub is_private: bool,

//     pub list: Option<DataSourceMethod>,
//     pub get: Option<DataSourceMethod>,
// }

// /// A D1 Navigation property, representing a relationship to another model
// /// through a foreign key or composite foreign key.
// #[derive(Serialize, Deserialize, Debug, Clone, Hash)]
// pub enum D1NavigationPropertyKind {
//     OneToOne {
//         /// The columns on the current model that reference the other model's primary key.
//         /// Multiple columns indicate a composite foreign key.
//         columns: Vec<Symbol>,
//     },
//     OneToMany {
//         /// The columns on the other model that reference the current model's primary key.
//         /// Multiple columns indicate a composite foreign key.
//         columns: Vec<Symbol>,
//     },

//     /// A many to many relationship expressed through a join table,
//     /// consisting of the two models primary keys (be they composite or not).
//     ManyToMany { column: Symbol },
// }

// #[derive(Serialize, Deserialize, Debug)]
// pub struct D1NavigationProperty {
//     pub hash: u64,

//     /// The field on the current model that represents the relationship
//     pub field: Symbol,

//     /// The model that this this navigation property points to
//     pub adj_model: Symbol,

//     /// The kind of navigation property, which encodes the relationship and foreign key structure.
//     pub kind: D1NavigationPropertyKind,
// }

// #[derive(Serialize, Deserialize, Debug, Hash)]
// pub struct ForeignKey {
//     pub hash: u64,
//     pub adj_model: Symbol,
//     pub columns: Vec<Symbol>,
// }

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

// #[derive(Serialize, Deserialize, Debug)]
// pub struct KvR2Property {
//     pub symbol: Symbol,

//     /// Key format e.g. "users/{id}/profile.jpg"
//     pub format: String,

//     /// The symbol of the environment variable binding the KV namespace
//     pub env_binding: Symbol,

//     /// If true, treat the key as a prefix for listing multiple keys.
//     pub list_prefix: bool,
// }

// #[derive(Serialize, Deserialize, Debug)]
// pub struct Model {
//     pub hash: u64,
//     pub symbol: Symbol,
//     pub d1_binding: Option<Symbol>,
//     pub columns: Vec<Symbol>,
//     pub primary_key_columns: Vec<Symbol>,
//     pub navigation_properties: Vec<D1NavigationProperty>,
//     pub foreign_keys: Vec<ForeignKey>,
//     pub key_params: Vec<Symbol>,
//     pub kv_properties: Vec<KvR2Property>,
//     pub r2_properties: Vec<KvR2Property>,

//     /// Each inner Vec represents a unique constraint, containing the column names that make up the constraint.
//     pub unique_constraints: Vec<Vec<Symbol>>,
// }

// impl Model {
//     pub fn has_d1(&self) -> bool {
//         self.d1_binding.is_some()
//     }

//     pub fn has_kv(&self) -> bool {
//         !self.kv_properties.is_empty()
//     }

//     pub fn has_r2(&self) -> bool {
//         !self.r2_properties.is_empty()
//     }

//     pub fn has_composite_pk(&self) -> bool {
//         self.primary_key_columns.len() > 1
//     }
// }

// #[derive(Serialize, Deserialize, Debug)]
// pub struct Service {
//     pub symbol: Symbol,

//     /// Class fields which are all injected dependencies.
//     pub attributes: Vec<Symbol>,

//     /// Injected symbols required to initialize the service.
//     pub initializer: Option<Vec<String>>,

//     /// API definitions.
//     pub methods: Vec<ApiMethod>,

//     pub source_path: PathBuf,
// }

// #[derive(Serialize, Deserialize, Debug)]
// pub struct PlainOldObject {
//     /// The symbol that defines the POO in the source code.
//     pub symbol: Symbol,

//     pub fields: Vec<Symbol>,
// }

// #[derive(Serialize, Deserialize)]
// pub struct WranglerEnv {
//     pub symbol: Symbol,
//     pub d1_bindings: Vec<Symbol>,
//     pub kv_bindings: Vec<Symbol>,
//     pub r2_bindings: Vec<Symbol>,
//     pub vars: Vec<(Symbol, String)>,
// }

// #[derive(Serialize, Deserialize, Default)]
// pub struct CloesceAst {
//     pub hash: u64,
//     pub project_name: String,
//     pub wrangler_env: Vec<WranglerEnv>,

//     pub models: Vec<Model>,
//     pub apis: Vec<Api>,
//     pub sources: Vec<DataSource>,
//     pub services: Vec<Service>,
//     pub poos: Vec<PlainOldObject>,
//     pub injectables: Vec<Symbol>,
// }

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
