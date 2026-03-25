use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::path::PathBuf;
use std::usize;

use indexmap::IndexMap;
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
    Inject(SymbolRef),

    /// A model, or plain old object, containing the name of the class.
    Object(SymbolRef),

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial(SymbolRef),

    /// A data source of some model
    DataSource(SymbolRef),

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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FileSpan {
    pub start: usize,
    pub end: usize,
    pub file: PathBuf,
}

pub type SymbolRef = usize;

#[derive(Clone)]
pub enum WranglerEnvBindingKind {
    D1,
    KV,
    R2,
}

#[derive(Clone)]
pub enum SymbolKind {
    ModelDecl,
    ModelField,
    ModelPrimaryKeyTag,
    ModelForeignKeyTag,
    ModelNavigationTag,
    ModelD1Tag,
    ModelKvTag,
    ModelR2Tag,
    WranglerEnvDecl,
    WranglerEnvBinding { kind: WranglerEnvBindingKind },
    WranglerEnvVar,
    PlainOldObjectDecl,
    PlainOldObjectField,
    Null,
}

impl Default for SymbolKind {
    fn default() -> Self {
        SymbolKind::Null
    }
}

#[derive(Clone)]
pub struct Symbol {
    pub id: usize,

    /// Empty for symbols that are not named (e.g. declarations)
    pub name: String,

    /// Void for symbols that have no type (e.g. declarations)
    pub cidl_type: CidlType,

    /// Default for symbols that have no parent (e.g. declarations)
    pub parent: SymbolRef,

    pub span: FileSpan,
    pub kind: SymbolKind,
}

impl Default for Symbol {
    fn default() -> Self {
        Symbol {
            id: usize::MAX,
            name: String::new(),
            cidl_type: CidlType::Void,
            parent: usize::MAX,
            span: FileSpan::default(),
            kind: SymbolKind::Null,
        }
    }
}

pub struct ForeignKey {
    pub adj_model: SymbolRef,
    pub columns: Vec<SymbolRef>,
}

/// A D1 Navigation property, representing a relationship to another model
/// through a foreign key or composite foreign key.
pub enum NavigationPropertyKind {
    OneToOne {
        /// The columns on the current model that reference the other model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<SymbolRef>,
    },
    OneToMany {
        /// The columns on the other model that reference the current model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<SymbolRef>,
    },

    /// A many to many relationship expressed through a join table,
    /// consisting of the two models primary keys (be they composite or not).
    ManyToMany,
}

pub struct NavigationProperty {
    pub hash: u64,
    pub symbol: SymbolRef,

    pub field: SymbolRef,
    pub adj_model: SymbolRef,
    pub kind: NavigationPropertyKind,
}

pub struct KvProperty {
    pub symbol: SymbolRef,
    pub field: SymbolRef,
    pub env_binding: SymbolRef,
    pub format: String,
}

pub struct R2Property {
    pub symbol: SymbolRef,
    pub field: SymbolRef,
    pub env_binding: SymbolRef,
    pub format: String,
}

impl NavigationProperty {
    // pub fn many_to_many_table_name(&self, parent_model: &Symbol) -> String {
    //     let mut names = [&parent_model.name, &self.adj_model.name];
    //     names.sort();
    //     format!("{}{}", names[0], names[1])
    // }
}

#[derive(Default)]
pub struct Model {
    pub hash: u64,
    pub symbol: SymbolRef,

    pub d1_binding: Option<SymbolRef>,
    pub columns: HashSet<SymbolRef>,
    pub primary_key_columns: HashSet<SymbolRef>,
    pub foreign_keys: Vec<ForeignKey>,
    pub navigation_properties: Vec<NavigationProperty>,

    pub key_fields: HashSet<SymbolRef>,
    pub kv_properties: Vec<KvProperty>,
    pub r2_properties: Vec<R2Property>,
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
pub struct SymbolTable {
    table: HashMap<SymbolRef, Symbol>,
}

impl SymbolTable {
    pub fn insert(&mut self, symbol: Symbol) -> Option<Symbol> {
        self.table.insert(symbol.id, symbol)
    }

    pub fn lookup(&self, id: usize) -> Option<&Symbol> {
        self.table.get(&id)
    }

    pub fn name(&self, id: SymbolRef) -> &str {
        self.lookup(id).map(|s| s.name.as_str()).unwrap_or("")
    }

    pub fn kind(&self, id: SymbolRef) -> Option<&SymbolKind> {
        self.lookup(id).map(|s| &s.kind)
    }
}

#[derive(Default)]
pub struct CloesceAst {
    pub wrangler_env: Option<WranglerEnv>,
    pub models: IndexMap<SymbolRef, Model>,
    pub table: SymbolTable,
}

pub struct WranglerEnv {
    pub symbol: SymbolRef,
    pub d1_bindings: HashSet<SymbolRef>,
    pub kv_bindings: HashSet<SymbolRef>,
    pub r2_bindings: HashSet<SymbolRef>,
    pub vars: HashSet<SymbolRef>,
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
