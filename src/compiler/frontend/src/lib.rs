use std::{collections::HashMap, path::PathBuf};

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

#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub enum EnvBindingKind {
    D1,
    R2,
    Kv,
}

pub type FileId = u16;

#[derive(Debug, Default, Clone)]
pub struct Symbol<'src> {
    /// [String::default()] for symbols with no name
    pub name: &'src str,

    /// [CidlType::default()] for symbols with no type
    pub cidl_type: CidlType<'src>,

    pub span: Span,
}

pub struct ApiBlock<'src> {
    pub symbol: Symbol<'src>,

    pub namespace: &'src str,
    pub methods: Vec<ApiBlockMethod<'src>>,
}

pub enum ApiBlockMethodParamKind<'src> {
    SelfParam {
        symbol: Symbol<'src>,
        data_source: Option<&'src str>,
    },
    Field(Symbol<'src>),
}

pub struct ApiBlockMethod<'src> {
    pub symbol: Symbol<'src>,

    pub http_verb: HttpVerb,
    pub return_type: CidlType<'src>,
    pub parameters: Vec<ApiBlockMethodParamKind<'src>>,
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

pub struct NavigationBlock<'src> {
    pub span: Span,

    // nav(AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(&'src str, &'src str)>,

    // { navName }
    pub field: Symbol<'src>,
}

#[derive(Clone)]
pub enum ForeignQualifier {
    Primary,
    Optional,
    Unique,
}

pub struct ForeignBlock<'src> {
    pub span: Span,

    // foreign(AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(&'src str, &'src str)>,

    // { currentModelField1, currentModelField2, ... }
    pub fields: Vec<Symbol<'src>>,

    pub nav: Option<Symbol<'src>>,

    pub qualifier: Option<ForeignQualifier>,
}

impl ForeignBlock<'_> {
    pub fn is_optional(&self) -> bool {
        matches!(self.qualifier, Some(ForeignQualifier::Optional))
    }
}

/// `kv(binding, "key/format/{id}") { name: type }`
pub struct KvBlock<'src> {
    pub span: Span,

    /// The KV namespace binding name
    pub env_binding: &'src str,

    /// The key format string, e.g. `"weather/data/{id}"`
    pub key_format: &'src str,

    /// The single identity field with its type
    pub field: Symbol<'src>,

    pub is_paginated: bool,
}

/// `r2(binding, "key/format/{id}") { name }`
pub struct R2Block<'src> {
    pub span: Span,

    /// R2 bucket binding name
    pub env_binding: &'src str,

    /// bucket key format string, e.g. `"weather/photos/{id}.jpg"`
    pub key_format: &'src str,

    /// has no type
    pub field: Symbol<'src>,

    // [paginated]
    pub is_paginated: bool,
}

pub enum UseTagParamKind<'src> {
    Crud(CrudKind),
    EnvBinding(&'src str),
}

pub struct UseTag<'src> {
    pub span: Span,
    pub params: Vec<UseTagParamKind<'src>>,
}

pub enum SqlBlockKind<'src> {
    Column(Symbol<'src>),
    Foreign(ForeignBlock<'src>),
}

pub enum PaginatedBlockKind<'src> {
    R2(R2Block<'src>),
    Kv(KvBlock<'src>),
}

pub enum ModelBlockKind<'src> {
    Column(Symbol<'src>),
    Foreign(ForeignBlock<'src>),
    Navigation(NavigationBlock<'src>),
    Kv(KvBlock<'src>),
    R2(R2Block<'src>),
    Primary {
        span: Span,
        blocks: Vec<SqlBlockKind<'src>>,
    },
    KeyField {
        span: Span,
        fields: Vec<Symbol<'src>>,
    },
    Unique {
        span: Span,
        blocks: Vec<SqlBlockKind<'src>>,
    },
    Paginated {
        span: Span,
        blocks: Vec<PaginatedBlockKind<'src>>,
    },
    Optional {
        span: Span,
        blocks: Vec<SqlBlockKind<'src>>,
    },
}

impl<'src> ModelBlockKind<'src> {
    pub fn symbols(&self) -> Vec<&Symbol<'src>> {
        match self {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Navigation(navigation_block) => vec![&navigation_block.field],
            ModelBlockKind::Kv(kv_block) => vec![&kv_block.field],
            ModelBlockKind::R2(r2_block) => vec![&r2_block.field],
            ModelBlockKind::Primary { blocks, .. }
            | ModelBlockKind::Unique { blocks, .. }
            | ModelBlockKind::Optional { blocks, .. } => blocks
                .iter()
                .flat_map(|block| match block {
                    SqlBlockKind::Column(symbol) => vec![symbol],
                    SqlBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
                })
                .collect(),
            ModelBlockKind::KeyField { fields, .. } => fields.iter().collect(),
            ModelBlockKind::Paginated { blocks, .. } => blocks
                .iter()
                .flat_map(|block| match block {
                    PaginatedBlockKind::Kv(kv_block) => vec![&kv_block.field],
                    PaginatedBlockKind::R2(r2_block) => vec![&r2_block.field],
                })
                .collect(),
        }
    }
}

pub struct ModelBlock<'src> {
    pub symbol: Symbol<'src>,
    pub use_tags: Vec<UseTag<'src>>,

    pub blocks: Vec<ModelBlockKind<'src>>,
}

impl<'src> ModelBlock<'src> {
    pub fn partition_use_tags(&self) -> (Vec<&CrudKind>, Vec<&'src str>) {
        let mut crud_tags = Vec::new();
        let mut env_binding_tags = Vec::new();

        for tag in &self.use_tags {
            for param in &tag.params {
                match param {
                    UseTagParamKind::Crud(crud_kind) => crud_tags.push(crud_kind),
                    UseTagParamKind::EnvBinding(binding) => env_binding_tags.push(*binding),
                }
            }
        }

        (crud_tags, env_binding_tags)
    }

    /// Iterate over all foreign blocks, be they top level or nested in primary/unique/optional blocks
    pub fn foreign_blocks(&self) -> impl Iterator<Item = &ForeignBlock<'src>> {
        self.blocks.iter().flat_map(|block| match block {
            ModelBlockKind::Foreign(foreign_block) => vec![foreign_block],
            ModelBlockKind::Primary { blocks, .. }
            | ModelBlockKind::Unique { blocks, .. }
            | ModelBlockKind::Optional { blocks, .. } => blocks
                .iter()
                .filter_map(|b| match b {
                    SqlBlockKind::Foreign(foreign_block) => Some(foreign_block),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        })
    }

    /// Iterate over navigation blocks (not including those in foreign blocks)
    pub fn navigation_blocks(&self) -> impl Iterator<Item = &NavigationBlock<'src>> {
        self.blocks.iter().filter_map(|block| match block {
            ModelBlockKind::Navigation(navigation_block) => Some(navigation_block),
            _ => None,
        })
    }

    /// Iterate over all SQL column blocks, be they top level or nested in primary/unique/optional blocks
    pub fn sql_symbols(&self) -> impl Iterator<Item = &Symbol<'src>> {
        self.blocks.iter().flat_map(|block| match block {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Primary { blocks, .. }
            | ModelBlockKind::Unique { blocks, .. }
            | ModelBlockKind::Optional { blocks, .. } => blocks
                .iter()
                .filter_map(|b| match b {
                    SqlBlockKind::Column(symbol) => Some(symbol),
                    SqlBlockKind::Foreign(_) => None,
                })
                .collect(),
            _ => vec![],
        })
    }
}

pub struct ServiceBlock<'src> {
    pub symbol: Symbol<'src>,
    pub fields: Vec<Symbol<'src>>,
}

pub struct PlainOldObjectBlock<'src> {
    pub symbol: Symbol<'src>,
    pub fields: Vec<Symbol<'src>>,
}

pub enum EnvBlockKind<'src> {
    D1 { symbols: Vec<Symbol<'src>> },
    R2 { symbols: Vec<Symbol<'src>> },
    Kv { symbols: Vec<Symbol<'src>> },
    Var { symbols: Vec<Symbol<'src>> },
}

pub struct EnvBlock<'src> {
    pub symbol: Symbol<'src>,
    pub blocks: Vec<EnvBlockKind<'src>>,
}

pub struct InjectBlock<'src> {
    pub symbol: Symbol<'src>,
    pub symbols: Vec<Symbol<'src>>,
}

pub enum AstBlockKind<'src> {
    Api(ApiBlock<'src>),
    DataSource(DataSourceBlock<'src>),
    Model(ModelBlock<'src>),
    Service(ServiceBlock<'src>),
    PlainOldObject(PlainOldObjectBlock<'src>),
    Env(EnvBlock<'src>),
    Inject(InjectBlock<'src>),
}

/// An IR for the raw parsed structure of a Cloesce project
#[derive(Default)]
pub struct ParseAst<'src> {
    pub blocks: Vec<AstBlockKind<'src>>,
}

impl<'src> ParseAst<'src> {
    fn merge(&mut self, mut other: ParseAst<'src>) {
        self.blocks.append(&mut other.blocks);
    }
}
