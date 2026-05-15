//! Responsible for lexing and parsing Cloesce source files into an [Ast], a direct representation of the
//! parsed source code. Additionally, houses the formatter for Cloesce source files.
//!
//! # Cloesce AST
//!
//! The AST is a direct representation of the parsed source code, with the only transformations being
//! the streaming of comments into a seperate "comment map" channel during lexing.
//!
//! ## Symbols
//!
//! The [Symbol] struct is used to represent any named entity in the source code, such as a model name, field name, API method name, etc.
//! It contains the symbol's name, type, span, and any tags applied to it. Not all [Symbol]s will have a meaningful type, or any tags,
//! but this struct is used for all named entities for consistency and ease of error reporting.
//!
//! ## Memory management
//!
//! All string data in the AST is borrowed from the original heap-allocated source string. The [Ast] is non-recursive at the node level
//! (with the exception of [CidlType]). Child nodes are stored in flat [Vec]s rather than via recursive node-to-node ownership,
//! so no arena or custom allocator is required.

pub mod err;
pub mod formatter;
pub mod lexer;
pub mod parser;

use idl::{CidlType, CrudKind, HttpVerb};
use indexmap::IndexMap;

use crate::lexer::Token;
pub use crate::lexer::{FileTable, Span};

macro_rules! contextual_keywords {
    ($($variant:ident => $str:literal),* $(,)?) => {
        #[derive(Clone, Debug)]
        pub enum Keyword {
            $($variant),*
        }

        impl Keyword {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Keyword::$variant => $str),*
                }
            }
        }

        impl From<Keyword> for Token<'static> {
            fn from(kw: Keyword) -> Self {
                match kw {
                    $(Keyword::$variant => Token::Ident($str)),*
                }
            }
        }
    };
}

contextual_keywords! {
    // Block declaration / infix
    Nav => "nav",
    Foreign => "foreign",
    Primary => "primary",
    Optional => "optional",
    Unique => "unique",
    Paginated => "paginated",
    KeyField => "keyfield",
    For => "for",
    Include => "include",

    // Block type
    Model => "model",
    Poo => "poo",
    Service => "service",
    Source => "source",
    Env => "env",
    Inject => "inject",
    Api => "api",

    // Sub-block / binding
    D1 => "d1",
    R2 => "r2",
    Kv => "kv",
    Vars => "vars",

    // Tag
    Use => "use",
    Crud => "crud",
    Internal => "internal",
    Instance => "instance",

    // CRUD / SQL method
    Get => "get",
    List => "list",
    Save => "save",
    Sql => "sql",

    // HTTP verb
    Post => "post",
    Put => "put",
    Patch => "patch",
    Delete => "delete",

    // Validator tag (numerical)
    LessThan => "lt",
    LessThanOrEqual => "lte",
    GreaterThan => "gt",
    GreaterThanOrEqual => "gte",
    Step => "step",

    // Validator tag (string)
    Len => "len",
    MinLen => "minlen",
    MaxLen => "maxlen",
    Regex => "regex",

    // Generic type
    GOption => "option",
    GArray => "array",
    GPaginated => "paginated",
    GKvObject => "kvobject",
    GPartial => "partial",

    // Primitive type
    TString => "string",
    TInt => "int",
    TReal => "real",
    TDate => "date",
    TBool => "bool",
    TJson => "json",
    TBlob => "blob",
    TStream => "stream",
    TR2Object => "R2Object",
}

pub fn fmt_cidl_type(ty: &CidlType) -> String {
    match ty {
        CidlType::Int => Keyword::TInt.as_str().into(),
        CidlType::Real => Keyword::TReal.as_str().into(),
        CidlType::String => Keyword::TString.as_str().into(),
        CidlType::Blob => Keyword::TBlob.as_str().into(),
        CidlType::Boolean => Keyword::TBool.as_str().into(),
        CidlType::DateIso => Keyword::TDate.as_str().into(),
        CidlType::Stream => Keyword::TStream.as_str().into(),
        CidlType::Json => Keyword::TJson.as_str().into(),
        CidlType::R2Object => Keyword::TR2Object.as_str().into(),
        CidlType::Object { name } | CidlType::UnresolvedReference { name } => name.to_string(),
        CidlType::Partial { object_name } => {
            format!("{}<{}>", Keyword::GPartial.as_str(), object_name)
        }
        CidlType::Array(inner) => {
            format!("{}<{}>", Keyword::GArray.as_str(), fmt_cidl_type(inner))
        }
        CidlType::Nullable(inner) => {
            format!("{}<{}>", Keyword::GOption.as_str(), fmt_cidl_type(inner))
        }
        CidlType::Paginated(inner) => {
            format!("{}<{}>", Keyword::GPaginated.as_str(), fmt_cidl_type(inner))
        }
        CidlType::KvObject(inner) => {
            format!("{}<{}>", Keyword::GKvObject.as_str(), fmt_cidl_type(inner))
        }
        _ => unreachable!("unsupported CIDL type in fmt_cidl_type"),
    }
}

/// A spanned value
#[derive(Debug, Clone)]
pub struct Spd<T> {
    pub inner: T,
    pub span: Span,
}

pub trait SpdSlice<T> {
    fn inners<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a;
}

impl<T> SpdSlice<T> for [Spd<T>] {
    fn inners<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        self.iter().map(|spd| &spd.inner)
    }
}

impl<T> SpdSlice<T> for Vec<Spd<T>> {
    fn inners<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        self.iter().map(|spd| &spd.inner)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentLiteral<'src> {
    Int(&'src str),
    Real(&'src str),
    Str(&'src str),
    Regex(&'src str),
}

#[derive(Debug, Clone)]
pub enum Tag<'src> {
    Use {
        binding: Spd<&'src str>,
    },
    Source {
        name: Spd<&'src str>,
    },
    Internal,
    Instance,
    Crud {
        kinds: Vec<Spd<CrudKind>>,
    },
    Inject {
        bindings: Vec<Symbol<'src>>,
    },
    Validator {
        name: Keyword,
        argument: ArgumentLiteral<'src>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Symbol<'src> {
    pub name: &'src str,

    /// [CidlType::default()] for symbols with no type
    pub cidl_type: CidlType<'src>,

    /// The span the symbol name (and type) occupies, not including any leading validator tags.
    pub span: Span,

    pub tags: Vec<Spd<Tag<'src>>>,
}

impl PartialEq for Symbol<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.span == other.span
    }
}

impl Eq for Symbol<'_> {}

impl std::hash::Hash for Symbol<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.span.hash(state);
    }
}

pub struct ApiBlock<'src> {
    /// The symbol for the API's name, e.g. `ApiName` in `api ApiName { ... }`
    pub symbol: Symbol<'src>,

    pub methods: Vec<Spd<ApiBlockMethod<'src>>>,
}

pub struct ApiBlockMethod<'src> {
    /// The symbol for the method name, e.g. `getUser` in `post getUser(...) { ... }`
    pub symbol: Symbol<'src>,

    pub http_verb: HttpVerb,
    pub return_type: CidlType<'src>,
    pub parameters: Vec<Spd<ApiBlockMethodParamKind<'src>>>,
}

pub enum ApiBlockMethodParamKind<'src> {
    SelfParam(Symbol<'src>),
    Param(Symbol<'src>),
}

pub struct DataSourceBlockMethod<'src> {
    pub parameters: Vec<Symbol<'src>>,
    pub raw_sql: &'src str,
}

// Index map is used here to preserve declaration order
pub struct ParsedIncludeTree<'src>(pub IndexMap<Symbol<'src>, ParsedIncludeTree<'src>>);

pub struct DataSourceBlock<'src> {
    /// The symbol for the data source itself, e.g. `SourceName`
    pub symbol: Symbol<'src>,

    /// The symbol for the model this data source is for, e.g. `for ModelName`
    pub model: Symbol<'src>,

    pub tree: ParsedIncludeTree<'src>,
    pub list: Option<Spd<DataSourceBlockMethod<'src>>>,
    pub get: Option<Spd<DataSourceBlockMethod<'src>>>,
}

pub struct NavigationBlock<'src> {
    // nav (AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(Symbol<'src>, Symbol<'src>)>,

    // { navName }
    pub nav: Spd<Symbol<'src>>,
}

#[derive(Clone)]
pub enum ForeignQualifier {
    Primary,
    Optional,
    Unique,
}

pub struct ForeignBlockNav<'src> {
    pub symbol: Symbol<'src>,
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
    pub nav: Option<Spd<ForeignBlockNav<'src>>>,

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

    /// has no type; validators are not applicable to R2 fields
    pub field: Symbol<'src>,

    // [paginated]
    pub is_paginated: bool,
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
    Primary(Vec<Spd<SqlBlockKind<'src>>>),
    KeyField(Vec<Symbol<'src>>),
    Unique(Vec<Spd<SqlBlockKind<'src>>>),
    Paginated(Vec<Spd<PaginatedBlockKind<'src>>>),
    Optional(Vec<Spd<SqlBlockKind<'src>>>),
}

impl<'src> ModelBlockKind<'src> {
    pub fn symbols(&self) -> Vec<&Symbol<'src>> {
        match self {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Navigation(nav_block) => vec![&nav_block.nav.inner],
            ModelBlockKind::Kv(kv_block) => vec![&kv_block.field],
            ModelBlockKind::R2(r2_block) => vec![&r2_block.field],
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
                .iter()
                .flat_map(|spd| match &spd.inner {
                    SqlBlockKind::Column(symbol) => vec![symbol],
                    SqlBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
                })
                .collect(),
            ModelBlockKind::KeyField(fields) => fields.iter().collect(),
            ModelBlockKind::Paginated(blocks) => blocks
                .iter()
                .flat_map(|spd| match &spd.inner {
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

    pub blocks: Vec<Spd<ModelBlockKind<'src>>>,
}

impl<'src> ModelBlock<'src> {
    /// Iterate over all foreign blocks, be they top level or nested in primary/unique/optional blocks
    pub fn foreign_blocks(&self) -> impl Iterator<Item = &ForeignBlock<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(foreign_block) => vec![foreign_block],
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
                .iter()
                .filter_map(|b| match &b.inner {
                    SqlBlockKind::Foreign(fb) => Some(fb),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        })
    }

    /// Iterate over navigation blocks (not including those in foreign blocks)
    pub fn navigation_blocks(&self) -> impl Iterator<Item = &NavigationBlock<'src>> {
        self.blocks.iter().filter_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(navigation_block) => Some(navigation_block),
            _ => None,
        })
    }

    /// Iterate over all SQL column blocks, be they top level or nested in primary/unique/optional blocks
    pub fn sql_symbols(&self) -> impl Iterator<Item = &Symbol<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.inner {
            ModelBlockKind::Column(symbol) => vec![symbol],
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => blocks
                .iter()
                .filter_map(|b| match &b.inner {
                    SqlBlockKind::Column(symbol) => Some(symbol),
                    SqlBlockKind::Foreign(_) => None,
                })
                .collect(),
            _ => vec![],
        })
    }
}

pub struct ServiceBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
}

pub struct PlainOldObjectBlock<'src> {
    /// The symbol for the POO name, e.g. `MyPoo` in `poo MyPoo { ... }`
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,
}

pub enum EnvBindingBlockKind {
    D1,
    R2,
    Kv,
    Var,
}

pub struct EnvBindingBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
    pub kind: EnvBindingBlockKind,
}

pub struct EnvBlock<'src> {
    pub blocks: Vec<Spd<EnvBindingBlock<'src>>>,
}

pub struct InjectBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
}

#[allow(clippy::large_enum_variant)]
pub enum AstBlockKind<'src> {
    Api(ApiBlock<'src>),
    DataSource(DataSourceBlock<'src>),
    Model(ModelBlock<'src>),
    Service(ServiceBlock<'src>),
    PlainOldObject(PlainOldObjectBlock<'src>),
    Env(EnvBlock<'src>),
    Inject(InjectBlock<'src>),
}

/// The raw parsed structure of a Cloesce source file, before semantic analysis and transformation into the IDL.
#[derive(Default)]
pub struct Ast<'src> {
    pub blocks: Vec<Spd<AstBlockKind<'src>>>,
}

impl<'src> Ast<'src> {
    fn merge(&mut self, mut other: Ast<'src>) {
        self.blocks.append(&mut other.blocks);
    }
}
