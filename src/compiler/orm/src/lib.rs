use ast::{CidlType, Number};
use serde_json::Value;

pub mod map;
pub mod select;
pub mod upsert;
pub mod validate;

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

#[derive(Debug, Clone)]
pub enum OrmErrorKind {
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
        write!(f, "{:?}", self)
    }
}

fn fmt_cidl_type(t: &CidlType) -> String {
    match t {
        CidlType::Void => "void".to_string(),
        CidlType::Int => "int".to_string(),
        CidlType::Uint => "uint".to_string(),
        CidlType::Real => "real".to_string(),
        CidlType::String => "string".to_string(),
        CidlType::Blob => "blob".to_string(),
        CidlType::Boolean => "boolean".to_string(),
        CidlType::DateIso => "date_iso".to_string(),
        CidlType::Stream => "stream".to_string(),
        CidlType::Json => "json".to_string(),
        CidlType::R2Object => "r2_object".to_string(),
        CidlType::Env => "env".to_string(),
        CidlType::Inject { name } => format!("inject '{}'", name),
        CidlType::Object { name } => format!("object '{}'", name),
        CidlType::Partial { object_name } => format!("partial '{}'", object_name),
        CidlType::DataSource { model_name } => format!("data_source '{}'", model_name),
        CidlType::Array(cidl_type) => format!("array<{}>", fmt_cidl_type(cidl_type)),
        CidlType::HttpResult(cidl_type) => format!("http_result<{}>", fmt_cidl_type(cidl_type)),
        CidlType::Nullable(cidl_type) => format!("nullable<{}>", fmt_cidl_type(cidl_type)),
        CidlType::Paginated(cidl_type) => format!("paginated<{}>", fmt_cidl_type(cidl_type)),
        CidlType::KvObject(cidl_type) => format!("kv_object<{}>", fmt_cidl_type(cidl_type)),
        CidlType::UnresolvedReference { name } => format!("unresolved_reference '{}'", name),
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
