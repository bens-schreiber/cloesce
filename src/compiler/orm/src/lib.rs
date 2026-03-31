pub mod map;
pub mod select;
pub mod upsert;
pub mod validate;

use std::fmt::Display;

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

#[derive(Debug)]
pub struct OrmError {
    pub kind: OrmErrorKind,
    pub context: String,
}

impl OrmError {
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

impl Display for OrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Kind: {:?} Context: {} ({})",
            self.kind, self.context, self.kind as u32
        )
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum OrmErrorKind {
    UnknownModel,
    ModelMissingD1,
    MissingPrimaryKey,
    MissingAttribute,
    MissingKeyParameter,
    TypeMismatch,
    CompositeKeyCannotAutoincrement,
}

impl OrmErrorKind {
    pub fn to_error(self) -> OrmError {
        let context = String::new();
        OrmError {
            kind: self,
            context,
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
        if !($cond) {
            fail!($kind)
        }
    };
    ($cond:expr, $kind:expr, $($arg:tt)*) => {
        if !($cond) {
            fail!($kind, $($arg)*)
        }
    };
}

pub type Result<T> = std::result::Result<T, OrmError>;
