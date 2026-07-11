//! The query plan IR.
//!
//! Consists of a sequence of [Stage]s, each of which contains a set of [Step]s that may run in parallel.
//! Each stage is intended to run after the previous stage has completed, and may read values from the hydrated
//! result produced by earlier stages.
//!
//! TODO: Does not yet support `save` operations.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct QueryPlan<'src> {
    pub stages: Vec<Stage<'src>>,
}

impl<'src> QueryPlan<'src> {
    /// Return the stage at `index`, creating it (and any stages before it)
    /// if it does not yet exist.
    pub fn stage_at(&mut self, index: usize) -> &mut Stage<'src> {
        if self.stages.len() <= index {
            self.stages.resize_with(index + 1, Stage::default);
        }
        &mut self.stages[index]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Stage<'src> {
    pub steps: Vec<Step<'src>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step<'src> {
    pub query: Query<'src>,

    /// The location in the hydrated result where this step's result is attached.
    ///
    /// An empty path means the result is to be attached at the root of the hydrated result.
    pub result: Vec<&'src str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Database<'src> {
    pub name: &'src str,
    pub kind: DatabaseKind,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum DatabaseKind {
    Kv,
    R2,
    D1,
    DurableObject,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Query<'src> {
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
        segments: Vec<KeySegment<'src>>,
        shard: Vec<(&'src str, ValueArg<'src>)>,
    },

    /// Build an object at [Step::result] out of runtime params or parent field values,
    /// without querying an external database.
    Synthesize {
        fields: Vec<(&'src str, ValueArg<'src>)>,

        /// Whether each parent object receives the object bare or as a singleton array.
        cardinality: MapCardinality,
    },

    /// Merge `fields` into every object already present at [Step::result].
    ///
    /// Unlike [Query::Synthesize], this never creates an object: if an earlier step
    /// attached nothing there (e.g. a GET that matched no row), the step is a noop.
    ///
    /// Used to place a SQL-backed model's non-shard route fields onto its rows.
    Tag {
        fields: Vec<(&'src str, ValueArg<'src>)>,
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
pub enum ValueArg<'src> {
    /// A scalar runtime parameter that must be provided to execute the [Step].
    Param(&'src str),

    /// A field read from the parent object (the object at the parent path of
    /// [Step::result]).
    ParentField(&'src str),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum KeySegment<'src> {
    /// Text between placeholders
    Literal(&'src str),

    /// A placeholder resolved from a runtime param or a parent object
    /// field
    Value(ValueArg<'src>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mapping<'src> {
    /// Whether each parent object receives a single object or an array of objects.
    ///
    /// - If a query returns more than one row but the cardinality is [Cardinality::One],
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JoinKeys<'src> {
    pub parent_key: &'src str,
    pub child_key: &'src str,
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
