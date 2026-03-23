use std::collections::BTreeMap;

use ast::{CidlType, CrudKind, HttpVerb};
use chumsky::span::SimpleSpan;

pub mod lexer;
pub mod parser;

/// A name that has been parsed but not yet resolved to a specific declaration.
pub struct UnresolvedName(pub String);

#[derive(Clone)]
pub struct SpannedTypedName {
    pub span: SimpleSpan,
    pub name: String,
    pub ty: CidlType,
}

#[derive(Clone)]
pub struct SpannedName {
    pub span: SimpleSpan,
    pub name: String,
}

pub struct ApiBlock {
    pub span: SimpleSpan,
    pub model_name: UnresolvedName,
    pub cruds: Vec<CrudKind>,
    pub methods: Vec<ApiMethod>,
}

pub struct ApiMethod {
    pub span_name: SpannedName,
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
    pub model_name: UnresolvedName,
    pub tree: IncludeTree,
    pub list: Option<DataSourceMethod>,
    pub get: Option<DataSourceMethod>,
}

/// A D1 Navigation property, representing a relationship to another model
/// through a foreign key or composite foreign key.
pub enum D1NavigationPropertyKind {
    OneToOne {
        /// The columns on the current model that reference the other model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<UnresolvedName>,
    },
    OneToMany {
        /// The columns on the other model that reference the current model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<UnresolvedName>,
    },

    /// A many to many relationship expressed through a join table,
    /// consisting of the two models primary keys (be they composite or not).
    ManyToMany { column: UnresolvedName },
}

pub struct D1NavigationProperty {
    /// The field on the current model that represents the relationship
    pub field_name: UnresolvedName,

    /// The model that this this navigation property points to
    pub adj_model_name: UnresolvedName,

    /// The kind of navigation property, which encodes the relationship and foreign key structure.
    pub kind: D1NavigationPropertyKind,
}

pub struct ForeignKey {
    pub adj_model_name: UnresolvedName,
    pub column_names: Vec<UnresolvedName>,
}

pub struct KvR2Field {
    pub typed_name: SpannedTypedName,

    /// Key format e.g. "users/{id}/profile.jpg"
    pub format: String,

    /// The symbol of the environment variable binding the KV namespace
    pub env_binding: UnresolvedName,
}

pub struct ModelBlock {
    pub span_name: SpannedName,
    pub d1_binding: Option<UnresolvedName>,

    pub columns: Vec<SpannedTypedName>,
    pub primary_key_columns: Vec<SpannedTypedName>,
    pub key_fields: Vec<SpannedTypedName>,
    pub kv_fields: Vec<KvR2Field>,
    pub r2_fields: Vec<KvR2Field>,

    pub navigation_properties: Vec<D1NavigationProperty>,
    pub foreign_keys: Vec<ForeignKey>,

    /// Each inner Vec represents a unique constraint, containing the column names that make up the constraint.
    pub unique_constraints: Vec<Vec<UnresolvedName>>,
}

pub struct ServiceBlock {
    pub span_name: SpannedName,
    pub fields: Vec<SpannedTypedName>,
}

pub struct PlainOldObjectBlock {
    pub span_name: SpannedName,
    pub fields: Vec<SpannedTypedName>,
}

pub struct WranglerEnvBlock {
    pub span: SimpleSpan,
    pub d1_bindings: Vec<SpannedName>,
    pub kv_bindings: Vec<SpannedName>,
    pub r2_bindings: Vec<SpannedName>,
    pub vars: Vec<SpannedTypedName>,
}

/// An IR representing the raw parsed structure of a Cloesce project
#[derive(Default)]
pub struct ParseAst {
    pub wrangler_env: Vec<WranglerEnvBlock>,
    pub models: Vec<ModelBlock>,
    pub apis: Vec<ApiBlock>,
    pub sources: Vec<DataSourceBlock>,
    pub services: Vec<ServiceBlock>,
    pub poos: Vec<PlainOldObjectBlock>,
    pub injectables: Vec<UnresolvedName>,
}
