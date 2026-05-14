use idl::Number;
use serde_json::Value;

pub mod map;
pub mod select;
pub mod upsert;
pub mod validate;

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

#[derive(Debug)]
pub enum OrmErrorKind {
    SerializeError { message: String },

    UnknownModel { name: String },
    UnknownDataSource { model: String, name: String },
    ModelMissingD1 { name: String },
    ModelKeyCannotAutoIncrement { model: String, field: String },
    MissingField { expected: String, missing: String },
    TypeMismatch { expected: String, got: Value },

    // Validators
    NotLessThan { expected: Number, got: Value },
    NotLessThanOrEqual { expected: Number, got: Value },
    NotGreaterThan { expected: Number, got: Value },
    NotGreaterThanOrEqual { expected: Number, got: Value },
    NotStep { expected: Number, got: Value },
    NotLength { expected: Number, got: Value },
    NotMinLength { expected: Number, got: Value },
    NotMaxLength { expected: Number, got: Value },
    UnmatchedRegex { got: Value, pattern: String },
}

impl std::fmt::Display for OrmErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrmErrorKind::SerializeError { message } => write!(f, "Serialization error: {message}"),
            OrmErrorKind::UnknownModel { name } => write!(f, "Unknown model: {name}"),
            OrmErrorKind::UnknownDataSource { model, name } => {
                write!(f, "Unknown data source '{name}' for model '{model}'")
            }
            OrmErrorKind::ModelMissingD1 { name } => {
                write!(f, "Model '{name}' is missing D1 metadata")
            }
            OrmErrorKind::ModelKeyCannotAutoIncrement { model, field } => write!(
                f,
                "Primary key field '{field}' on model '{model}' cannot be auto-incrementing"
            ),
            OrmErrorKind::MissingField { expected, missing } => {
                write!(f, "Missing field: expected '{expected}', got '{missing}'")
            }
            OrmErrorKind::TypeMismatch { expected, got } => {
                write!(f, "Type mismatch: expected '{expected}', got '{got}'")
            }
            OrmErrorKind::NotLessThan { expected, got } => {
                write!(
                    f,
                    "Validation error: expected value less than {expected}, got {got}"
                )
            }
            OrmErrorKind::NotLessThanOrEqual { expected, got } => write!(
                f,
                "Validation error: expected value less than or equal to {expected}, got {got}"
            ),
            OrmErrorKind::NotGreaterThan { expected, got } => {
                write!(
                    f,
                    "Validation error: expected value greater than {expected}, got {got}"
                )
            }
            OrmErrorKind::NotGreaterThanOrEqual { expected, got } => write!(
                f,
                "Validation error: expected value greater than or equal to {expected}, got {got}"
            ),
            OrmErrorKind::NotStep { expected, got } => {
                write!(
                    f,
                    "Validation error: expected value to be a multiple of {expected}, got {got}"
                )
            }
            OrmErrorKind::NotLength { expected, got } => {
                write!(
                    f,
                    "Validation error: expected length of value to be {expected}, got {got}"
                )
            }
            OrmErrorKind::NotMinLength { expected, got } => write!(
                f,
                "Validation error: expected length of value to be at least {expected}, got {got}"
            ),
            OrmErrorKind::NotMaxLength { expected, got } => write!(
                f,
                "Validation error: expected length of value to be at most {expected}, got {got}"
            ),
            OrmErrorKind::UnmatchedRegex { got, pattern } => write!(
                f,
                "Validation error: expected value to match regex pattern '{pattern}', got '{got}'"
            ),
        }
    }
}

#[macro_export]
macro_rules! fail {
    ($kind:expr) => {
        return Err($kind)
    };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $kind:expr) => {
        if !($cond) {
            fail!($kind)
        }
    };
}

pub type Result<T> = std::result::Result<T, OrmErrorKind>;
