//! The query module splits into two planners:
//! - [select] gets or lists
//! - [save] upserts

pub mod explain;
pub mod save;
pub mod select;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Database<'src> {
    pub name: &'src str,
    pub kind: DatabaseKind,
}

impl<'src> From<&'src idl::ModelBacking<'src>> for Database<'src> {
    fn from(backing: &'src idl::ModelBacking) -> Self {
        Database {
            name: backing.binding,
            kind: match backing.kind {
                idl::BackingKind::D1 => DatabaseKind::D1,
                idl::BackingKind::DurableObject => DatabaseKind::DurableObject,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum DatabaseKind {
    Kv,
    R2,
    D1,
    DurableObject,
}
