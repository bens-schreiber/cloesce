use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use ast::{CidlType, CrudKind, HttpVerb};
use chumsky::span::SimpleSpan;

pub mod err;
pub mod formatter;
pub mod lexer;
pub mod parser;

pub type Span = SimpleSpan<usize, FileId>;

/// A spanned block
pub struct Spd<T> {
    pub block: T,
    pub span: Span,
}

pub trait SpdSlice<T> {
    fn blocks<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a;
}

impl<T> SpdSlice<T> for [Spd<T>] {
    fn blocks<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        self.iter().map(|spd| &spd.block)
    }
}

impl<T> SpdSlice<T> for Vec<Spd<T>> {
    fn blocks<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        self.iter().map(|spd| &spd.block)
    }
}

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

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Hash, Eq)]
pub struct Symbol<'src> {
    pub name: &'src str,

    /// [CidlType::default()] for symbols with no type
    pub cidl_type: CidlType<'src>,

    /// The span the symbol name occupies.
    pub span: Span,
}

pub struct ApiBlock<'src> {
    /// The symbol for the API's name, e.g. `ApiName` in `api ApiName { ... }`
    pub symbol: Symbol<'src>,

    pub methods: Vec<Spd<ApiBlockMethod<'src>>>,
}

pub enum ApiBlockMethodParamKind<'src> {
    SelfParam {
        /// The symbol for the `self` parameter, e.g. `self`
        symbol: Symbol<'src>,

        /// A data source block if tagged, e.g. `[source MySource] self`
        data_source: Option<Symbol<'src>>,
    },
    Field(Symbol<'src>),
}

pub struct ApiBlockMethod<'src> {
    /// The symbol for the method name, e.g. `getUser` in `post getUser(...) { ... }`
    pub symbol: Symbol<'src>,

    pub http_verb: HttpVerb,
    pub return_type: CidlType<'src>,
    pub parameters: Vec<Spd<ApiBlockMethodParamKind<'src>>>,
}

pub struct DataSourceBlockMethod<'src> {
    pub parameters: Vec<Symbol<'src>>,
    pub raw_sql: &'src str,
}

pub struct ParsedIncludeTree<'src>(pub BTreeMap<Symbol<'src>, ParsedIncludeTree<'src>>);

pub struct DataSourceBlock<'src> {
    /// The symbol for the data source itself, e.g. `SourceName`
    pub symbol: Symbol<'src>,

    /// The symbol for the model this data source is for, e.g. `for ModelName`
    pub model: Symbol<'src>,

    pub tree: ParsedIncludeTree<'src>,
    pub list: Option<Spd<DataSourceBlockMethod<'src>>>,
    pub get: Option<Spd<DataSourceBlockMethod<'src>>>,
    pub is_internal: bool,
}

pub struct NavigationBlock<'src> {
    // nav (AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(Symbol<'src>, Symbol<'src>)>,

    // { navName }
    pub symbol: Symbol<'src>,
}

#[derive(Clone)]
pub enum ForeignQualifier {
    Primary,
    Optional,
    Unique,
}

pub struct ForeignBlock<'src> {
    // foreign(AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(Symbol<'src>, Symbol<'src>)>,

    // { currentModelField1, currentModelField2, ... }
    pub fields: Vec<Symbol<'src>>,

    /// Nav field to the adjacent model, ex:
    /// ```cloesce
    /// foreign (...) {
    ///     ...
    ///     nav { navSymbol }
    /// }
    /// ```
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
    /// The KV namespace binding name
    pub env_binding: Symbol<'src>,

    /// The key format string, e.g. `"weather/data/{id}"`
    pub key_format: &'src str,

    /// The single identity field with its type
    pub field: Symbol<'src>,

    pub is_paginated: bool,
}

/// `r2(binding, "key/format/{id}") { name }`
pub struct R2Block<'src> {
    /// R2 bucket binding name
    pub env_binding: Symbol<'src>,

    /// bucket key format string, e.g. `"weather/photos/{id}.jpg"`
    pub key_format: &'src str,

    /// has no type
    pub field: Symbol<'src>,

    // [paginated]
    pub is_paginated: bool,
}

pub enum UseTagParamKind<'src> {
    Crud(Spd<CrudKind>),
    EnvBinding(Symbol<'src>),
}

pub struct UseTag<'src> {
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
    Primary(Vec<SqlBlockKind<'src>>),
    KeyField(Vec<Symbol<'src>>),
    Unique(Vec<SqlBlockKind<'src>>),
    Paginated(Vec<PaginatedBlockKind<'src>>),
    Optional(Vec<SqlBlockKind<'src>>),
}

impl<'src> ModelBlockKind<'src> {
    pub fn symbols(&self) -> Vec<&Symbol<'src>> {
        match self {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Navigation(navigation_block) => vec![&navigation_block.symbol],
            ModelBlockKind::Kv(kv_block) => vec![&kv_block.field],
            ModelBlockKind::R2(r2_block) => vec![&r2_block.field],
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
                .iter()
                .flat_map(|block| match block {
                    SqlBlockKind::Column(symbol) => vec![symbol],
                    SqlBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
                })
                .collect(),
            ModelBlockKind::KeyField(fields) => fields.iter().collect(),
            ModelBlockKind::Paginated(blocks) => blocks
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
    /// The symbol for the model name, e.g. `ModelName` in `model ModelName { ... }`
    pub symbol: Symbol<'src>,

    pub use_tags: Vec<Spd<UseTag<'src>>>,
    pub blocks: Vec<Spd<ModelBlockKind<'src>>>,
}

impl<'src> ModelBlock<'src> {
    pub fn partition_use_tags(&self) -> (Vec<&CrudKind>, Vec<&Symbol<'src>>) {
        let mut crud_tags = Vec::new();
        let mut env_binding_tags = Vec::new();

        for tag in &self.use_tags {
            for param in &tag.block.params {
                match param {
                    UseTagParamKind::Crud(spd) => crud_tags.push(&spd.block),
                    UseTagParamKind::EnvBinding(binding) => env_binding_tags.push(binding),
                }
            }
        }

        (crud_tags, env_binding_tags)
    }

    /// Iterate over all foreign blocks, be they top level or nested in primary/unique/optional blocks
    pub fn foreign_blocks(&self) -> impl Iterator<Item = &ForeignBlock<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.block {
            ModelBlockKind::Foreign(foreign_block) => vec![foreign_block],
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    SqlBlockKind::Foreign(fb) => Some(fb),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        })
    }

    /// Iterate over navigation blocks (not including those in foreign blocks)
    pub fn navigation_blocks(&self) -> impl Iterator<Item = &NavigationBlock<'src>> {
        self.blocks.iter().filter_map(|spd| match &spd.block {
            ModelBlockKind::Navigation(navigation_block) => Some(navigation_block),
            _ => None,
        })
    }

    /// Iterate over all SQL column blocks, be they top level or nested in primary/unique/optional blocks
    pub fn sql_symbols(&self) -> impl Iterator<Item = &Symbol<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.block {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
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
    /// The symbol for the service name, e.g. `MyAppService` in `service MyAppService { ... }`
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,
}

pub struct PlainOldObjectBlock<'src> {
    /// The symbol for the POO name, e.g. `MyPoo` in `poo MyPoo { ... }`
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,
}

pub enum EnvBlockKind {
    D1,
    R2,
    Kv,
    Var,
}

pub struct EnvBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
    pub kind: EnvBlockKind,
}

pub struct InjectBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
}

pub enum AstBlockKind<'src> {
    Api(ApiBlock<'src>),
    DataSource(DataSourceBlock<'src>),
    Model(ModelBlock<'src>),
    Service(ServiceBlock<'src>),
    PlainOldObject(PlainOldObjectBlock<'src>),
    Env(Vec<Spd<EnvBlock<'src>>>),
    Inject(InjectBlock<'src>),
}

/// An IR for the raw parsed structure of a Cloesce project
#[derive(Default)]
pub struct ParseAst<'src> {
    pub blocks: Vec<Spd<AstBlockKind<'src>>>,
}

impl<'src> ParseAst<'src> {
    fn merge(&mut self, mut other: ParseAst<'src>) {
        self.blocks.append(&mut other.blocks);
    }
}
