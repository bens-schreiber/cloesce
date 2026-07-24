//! Responsible for lexing and parsing Cloesce source files into an [Ast], a direct representation of the
//! parsed source code. Additionally, houses the formatter for Cloesce source files.
//!
//! The AST is a direct representation of the parsed source code, with the only transformations being
//! the streaming of comments into a seperate "comment map" channel during lexing.

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
    One => "one",
    Many => "many",
    Foreign => "foreign",
    Primary => "primary",
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
    Var => "var",
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

    // Tag
    Crud => "crud",
    Internal => "internal",
    Instance => "instance",
    Header => "header",


    // Validator tag (numeric)
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
}

/// Formats a [CidlType] into its string representation via [Keyword::as_str]
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
        CidlType::KvObject(inner) => {
            format!("{}<{}>", Keyword::GKvObject.as_str(), fmt_cidl_type(inner))
        }
        CidlType::Void => panic!("void type should not appear in CidlType formatting"),
    }
}

/// A wrapper around some spanned value
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

/// Any literal argument passed in the Cloesce source.
///
/// Each has a [lexer::Token] variant that corresponds to it.
#[derive(Debug, Clone, PartialEq)]
pub enum ArgumentLiteral<'src> {
    /// [lexer::Token::IntLit]
    Int(&'src str),

    /// [lexer::Token::RealLit]
    Real(&'src str),

    /// [lexer::Token::StringLit]
    Str(&'src str),

    /// [lexer::Token::RegexLit]
    Regex(&'src str),
}

#[derive(Debug, Clone)]
pub struct InjectInitializer<'src> {
    pub target: Symbol<'src>,
    pub arg: Vec<Symbol<'src>>,
}

#[derive(Debug, Clone)]
pub enum InjectEntry<'src> {
    /// A flat binding that requires no initializers
    Binding(Symbol<'src>),

    /// A binding that requires initializers, e.g. `Durable::{t1(arg1), t2(arg2)}`
    ///
    /// NOTE: Currently used in only Durable Object Context injection
    Context {
        /// The bound target, e.g. `Durable` in `Durable::t1(arg1)`
        symbol: Symbol<'src>,

        /// The constructor initializers, e.g. `t1(arg1)`
        initializers: Vec<InjectInitializer<'src>>,
    },
}

/// Any `[tag]` attached to a symbol
#[derive(Debug, Clone)]
pub enum Tag<'src> {
    /// [Keyword::Internal]
    Internal,

    /// [Keyword::Header]
    Header,

    /// [Keyword::Instance]
    Instance,

    /// [Keyword::Crud]
    Crud { kinds: Vec<Spd<CrudKind>> },

    /// `[Keyword argument]` where [Keyword] _should_ be a validator keyword (e.g [Keyword::LessThan])
    Validator {
        name: Keyword,
        argument: ArgumentLiteral<'src>,
    },
}

/// A symbol representing a named entity in the source code.
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

/// [Keyword::Api]
pub struct ApiBlock<'src> {
    /// The symbol for the API's name, e.g. `ApiName` in `api ApiName { ... }`
    pub symbol: Symbol<'src>,

    pub methods: Vec<Spd<ApiBlockMethod<'src>>>,
}

pub struct MethodInjectBlock<'src> {
    pub entries: Vec<Spd<InjectEntry<'src>>>,
}

pub struct MethodSourceBlock<'src> {
    pub source: Symbol<'src>,
}

pub struct ApiBlockMethod<'src> {
    /// The symbol for the method name, e.g. `getUser` in `post getUser(...) { ... }`
    ///
    /// The [CidlType] of this symbol represents the return type of the API method.
    pub symbol: Symbol<'src>,

    pub http_verb: HttpVerb,
    pub parameters: Vec<Symbol<'src>>,
    pub injects: Vec<Spd<MethodInjectBlock<'src>>>,
    pub sources: Vec<Spd<MethodSourceBlock<'src>>>,
}

pub struct DataSourceBlockMethod<'src> {
    pub method: Symbol<'src>,
    pub parameters: Vec<Symbol<'src>>,
    pub injects: Vec<Spd<MethodInjectBlock<'src>>>,
    pub sources: Vec<Spd<MethodSourceBlock<'src>>>,
}

pub struct ParsedIncludeTree<'src>(
    // IndexMap is used to preserve declaration order
    pub IndexMap<Symbol<'src>, ParsedIncludeTree<'src>>,
);

/// [Keyword::Source]
pub struct DataSourceBlock<'src> {
    /// The symbol for the data source itself, e.g. `SourceName`
    pub symbol: Symbol<'src>,

    /// The symbol for the model this data source is for, e.g. `for ModelName`
    pub model: Symbol<'src>,

    /// [Keyword::Include]
    pub tree: Option<ParsedIncludeTree<'src>>,

    /// [Keyword::List]
    pub list: Option<Spd<DataSourceBlockMethod<'src>>>,

    /// [Keyword::Get]
    pub get: Option<Spd<DataSourceBlockMethod<'src>>>,

    /// [Keyword::Save]
    pub save: Option<Spd<DataSourceBlockMethod<'src>>>,
}

/// The explicit cardinality of a relationship, as stated by the `one` or `many`
/// keyword that opens a [NavigationBlock].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    /// [Keyword::One]
    One,

    /// [Keyword::Many]
    Many,
}

pub struct NavigationKey<'src> {
    /// The discriminator field on the target model (its `route`/`primary` field).
    pub target: Symbol<'src>,

    /// The local field on the current model that supplies the target's discriminator,
    /// if any. If `None`, the relationship is discriminator-less.
    pub local: Option<Symbol<'src>>,
}

pub struct NavigationBlock<'src> {
    /// `One | Many`, from the opening keyword.
    pub cardinality: Cardinality,

    /// The target model, e.g. `Model` in `one Model::... { field }`.
    pub model: Symbol<'src>,

    /// The discriminator key pairs.
    pub keys: Vec<NavigationKey<'src>>,

    /// The result field name declared in `{ ... }`.
    pub field: Spd<Symbol<'src>>,
}

/// [Keyword::Foreign]
pub struct ForeignBlock<'src> {
    /// The referenced model, e.g. `AdjModel` in `foreign AdjModel::field { ... }`.
    pub model: Symbol<'src>,

    /// The referenced fields on `model`.
    pub targets: Vec<Symbol<'src>>,

    // { currentModelField1, currentModelField2, ... }
    pub fields: Vec<Symbol<'src>>,

    pub is_optional: bool,
}

pub struct KvFieldArgument<'src> {
    /// A reference to either a binding template of the KV Namespace
    /// or a Durable Object shard iff the KV Field references a DO binding.
    pub target: Symbol<'src>,

    /// - If 1 => `target(local)`
    /// - If >1 => `target(local1, local2, ...)`
    pub local: Vec<Symbol<'src>>,
}

pub struct KvFieldBlock<'src> {
    /// The KV binding name (e.g. `UserMetadata`)
    pub binding: Symbol<'src>,

    /// The model fields to be passed as arguments to the binding's field
    pub args: Vec<KvFieldArgument<'src>>,

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
    Primary(Vec<Spd<SqlBlockKind<'src>>>),
    Unique(Vec<Symbol<'src>>),
    Navigation(NavigationBlock<'src>),
    Kv(KvFieldBlock<'src>),
    R2(R2FieldBlock<'src>),
    Route(Vec<Symbol<'src>>),
}

impl<'src> ModelBlockKind<'src> {
    /// Returns the field-declaration symbols introduced by this block.
    pub fn symbols(&self) -> Vec<&Symbol<'src>> {
        match self {
            ModelBlockKind::Column(symbols) => symbols.iter().collect(),
            ModelBlockKind::Route(symbols) => symbols.iter().collect(),
            ModelBlockKind::Foreign(foreign_block) => foreign_block.fields.iter().collect(),
            ModelBlockKind::Navigation(navigation_block) => vec![&navigation_block.field.inner],
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

/// [Keyword::Model]
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

/// [Keyword::Poo]
pub struct PlainOldObjectBlock<'src> {
    /// The symbol for the POO name, e.g. `MyPoo` in `poo MyPoo { ... }`
    pub symbol: Symbol<'src>,

    pub fields: Vec<Symbol<'src>>,
}

/// [Keyword::D1]
pub struct D1BindingBlock<'src> {
    pub bindings: Vec<Symbol<'src>>,
}

/// [Keyword::Var]
pub struct VarBlock<'src> {
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

/// [Keyword::Kv]
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
}

/// [Keyword::R2]
pub struct R2BindingBlock<'src> {
    /// The binding name, e.g. `UserAvatars`.
    pub symbol: Symbol<'src>,

    pub templates: Vec<Spd<R2BindingTemplate<'src>>>,
}

/// [Keyword::Shard]
pub struct DurableShardBlock<'src> {
    pub fields: Vec<Symbol<'src>>,
}

/// [Keyword::Durable]
pub struct DurableBindingBlock<'src> {
    /// The binding name, e.g. `LeaderboardDo`.
    pub symbol: Symbol<'src>,

    pub shard_blocks: Vec<Spd<DurableShardBlock<'src>>>,

    /// Identical in shape to [KvBindingTemplate] since DO storage
    /// mirrors KV semantics.
    pub templates: Vec<Spd<KvBindingTemplate<'src>>>,
}

/// [Keyword::Inject]
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
    Var(VarBlock<'src>),
    Inject(InjectBlock<'src>),
}

/// The raw parsed structure of a Cloesce source file
/// before semantic analysis and transformation into the IDL.
#[derive(Default)]
pub struct Ast<'src> {
    pub blocks: Vec<Spd<AstBlockKind<'src>>>,
}

impl<'src> Ast<'src> {
    /// Merges another [Ast] into this one by appending all of the blocks from the other into this one's blocks.
    ///
    /// NOTE: Cloesce does not have any import constructs, so this naive "merge blocks together" strategy
    /// is sufficient for creating an AST representing the entirety of a multi-file project.
    fn merge(&mut self, mut other: Ast<'src>) {
        self.blocks.append(&mut other.blocks);
    }
}
