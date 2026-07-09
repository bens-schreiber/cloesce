//! The query plan IR.
//!
//! TODO: Does not yet support `save` operations.

use serde::Serialize;

/// A complete description of how the runtime should execute a single operation:
/// an ordered pipeline of [Stage]s.
///
/// Stages execute sequentially; each stage may read values from the hydrated
/// result produced by earlier stages. A plan is pure data: serializable, and
/// executable with no knowledge of the schema or the relationships between Models.
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

/// A set of [Step] that may run in parallel.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Stage<'src> {
    pub steps: Vec<Step<'src>>,
}

/// A single action in a [Stage] that stores its result in the hydrated result
/// of the [QueryPlan] at the location specified by [Step::result].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Step<'src> {
    pub database: Database<'src>,

    pub query: Query<'src>,

    /// Positional values bound into [Step::query]: placeholder `?N` resolves
    /// to `arguments[N - 1]`.
    pub arguments: Vec<Argument<'src>>,

    /// The location in the hydrated result where this step's result is attached.
    pub result: ObjectPath<'src>,

    /// How this step's rows are shaped and attached at [Step::result].
    pub mapping: Mapping<'src>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Database<'src> {
    pub name: &'src str,
    pub kind: DatabaseKind<'src>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum DatabaseKind<'src> {
    Kv,
    R2,
    D1,
    DurableObject {
        /// Arguments supplying the shard values needed to construct a specific
        /// Durable Object stub.
        ///
        /// A [Argument::Spread] shard value fans the step out: one stub per
        /// distinct value, the same query executed against each, and each
        /// stub's rows tagged with its shard value for joining
        shard: Vec<Argument<'src>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Query<'src> {
    /// A SQL query to execute against a Durable Object or D1 database,
    /// composed of positional `?N` placeholders referencing [Step::arguments].
    ///
    /// For example:
    /// - sql => `SELECT * FROM users WHERE id = ?1 AND name = ?2`
    /// - arguments => `vec![Argument::Param("id"), Argument::Param("name")]`
    Sql { sql: String },

    /// An operation executed against a KV or R2 storage.
    ///
    /// Always has a [Cardinality::One] result, which is the value read from the storage
    /// (regardless of the stored values type).
    Key {
        /// The key to read from the storage, composed of `{{param}}` placeholders
        /// referencing [Step::arguments].
        key: &'src str,
    },
}

/// A single positional value bound into a [Query].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Argument<'src> {
    /// A scalar parameter from the runtime that must exist in order to execute
    /// this [Step], and could be referenced in a SQL query or Durable Object shard key.
    Param(&'src str),

    /// A scalar from the hydrated result of a previous [Stage] that must exist
    /// in order to execute this [Step].
    Scalar(ObjectPath<'src>),

    /// Every value at the path across the hydrated result of a previous [Stage],
    /// deduplicated, with nulls dropped.
    ///
    /// The runtime expands the placeholder into a value list (e.g. `id IN (?1)`),
    /// chunked by the backend's bind-parameter limit.
    Spread(ObjectPath<'src>),
}

/// A chain of field names that navigates to a value in the hydrated result
/// of a previous [Stage].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ObjectPath<'src> {
    /// Value exists in the root(s) of the hydrated result,
    /// e.g. `id` in the object `{ "id": 1 }`
    Root,

    /// Value is nested in the hydrated result,
    /// e.g. `dog.id` in the object `{ "dog": { "id": 1 } }`.
    ///
    /// Provides a path of field names to navigate to the value, e.g. `["dog", "id"]`.
    Field(Vec<&'src str>),
}

/// How a [Step]'s raw rows become values in the hydrated result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mapping<'src> {
    /// Whether each parent object receives a single object or an array of objects.
    ///
    /// - If a query returns more than one row but the cardinality is [Cardinality::One],
    ///   the runtime will take the first row only.
    pub cardinality: Cardinality,

    /// How rows are distributed among parent objects: a row is attached to every
    /// parent where all pairs satisfy `parent[parent_key] == row[child_key]`.
    ///
    /// Empty on root steps (there is no parent to join into) and on
    /// discriminator-less navigations, where every parent receives the same value(s).
    pub join: Vec<JoinKeys<'src>>,
}

impl Mapping<'_> {
    pub fn one() -> Self {
        Self {
            cardinality: Cardinality::One,
            join: vec![],
        }
    }

    pub fn many() -> Self {
        Self {
            cardinality: Cardinality::Many,
            join: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct JoinKeys<'src> {
    pub parent_key: &'src str,
    pub child_key: &'src str,
}

/// Whether a step's SQLite result should be mapped to a single object
/// or an array of objects in the hydrated result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Cardinality {
    /// Take the first row only.
    One,
    /// Take all rows.
    Many,
}

impl From<idl::NavigationCardinality> for Cardinality {
    fn from(c: idl::NavigationCardinality) -> Self {
        match c {
            idl::NavigationCardinality::One => Cardinality::One,
            idl::NavigationCardinality::Many => Cardinality::Many,
        }
    }
}
