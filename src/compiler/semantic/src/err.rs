use std::borrow::Cow;

use frontend::Span;

use crate::Symbol;

pub type BatchResult<'src, 'p, T> = std::result::Result<T, Vec<SemanticError<'src, 'p>>>;

#[derive(Debug, Clone)]
pub enum SemanticError<'src, 'p> {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol {
        first: &'p Symbol<'src>,
        second: &'p Symbol<'src>,
    },

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol {
        span: Span,
        name: &'src str,
    },

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding {
        model: &'p Symbol<'src>,
    },

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding {
        model: &'p Symbol<'src>,
        binding: &'src str,
    },

    D1ModelMultipleD1Bindings {
        model: &'p Symbol<'src>,
        bindings: Vec<&'src str>,
    },

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey {
        model: &'p Symbol<'src>,
    },

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType {
        column: &'p Symbol<'src>,
    },

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey {
        column: &'p Symbol<'src>,
    },

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferencesSelf {
        model: &'p Symbol<'src>,
        foreign_key: Span,
    },

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        span: Span,
        binding: &'src str,
    },

    // A foreign key references a field that is not a valid SQLite type
    ForeignKeyInvalidColumnType {
        span: Span,
        field: &'p Symbol<'src>,
    },

    /// A foreign key has a different number of adj references than local fields
    ForeignKeyInconsistentFieldAdj {
        span: Span,
        adj_count: usize,
        field_count: usize,
    },

    /// A foreign key or navigation adj list references more than one model name
    InconsistentModelAdjacency {
        span: Span,
        first_model: &'src str,
        second_model: &'src str,
    },

    NavigationReferencesDifferentDatabase {
        span: Span,
        binding: &'src str,
    },

    /// A many-to-many navigation property requires exactly one reciprocal M2M nav on the adjacent model, but none was found.
    NavigationMissingReciprocalM2M {
        span: Span,
    },

    /// A many-to-many navigation property found multiple reciprocal M2M navs on the adjacent model.
    NavigationAmbiguousM2M {
        span: Span,
    },

    CyclicalRelationship {
        cycle: Vec<&'src str>,
    },

    /// A KV tag references an env binding that is not a KV namespace
    KvInvalidBinding {
        span: Span,
        binding: &'src str,
    },

    /// An R2 tag references an env binding that is not an R2 bucket
    R2InvalidBinding {
        span: Span,
        binding: &'src str,
    },

    /// A KV/R2 key format string references a variable that is not a field or key param on the model
    KvR2UnknownKeyVariable {
        span: Span,
        variable: &'src str,
    },

    /// A KV/R2 key format string has invalid syntax (e.g. nested or unclosed braces)
    KvR2InvalidKeyFormat {
        span: Span,
        reason: String,
    },

    /// A KV/R2 tag references a field that does not exist on the model
    KvR2InvalidField {
        span: Span,
        field: &'src str,
    },

    /// A Kv/R2 key param must be of type &'src str
    KvR2InvalidKeyParam {
        span: Span,
        field: &'p Symbol<'src>,
    },

    PlainOldObjectInvalidFieldType {
        field: &'p Symbol<'src>,
    },

    /// A data source references a model that does not exist or is not a model.
    DataSourceUnknownModelReference {
        source: &'p Symbol<'src>,
    },

    /// A data source include tree references a name that is not a navigation property, KV, or R2 on the model.
    DataSourceInvalidIncludeTreeReference {
        source: &'p Symbol<'src>,
        model: &'src str,
        name: Cow<'src, str>,
    },

    /// A data source method parameter is not a valid SQLite type.
    DataSourceInvalidMethodParam {
        source: &'p Symbol<'src>,
        param: &'p Symbol<'src>,
    },

    /// A data source method SQL references a `$name` placeholder that does not match any parameter
    /// (and is not the reserved `$include` placeholder).
    DataSourceUnknownSqlParam {
        source: &'p Symbol<'src>,
        name: String,
    },

    /// A model has a CRUD operation that is not supported for its backing store.
    UnsupportedCrudOperation {
        model: &'p Symbol<'src>,
    },

    /// An API block references a model that does not exist.
    ApiUnknownNamespaceReference {
        api: &'p Symbol<'src>,
    },

    /// A non-static API method has a data source but the method is marked static.
    ApiStaticMethodWithDataSource {
        method: &'p Symbol<'src>,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: &'p Symbol<'src>,
        data_source: &'src str,
    },

    /// An API method has an invalid return type.
    ApiInvalidReturn {
        method: &'p Symbol<'src>,
    },

    /// An API method has an invalid parameter.
    ApiInvalidParam {
        method: &'p Symbol<'src>,
        param: &'p Symbol<'src>,
    },

    /// An API method uses a reserved name (e.g. $get, $list, $save)
    ApiReservedMethod {
        method: &'p Symbol<'src>,
    },
}

#[derive(Debug, Default)]
pub struct ErrorSink<'src, 'p> {
    pub errors: Vec<SemanticError<'src, 'p>>,
}

impl<'src, 'p> ErrorSink<'src, 'p> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: SemanticError<'src, 'p>) {
        self.errors.push(kind);
    }

    pub fn drain(&mut self) -> Vec<SemanticError<'src, 'p>> {
        std::mem::take(&mut self.errors)
    }

    pub fn extend(&mut self, other: Vec<SemanticError<'src, 'p>>) {
        self.errors.extend(other);
    }

    /// Returns Err if any errors were accumulated
    pub fn finish(self) -> std::result::Result<(), Vec<SemanticError<'src, 'p>>> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }
}

/// If the condition is false, pushes an error into the sink but continues execution
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $sink:expr, $kind:expr) => {
        if !$cond {
            $sink.push($kind)
        }
    };
}
