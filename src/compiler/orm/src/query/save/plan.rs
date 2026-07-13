//! The save (upsert) plan IR.
//!
//! Consists of a sequence of stages, each containing a set of steps that may execute in parallel.
//!
//! Each stage is intended to run after the previous stage has completed, and may read values from the hydrated
//! result produced by earlier stages.
//!
//! NOTE: The `'src` lifetime is used to tie the plan to both the lifetime of the source code
//! that made the Cloesce IDL, as well as the JSON payload (for the sake of this planner, they are the same).

use serde::Serialize;

use crate::query::{Database, TemplateSegment};

/// The name of a "temporary table" used to capture the primary key of a just-inserted row for a later
/// read-back. Stores only generated primary keys (auto-increment integers).
///
/// Because D1 does not support any modification of the SQL schema during actual execution, this table
/// is *not an actual SQLite temporary table*, but a specially reserved table name made during Cloesce migrations.
///
/// Although many ORMs resolve the "read-back inserted id" problem by simply using a transaction, D1 has no
/// real transaction capability (just "batching", which cannot read the result of a previous statement).
pub const TMP_TABLE: &str = "$cloesce_tmp";

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SavePlan<'src> {
    pub stages: Vec<SaveStage<'src>>,
}

impl<'src> SavePlan<'src> {
    /// Return the stage at `index`, creating it (and any stages before it)
    /// if it does not yet exist.
    pub fn stage_at(&mut self, index: usize) -> &mut SaveStage<'src> {
        if self.stages.len() <= index {
            self.stages.resize_with(index + 1, SaveStage::default);
        }
        &mut self.stages[index]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct SaveStage<'src> {
    pub steps: Vec<SaveStep<'src>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SaveStep<'src> {
    pub query: SaveQuery<'src>,

    /// The location in the hydrated result where this step's result is attached.
    ///
    /// An empty path means the result is to be attached at the root of the hydrated result.
    pub result: Vec<PathSegment<'src>>,
}

/// One hop in a hydrated-body path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PathSegment<'src> {
    Field(&'src str),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SaveQuery<'src> {
    /// Ordered statements executed as one transaction / batch against a
    /// single database (a single DO stub when sharded).
    ///
    /// NOTE: Atomicity is per batch only. Statements within a batch are one
    /// transaction, but cross-batch / cross-stage / cross-storage writes are not.
    SqlBatch {
        database: Database<'src>,
        statements: Vec<SqlStatement<'src>>,

        /// For a Durable Object, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwise.
        shard: Vec<(&'src str, SaveArg<'src>)>,
    },

    /// Write a value to KV / R2 / DO-KV at the key described by `segments`,
    /// then attach the written value at [SaveStep::result].
    KeyWrite {
        database: Database<'src>,
        key: Vec<TemplateSegment<'src, SaveArg<'src>>>,
        value: &'src serde_json::Value,

        /// Workers-KV metadata,
        metadata: Option<&'src serde_json::Value>,

        /// For a Durable Object, the `(field, value)` pairs
        /// routing to specific stubs. Empty otherwis
        shard: Vec<(&'src str, SaveArg<'src>)>,
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
        fields: Vec<(&'src str, SaveArg<'src>)>,
        create: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SqlStatement<'src> {
    /// INSERT / UPDATE / tmp-capture / tmp-DELETE.
    ///
    /// `?N` placeholders reference `arguments` (1-based); inline expressions (tmp subqueries,
    /// `last_insert_rowid()`) consume no placeholder.
    Write {
        sql: String,
        arguments: Vec<SaveArg<'src>>,
    },

    /// A trailing read-back SELECT for ONE saved instance; its single row
    /// is attached at `result` (intermediate objects/arrays created on demand).
    Hydrate {
        sql: String,
        arguments: Vec<SaveArg<'src>>,
        result: Vec<PathSegment<'src>>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum SaveArg<'src> {
    /// A value from the save payload
    Payload(&'src serde_json::Value),

    /// A value read from the hydrated result at an exact path (a generated PK
    /// hydrated by an earlier stage's read-back).
    Result(Vec<PathSegment<'src>>),
}
