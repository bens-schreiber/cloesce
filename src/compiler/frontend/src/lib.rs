use std::{collections::BTreeMap, path::PathBuf};

use ast::{CidlType, CrudKind, HttpVerb, IncludeTree};
use chumsky::span::SimpleSpan;

use crate::parser::ParseId;

pub mod lexer;
pub mod parser;

#[derive(Clone)]
pub struct SpannedTypedName {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
    pub cidl_type: CidlType,
}

#[derive(Clone)]
pub struct SpannedName {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
}

pub struct ApiBlock {
    pub id: ParseId,
    pub name: String,
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub model: ParseId,
    pub cruds: Vec<CrudKind>,
    pub methods: Vec<ApiBlockMethod>,
}

pub struct ApiBlockMethod {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub data_source_name: Option<ParseId>,
    pub return_type: CidlType,
    pub parameters: Vec<SpannedTypedName>,
}

pub struct DataSourceBlockMethod {
    pub span: SimpleSpan,
    pub parameters: Vec<SpannedTypedName>,
    pub raw_sql: String,
}

pub struct DataSourceBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub model: ParseId,
    pub tree: IncludeTree,
    pub list: Option<DataSourceBlockMethod>,
    pub get: Option<DataSourceBlockMethod>,
}

pub struct NavigationTag {
    pub id: ParseId,
    pub span: SimpleSpan,

    /// The field on the current model that represents the relationship
    pub field: ParseId,

    /// The model that this this navigation property points to
    pub adj_model: ParseId,

    /// All columns involved in the relationship
    pub fields: Vec<ParseId>,

    pub is_many_to_many: bool,
}

pub struct ForeignKeyTag {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub adj_model: ParseId,
    pub references: Vec<(ParseId, ParseId)>, // (current model field, adjacent model field)
}

pub struct KvR2Tag {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub field: ParseId,

    /// Key format e.g. "users/{id}/profile.jpg"
    pub format: String,

    /// The symbol of the environment variable binding the KV namespace
    pub env_binding: ParseId,
}

pub struct UniqueTag {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub fields: Vec<ParseId>,
}

pub struct D1Tag {
    pub id: ParseId,
    pub span: SimpleSpan,

    /// The symbol of the environment variable binding the D1 database
    pub env_binding: ParseId,
}

pub struct KeyFieldTag {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub field: ParseId,
}

pub struct PrimaryKeyTag {
    pub id: ParseId,
    pub span: SimpleSpan,

    pub field: ParseId,
}

pub struct ModelBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,

    pub primary_keys: Vec<PrimaryKeyTag>,
    pub d1_binding: Option<D1Tag>,
    pub key_fields: Vec<KeyFieldTag>,
    pub unique_constraints: Vec<UniqueTag>,
    pub kvs: Vec<KvR2Tag>,
    pub r2s: Vec<KvR2Tag>,

    pub navigation_properties: Vec<NavigationTag>,
    pub foreign_keys: Vec<ForeignKeyTag>,
}

pub struct ServiceBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,
}

pub struct PlainOldObjectBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub name: String,
    pub file: PathBuf,

    pub fields: Vec<SpannedTypedName>,
}

pub struct WranglerEnvBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub d1_bindings: Vec<SpannedName>,
    pub kv_bindings: Vec<SpannedName>,
    pub r2_bindings: Vec<SpannedName>,
    pub vars: Vec<SpannedTypedName>,
}

pub struct InjectBlock {
    pub id: ParseId,
    pub span: SimpleSpan,
    pub file: PathBuf,

    pub refs: Vec<ParseId>,
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
