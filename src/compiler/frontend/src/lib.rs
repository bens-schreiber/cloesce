use std::path::PathBuf;

use ast::{CidlType, CrudKind, HttpVerb, IncludeTree};
use chumsky::span::SimpleSpan;

pub mod lexer;
pub mod parser;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct FileSpan {
    pub start: usize,
    pub end: usize,
    pub file: PathBuf,
}

impl FileSpan {
    pub fn from_simple_span(span: SimpleSpan) -> Self {
        FileSpan {
            start: span.start,
            end: span.end,
            file: PathBuf::default(), // TODO: track files
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WranglerEnvBindingKind {
    D1,
    R2,
    Kv,
}

#[derive(Clone, Default, Debug)]
pub enum SymbolKind {
    ModelDecl,
    ModelField,

    WranglerEnvDecl,
    WranglerEnvBinding {
        kind: WranglerEnvBindingKind,
    },
    WranglerEnvVar,

    PlainOldObjectDecl,
    PlainOldObjectField,

    ApiDecl,
    ApiMethodDecl,
    ApiMethodParam,

    DataSourceDecl,
    DataSourceMethodDecl,
    DataSourceMethodParam,

    ServiceDecl,
    ServiceField,

    InjectDecl,

    #[default]
    Null,
}

#[derive(Clone, Debug, Default)]
pub struct Symbol {
    /// [String::default()] for symbols with no name
    pub name: String,

    /// [CidlType::default()] for symbols with no type
    pub cidl_type: CidlType,

    /// [String::default()] for symbols with no parent
    pub parent_name: String,

    pub span: FileSpan,
    pub kind: SymbolKind,
}

pub struct ApiBlock {
    pub symbol: Symbol,

    pub namespace: String,
    pub methods: Vec<ApiBlockMethod>,
}

pub struct ApiBlockMethod {
    pub symbol: Symbol,

    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub data_source: Option<String>,
    pub return_type: CidlType,
    pub parameters: Vec<Symbol>,
}

pub struct DataSourceBlockMethod {
    pub span: SimpleSpan,
    pub parameters: Vec<Symbol>,
    pub raw_sql: String,
}

pub struct DataSourceBlock {
    pub symbol: Symbol,

    pub model: String,
    pub tree: IncludeTree,
    pub list: Option<DataSourceBlockMethod>,
    pub get: Option<DataSourceBlockMethod>,
}

pub struct NavigationTag {
    pub span: SimpleSpan,

    /// The field on the current model that represents the relationship
    pub field: String,

    /// All columns involved in the relationship
    /// (model, field)
    pub fields: Vec<(String, String)>,
    pub is_many_to_many: bool,
}

pub struct ForeignKeyTag {
    pub span: SimpleSpan,

    pub adj_model: String,

    /// (this model field, adjacent model field)
    pub references: Vec<(String, String)>,
}

pub struct KvR2Tag {
    pub span: SimpleSpan,

    pub field: String,

    /// Key format e.g. "users/{id}/profile.jpg"
    pub format: String,

    /// The symbol of the environment variable binding the KV namespace
    pub env_binding: String,
}

pub struct UniqueTag {
    pub span: SimpleSpan,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct D1Tag {
    pub span: SimpleSpan,

    /// The symbol of the environment variable binding the D1 database
    pub env_binding: String,
}

pub struct KeyFieldTag {
    pub span: SimpleSpan,
    pub field: String,
}

pub struct PrimaryKeyTag {
    pub span: SimpleSpan,
    pub field: String,
}

pub struct ModelBlock {
    pub symbol: Symbol,

    pub fields: Vec<Symbol>,

    pub primary_keys: Vec<PrimaryKeyTag>,
    pub d1_binding: Option<D1Tag>,
    pub key_fields: Vec<KeyFieldTag>,
    pub unique_constraints: Vec<UniqueTag>,
    pub kvs: Vec<KvR2Tag>,
    pub r2s: Vec<KvR2Tag>,

    pub navigation_properties: Vec<NavigationTag>,
    pub foreign_keys: Vec<ForeignKeyTag>,

    pub cruds: Vec<CrudKind>,
}

pub struct ServiceBlock {
    pub symbol: Symbol,
    pub fields: Vec<Symbol>,
}

pub struct PlainOldObjectBlock {
    pub symbol: Symbol,
    pub fields: Vec<Symbol>,
}

pub struct WranglerEnvBlock {
    pub symbol: Symbol,
    pub d1_bindings: Vec<Symbol>,
    pub kv_bindings: Vec<Symbol>,
    pub r2_bindings: Vec<Symbol>,
    pub vars: Vec<Symbol>,
}

pub struct InjectBlock {
    pub symbol: Symbol,
    pub fields: Vec<Symbol>,
}

/// An IR for the raw parsed structure of a Cloesce project
#[derive(Default)]
pub struct ParseAst {
    pub wrangler_envs: Vec<WranglerEnvBlock>,
    pub models: Vec<ModelBlock>,
    pub apis: Vec<ApiBlock>,
    pub sources: Vec<DataSourceBlock>,
    pub services: Vec<ServiceBlock>,
    pub poos: Vec<PlainOldObjectBlock>,
    pub injects: Vec<InjectBlock>,
}
