pub type Result<T> = std::result::Result<T, GeneratorError>;

#[derive(Debug)]
pub enum GeneratorPhase {
    External,
    ModelAnalysis,
    WranglerAnalysis,
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
    UnexpectedVoid,
    UnexpectedInject,
    NotYetSupported,
    InvalidMapping,
    MissingPrimaryKey,
    MismatchedForeignKeyTypes,
    MismatchedNavigationPropertyTypes,
    InvalidNavigationPropertyReference,
    CyclicalDependency,
    UnknownIncludeTreeReference,
    ExtraneousManyToManyReferences,
    MissingManyToManyReference,
    MissingWranglerEnv,
    MissingWranglerVariable,
    MissingWranglerD1Binding,
    MissingWranglerKVNamespace,
    InconsistentWranglerBinding,
    InvalidStream,
    InvalidModelReference,
    InvalidKeyFormat,
    UnknownKeyReference,
    UnsupportedCrudOperation,
}

impl GeneratorErrorKind {
    pub fn to_error(self) -> GeneratorError {
        let (description, suggestion, phase) = match self {
            /* ---- MODELS ---- */
            GeneratorErrorKind::NullSqlType => (
                "Model attributes cannot be literally null",
                "Remove 'null' from your Model definition.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::NullPrimaryKey => (
                "Primary keys cannot be nullable",
                "Remove 'null' from the primary key definition",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidSqlType => (
                "Model attributes must be valid SQLite types: Integer, Real, Text",
                "Consider using a navigation property or creating another model.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnknownObject => (
                "Found a reference to an unknown object.",
                "Consider marking the object as a Model or injected dependency.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnexpectedVoid => (
                "Void cannot be an attribute or parameter, only a return type.",
                "Remove `void`",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnexpectedInject => (
                "Attributes and return types cannot be injected values.",
                "Remove the value.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::NotYetSupported => (
                "This feature will be supported in an upcoming Cloesce release.",
                "",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidMapping => {
                ("CIDL is ill-formatted", "", GeneratorPhase::ModelAnalysis)
            }
            GeneratorErrorKind::MissingPrimaryKey => (
                "Models which have columns defined must have a primary key.",
                "Add a primary key to the Model definition.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::MismatchedForeignKeyTypes => (
                "Mismatched foreign keys",
                "Foreign keys must be the same type as their reference",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::MismatchedNavigationPropertyTypes => (
                "Navigation property references must match attribute types",
                "TODO: a good suggestion here",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidNavigationPropertyReference => (
                "Navigation property references must be to foreign keys or other navigation properties",
                "TODO: a good suggestion here",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::CyclicalDependency => (
                "Model and Service composition cannot be cyclical",
                "In Models, allow a navigation property to be null. In Services prefer direct dependency injection.)",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnknownIncludeTreeReference => (
                "Unknown reference in Include Tree definition",
                "",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::ExtraneousManyToManyReferences => (
                "Only two navigation properties can reference a many to many table",
                "Remove a reference",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::MissingManyToManyReference => (
                "Many to Many navigation properties must have a correlated reference on the adjacent model.",
                "TODO: a good indicator of where to add the nav prop",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidStream => (
                "Streams cannot be nullable, apart of an object or in an array. In a method, they must be the only parameter.",
                "Use a `Blob` type",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidModelReference => (
                "Unknown or invalid Model reference.",
                "References to a Model must be a defined matching type.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::InvalidKeyFormat => (
                "KV Model keys must be formatted correctly.",
                "Ensure your key format wraps dynamic segments in curly braces, e.g., `user:{userId}:settings`.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnknownKeyReference => (
                "KeyFormat references an unknown attribute.",
                "Ensure all dynamic segments in the key format correspond to defined attributes in the Model.",
                GeneratorPhase::ModelAnalysis,
            ),
            GeneratorErrorKind::UnsupportedCrudOperation => (
                "The specified CRUD operation is not supported for this Model type.",
                "Refer to the documentation for supported operations on this Model type.",
                GeneratorPhase::ModelAnalysis,
            ),

            /* ---- WRANGLER ---- */
            GeneratorErrorKind::InconsistentWranglerBinding => (
                "Wrangler config definitions must be consistent with the WranglerEnv definition",
                "Change your WranglerEnv's bindings to match the Wrangler file",
                GeneratorPhase::WranglerAnalysis,
            ),
            GeneratorErrorKind::MissingWranglerEnv => (
                "A WranglerEnv definition is required to use Models.",
                "Add a WranglerEnv definition to your backend code.",
                GeneratorPhase::WranglerAnalysis,
            ),
            GeneratorErrorKind::MissingWranglerVariable => (
                "A Wrangler config variable binding is required to define a variable in the WranglerEnv",
                "Add the variable binding to your Wrangler configuration.",
                GeneratorPhase::WranglerAnalysis,
            ),
            GeneratorErrorKind::MissingWranglerD1Binding => (
                "A Wrangler config D1 database binding is required to define a D1 Model.",
                "Add the D1 database binding to your Wrangler configuration.",
                GeneratorPhase::WranglerAnalysis,
            ),
            GeneratorErrorKind::MissingWranglerKVNamespace => (
                "A Wrangler config KV namespace binding is required to define a KV Model.",
                "Add the KV namespace binding to your Wrangler configuration.",
                GeneratorPhase::WranglerAnalysis,
            ),

            /* ---- EXTERNAL ---- */
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
