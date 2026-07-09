//! Cloesce Query Planner
//!
//! Converts an [Operation] into a [QueryPlan], detailing how the runtime should execute
//! the operation against the underlying data sources.
//!
//! [plan] will try its best to create as few [Stage]s as possible to hydrate
//! the requested [IncludeTree].
//!
//! # SQLite (D1, Durable Objects)
//!
//! A relationship between two SQLite backed Models falls under two categories:
//! - Both Models are backed by the same database
//! - Model databases are disjoint
//!
//! Both cases are handled the exact same way, and never with a SQL JOIN: select the
//! base model first, then in a later stage select the related model(s) using the
//! hydrated result of the base model as bindings to the related query.
//!
//! NOTE: Even if the parameters to hydrate a Model and its related Model are known at
//! runtime, the planner still places the child in a later [Stage] than its parent,
//! because the parent rows **must** exist to supply the child's bindings.

use idl::{BackingKind, CloesceIdl, IncludeTree, Model, ModelBacking, NavigationField};

use crate::query::plan::{
    Argument, Database, DatabaseKind, JoinKeys, Mapping, ObjectPath, Query, QueryPlan, Step,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Get,
    List,
    Save,
}

/// The runtime parameter that bounds the number of root rows a `List` returns.
const LIMIT_PARAM: &str = "limit";

/// Convert an [Operation] into a [QueryPlan] for the given model.
///
/// # Parameters
/// - `operation`: The kind operation to plan.
/// - `model`: The model on which to execute the operation.
/// - `idl`: The Cloesce IDL containing the schema information.
/// - `tree`: The include tree specifying which fields and relations to hydrate.
pub fn plan<'src>(
    operation: Operation,
    model: &str,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
) -> QueryPlan<'src> {
    let mut plan = QueryPlan::default();

    let Some(model) = idl.models.get(model) else {
        return plan; // Fail silently if the model is not found
    };

    // Every shard field value comes from runtime parameters
    let shard = model
        .backing
        .as_ref()
        .map(|b| {
            b.fields
                .iter()
                .map(|f| Argument::Param(f))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // TODO: models dont always have a database
    let database = database(model.backing.as_ref().unwrap(), shard);

    match operation {
        Operation::Save => todo!("Not yet implemented"),
        Operation::Get => {
            // GET is always a fetch-by-pk. Gather all WHERE predicates, e.g.
            // "id = ?1", ... "name = ?N"
            let predicates = model
                .primary_columns
                .iter()
                .enumerate()
                .map(|(i, c)| format!("\"{}\" = ?{}", c.field.name, i + 1))
                .collect::<Vec<_>>();

            // Every primary key columns value comes from runtime parameters
            let arguments = model
                .primary_columns
                .iter()
                .map(|c| Argument::Param(c.field.name.as_ref()))
                .collect::<Vec<_>>();

            plan.stage_at(0).steps.push(Step {
                database,
                query: Query::Sql {
                    sql: select_sql(model, &predicates, false),
                },
                arguments,
                result: ObjectPath::Root,
                mapping: Mapping::one(),
            });

            select_navs(model, idl, tree, &mut plan, &[], 0);
        }
        Operation::List => {
            // LIST takes only the database and `limit` arguments
            plan.stage_at(0).steps.push(Step {
                database,
                query: Query::Sql {
                    sql: select_sql(model, &[], true),
                },
                arguments: vec![Argument::Param(LIMIT_PARAM)],
                result: ObjectPath::Root,
                mapping: Mapping::many(),
            });

            select_navs(model, idl, tree, &mut plan, &[], 0);
        }
    }

    plan
}

/// Recurse the include tree, emitting one nav step per included [NavigationField]
/// whose target uses sqlite.
///
/// A nav at include-tree stage `s` is placed in stage `s + 1`, meaning
/// parents are resolved in a stage before their children, who can be resolved in parallel
/// in the next stage.
fn select_navs<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut QueryPlan<'src>,
    parent_path: &[&'src str],
    depth: usize,
) {
    for nav in &model.navigation_fields {
        let Some(subtree) = tree.0.get(nav.field.name.as_ref()) else {
            continue;
        };
        let Some(target) = idl.models.get(nav.model_reference) else {
            continue;
        };
        let Some(backing) = nav.target_backing.as_ref() else {
            continue;
        };
        if !target.uses_sqlite() {
            continue;
        }

        let mut path = parent_path.to_vec();
        path.push(nav.field.name.as_ref());

        select_nav(nav, target, backing, parent_path, &path, plan, depth + 1);
        select_navs(target, idl, subtree, plan, &path, depth + 1);
    }

    /// Emit a single nav step for `nav` targeting `target` at stage `stage`.
    fn select_nav<'src>(
        nav: &'src NavigationField<'src>,
        target: &'src Model<'src>,
        backing: &'src ModelBacking<'src>,
        parent_path: &[&'src str],
        path: &[&'src str],
        plan: &mut QueryPlan<'src>,
        stage: usize,
    ) {
        let spread = |local: &'src str| {
            Argument::Spread(ObjectPath::Field(
                parent_path.iter().copied().chain([local]).collect(),
            ))
        };

        // A key whose `target` names a field of the target's backing (a DO shard/route
        // field, not a column) is a shard key; every other key is a SQL predicate.
        let (shard_keys, sql_keys): (Vec<_>, Vec<_>) = nav
            .keys
            .iter()
            .partition(|k| backing.fields.contains(&k.target));

        // DO shard keys are supplied by spreading the parent's values; each distinct value
        // fans the step out to its own stub, and rows come back tagged with the shard value
        // under the shard field name so stitching stays uniform.
        let shard = shard_keys.iter().map(|k| spread(k.local)).collect();

        // Remaining keys become `target IN (?N)` predicates fed by the parent's values,
        // one placeholder per key.
        let predicates = sql_keys
            .iter()
            .enumerate()
            .map(|(i, k)| format!("\"{}\" IN (?{})", k.target, i + 1))
            .collect::<Vec<_>>();
        let bindings = sql_keys.iter().map(|k| spread(k.local)).collect();

        let join = shard_keys
            .iter()
            .chain(&sql_keys)
            .map(|k| JoinKeys {
                parent_key: k.local,
                child_key: k.target,
            })
            .collect();

        plan.stage_at(stage).steps.push(Step {
            database: database(backing, shard),
            query: Query::Sql {
                sql: select_sql(target, &predicates, false),
            },
            arguments: bindings,
            result: ObjectPath::Field(path.to_vec()),
            mapping: Mapping {
                cardinality: nav.cardinality.clone().into(),
                join,
            },
        });
    }
}

/// Build an ordered `SELECT` over the model's explicit columns (never `SELECT *`).
///
/// Every select is ordered by all primary key columns.
fn select_sql(model: &Model, preds: &[String], limit: bool) -> String {
    let columns = model
        .primary_columns
        .iter()
        .chain(&model.columns)
        .map(|c| format!("\"{}\"", c.field.name))
        .collect::<Vec<_>>()
        .join(", ");

    // ex: `SELECT "id", "name" FROM "Horse"`
    let mut sql = format!("SELECT {columns} FROM \"{}\"", model.name);

    if !preds.is_empty() {
        // ... WHERE "id" = ?1 AND "name" = ?2
        sql.push_str(&format!(" WHERE {}", preds.join(" AND ")));
    }

    // ... ORDER BY "id" ASC, "name" ASC
    let order = model
        .primary_columns
        .iter()
        .map(|c| format!("\"{}\" ASC", c.field.name))
        .collect::<Vec<_>>()
        .join(", ");
    sql.push_str(&format!(" ORDER BY {order}"));

    if limit {
        // ... LIMIT ?N
        sql.push_str(&format!(" LIMIT ?{}", preds.len() + 1));
    }

    sql
}

fn database<'src>(backing: &'src ModelBacking, shard: Vec<Argument<'src>>) -> Database<'src> {
    Database {
        name: backing.binding,
        kind: match backing.kind {
            BackingKind::D1 => DatabaseKind::D1,
            BackingKind::DurableObject => DatabaseKind::DurableObject { shard },
        },
    }
}
