use frontend::parser::ParseId;

use crate::FileSpan;

pub type BatchResult<T> = std::result::Result<T, Vec<CompilerErrorKind>>;

#[derive(Debug, Clone)]
pub enum CompilerErrorKind {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol {
        symbol: ParseId,
        first_span: FileSpan,
        second_span: FileSpan,
    },

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol {
        symbol: ParseId,
    },

    /// A wrangler environment was defined more than once within the project.
    MultipleWranglerEnvBlocks {
        first: ParseId,
        second: ParseId,
    },

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding {
        model: ParseId,
    },

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding {
        model: ParseId,
        tag: ParseId,
    },

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey {
        model: ParseId,
    },

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType {
        column: ParseId,
    },

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey {
        column: ParseId,
    },

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferenceSelf {
        model: ParseId,
        foreign_key: ParseId,
    },

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        tag: ParseId,
        binding: ParseId,
    },

    ForeignKeyReferencesInvalidOrUnknownColumn {
        tag: ParseId,
        column: ParseId,
    },

    /// A foreign key can only be to a single adjacent model
    ForeignKeyReferencesMultipleModels {
        tag: ParseId,
        first_model: ParseId,
        second_model: ParseId,
    },

    /// A foreign key must reference a column of the same type (e.g. you can't reference an Integer column from a String column)
    ForeignKeyReferencesIncompatibleColumnType {
        tag: ParseId,
        column: ParseId,
        adj_column: ParseId,
    },

    /// All columns involved in a foreign key must be consistently nullable or non-nullable
    ForeignKeyInconsistentNullability {
        tag: ParseId,
        first_column: ParseId,
        second_column: ParseId,
    },

    /// A column in a D1 model can only participate in a single foreign key relationship
    ForeignKeyColumnAlreadyInForeignKey {
        tag: ParseId,
        column: ParseId,
    },

    NavigationPropertyReferencesInvalidOrUnknownField {
        tag: ParseId,
        field: ParseId,
    },

    NavigationPropertyReferencesSelf {
        model: ParseId,
        tag: ParseId,
    },

    NavigationPropertyReferencesDifferentDatabase {
        tag: ParseId,
        binding: ParseId,
    },

    /// A field in a D1 model can only participate in a single navigation property
    NavigationPropertyFieldAlreadyInNavigationProperty {
        tag: ParseId,
        field: ParseId,
    },

    /// A many-to-many navigation property requires exactly one reciprocal M2M nav on the adjacent model, but none was found.
    NavigationPropertyMissingReciprocalM2M {
        tag: ParseId,
    },

    /// A many-to-many navigation property found multiple reciprocal M2M navs on the adjacent model.
    NavigationPropertyAmbiguousM2M {
        tag: ParseId,
        first_m2m_nav: ParseId,
        second_m2m_nav: ParseId,
    },

    UniqueConstraintReferencesInvalidOrUnknownField {
        tag: ParseId,
        field: ParseId,
    },

    CyclicalRelationship {
        cycle: Vec<ParseId>,
    },

    /// A KV tag references an env binding that is not a KV namespace
    KvInvalidBinding {
        tag: ParseId,
        binding: ParseId,
    },

    /// An R2 tag references an env binding that is not an R2 bucket
    R2InvalidBinding {
        tag: ParseId,
        binding: ParseId,
    },

    /// A KV/R2 key format string references a variable that is not a field or key param on the model
    KvR2UnknownKeyVariable {
        tag: ParseId,
        variable: String,
    },

    /// A KV/R2 key format string has invalid syntax (e.g. nested or unclosed braces)
    KvR2InvalidKeyFormat {
        tag: ParseId,
        reason: String,
    },

    /// A KV/R2 tag references a field that does not exist on the model
    KvR2InvalidField {
        tag: ParseId,
        field: ParseId,
    },

    /// A Kv/R2 key param must be of type String
    KvR2InvalidKeyParam {
        tag: ParseId,
        field: ParseId,
    },

    PlainOldObjectInvalidFieldType {
        field: ParseId,
    },

    /// A service field must be of type Inject or another Service.
    ServiceInvalidFieldType {
        field: ParseId,
    },

    /// A data source references a model that does not exist or is not a model.
    DataSourceUnknownModelReference {
        source: ParseId,
    },

    /// A data source include tree references a name that is not a navigation property, KV, or R2 on the model.
    DataSourceInvalidIncludeTreeReference {
        source: ParseId,
        model: ParseId,
        name: String,
    },

    /// A data source method parameter is not a valid SQLite type.
    DataSourceInvalidMethodParam {
        source: ParseId,
        param: ParseId,
    },

    /// A model has a CRUD operation that is not supported for its backing store.
    UnsupportedCrudOperation {
        model: ParseId,
    },

    /// An API block references a model that does not exist.
    ApiUnknownModelReference {
        api: ParseId,
    },

    /// A non-static API method has a data source but the method is marked static.
    ApiStaticMethodWithDataSource {
        method: ParseId,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: ParseId,
        data_source: ParseId,
    },

    /// An API method has an invalid return type.
    ApiInvalidReturn {
        method: ParseId,
    },

    /// An API method has an invalid parameter.
    ApiInvalidParam {
        method: ParseId,
        param: ParseId,
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
