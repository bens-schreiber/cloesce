use std::{borrow::Cow, collections::HashMap, path::PathBuf};

use ast::{CidlType, CrudKind, HttpVerb, IncludeTree};
use chumsky::span::SimpleSpan;

pub mod fmt;
pub mod lexer;
pub mod parser;

pub type Span = SimpleSpan<usize, FileId>;

pub struct FileTable<'src> {
    table: HashMap<FileId, (&'src str, PathBuf)>,
}

impl<'src> FileTable<'src> {
    /// Panics if the ID is not found
    pub fn resolve(&self, file_id: FileId) -> (&str, &PathBuf) {
        let (src, path) = self.table.get(&file_id).expect("invalid file ID");
        (src, path)
    }

    pub fn cache(&self) -> impl ariadne::Cache<String> + '_ {
        ariadne::sources(
            self.table
                .values()
                .map(|(src, path)| (path.display().to_string(), *src)),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WranglerEnvBindingKind {
    D1,
    R2,
    Kv,
}

impl std::fmt::Display for WranglerEnvBindingKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WranglerEnvBindingKind::D1 => write!(f, "D1"),
            WranglerEnvBindingKind::R2 => write!(f, "R2"),
            WranglerEnvBindingKind::Kv => write!(f, "Kv"),
        }
    }
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
    DataSourceMethodParam,

    ServiceDecl,
    ServiceField,

    InjectDecl,

    #[default]
    Null,
}

pub type FileId = u16;

#[derive(Debug, Default, Clone)]
pub struct Symbol<'src> {
    /// [String::default()] for symbols with no name
    pub name: &'src str,

    /// [CidlType::default()] for symbols with no type
    pub cidl_type: CidlType<'src>,

    /// [String::default()] for symbols with no parent
    /// Uses a [Cow] to avoid unnecessary allocations for symbols with parents,
    /// which are often just references to other symbols
    pub parent_name: Cow<'src, str>,

    pub kind: SymbolKind,

    pub span: Span,
}

pub struct ApiBlock<'src> {
    pub symbol: Symbol<'src>,

    pub namespace: &'src str,
    pub methods: Vec<ApiBlockMethod<'src>>,
}

pub struct ApiBlockMethod<'src> {
    pub symbol: Symbol<'src>,

    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub data_source: Option<&'src str>,
    pub return_type: CidlType<'src>,
    pub parameters: Vec<Symbol<'src>>,
}

pub struct DataSourceBlockMethod<'src> {
    pub span: Span,
    pub parameters: Vec<Symbol<'src>>,
    pub raw_sql: &'src str,
}

pub struct DataSourceBlock<'src> {
    pub symbol: Symbol<'src>,

    pub model: &'src str,
    pub tree: IncludeTree<'src>,
    pub list: Option<DataSourceBlockMethod<'src>>,
    pub get: Option<DataSourceBlockMethod<'src>>,
    pub is_internal: bool,
}

pub struct NavigationTag<'src> {
    pub span: Span,

    /// The field on the current model that represents the relationship
    pub field: &'src str,

    /// All columns involved in the relationship
    /// (model, field)
    pub fields: Vec<(&'src str, &'src str)>,
    pub is_many_to_many: bool,
}

pub struct ForeignKeyTag<'src> {
    pub span: Span,

    pub adj_model: &'src str,

    /// (this model field, adjacent model field)
    pub references: Vec<(&'src str, &'src str)>,
}

pub struct KvR2Tag<'src> {
    pub span: Span,

    pub field: &'src str,

    /// Key format e.g. "users/{id}/profile.jpg"
    pub format: &'src str,

    /// The symbol of the environment variable binding the KV namespace
    pub env_binding: &'src str,
}

pub struct UniqueTag<'src> {
    pub span: Span,
    pub fields: Vec<&'src str>,
}

#[derive(Clone, Debug)]
pub struct D1Tag<'src> {
    pub span: Span,

    /// The symbol of the environment variable binding the D1 database
    pub env_binding: &'src str,
}

pub struct KeyFieldTag<'src> {
    pub span: Span,
    pub field: &'src str,
}

pub struct PrimaryKeyTag<'src> {
    pub span: Span,
    pub field: &'src str,
}

pub struct ModelBlock<'src> {
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,

    pub primary_keys: Vec<PrimaryKeyTag<'src>>,
    pub d1_binding: Option<D1Tag<'src>>,
    pub key_fields: Vec<KeyFieldTag<'src>>,
    pub unique_constraints: Vec<UniqueTag<'src>>,
    pub kvs: Vec<KvR2Tag<'src>>,
    pub r2s: Vec<KvR2Tag<'src>>,

    pub navigation_properties: Vec<NavigationTag<'src>>,
    pub foreign_keys: Vec<ForeignKeyTag<'src>>,

    pub cruds: Vec<CrudKind>,
}

pub struct ServiceBlock<'src> {
    pub symbol: Symbol<'src>,
    pub fields: Vec<Symbol<'src>>,
}

pub struct PlainOldObjectBlock<'src> {
    pub symbol: Symbol<'src>,
    pub fields: Vec<Symbol<'src>>,
}

pub struct WranglerEnvBlock<'src> {
    pub symbol: Symbol<'src>,
    pub d1_bindings: Vec<Symbol<'src>>,
    pub kv_bindings: Vec<Symbol<'src>>,
    pub r2_bindings: Vec<Symbol<'src>>,
    pub vars: Vec<Symbol<'src>>,
}

pub struct InjectBlock<'src> {
    pub symbol: Symbol<'src>,
    pub fields: Vec<Symbol<'src>>,
}

/// An IR for the raw parsed structure of a Cloesce project
#[derive(Default)]
pub struct ParseAst<'src> {
    pub wrangler_envs: Vec<WranglerEnvBlock<'src>>,
    pub models: Vec<ModelBlock<'src>>,
    pub apis: Vec<ApiBlock<'src>>,
    pub sources: Vec<DataSourceBlock<'src>>,
    pub services: Vec<ServiceBlock<'src>>,
    pub poos: Vec<PlainOldObjectBlock<'src>>,
    pub injects: Vec<InjectBlock<'src>>,
}

impl<'src> ParseAst<'src> {
    fn merge(&mut self, other: ParseAst<'src>) {
        self.wrangler_envs.extend(other.wrangler_envs);
        self.models.extend(other.models);
        self.apis.extend(other.apis);
        self.sources.extend(other.sources);
        self.services.extend(other.services);
        self.poos.extend(other.poos);
        self.injects.extend(other.injects);
    }
}
