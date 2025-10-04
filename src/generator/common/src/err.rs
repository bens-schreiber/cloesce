use std::fmt::Display;

pub type Result<T> = std::result::Result<T, GeneratorError>;

#[derive(Debug)]
pub struct GeneratorError {
    pub description: String,
    pub suggestion: String,
    pub context: String,
}

impl Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Description: {} Suggestion: {} Context: {}",
            self.description, self.suggestion, self.context
        )
    }
}

impl GeneratorError {
    fn new(description: String, suggestion: String) -> Self {
        Self {
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

pub enum GeneratorErrorKind {
    NullSqlType,
    InvalidSqlType,
    UnknownModel,
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
    pub fn to_error(&self) -> GeneratorError {
        match self {
            GeneratorErrorKind::NullSqlType => GeneratorError::new(
                "Model attributes cannot be literally null".into(),
                "Remove 'null' from your Model definition.".into(),
            ),
            GeneratorErrorKind::InvalidSqlType => GeneratorError::new(
                "Model attributes must be valid SQLite types: Integer, Real, Text, Blob".into(),
                "Consider using a navigation property or creating another model.".into(),
            ),
            GeneratorErrorKind::UnknownModel => GeneratorError::new(
                "Model attributes must be valid SQLite types: Integer, Real, Text, Blob".into(),
                "Consider using a navigation property or creating another model.".into(),
            ),
            GeneratorErrorKind::UnexpectedVoid => GeneratorError::new(
                "Void cannot be an attribute or parameter, only a return type.".into(),
                "Remove `void`".into(),
            ),
            GeneratorErrorKind::NotYetSupported => GeneratorError::new(
                "This feature will be supported in an upcoming Cloesce release.".into(),
                String::default(),
            ),
            GeneratorErrorKind::InvalidMapping => {
                GeneratorError::new("CIDL is ill-formatted".into(), String::default())
            }
            GeneratorErrorKind::InvalidApiDomain => GeneratorError::new(
                "Invalid or ill-formatted API domain".into(),
                "API's must be of the form: http://domain.com/path/to/api".into(),
            ),
            GeneratorErrorKind::MismatchedNavigationPropertyTypes => GeneratorError::new(
                "Navigation property references must match attribute types".into(),
                "TODO: a good suggestion here".into(),
            ),
            GeneratorErrorKind::InvalidNavigationPropertyReference => GeneratorError::new(
                "Navigation property references must be to foreign keys or other navigation properties".into(),
                "TODO: a good suggestion here".into(),
            ),
            GeneratorErrorKind::CyclicalModelDependency => GeneratorError::new(
                "Model composition cannot be cyclical".into(),
                "Allow a navigation property to be null".into(),
            ),
            GeneratorErrorKind::UnknownIncludeTreeReference => GeneratorError::new(
                "Unknown reference in Include Tree definition".into(),
                String::default(),
            ),
            GeneratorErrorKind::ExtraneousManyToManyReferences => GeneratorError::new(
                "Only two navigation properties can reference a many to many table".into(),
                "Remove a reference".into(),
            ),
            GeneratorErrorKind::MissingManyToManyReference => GeneratorError::new(
                "Many to Many navigation properties must have a correlated reference on the adjacent model.".into(),
                "TODO: a good indicator of where to add the nav prop".into()
            ),
        }
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
