pub type Result<T> = std::result::Result<T, GeneratorError>;

#[derive(Debug)]
pub enum GeneratorPhase {
    External,
    SemanticAnalysis,
    Wrangler,
}

#[derive(Debug)]
pub struct GeneratorError {
    pub phase: GeneratorPhase,
    pub kind: GeneratorErrorKind,
    pub description: String,
    pub suggestion: String,
    pub context: String,
}

impl GeneratorError {
    fn new(
        kind: GeneratorErrorKind,
        phase: GeneratorPhase,
        description: String,
        suggestion: String,
    ) -> Self {
        Self {
            kind,
            phase,
            description,
            suggestion,
            context: Default::default(),
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        let ctx = ctx.into();
        if self.context.is_empty() {
            self.context = ctx;
        } else {
            // Prepend new context
            self.context = format!("{ctx}: {}", self.context);
        }
        self
    }
}

#[derive(Debug)]
pub enum GeneratorErrorKind {
    InvalidInputFile,
    NullSqlType,
    NullPrimaryKey,
    InvalidSqlType,
    UnknownObject,
    UnknownDataSourceReference,
    UnexpectedVoid,
    UnexpectedInject,
    MissingOrExtraneousDataSource,
    NotYetSupported,
    InvalidMapping,
    MismatchedForeignKeyTypes,
    MismatchedNavigationPropertyTypes,
    InvalidNavigationPropertyReference,
    CyclicalDependency,
    UnknownIncludeTreeReference,
    ExtraneousManyToManyReferences,
    MissingManyToManyReference,
    InconsistentWranglerBinding,
    InvalidStream,
}

impl GeneratorErrorKind {
    pub fn to_error(self) -> GeneratorError {
        let (description, suggestion, phase) = match self {
            GeneratorErrorKind::NullSqlType => (
                "Model attributes cannot be literally null",
                "Remove 'null' from your Model definition.",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::NullPrimaryKey => (
                "Primary keys cannot be nullable",
                "Remove 'null' from the primary key definition",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::InvalidSqlType => (
                "Model attributes must be valid SQLite types: Integer, Real, Text",
                "Consider using a navigation property or creating another model.",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::UnknownObject => (
                "Objects must be decorated appropriately as a Model, PlainOldObject, or Inject",
                "Consider using a decorator on the object.",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::UnknownDataSourceReference => (
                "Data sources must reference a model",
                "",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::UnexpectedVoid => (
                "Void cannot be an attribute or parameter, only a return type.",
                "Remove `void`",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::UnexpectedInject => (
                "Attributes and return types cannot be injected values.",
                "Remove the value.",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::MissingOrExtraneousDataSource => (
                "All instantiated methods must have one data source parameter.",
                "Add a data source parameter, or remove extras.",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::NotYetSupported => (
                "This feature will be supported in an upcoming Cloesce release.",
                "",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::InvalidMapping => {
                ("CIDL is ill-formatted", "", GeneratorPhase::SemanticAnalysis)
            }
            GeneratorErrorKind::MismatchedForeignKeyTypes => (
                "Mismatched foreign keys",
                "Foreign keys must be the same type as their reference",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::MismatchedNavigationPropertyTypes => (
                "Navigation property references must match attribute types",
                "TODO: a good suggestion here",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::InvalidNavigationPropertyReference => (
                "Navigation property references must be to foreign keys or other navigation properties",
                "TODO: a good suggestion here",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::CyclicalDependency => (
                "Model and Service composition cannot be cyclical",
                "In Models, allow a navigation property to be null. In Services prefer direct dependency injection.)",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::UnknownIncludeTreeReference => (
                "Unknown reference in Include Tree definition",
                "",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::ExtraneousManyToManyReferences => (
                "Only two navigation properties can reference a many to many table",
                "Remove a reference",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::MissingManyToManyReference => (
                "Many to Many navigation properties must have a correlated reference on the adjacent model.",
                "TODO: a good indicator of where to add the nav prop",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::InvalidStream => (
                "Streams cannot be nullable, apart of an object or in an array. In a method, they must be the only parameter.",
                "Use a `Blob` type",
                GeneratorPhase::SemanticAnalysis,
            ),
            GeneratorErrorKind::InconsistentWranglerBinding => (
                "Wrangler file definitions must be consistent with the WranglerEnv definition",
                "Change your WranglerEnv's bindings to match the Wrangler file",
                GeneratorPhase::Wrangler,
            ),

            // Generic error, handeled seperately from all others
            GeneratorErrorKind::InvalidInputFile => ("", "", GeneratorPhase::External),
        };

        GeneratorError::new(self, phase, description.into(), suggestion.into())
    }
}

#[macro_export]
macro_rules! fail {
    ($kind:expr) => {
        return Err($kind.to_error())
    };
    ($kind:expr, $($arg:tt)*) => {
        return Err($kind.to_error().with_context(format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $kind:expr) => {
        if !$cond {
            return Err($kind.to_error())
        }
    };
    ($cond:expr, $kind:expr, $($arg:tt)*) => {
        if !$cond {
            return Err($kind.to_error().with_context(format!($($arg)*)))
        }
    };
}
