pub type Result<T> = std::result::Result<T, CompilerErrorKind>;
pub type BatchResult<T> = std::result::Result<T, Vec<CompilerErrorKind>>;

#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum CompilerErrorKind {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol,

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol,

    /// A wrangler environment was defined more than once within the project.
    MultipleWranglerEnvBlocks,

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// An environment block has a binding that is inconsistent with the actual wrangler configuration.
    WranglerBindingInconsistentWithSpec,

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding,

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding,

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey,

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType,

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey,

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferenceSelf,

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase,

    ForeignKeyReferencesInvalidOrUnknownColumn,

    /// A foreign key must reference a column of the same type (e.g. you can't reference an Integer column from a String column)
    ForeignKeyReferencesIncompatibleColumnType,

    /// All columns involved in a foreign key must be consistently nullable or non-nullable
    ForeignKeyInconsistentNullability,

    ForeignKeyReferencesNonD1Model,

    /// A column in a D1 model can only participate in a single foreign key relationship
    ForeignKeyColumnAlreadyInForeignKey,

    NavigationPropertyReferencesInvalidOrUnknownColumn,

    NavigationPropertyReferencesSelf,
    // UnknownBinding,
    // MultipleWranglerEnvs,
    // NullSqlType,
    // NullPrimaryKey,
    // InvalidSqlType,
    // UnknownObject,
    // UnexpectedVoid,
    // UnexpectedInject,
    // NotYetSupported,
    // InvalidMapping,
    // MissingPrimaryKey,
    // MismatchedForeignKeyTypes,
    // MismatchedNavigationPropertyTypes,
    // InvalidNavigationPropertyReference,
    // CyclicalDependency,
    // UnknownIncludeTreeReference,
    // UnknownDataSourceReference,
    // InvalidDataSourceReference,
    // ExtraneousManyToManyReferences,
    // MissingManyToManyReference,
    // MissingWranglerEnv,
    // InconsistentWranglerBinding,
    // InvalidStream,
    // InvalidModelReference,
    // InvalidKeyFormat,
    // UnknownKeyReference,
    // UnsupportedCrudOperation,
    // UnknownCompositeKeyReference,
    // InvalidCompositeKey,
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

/// A fatal error that immediately returns from the current function with Err
#[macro_export]
macro_rules! bail {
    ($kind:expr) => {
        return Err($kind)
    };
}

/// If the condition is false, pushes a fatal error into the sink and returns Err immediately
#[macro_export]
macro_rules! ensure_bail {
    ($cond:expr, $sink:expr, $kind:expr) => {
        if !$cond {
            return Err($sink.bail($kind));
        }
    };
}

/// If the condition is false, pushes an error into the sink but continues execution
#[macro_export]
macro_rules! ensure_sink {
    ($cond:expr, $sink:expr, $kind:expr) => {
        if !$cond {
            $sink.push($kind)
        }
    };
}
