use ast::{FileSpan, SymbolRef};

pub type BatchResult<T> = std::result::Result<T, Vec<CompilerErrorKind>>;

#[derive(Debug, Clone)]
pub enum CompilerErrorKind {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol {
        symbol: SymbolRef,
        first_span: FileSpan,
        second_span: FileSpan,
    },

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol {
        symbol: SymbolRef,
    },

    /// A wrangler environment was defined more than once within the project.
    MultipleWranglerEnvBlocks {
        first: SymbolRef,
        second: SymbolRef,
    },

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// An environment block has a binding that is inconsistent with the actual wrangler configuration.
    WranglerBindingInconsistentWithSpec {
        binding: SymbolRef,
    },

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding {
        model: SymbolRef,
    },

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding {
        model: SymbolRef,
        tag: SymbolRef,
    },

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey {
        model: SymbolRef,
    },

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType {
        column: SymbolRef,
    },

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey {
        column: SymbolRef,
    },

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferenceSelf {
        model: SymbolRef,
        foreign_key: SymbolRef,
    },

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        tag: SymbolRef,
        binding: SymbolRef,
    },

    ForeignKeyReferencesInvalidOrUnknownColumn {
        tag: SymbolRef,
        column: SymbolRef,
    },

    /// A foreign key can only be to a single adjacent model
    ForeignKeyReferencesMultipleModels {
        tag: SymbolRef,
        first_model: SymbolRef,
        second_model: SymbolRef,
    },

    /// A foreign key must reference a column of the same type (e.g. you can't reference an Integer column from a String column)
    ForeignKeyReferencesIncompatibleColumnType {
        tag: SymbolRef,
        column: SymbolRef,
        adj_column: SymbolRef,
    },

    /// All columns involved in a foreign key must be consistently nullable or non-nullable
    ForeignKeyInconsistentNullability {
        tag: SymbolRef,
        first_column: SymbolRef,
        second_column: SymbolRef,
    },

    /// A column in a D1 model can only participate in a single foreign key relationship
    ForeignKeyColumnAlreadyInForeignKey {
        tag: SymbolRef,
        column: SymbolRef,
    },

    NavigationPropertyReferencesInvalidOrUnknownField {
        tag: SymbolRef,
        field: SymbolRef,
    },

    NavigationPropertyReferencesSelf {
        model: SymbolRef,
        tag: SymbolRef,
    },

    NavigationPropertyReferencesDifferentDatabase {
        tag: SymbolRef,
        binding: SymbolRef,
    },

    /// A field in a D1 model can only participate in a single navigation property
    NavigationPropertyFieldAlreadyInNavigationProperty {
        tag: SymbolRef,
        field: SymbolRef,
    },

    /// A many-to-many navigation property requires exactly one reciprocal M2M nav on the adjacent model, but none was found.
    NavigationPropertyMissingReciprocalM2M {
        tag: SymbolRef,
    },

    /// A many-to-many navigation property found multiple reciprocal M2M navs on the adjacent model.
    NavigationPropertyAmbiguousM2M {
        tag: SymbolRef,
        first_m2m_nav: SymbolRef,
        second_m2m_nav: SymbolRef,
    },

    UniqueConstraintReferencesInvalidOrUnknownField {
        tag: SymbolRef,
        field: SymbolRef,
    },

    CyclicalRelationship {
        cycle: Vec<SymbolRef>,
    },

    /// A KV tag references an env binding that is not a KV namespace
    KvInvalidBinding {
        tag: SymbolRef,
        binding: SymbolRef,
    },

    /// An R2 tag references an env binding that is not an R2 bucket
    R2InvalidBinding {
        tag: SymbolRef,
        binding: SymbolRef,
    },

    /// A KV/R2 key format string references a variable that is not a field or key param on the model
    KvR2UnknownKeyVariable {
        tag: SymbolRef,
        variable: String,
    },

    /// A KV/R2 key format string has invalid syntax (e.g. nested or unclosed braces)
    KvR2InvalidKeyFormat {
        tag: SymbolRef,
        reason: String,
    },

    /// A KV/R2 tag references a field that does not exist on the model
    KvR2InvalidField {
        tag: SymbolRef,
        field: SymbolRef,
    },

    /// A Kv/R2 key param must be of type String
    KvR2InvalidKeyParam {
        tag: SymbolRef,
        field: SymbolRef,
    },

    PlainOldObjectInvalidFieldType {
        field: SymbolRef,
    },

    /// A service field must be of type Inject or another Service.
    ServiceInvalidFieldType {
        field: SymbolRef,
    },

    /// An API block references a model that does not exist.
    ApiUnknownModelReference {
        api: SymbolRef,
    },

    /// A non-static API method has a data source but the method is marked static.
    ApiStaticMethodWithDataSource {
        method: SymbolRef,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: SymbolRef,
        data_source: SymbolRef,
    },

    /// An API method has an invalid return type.
    ApiInvalidReturn {
        method: SymbolRef,
    },

    /// An API method has an invalid parameter.
    ApiInvalidParam {
        method: SymbolRef,
        param: SymbolRef,
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
