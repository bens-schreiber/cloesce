use std::fmt::Display;

pub type Result<T> = std::result::Result<T, GeneratorError>;

#[derive(Debug)]
pub enum GeneratorPhase {
    EarlyAstValidation,
    D1,
    Workers,
}

#[derive(Debug)]
pub struct GeneratorError {
    pub phase: GeneratorPhase,
    pub kind: GeneratorErrorKind,
    pub description: String,
    pub suggestion: String,
    pub context: String,
}

impl Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Description: {} Suggestion: {} Context: {} Kind: {:?} Phase: {:?}",
            self.description, self.suggestion, self.context, self.kind, self.phase,
        )
    }
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
    InvalidSqlType,
    UnknownObject,
    UnexpectedVoid,
    NotYetSupported,
    InvalidMapping,
    InvalidApiDomain,
    MismatchedNavigationPropertyTypes,
    InvalidNavigationPropertyReference,
    CyclicalModelDependency,
    UnknownIncludeTreeReference,
    ExtraneousManyToManyReferences,
    MissingManyToManyReference,
}

impl GeneratorErrorKind {
    pub fn to_error(self) -> GeneratorError {
        let (description, suggestion, phase) = match self {
            GeneratorErrorKind::NullSqlType => (
                "Model attributes cannot be literally null",
                "Remove 'null' from your Model definition.",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::InvalidSqlType => (
                "Model attributes must be valid SQLite types: Integer, Real, Text, Blob",
                "Consider using a navigation property or creating another model.",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::UnknownObject => (
                "Objects must be decorated appropriately as a Model or PlainOldObject",
                "Consider using a decorator on the object.",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::UnexpectedVoid => (
                "Void cannot be an attribute or parameter, only a return type.",
                "Remove `void`",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::NotYetSupported => (
                "This feature will be supported in an upcoming Cloesce release.",
                "",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::InvalidMapping => (
                "CIDL is ill-formatted",
                "",
                GeneratorPhase::EarlyAstValidation,
            ),
            GeneratorErrorKind::InvalidApiDomain => (
                "Invalid or ill-formatted API domain",
                "API's must be of the form: http://domain.com/path/to/api",
                GeneratorPhase::Workers,
            ),
            GeneratorErrorKind::MismatchedNavigationPropertyTypes => (
                "Navigation property references must match attribute types",
                "TODO: a good suggestion here",
                GeneratorPhase::D1,
            ),
            GeneratorErrorKind::InvalidNavigationPropertyReference => (
                "Navigation property references must be to foreign keys or other navigation properties",
                "TODO: a good suggestion here",
                GeneratorPhase::D1,
            ),
            GeneratorErrorKind::CyclicalModelDependency => (
                "Model composition cannot be cyclical",
                "Allow a navigation property to be null",
                GeneratorPhase::D1,
            ),
            GeneratorErrorKind::UnknownIncludeTreeReference => (
                "Unknown reference in Include Tree definition",
                "",
                GeneratorPhase::D1,
            ),
            GeneratorErrorKind::ExtraneousManyToManyReferences => (
                "Only two navigation properties can reference a many to many table",
                "Remove a reference",
                GeneratorPhase::D1,
            ),
            GeneratorErrorKind::MissingManyToManyReference => (
                "Many to Many navigation properties must have a correlated reference on the adjacent model.",
                "TODO: a good indicator of where to add the nav prop",
                GeneratorPhase::D1,
            ),

            // Generic error, handeled seperately from all others
            GeneratorErrorKind::InvalidInputFile => ("", "", GeneratorPhase::EarlyAstValidation),
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
