use std::collections::HashMap;

use idl::{CloesceIdl, IncludeTree, Model, ModelBacking, NavigationField, TemplateSegment};

use crate::query::select::plan::{
    JoinKeys, Mapping, Select, SelectArg, SelectPlan, SelectStep, SqlArgument, SqlSegment,
    TableParent,
};
use crate::query::{Database, DatabaseKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectOperation {
    Get,
    List,
}

/// The runtime parameter that bounds the number of root rows a `List` returns.
const LIMIT_PARAM: &str = "limit";

/// Converts a [SelectOperation] into a [SelectPlan], detailing how the runtime should execute
/// the operation against the underlying data sources.
///
/// [plan] will create as few [crate::query::select::plan::SelectStage] as possible to hydrate
/// the requested [IncludeTree].
pub fn plan<'src>(
    operation: SelectOperation,
    model: &str,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
) -> SelectPlan<'src> {
    let mut plan = SelectPlan::default();

    let Some(model) = idl.models.get(model) else {
        // Fail silently if the model is not found
        return plan;
    };

    let mapping = match operation {
        SelectOperation::Get => Mapping::one(),
        SelectOperation::List => Mapping::many(),
    };

    let mut params = Params::default();
    for f in &model.route_fields {
        // Every route field is required to be supplied by the runtime
        // This includes Durabe Object shard fields, which are always route fields.
        let name = f.name.as_ref();
        params.map.insert(name, (SelectArg::Param(name.into()), 0));
    }
    if operation == SelectOperation::Get {
        for c in &model.primary_columns {
            // In Get operations, every primary key column is required to be supplied
            // by the runtime
            let name = c.field.name.as_ref();
            params.map.insert(name, (SelectArg::Param(name.into()), 0));
        }
    }

    // The root table (id 0) hydrates the top-level result.
    let root = plan.register_table(None);

    if let Some(backing) = model.backing.as_ref().filter(|_| model.uses_sqlite()) {
        // Every root shard field value comes from runtime parameters
        let shard = backing
            .fields
            .iter()
            .map(|f| (*f, SelectArg::Param((*f).into())))
            .collect::<Vec<_>>();

        // Every route field (shard fields included) rides onto each result row,
        // sourced from runtime parameters.
        let route_fields = model
            .route_fields
            .iter()
            .map(|f| (f.name.as_ref(), SelectArg::Param(f.name.clone())))
            .collect::<Vec<_>>();

        let query = match operation {
            SelectOperation::Get => {
                // GET is always a fetch-by-pk. Gather all WHERE predicates, e.g.
                // `"id" = ` Bind(0), ... `"name" = ` Bind(N-1)
                let predicates = model
                    .primary_columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        vec![
                            SqlSegment::Literal(format!("\"{}\" = ", c.field.name)),
                            SqlSegment::Bind(i),
                        ]
                    })
                    .collect::<Vec<_>>();

                // Every primary key column's value comes from runtime parameters
                let arguments = model
                    .primary_columns
                    .iter()
                    .map(|c| SqlArgument::scalar(SelectArg::Param(c.field.name.as_ref().into())))
                    .collect::<Vec<_>>();

                Select::Sql {
                    database: backing.into(),
                    sql: select_sql(model, &predicates, None),
                    arguments,
                    shard,
                    mapping,
                    route_fields,
                }
            }
            SelectOperation::List => {
                let pks = &model.primary_columns;

                // ex: `"id" > ` Bind(0) or `("region", "num") > (` Bind(0) `, ` Bind(1) `)`
                let predicate = if pks.len() == 1 {
                    vec![
                        SqlSegment::Literal(format!("\"{}\" > ", pks[0].field.name)),
                        SqlSegment::Bind(0),
                    ]
                } else {
                    let cols = pks
                        .iter()
                        .map(|c| format!("\"{}\"", c.field.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let mut predicate = vec![SqlSegment::Literal(format!("({cols}) > ("))];
                    for i in 0..pks.len() {
                        if i > 0 {
                            predicate.push(SqlSegment::Literal(", ".into()));
                        }
                        predicate.push(SqlSegment::Bind(i));
                    }
                    predicate.push(SqlSegment::Literal(")".into()));
                    predicate
                };

                // One `lastSeen_<pk>` cursor value per pk column, then `limit`.
                let arguments = pks
                    .iter()
                    .map(|c| SelectArg::Param(format!("lastSeen_{}", c.field.name).into()))
                    .chain(std::iter::once(SelectArg::Param(LIMIT_PARAM.into())))
                    .map(SqlArgument::scalar)
                    .collect::<Vec<_>>();

                Select::Sql {
                    database: backing.into(),
                    sql: select_sql(model, &[predicate], Some(pks.len())),
                    arguments,
                    shard,
                    mapping,
                    route_fields,
                }
            }
        };

        plan.stage_at(0)
            .steps
            .push(SelectStep { query, table: root });
    } else {
        // A non-sqlite-backed model has no database to select from, just a state
        // synthesized from its route fields, which must be supplied by the runtime.
        //
        // Without a SQLite backing, a [MapCardinality::Many] model is coerced into
        // a singleton list.
        let fields = model
            .route_fields
            .iter()
            .map(|f| (f.name.as_ref(), SelectArg::Param(f.name.clone())))
            .collect();

        plan.stage_at(0).steps.push(SelectStep {
            query: Select::Synthesize {
                fields,
                cardinality: mapping.cardinality,
            },
            table: root,
        });
    }

    hydrate_model(model, idl, tree, &mut plan, &params, root, 0);

    plan
}

fn hydrate_model<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut SelectPlan<'src>,
    params: &Params<'src>,
    table: usize,
    stage: usize,
) {
    select_keys(model, idl, tree, plan, params, table, stage);
    select_navs(model, idl, tree, plan, params, table, stage);
}

/// Emit one [Select::Key] [SelectStep] per included R2 and KV field of `model`.
///
/// The step runs in the latest stage at which any of its key-template or shard inputs becomes
/// readable: `stage` for inputs sourced from params, `owner stage + 1` for inputs sourced from a
/// hydrated result.
fn select_keys<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut SelectPlan<'src>,
    params: &Params<'src>,
    table: usize,
    stage: usize,
) {
    let mut push = |field: &'src str,
                    database: Database<'src>,
                    key: &'src [TemplateSegment<'src, &'src str>],
                    shard_fields: &[&'src str]| {
        if !tree.0.contains_key(field) {
            // Include tree does not request this field, skip.
            return;
        }

        // The key template and shard fields are owned by `model` at `table`, hydrated at `stage`.
        let mut inputs = shard_fields.to_vec();
        let segments = key
            .iter()
            .map(|segment| match segment {
                TemplateSegment::Literal(text) => TemplateSegment::Literal(text.clone()),
                TemplateSegment::Value(arg) => {
                    inputs.push(*arg);
                    TemplateSegment::Value(params.arg(table, arg))
                }
            })
            .collect::<Vec<_>>();
        let shard = shard_fields
            .iter()
            .map(|f| (*f, params.arg(table, f)))
            .collect();

        // The step runs no earlier than the latest stage any of its inputs becomes readable.
        let step_stage = inputs
            .iter()
            .map(|f| params.min_stage(stage, f))
            .fold(stage, usize::max);

        let key_table = plan.register_table(Some(TableParent { table, field }));

        plan.stage_at(step_stage).steps.push(SelectStep {
            query: Select::Key {
                database,
                segments,
                shard,
            },
            table: key_table,
        });
    };

    for r2 in &model.r2_fields {
        let database = Database {
            name: r2.binding,
            kind: DatabaseKind::R2,
        };

        push(r2.field.name.as_ref(), database, &r2.segments, &[]);
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

        push(kv.field.name.as_ref(), database, &kv.segments, shard);
    }
}

/// Recurse the include tree, emitting one nav step per included [NavigationField].
///
/// Each nav key local is owned by the parent model at `parent_path` (hydrated at `depth`); its
/// source is either a runtime param (readable from stage 0) or the parent's hydrated result
/// (readable at `depth + 1`). The nav runs in the latest stage any of its key locals becomes
/// readable, and the child inherits every key's source (keyed by its target field).
fn select_navs<'src>(
    model: &'src Model<'src>,
    idl: &'src CloesceIdl<'src>,
    tree: &IncludeTree<'src>,
    plan: &mut SelectPlan<'src>,
    params: &Params<'src>,
    parent_table: usize,
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

        let nav_table = plan.register_table(Some(TableParent {
            table: parent_table,
            field: nav.field.name.as_ref(),
        }));

        // The nav runs no earlier than the latest stage any of its key locals (owned by the
        // parent model at `parent_table`, hydrated at `depth`) becomes readable.
        let stage = nav
            .keys
            .iter()
            .map(|k| params.min_stage(depth, k.local))
            .fold(depth, usize::max);

        // The child inherits every nav key: its target field is fixed by the local's source.
        let child_params = Params {
            map: nav
                .keys
                .iter()
                .map(|k| {
                    (
                        k.target,
                        (
                            params.arg(parent_table, k.local),
                            params.min_stage(depth, k.local),
                        ),
                    )
                })
                .collect(),
        };

        if let Some(backing) = target.backing.as_ref().filter(|_| target.uses_sqlite()) {
            select_nav(
                nav,
                target,
                backing,
                params,
                parent_table,
                nav_table,
                plan,
                stage,
            );
        } else {
            synthesize_nav(nav, params, parent_table, nav_table, plan, stage);
        }

        hydrate_model(target, idl, subtree, plan, &child_params, nav_table, stage);
    }

    /// Emit a [Select::Synthesize] nav step
    fn synthesize_nav<'src>(
        nav: &'src NavigationField<'src>,
        params: &Params<'src>,
        parent_table: usize,
        nav_table: usize,
        plan: &mut SelectPlan<'src>,
        stage: usize,
    ) {
        let fields = nav
            .keys
            .iter()
            .map(|k| (k.target, params.arg(parent_table, k.local)))
            .collect();

        plan.stage_at(stage).steps.push(SelectStep {
            query: Select::Synthesize {
                fields,
                cardinality: nav.cardinality.clone().into(),
            },
            table: nav_table,
        });
    }

    /// Emit a single SQL step for `nav` targeting SQLite-backed `target` at `stage`,
    /// attaching any route fields the target carries (shard fields included) onto its rows.
    #[allow(clippy::too_many_arguments)]
    fn select_nav<'src>(
        nav: &'src NavigationField<'src>,
        target: &'src Model<'src>,
        backing: &'src ModelBacking<'src>,
        params: &Params<'src>,
        parent_table: usize,
        nav_table: usize,
        plan: &mut SelectPlan<'src>,
        stage: usize,
    ) {
        let is_shard = |t: &str| backing.fields.contains(&t);
        let is_route = |t: &str| target.route_fields.iter().any(|f| f.name == t) && !is_shard(t);

        let shard = nav
            .keys
            .iter()
            .filter(|k| is_shard(k.target))
            .map(|k| (k.target, params.arg(parent_table, k.local)))
            .collect();

        // Remaining (non-shard, non-route) keys drive the nav's `IN` predicate.
        let sql_keys = nav
            .keys
            .iter()
            .filter(|k| !is_shard(k.target) && !is_route(k.target))
            .collect::<Vec<_>>();

        // - A single key spreads its distinct values into a scalar `"a" IN (?, ?, ...)`
        // - multiple keys spread together as row-value tuples in `("a", "b") IN (VALUES (?, ?), ...)`
        let (predicates, arguments) = if let [k] = sql_keys.as_slice() {
            let predicate = vec![
                SqlSegment::Literal(format!("\"{}\" IN (", k.target)),
                SqlSegment::Bind(0),
                SqlSegment::Literal(")".into()),
            ];
            let arguments = vec![SqlArgument::spread(params.arg(parent_table, k.local))];
            (vec![predicate], arguments)
        } else if sql_keys.is_empty() {
            (vec![], vec![])
        } else {
            let cols = sql_keys
                .iter()
                .map(|k| format!("\"{}\"", k.target))
                .collect::<Vec<_>>()
                .join(", ");
            let predicate = vec![
                SqlSegment::Literal(format!("({cols}) IN (VALUES ")),
                SqlSegment::Bind(0),
                SqlSegment::Literal(")".into()),
            ];
            let group = sql_keys
                .iter()
                .map(|k| params.arg(parent_table, k.local))
                .collect();
            let arguments = vec![SqlArgument::tuple(group)];
            (vec![predicate], arguments)
        };

        let join = nav
            .keys
            .iter()
            .filter(|k| !is_route(k.target))
            .map(|k| JoinKeys {
                parent_key: k.local,
                child_key: k.target,
            })
            .collect();

        // Route fields (shard fields included) ride onto the rows the SQL step produces.
        let route_fields = nav
            .keys
            .iter()
            .filter(|k| is_shard(k.target) || is_route(k.target))
            .map(|k| (k.target, params.arg(parent_table, k.local)))
            .collect();

        plan.stage_at(stage).steps.push(SelectStep {
            query: Select::Sql {
                database: backing.into(),
                sql: select_sql(target, &predicates, None),
                arguments,
                shard,
                mapping: Mapping {
                    cardinality: nav.cardinality.clone().into(),
                    join,
                },
                route_fields,
            },
            table: nav_table,
        });
    }
}

/// Build an ordered SQL `SELECT` over the model's columns as [SqlSegment]s, ordered by
/// primary key column(s). Each predicate is already split into its own segments, and
/// `limit_bind` (0-based) appends a trailing `LIMIT` placeholder.
fn select_sql(
    model: &Model,
    preds: &[Vec<SqlSegment>],
    limit_bind: Option<usize>,
) -> Vec<SqlSegment> {
    let columns = model
        .primary_columns
        .iter()
        .chain(&model.columns)
        .map(|c| format!("\"{}\"", c.field.name))
        .collect::<Vec<_>>()
        .join(", ");
    let order = model
        .primary_columns
        .iter()
        .map(|c| format!("\"{}\" ASC", c.field.name))
        .collect::<Vec<_>>()
        .join(", ");

    // ex: `SELECT "id", "name" FROM "Horse"`
    let mut segments = vec![SqlSegment::Literal(format!(
        "SELECT {columns} FROM \"{}\"",
        model.name
    ))];

    if !preds.is_empty() {
        // ... WHERE "id" = ?1 AND "name" = ?2
        segments.push(SqlSegment::Literal(" WHERE ".into()));
        for (i, pred) in preds.iter().enumerate() {
            if i > 0 {
                segments.push(SqlSegment::Literal(" AND ".into()));
            }
            segments.extend(pred.iter().cloned());
        }
    }

    // ... ORDER BY "id" ASC, "name" ASC
    segments.push(SqlSegment::Literal(format!(" ORDER BY {order}")));

    if let Some(bind) = limit_bind {
        // ... LIMIT ?N
        segments.push(SqlSegment::Literal(" LIMIT ".into()));
        segments.push(SqlSegment::Bind(bind));
    }

    merge_literals(segments)
}

/// Coalesce adjacent [SqlSegment::Literal]s into one, so a composed statement carries a
/// single literal between binds.
fn merge_literals(segments: Vec<SqlSegment>) -> Vec<SqlSegment> {
    segments.into_iter().fold(Vec::new(), |mut acc, seg| {
        match (acc.last_mut(), seg) {
            (Some(SqlSegment::Literal(prev)), SqlSegment::Literal(text)) => prev.push_str(&text),
            (_, seg) => acc.push(seg),
        }
        acc
    })
}

/// Maps a field on the current model to the [SelectArg] that fixes its value,
/// paired with the first stage at which that value is readable.
#[derive(Default)]
struct Params<'src> {
    map: HashMap<&'src str, (SelectArg<'src>, usize)>,
}

impl<'src> Params<'src> {
    /// Resolve `field`, owned by the model at `table`, to its source.
    fn arg(&self, table: usize, field: &'src str) -> SelectArg<'src> {
        self.map
            .get(field)
            .map(|(arg, _)| arg.clone())
            .unwrap_or(SelectArg::Field { table, field })
    }

    /// The first stage at which `field`'s value is readable.
    fn min_stage(&self, parent_stage: usize, field: &'src str) -> usize {
        self.map
            .get(field)
            .map_or(parent_stage + 1, |(_, stage)| *stage)
    }
}
