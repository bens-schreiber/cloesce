//! # Cloesce Query Planner
//!
//! Converts an [Operation] into a [QueryPlan], detailing how the runtime should execute
//! the operation against the underlying data sources.
//!
//! [plan] will create as few [crate::query::plan::Stage] as possible to hydrate
//! the requested [IncludeTree].

use std::collections::HashMap;

use idl::{BackingKind, CloesceIdl, IncludeTree, Model, ModelBacking, NavigationField};

use crate::query::plan::{
    Database, DatabaseKind, JoinKeys, KeySegment, Mapping, Query, QueryPlan, SqlArg, Step, ValueArg,
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
pub fn plan<'src>(
    operation: Operation,
    model: &str,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
) -> QueryPlan<'src> {
    let mut plan = QueryPlan::default();

    let Some(model) = idl.models.get(model) else {
        // Fail silently if the model is not found
        return plan;
    };

    let mapping = match operation {
        Operation::Get => Mapping::one(),
        Operation::List => Mapping::many(),
        Operation::Save => todo!("Not yet implemented"),
    };

    let mut params = Params::default();
    for f in &model.route_fields {
        // Every route field is required to be supplied by the runtime
        // This includes Durabe Object shard fields, which are always route fields.
        params.map.insert(f.name.as_ref(), f.name.as_ref());
    }
    if operation == Operation::Get {
        for c in &model.primary_columns {
            // In Get operations, every primary key column is required to be supplied
            // by the runtime
            params
                .map
                .insert(c.field.name.as_ref(), c.field.name.as_ref());
        }
    }

    if let Some(backing) = model.backing.as_ref().filter(|_| model.uses_sqlite()) {
        // Every root shard field value comes from runtime parameters
        let shard = backing
            .fields
            .iter()
            .map(|f| (*f, SqlArg::Param(f)))
            .collect::<Vec<_>>();

        let query = match operation {
            Operation::Get => {
                // GET is always a fetch-by-pk. Gather all WHERE predicates, e.g.
                // "id = ?1", ... "name = ?N"
                let predicates = model
                    .primary_columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("\"{}\" = ?{}", c.field.name, i + 1))
                    .collect::<Vec<_>>();

                // Every primary key column's value comes from runtime parameters
                let arguments = model
                    .primary_columns
                    .iter()
                    .map(|c| SqlArg::Param(c.field.name.as_ref()))
                    .collect::<Vec<_>>();

                Query::Sql {
                    database: database(backing),
                    sql: select_sql(model, &predicates, false),
                    arguments,
                    shard,
                    mapping,
                }
            }
            // LIST takes only the `limit` argument
            Operation::List => Query::Sql {
                database: database(backing),
                sql: select_sql(model, &[], true),
                arguments: vec![SqlArg::Param(LIMIT_PARAM)],
                shard,
                mapping,
            },
            _ => todo!("Not yet implemented"),
        };

        plan.stage_at(0).steps.push(Step {
            query,
            result: vec![],
        });

        // Non-shard route fields aren't columns and aren't merged by the SQL shard, so a
        // [Query::Tag] (all `Param` at the root) sets them onto every row in-stage.
        let route_tags = model
            .route_fields
            .iter()
            .map(|f| f.name.as_ref())
            .filter(|f| !backing.fields.contains(f))
            .map(|f| (f, ValueArg::Param(f)))
            .collect::<Vec<_>>();
        if !route_tags.is_empty() {
            plan.stage_at(0).steps.push(Step {
                query: Query::Tag { fields: route_tags },
                result: vec![],
            });
        }
    } else {
        // A non-sqlite-backed model has no database to select from, just a state
        // synthesized from its route fields, which must be supplied by the runtime.
        //
        // Without a SQLite backing, a [MapCardinality::Many] model is coerced into
        // a singleton list.
        let fields = model
            .route_fields
            .iter()
            .map(|f| (f.name.as_ref(), ValueArg::Param(f.name.as_ref())))
            .collect();

        plan.stage_at(0).steps.push(Step {
            query: Query::Synthesize {
                fields,
                cardinality: mapping.cardinality,
            },
            result: vec![],
        });
    }

    hydrate_model(model, idl, tree, &mut plan, &params, &[], 0);

    plan
}

fn hydrate_model<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut QueryPlan<'src>,
    params: &Params<'src>,
    path: &[&'src str],
    stage: usize,
) {
    select_keys(model, idl, tree, plan, params, path, stage);
    select_navs(model, idl, tree, plan, params, path, stage);
}

/// Emit one [Query::Key] [Step] per included R2 and KV field of `model`.
///
/// - If all placeholders on the key-template are in the set of [Params],
///   the step runs in `stage`.
///
/// - If any placeholder is not in the set of [Params], the step must run in
///   `stage + 1`.
fn select_keys<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut QueryPlan<'src>,
    params: &Params<'src>,
    path: &[&'src str],
    stage: usize,
) {
    let mut push =
        |field: &'src str, database: Database<'src>, key: &'src str, shard_fields: &[&'src str]| {
            if !tree.0.contains_key(field) {
                // Include tree does not request this field, skip.
                return;
            }

            let segments = key_segments(key, params);
            let segments_covered = segments.iter().all(|s| {
                matches!(
                    s,
                    KeySegment::Literal(_) | KeySegment::Value(ValueArg::Param(_))
                )
            });

            let shard = shard_fields
                .iter()
                .map(|f| (*f, params.value_arg(f)))
                .collect::<Vec<_>>();
            let shards_covered = shard
                .iter()
                .all(|(_, arg)| matches!(arg, ValueArg::Param(_)));

            let step_stage = if segments_covered && shards_covered {
                stage
            } else {
                stage + 1
            };

            let mut result = path.to_vec();
            result.push(field);

            plan.stage_at(step_stage).steps.push(Step {
                query: Query::Key {
                    database,
                    segments,
                    shard,
                },
                result,
            });
        };

    for r2 in &model.r2_fields {
        let database = Database {
            name: r2.binding,
            kind: DatabaseKind::R2,
        };

        push(r2.field.name.as_ref(), database, &r2.key_format, &[]);
    }

    for kv in &model.kv_fields {
        let is_do_kv = idl
            .wrangler_env
            .durable_bindings
            .iter()
            .any(|b| b.name == kv.binding);

        let (kind, shard) = if is_do_kv {
            (DatabaseKind::DurableObject, kv.shard_fields.as_slice())
        } else {
            (DatabaseKind::Kv, [].as_slice())
        };

        let database = Database {
            name: kv.binding,
            kind,
        };

        push(kv.field.name.as_ref(), database, &kv.key_format, shard);
    }
}

/// Recurse the include tree, emitting one nav step per included [NavigationField].
///
/// A nav whose key locals are all param-covered runs in `depth` (sourcing them from
/// params); otherwise it waits for `depth + 1`, spreading the parent's hydrated values.
fn select_navs<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut QueryPlan<'src>,
    params: &Params<'src>,
    parent_path: &[&'src str],
    depth: usize,
) {
    for nav in &model.navigation_fields {
        let Some(subtree) = tree.0.get(nav.field.name.as_ref()) else {
            // Include tree does not request this nav, skip.
            continue;
        };
        let Some(target) = idl.models.get(nav.model_reference) else {
            // Fail silently (this should be unreachable)
            continue;
        };

        let mut path = parent_path.to_vec();
        path.push(nav.field.name.as_ref());

        let child_params = {
            // Child inherits a param for each nav key whose local is covered
            let map = nav
                .keys
                .iter()
                .filter_map(|k| params.map.get(k.local).map(|p| (k.target, *p)))
                .collect::<HashMap<_, _>>();

            Params { map }
        };

        let covered = nav.keys.iter().all(|k| params.map.contains_key(k.local));
        let stage = if covered {
            // All inputs are covered, run in parallel
            depth
        } else {
            // Some input is not covered, wait for the parent to finish
            depth + 1
        };

        if let Some(backing) = target.backing.as_ref().filter(|_| target.uses_sqlite()) {
            select_nav(nav, target, backing, params, &path, plan, stage);
        } else {
            synthesize_nav(nav, params, &path, plan, stage);
        }

        hydrate_model(target, idl, subtree, plan, &child_params, &path, stage);
    }

    /// Emit a [Query::Synthesize] nav step
    fn synthesize_nav<'src>(
        nav: &'src NavigationField<'src>,
        params: &Params<'src>,
        path: &[&'src str],
        plan: &mut QueryPlan<'src>,
        stage: usize,
    ) {
        let fields = nav
            .keys
            .iter()
            .map(|k| (k.target, params.value_arg(k.local)))
            .collect();

        plan.stage_at(stage).steps.push(Step {
            query: Query::Synthesize {
                fields,
                cardinality: nav.cardinality.clone().into(),
            },
            result: path.to_vec(),
        });
    }

    /// Emit a single SQL step for `nav` targeting SQLite-backed `target` at `stage`,
    /// plus a [Query::Tag] for any non-shard route fields the target carries.
    fn select_nav<'src>(
        nav: &'src NavigationField<'src>,
        target: &'src Model<'src>,
        backing: &'src ModelBacking<'src>,
        params: &Params<'src>,
        path: &[&'src str],
        plan: &mut QueryPlan<'src>,
        stage: usize,
    ) {
        let is_shard = |t: &str| backing.fields.contains(&t);
        let is_route = |t: &str| target.route_fields.iter().any(|f| f.name == t) && !is_shard(t);

        let shard = nav
            .keys
            .iter()
            .filter(|k| is_shard(k.target))
            .map(|k| (k.target, params.sql_arg(k.local)))
            .collect();

        // Remaining (non-shard, non-route) keys become `target IN (?N)` predicates.
        let sql_keys = nav
            .keys
            .iter()
            .filter(|k| !is_shard(k.target) && !is_route(k.target))
            .collect::<Vec<_>>();
        let predicates = sql_keys
            .iter()
            .enumerate()
            .map(|(i, k)| format!("\"{}\" IN (?{})", k.target, i + 1))
            .collect::<Vec<_>>();
        let bindings = sql_keys.iter().map(|k| params.sql_arg(k.local)).collect();

        let join = nav
            .keys
            .iter()
            .filter(|k| !is_route(k.target))
            .map(|k| JoinKeys {
                parent_key: k.local,
                child_key: k.target,
            })
            .collect();

        plan.stage_at(stage).steps.push(Step {
            query: Query::Sql {
                database: database(backing),
                sql: select_sql(target, &predicates, false),
                arguments: bindings,
                shard,
                mapping: Mapping {
                    cardinality: nav.cardinality.clone().into(),
                    join,
                },
            },
            result: path.to_vec(),
        });

        // Non-shard route fields ride onto the rows the SQL step just produced.
        let route_tags = nav
            .keys
            .iter()
            .filter(|k| is_route(k.target))
            .map(|k| (k.target, params.value_arg(k.local)))
            .collect::<Vec<_>>();
        if !route_tags.is_empty() {
            plan.stage_at(stage).steps.push(Step {
                query: Query::Tag { fields: route_tags },
                result: path.to_vec(),
            });
        }
    }
}

/// Build an ordered `SELECT` over the model's columns. Every select is ordered by
/// primary key column(s).
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

/// Maps a field on the current model to the runtime parameter that fixes its value.
///
/// A field present in the map is "param-covered" if its value is known without querying a
/// database, so a [Step] consuming it may run in its parent stage rather than `parent stage + 1`.
#[derive(Default)]
struct Params<'src> {
    map: HashMap<&'src str, &'src str>,
}

impl<'src> Params<'src> {
    fn value_arg(&self, field: &'src str) -> ValueArg<'src> {
        match self.map.get(field) {
            Some(param) => ValueArg::Param(param),
            None => ValueArg::ParentField(field),
        }
    }

    fn sql_arg(&self, field: &'src str) -> SqlArg<'src> {
        match self.map.get(field) {
            Some(param) => SqlArg::Param(param),
            None => SqlArg::Spread(field),
        }
    }
}

/// Parse a key template into ordered [KeySegment]
fn key_segments<'src>(key: &'src str, params: &Params<'src>) -> Vec<KeySegment<'src>> {
    fn push_literal<'src>(segments: &mut Vec<KeySegment<'src>>, text: &'src str) {
        if !text.is_empty() {
            segments.push(KeySegment::Literal(text));
        }
    }

    let mut segments = Vec::new();
    let mut rest = key;
    while let Some(open) = rest.find('{') {
        push_literal(&mut segments, &rest[..open]);
        let close = rest[open..].find('}').expect("validated key template") + open;
        segments.push(KeySegment::Value(params.value_arg(&rest[open + 1..close])));
        rest = &rest[close + 1..];
    }

    push_literal(&mut segments, rest);
    segments
}

fn database<'src>(backing: &'src ModelBacking) -> Database<'src> {
    Database {
        name: backing.binding,
        kind: match backing.kind {
            BackingKind::D1 => DatabaseKind::D1,
            BackingKind::DurableObject => DatabaseKind::DurableObject,
        },
    }
}
