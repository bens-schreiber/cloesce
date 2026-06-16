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
    Column => "column",
    Route => "route",
    For => "for",
    Include => "include",

    // Block type
    Model => "model",
    Poo => "poo",
    Source => "source",
    Inject => "inject",
    Api => "api",
    Vars => "vars",
    D1 => "d1",
    R2 => "r2",
    Kv => "kv",
    Durable => "durable",
    Shard => "shard",

    // CRUD / SQL method
    Get => "get",
    List => "list",
    Save => "save",

    // HTTP verb
    Post => "post",
    Put => "put",
    Patch => "patch",
    Delete => "delete",

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
    TR2Object => "r2object",

    // Tag
    Crud => "crud",
    Internal => "internal",
    Instance => "instance",

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
        CidlType::Object { name } => name.to_string(),
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
        CidlType::Void => panic!("void type should not appear in CidlType formatting"),
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
pub struct DurableInitializer<'src> {
    pub symbol: Symbol<'src>,
    pub args: Vec<Symbol<'src>>,
}

/// A single entry in an `[inject ...]` tag.
#[derive(Debug, Clone)]
pub enum InjectEntry<'src> {
    Binding(Symbol<'src>),
    Context(DurableInitializer<'src>),
}

#[derive(Debug, Clone)]
pub enum Tag<'src> {
    Source {
        name: Spd<&'src str>,
    },
    Internal,
    Instance,
    Crud {
        kinds: Vec<Spd<CrudKind>>,
    },
    Inject {
        entries: Vec<InjectEntry<'src>>,
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
    ///
    /// The [CidlType] of this symbol represents the return type of the API method.
    pub symbol: Symbol<'src>,

    pub http_verb: HttpVerb,
    pub parameters: Vec<Spd<ApiBlockMethodParamKind<'src>>>,
}

pub enum ApiBlockMethodParamKind<'src> {
    SelfParam(Symbol<'src>),
    Param(Symbol<'src>),
}

pub struct DataSourceBlockMethod<'src> {
    pub method: Symbol<'src>,
    pub parameters: Vec<Symbol<'src>>,
    /// Always empty after proposal 7; retained until semantic crate drops it.
    pub raw_sql: &'src str,
}

// Index map is used here to preserve declaration order
pub struct ParsedIncludeTree<'src>(pub IndexMap<Symbol<'src>, ParsedIncludeTree<'src>>);

pub struct DataSourceBlock<'src> {
    /// The symbol for the data source itself, e.g. `SourceName`
    pub symbol: Symbol<'src>,

    /// The symbol for the model this data source is for, e.g. `for ModelName`
    pub model: Symbol<'src>,

    pub tree: Option<ParsedIncludeTree<'src>>,
    pub list: Option<Spd<DataSourceBlockMethod<'src>>>,
    pub get: Option<Spd<DataSourceBlockMethod<'src>>>,
    pub save: Option<Spd<DataSourceBlockMethod<'src>>>,
}

pub struct NavAdj<'src> {
    /// `AdjModel` in `AdjModel::field`
    pub model: Symbol<'src>,

    /// `field` in `AdjModel::field`.
    pub field: Option<Symbol<'src>>,

    /// The local FK column on the current model: the `(localKey)` part.
    /// `Some` => 1:1 entry, `None` => 1:M entry.
    pub local_key: Option<Symbol<'src>>,
}

pub struct NavigationBlock<'src> {
    pub adj: Vec<NavAdj<'src>>,

    // { navName }
    pub nav: Spd<Symbol<'src>>,
}

impl<'src> NavigationBlock<'src> {
    /// A nav is 1:1 iff it carries local keys
    pub fn is_one_to_one(&self) -> bool {
        self.adj
            .first()
            .map(|a| a.local_key.is_some())
            .unwrap_or(false)
    }
}

pub struct ForeignBlock<'src> {
    // foreign(AdjModel::field1, AdjModel::field2, ...)
    pub adj: Vec<(Symbol<'src>, Symbol<'src>)>,

    // { currentModelField1, currentModelField2, ... }
    pub fields: Vec<Symbol<'src>>,

    pub is_optional: bool,
}

pub struct KvFieldBlock<'src> {
    /// The KV binding name (e.g. `UserMetadata`)
    pub binding: Symbol<'src>,

    /// The field on the KV binding being referenced
    pub binding_template: Symbol<'src>,

    /// The model fields to be passed as arguments to the binding's field
    pub args: Vec<Symbol<'src>>,

    /// The local field on the model representing this binding field
    pub field: Symbol<'src>,
}

pub struct R2FieldBlock<'src> {
    /// The R2 binding name
    pub binding: Symbol<'src>,

    /// The field on the R2 binding being referenced
    pub binding_template: Symbol<'src>,

    /// The model fields to be passed as arguments to the binding's field
    pub args: Vec<Symbol<'src>>,

    /// The local field on the model representing this binding field
    pub field: Symbol<'src>,
}

pub enum SqlBlockKind<'src> {
    Column(Symbol<'src>),
    Foreign(ForeignBlock<'src>),
}

pub enum ModelBlockKind<'src> {
    Column(Vec<Symbol<'src>>),
    Foreign(ForeignBlock<'src>),
    Navigation(NavigationBlock<'src>),
    Kv(KvFieldBlock<'src>),
    R2(R2FieldBlock<'src>),
    Primary(Vec<Spd<SqlBlockKind<'src>>>),
    Route(Vec<Symbol<'src>>),
    Unique(Vec<Symbol<'src>>),
}

impl<'src> ModelBlockKind<'src> {
    /// Returns the field-declaration symbols introduced by this block.
    pub fn symbols(&self) -> Vec<&Symbol<'src>> {
        match self {
            ModelBlockKind::Column(symbols) => symbols.iter().collect(),
            ModelBlockKind::Route(symbols) => symbols.iter().collect(),
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Navigation(nav_block) => vec![&nav_block.nav.inner],
            ModelBlockKind::Kv(kv_block) => vec![&kv_block.field],
            ModelBlockKind::R2(r2_block) => vec![&r2_block.field],
            ModelBlockKind::Primary(blocks) => blocks
                .iter()
                .flat_map(|spd| match &spd.inner {
                    SqlBlockKind::Column(symbol) => vec![symbol],
                    SqlBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
                })
                .collect(),
            ModelBlockKind::Unique(_) => vec![],
        }
    }
}

pub struct ModelBlock<'src> {
    /// The symbol for the model name, e.g. `ModelName` in `model ModelName { ... }`
    pub symbol: Symbol<'src>,

    /// `for SomeBinding` in `model M for SomeBinding { ... }`.
    pub database_binding: Option<Symbol<'src>>,

    /// Arguments of a database binding, e.g.
    /// `(shardKey1, shardKey2)` in `model M for SomeBinding(shardKey1, shardKey2) { ...
    pub shard_args: Option<Vec<Symbol<'src>>>,

    pub blocks: Vec<Spd<ModelBlockKind<'src>>>,
}

impl<'src> ModelBlock<'src> {
    /// Iterate over all foreign blocks, be they top level or nested in primary blocks
    pub fn foreign_blocks(&self) -> impl Iterator<Item = &ForeignBlock<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(foreign_block) => vec![foreign_block],
            ModelBlockKind::Primary(blocks) => blocks
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

    /// Iterate over all SQL column blocks, be they top level or nested in primary blocks
    pub fn sql_symbols(&self) -> impl Iterator<Item = &Symbol<'src>> {
        self.blocks.iter().flat_map(|spd| match &spd.inner {
            ModelBlockKind::Column(symbols) => symbols.iter().collect(),
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Primary(blocks) => blocks
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

pub struct PlainOldObjectBlock<'src> {
    /// The symbol for the POO name, e.g. `MyPoo` in `poo MyPoo { ... }`
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,
}

pub struct D1BindingBlock<'src> {
    pub bindings: Vec<Symbol<'src>>,
}

pub struct VarsBlock<'src> {
    pub vars: Vec<Symbol<'src>>,
}

pub struct KvBindingTemplate<'src> {
    /// The symbol naming the field
    ///
    /// The [CidlType] of this symbol represents the return type of the binding field.
    pub symbol: Symbol<'src>,

    /// The parameters required to construct a key for this field.
    pub params: Vec<Symbol<'src>>,

    /// The key format string (e.g. `"metadata/{id}"`)
    pub key_format: &'src str,
}

pub struct KvBindingBlock<'src> {
    /// The binding name, e.g. `UserMetadata`.
    pub symbol: Symbol<'src>,

    pub templates: Vec<Spd<KvBindingTemplate<'src>>>,
}

pub struct R2BindingTemplate<'src> {
    /// The symbol naming the field
    pub symbol: Symbol<'src>,

    /// The parameters required to construct a key for this field.
    pub params: Vec<Symbol<'src>>,

    /// The key format string (e.g. `"key/{id}"`)
    pub key_format: &'src str,

    /// If true, the field returns a `Paginated<R2Object>``
    pub is_paginated: bool,
}

pub struct R2BindingBlock<'src> {
    /// The binding name, e.g. `UserAvatars`.
    pub symbol: Symbol<'src>,

    pub templates: Vec<Spd<R2BindingTemplate<'src>>>,
}

pub struct DurableShardBlock<'src> {
    pub fields: Vec<Symbol<'src>>,
}

pub struct DurableBindingBlock<'src> {
    /// The binding name, e.g. `LeaderboardDo`.
    pub symbol: Symbol<'src>,

    pub shard_blocks: Vec<Spd<DurableShardBlock<'src>>>,

    /// Identical in shape to [KvBindingTemplate] since DO storage
    /// mirrors KV semantics.
    pub templates: Vec<Spd<KvBindingTemplate<'src>>>,
}

pub struct InjectBlock<'src> {
    pub symbols: Vec<Symbol<'src>>,
}

#[allow(clippy::large_enum_variant)]
pub enum AstBlockKind<'src> {
    Api(ApiBlock<'src>),
    DataSource(DataSourceBlock<'src>),
    Model(ModelBlock<'src>),
    PlainOldObject(PlainOldObjectBlock<'src>),
    D1Binding(D1BindingBlock<'src>),
    KvBinding(KvBindingBlock<'src>),
    R2Binding(R2BindingBlock<'src>),
    DurableBinding(DurableBindingBlock<'src>),
    Vars(VarsBlock<'src>),
    Inject(InjectBlock<'src>),
}

/// The raw parsed structure of a Cloesce source file
/// before semantic analysis and transformation into the IDL.
#[derive(Default)]
pub struct Ast<'src> {
    pub blocks: Vec<Spd<AstBlockKind<'src>>>,
}

impl<'src> Ast<'src> {
    fn merge(&mut self, mut other: Ast<'src>) {
        self.blocks.append(&mut other.blocks);
    }
}
