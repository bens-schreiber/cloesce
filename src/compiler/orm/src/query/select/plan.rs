//! The select (read) query plan IR.
//!
//! Consists of a sequence of stages, each containing a set of steps that may execute in parallel.
//!
//! Each stage is intended to run after the previous stage has completed, and may read values from the hydrated
//! result produced by earlier stages.

use serde::Serialize;

use crate::query::{Database, TemplateSegment};

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SelectPlan<'src> {
    pub stages: Vec<SelectStage<'src>>,
}

impl<'src> SelectPlan<'src> {
    /// Return the stage at `index`, creating it (and any stages before it)
    /// if it does not yet exist.
    pub fn stage_at(&mut self, index: usize) -> &mut SelectStage<'src> {
        if self.stages.len() <= index {
            self.stages.resize_with(index + 1, SelectStage::default);
        }
        &mut self.stages[index]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SelectStage<'src> {
    pub steps: Vec<SelectStep<'src>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectStep<'src> {
    pub query: Select<'src>,

    /// The location in the hydrated result where this step's result is attached.
    ///
    /// An empty path means the result is to be attached at the root of the hydrated result.
    pub result: Vec<&'src str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Select<'src> {
    /// A SQL query to execute against a Durable Object or D1 database, composed of
    /// positional `?N` placeholders referencing `arguments` (1-based).
    ///
    /// For example:
    /// - sql => `SELECT * FROM users WHERE id = ?1 AND name = ?2`
    /// - arguments => `vec![SqlArg::Param("id"), SqlArg::Param("name")]`
    Sql {
        database: Database<'src>,
        sql: String,
        arguments: Vec<SqlArg<'src>>,
        mapping: Mapping<'src>,

        /// For a [DatabaseKind::DurableObject] step, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwise.
        ///
        /// - A [SqlArg::Spread] value fans the step out: one stub per distinct
        ///   value, the same query executed against each.
        ///
        /// - A [SqlArg::Param] value (a root step) addresses the single stub fixed
        ///   by the request.
        shard: Vec<(&'src str, SqlArg<'src>)>,
    },

    /// An operation executed against a KV, R2, or Durable Object KV storage.
    Key {
        database: Database<'src>,
        key: Vec<TemplateSegment<'src, SelectArg<'src>>>,

        /// For a Durable Object, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwis
        shard: Vec<(&'src str, SelectArg<'src>)>,
    },

    /// Set `fields` on the object(s) at [Step::result] from runtime params or parent
    /// field values, without querying an external database.
    ///
    /// - When `create` is true, the object is built fresh and attached: a backing-less
    ///   model's whole state, or a backing-less nav target built from its parent.
    ///
    /// - When `create` is false, the fields are merged onto whatever an earlier step
    ///   already attached here, and a slot with no such object is left untouched.
    Synthesize {
        fields: Vec<(&'src str, SelectArg<'src>)>,

        /// Whether each parent object receives the object bare or as a singleton array.
        cardinality: MapCardinality,

        /// Whether to build the object fresh (true) or merge onto an existing one (false).
        create: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SqlArg<'src> {
    /// A scalar runtime parameter that must be provided to execute the [Step].
    Param(&'src str),

    /// Every value of the named field across the parents of the step's own
    /// [Step::result] path
    Spread(&'src str),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mapping<'src> {
    /// Whether each parent object receives a single object or an array of objects.
    ///
    /// - If a query returns more than one row but the cardinality is [MapCardinality::One],
    ///   the runtime will take the first row only.
    pub cardinality: MapCardinality,

    /// How rows are distributed among parent objects: a row is attached to every
    /// parent where all pairs satisfy `parent[parent_key] == row[child_key]`.
    ///
    /// If empty, every parent receives the same result.
    pub join: Vec<JoinKeys<'src>>,
}

impl Mapping<'_> {
    pub fn one() -> Self {
        Self {
            cardinality: MapCardinality::One,
            join: vec![],
        }
    }

    pub fn many() -> Self {
        Self {
            cardinality: MapCardinality::Many,
            join: vec![],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum MapCardinality {
    One,
    Many,
}

impl From<idl::NavigationCardinality> for MapCardinality {
    fn from(c: idl::NavigationCardinality) -> Self {
        match c {
            idl::NavigationCardinality::One => MapCardinality::One,
            idl::NavigationCardinality::Many => MapCardinality::Many,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JoinKeys<'src> {
    pub parent_key: &'src str,
    pub child_key: &'src str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SelectArg<'src> {
    /// A scalar runtime parameter that must be provided to execute the step.
    Param(&'src str),

    /// A field read from the parent object (the object at the parent path of
    /// the step's result).
    ParentField(&'src str),
}
