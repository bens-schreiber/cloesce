use frontend::{D1Tag, FileSpan};

use crate::Symbol;

pub type BatchResult<T> = std::result::Result<T, Vec<CompilerErrorKind>>;

#[derive(Debug, Clone)]
pub enum CompilerErrorKind {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol {
        first: Symbol,
        second: Symbol,
    },

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol {
        span: FileSpan,
    },

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding {
        model: Symbol,
    },

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding {
        model: Symbol,
        tag: D1Tag,
    },

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey {
        model: Symbol,
    },

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType {
        column: Symbol,
    },

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey {
        column: Symbol,
    },

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferencesSelf {
        model: Symbol,
        foreign_key: FileSpan,
    },

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        tag: FileSpan,
        binding: String,
    },

    ForeignKeyReferencesInvalidOrUnknownColumn {
        tag: FileSpan,
        column: String,
    },

    /// A foreign key can only be to a single adjacent model
    ForeignKeyReferencesMultipleModels {
        tag: FileSpan,
        first_model: String,
        second_model: String,
    },

    /// A foreign key must reference a column of the same type (e.g. you can't reference an Integer column from a String column)
    ForeignKeyReferencesIncompatibleColumnType {
        tag: FileSpan,
        column: Symbol,
        adj_column: Symbol,
    },

    /// All columns involved in a foreign key must be consistently nullable or non-nullable
    ForeignKeyInconsistentNullability {
        tag: FileSpan,
        first_column: Symbol,
        second_column: Symbol,
    },

    /// A column in a D1 model can only participate in a single foreign key relationship
    ForeignKeyColumnAlreadyInForeignKey {
        tag: FileSpan,
        column: Symbol,
    },

    NavigationPropertyReferencesInvalidOrUnknownField {
        tag: FileSpan,
        field: String,
    },

    NavigationPropertyReferencesDifferentDatabase {
        tag: FileSpan,
        binding: String,
    },

    /// A field in a D1 model can only participate in a single navigation property
    NavigationPropertyFieldAlreadyInNavigationProperty {
        tag: FileSpan,
        field: Symbol,
    },

    /// A many-to-many navigation property requires exactly one reciprocal M2M nav on the adjacent model, but none was found.
    NavigationPropertyMissingReciprocalM2M {
        tag: FileSpan,
    },

    /// A many-to-many navigation property found multiple reciprocal M2M navs on the adjacent model.
    NavigationPropertyAmbiguousM2M {
        tag: FileSpan,
    },

    UniqueConstraintReferencesInvalidOrUnknownField {
        tag: FileSpan,
        field: String,
    },

    CyclicalRelationship {
        cycle: Vec<String>,
    },

    /// A KV tag references an env binding that is not a KV namespace
    KvInvalidBinding {
        tag: FileSpan,
        binding: String,
    },

    /// An R2 tag references an env binding that is not an R2 bucket
    R2InvalidBinding {
        tag: FileSpan,
        binding: String,
    },

    /// A KV/R2 key format string references a variable that is not a field or key param on the model
    KvR2UnknownKeyVariable {
        tag: FileSpan,
        variable: String,
    },

    /// A KV/R2 key format string has invalid syntax (e.g. nested or unclosed braces)
    KvR2InvalidKeyFormat {
        tag: FileSpan,
        reason: String,
    },

    /// A KV/R2 tag references a field that does not exist on the model
    KvR2InvalidField {
        tag: FileSpan,
        field: String,
    },

    /// A Kv/R2 key param must be of type String
    KvR2InvalidKeyParam {
        tag: FileSpan,
        field: Symbol,
    },

    PlainOldObjectInvalidFieldType {
        field: Symbol,
    },

    /// A service field must be of type Inject or another Service.
    ServiceInvalidFieldType {
        field: Symbol,
    },

    /// A data source references a model that does not exist or is not a model.
    DataSourceUnknownModelReference {
        source: Symbol,
    },

    /// A data source include tree references a name that is not a navigation property, KV, or R2 on the model.
    DataSourceInvalidIncludeTreeReference {
        source: Symbol,
        model: String,
        name: String,
    },

    /// A data source method parameter is not a valid SQLite type.
    DataSourceInvalidMethodParam {
        source: Symbol,
        param: Symbol,
    },

    /// A data source method SQL references a `$name` placeholder that does not match any parameter
    /// (and is not the reserved `$include` placeholder).
    DataSourceUnknownSqlParam {
        source: Symbol,
        name: String,
    },


    /// A model has a CRUD operation that is not supported for its backing store.
    UnsupportedCrudOperation {
        model: Symbol,
    },

    /// An API block references a model that does not exist.
    ApiUnknownNamespaceReference {
        api: Symbol,
    },

    /// A non-static API method has a data source but the method is marked static.
    ApiStaticMethodWithDataSource {
        method: Symbol,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: Symbol,
        data_source: String,
    },

    /// An API method has an invalid return type.
    ApiInvalidReturn {
        method: Symbol,
    },

    /// An API method has an invalid parameter.
    ApiInvalidParam {
        method: Symbol,
        param: Symbol,
    },

    /// An API method uses a reserved name (e.g. $get, $list, $save)
    ApiReservedMethod {
        method: Symbol,
    },
}

#[derive(Debug, Default)]
pub struct ErrorSink {
    pub errors: Vec<CompilerErrorKind>,
}

impl ErrorSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: CompilerErrorKind) {
        self.errors.push(kind);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn drain(&mut self) -> Vec<CompilerErrorKind> {
        std::mem::take(&mut self.errors)
    }

    pub fn extend(&mut self, other: Vec<CompilerErrorKind>) {
        self.errors.extend(other);
    }

    /// Push a fatal error, drain everything, and return as Err payload
    pub fn bail(&mut self, kind: CompilerErrorKind) -> Vec<CompilerErrorKind> {
        self.push(kind);
        self.drain()
    }

    /// Returns Err if any errors were accumulated
    pub fn finish(self) -> std::result::Result<(), Vec<CompilerErrorKind>> {
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
