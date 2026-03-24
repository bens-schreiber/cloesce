use std::{collections::BTreeMap, path::PathBuf};

use ast::{CidlType, CrudKind, HttpVerb};
use chumsky::span::SimpleSpan;

pub mod lexer;
pub mod parser;

/// A name that has been parsed but not yet resolved to a specific declaration.
#[derive(PartialEq, Eq, Hash, Clone)]
pub struct UnresolvedName(pub String);

#[derive(Clone)]
pub struct SpannedTypedName {
    pub span: SimpleSpan,
    pub name: String,
    pub cidl_type: CidlType,
}

#[derive(Clone)]
pub struct SpannedName {
    pub span: SimpleSpan,
    pub name: String,
}

pub struct ApiBlock {
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub model: UnresolvedName,
    pub cruds: Vec<CrudKind>,
    pub methods: Vec<ApiMethod>,
}

pub struct ApiMethod {
    pub span: SimpleSpan,
    pub name: String,

    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub data_source_name: Option<UnresolvedName>,
    pub return_type: CidlType,
    pub parameters: Vec<SpannedTypedName>,
}

pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

pub struct DataSourceMethod {
    pub span: SimpleSpan,
    pub parameters: Vec<SpannedTypedName>,
    pub raw_sql: String,
}

pub struct DataSourceBlock {
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub model: UnresolvedName,
    pub tree: IncludeTree,
    pub list: Option<DataSourceMethod>,
    pub get: Option<DataSourceMethod>,
}

pub struct D1NavigationProperty {
    /// The field on the current model that represents the relationship
    pub field: UnresolvedName,

    /// The model that this this navigation property points to
    pub adj_model: UnresolvedName,

    /// All columns involved in the relationship
    pub fields: Vec<UnresolvedName>,

    pub is_many_to_many: bool,
}

pub struct ForeignKey {
    pub adj_model: UnresolvedName,
    pub references: Vec<(UnresolvedName, UnresolvedName)>,
}

pub struct KvR2 {
    pub field: UnresolvedName,
    pub span: SimpleSpan,
    pub cidl_type: CidlType,

    /// Key format e.g. "users/{id}/profile.jpg"
    pub format: String,

    /// The symbol of the environment variable binding the KV namespace
    pub env_binding: UnresolvedName,
}

pub struct ModelBlock {
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,

    pub primary_keys: Vec<UnresolvedName>,
    pub d1_binding: Option<UnresolvedName>,
    pub key_fields: Vec<UnresolvedName>,
    pub unique_constraints: Vec<Vec<UnresolvedName>>,
    pub kvs: Vec<KvR2>,
    pub r2s: Vec<KvR2>,

    pub navigation_properties: Vec<D1NavigationProperty>,
    pub foreign_keys: Vec<ForeignKey>,
}

pub struct ServiceBlock {
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,
}

pub struct PlainOldObjectBlock {
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,
}

pub struct WranglerEnvBlock {
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub d1_bindings: Vec<SpannedName>,
    pub kv_bindings: Vec<SpannedName>,
    pub r2_bindings: Vec<SpannedName>,
    pub vars: Vec<SpannedTypedName>,
}

pub struct InjectBlock {
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub names: Vec<String>,
}

/// An IR representing the raw parsed structure of a Cloesce project
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
