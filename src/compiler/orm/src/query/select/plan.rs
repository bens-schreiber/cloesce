//! The select (read) query plan IR.
//!
//! Consists of a sequence of stages, each containing a set of steps that may execute in parallel.
//! A plan additionally stores a list of linked tables (result objects from each individual query),
//! which are referenced by each step to indicate where the step's result should be hydrated into the final result.
//!
//! Each stage is intended to run after the previous stage has completed, and may read values from the hydrated
//! result produced by earlier stages.

use std::borrow::Cow;

use serde::Serialize;

use crate::query::{Database, TemplateSegment};

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SelectPlan<'src> {
    pub tables: Vec<TableDef<'src>>,
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

    /// Register a table with the given parent, returning its id.
    pub fn register_table(&mut self, parent: Option<TableParent<'src>>) -> usize {
        let id = self.tables.len();
        self.tables.push(TableDef { parent });
        id
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableDef<'src> {
    /// Where this table attaches in the hydrated result; `None` only for the root.
    pub parent: Option<TableParent<'src>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableParent<'src> {
    pub table: usize,
    pub field: &'src str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SelectStage<'src> {
    pub steps: Vec<SelectStep<'src>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectStep<'src> {
    pub query: Select<'src>,

    /// The table this step's result hydrates.
    pub table: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Select<'src> {
    /// A SQL query to execute against a Durable Object or D1 database, composed of
    /// literal [SqlSegment]s and [SqlSegment::Bind] placeholders referencing
    /// `arguments` (0-based).
    ///
    /// For example:
    /// - sql => `SELECT * FROM users WHERE id = ` `Bind(0)` ` AND name = ` `Bind(1)`
    /// - arguments => `vec![SqlArgument { Param("id"), .. }, SqlArgument { Param("name"), .. }]`
    Sql {
        database: Database<'src>,
        sql: Vec<SqlSegment>,
        arguments: Vec<SqlArgument<'src>>,
        mapping: Mapping<'src>,

        /// For a [Database::DurableObject] step, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwise.
        shard: Vec<(&'src str, SelectArg<'src>)>,

        /// Route fields to attach to every row of the result, including
        /// shard fields.
        route_fields: Vec<(&'src str, SelectArg<'src>)>,
    },

    /// An operation executed against a KV, R2, or Durable Object KV storage.
    Key {
        database: Database<'src>,
        segments: Vec<TemplateSegment<'src, SelectArg<'src>>>,

        /// For a Durable Object, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwis
        shard: Vec<(&'src str, SelectArg<'src>)>,
    },

    /// Set `fields` on the object(s) of the step's [SelectStep::table] from runtime
    /// params or parent field values, without querying an external database.
    ///
    /// Will never synthesize onto the result(s) of a [Select::Sql] call, only
    /// on some non-sql backed parent object.
    Synthesize {
        fields: Vec<(&'src str, SelectArg<'src>)>,
        cardinality: MapCardinality,
    },
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
    Param(Cow<'src, str>),

    /// A field of a table hydrated by an earlier step.
    Field { table: usize, field: &'src str },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SqlSegment {
    /// Verbatim SQL text.
    Literal(String),
    /// A placeholder bound to `arguments[i]` (0-based).
    Bind(usize),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SqlArgument<'src> {
    pub value: SelectArg<'src>,

    /// Expand to a `(?, ?, ...)` list instead of a single `?`, for use in `IN` clauses.
    pub spread: bool,
}

impl<'src> SqlArgument<'src> {
    /// A single-value binding.
    pub fn scalar(value: SelectArg<'src>) -> Self {
        Self {
            value,
            spread: false,
        }
    }

    /// A binding that spreads its distinct values.
    pub fn spread(value: SelectArg<'src>) -> Self {
        Self {
            value,
            spread: true,
        }
    }
}
