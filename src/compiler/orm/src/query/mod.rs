//! The query module splits into two planners:
//! - [select] gets or lists
//! - [save] upserts

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

/// A template for a KV/R2 key, e.g. `users/{user_id}` where
/// `user_id` is a dynamic value and `users/` is a literal prefix.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum TemplateSegment<'src, V> {
    /// Text between placeholders
    Literal(&'src str),
    Value(V),
}

impl<'src, V> TemplateSegment<'src, V> {
    pub fn parse(
        t: &'src str,
        mut value: impl FnMut(&'src str) -> V,
    ) -> Vec<TemplateSegment<'src, V>> {
        let mut segments = Vec::new();
        let mut rest = t;
        while let Some(open) = rest.find('{') {
            if open > 0 {
                segments.push(TemplateSegment::Literal(&rest[..open]));
            }
            let close = rest[open..].find('}').expect("validated key template") + open;
            let arg = &rest[open + 1..close];
            segments.push(TemplateSegment::Value(value(arg)));
            rest = &rest[close + 1..];
        }
        if !rest.is_empty() {
            segments.push(TemplateSegment::Literal(rest));
        }
        segments
    }
}
