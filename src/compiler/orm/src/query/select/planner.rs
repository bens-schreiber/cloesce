use std::collections::HashMap;

use idl::{CloesceIdl, IncludeTree, Model, ModelBacking, NavigationField};

use crate::query::select::plan::{JoinKeys, Mapping, Select, SelectArg, SelectPlan, SelectStep};
use crate::query::{Database, DatabaseKind, TemplateSegment};

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
                    .map(|c| SelectArg::Param(c.field.name.as_ref().into()))
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

                // ex: `"id" > ?1` or `("region", "num") > (?1, ?2)`
                let placeholders = (1..=pks.len()).map(|i| format!("?{i}")).collect::<Vec<_>>();
                let predicate = if pks.len() == 1 {
                    format!("\"{}\" > {}", pks[0].field.name, placeholders[0])
                } else {
                    let cols = pks
                        .iter()
                        .map(|c| format!("\"{}\"", c.field.name))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({}) > ({})", cols, placeholders.join(", "))
                };

                // One `lastSeen_<pk>` cursor value per pk column, then `limit`.
                let arguments = pks
                    .iter()
                    .map(|c| SelectArg::Param(format!("lastSeen_{}", c.field.name).into()))
                    .chain(std::iter::once(SelectArg::Param(LIMIT_PARAM.into())))
                    .collect::<Vec<_>>();

                Select::Sql {
                    database: backing.into(),
                    sql: select_sql(model, &[predicate], Some(pks.len() + 1)),
                    arguments,
                    shard,
                    mapping,
                    route_fields,
                }
            }
        };

        plan.stage_at(0).steps.push(SelectStep {
            query,
            result: vec![],
        });
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
    plan: &mut SelectPlan<'src>,
    params: &Params<'src>,
    path: &[&'src str],
    stage: usize,
) {
    select_keys(model, idl, tree, plan, params, path, stage);
    select_navs(model, idl, tree, plan, params, path, stage);
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
    path: &[&'src str],
    stage: usize,
) {
    let mut push =
        |field: &'src str, database: Database<'src>, key: &'src str, shard_fields: &[&'src str]| {
            if !tree.0.contains_key(field) {
                // Include tree does not request this field, skip.
                return;
            }

            // The key template and shard fields are owned by `model` at `path`, hydrated at `stage`.
            let mut inputs = shard_fields.to_vec();
            let segments = TemplateSegment::parse(key, |arg| {
                inputs.push(arg);
                params.arg(path, arg)
            });
            let shard = shard_fields
                .iter()
                .map(|f| (*f, params.arg(path, f)))
                .collect();

            // The step runs no earlier than the latest stage any of its inputs becomes readable.
            let step_stage = inputs
                .iter()
                .map(|f| params.min_stage(stage, f))
                .fold(stage, usize::max);

            let mut result = path.to_vec();
            result.push(field);

            plan.stage_at(step_stage).steps.push(SelectStep {
                query: Select::Key {
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

        // The nav runs no earlier than the latest stage any of its key locals (owned by the
        // parent model at `parent_path`, hydrated at `depth`) becomes readable.
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
                            params.arg(parent_path, k.local),
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
                parent_path,
                &path,
                plan,
                stage,
            );
        } else {
            synthesize_nav(nav, params, parent_path, &path, plan, stage);
        }

        hydrate_model(target, idl, subtree, plan, &child_params, &path, stage);
    }

    /// Emit a [Select::Synthesize] nav step
    fn synthesize_nav<'src>(
        nav: &'src NavigationField<'src>,
        params: &Params<'src>,
        parent_path: &[&'src str],
        path: &[&'src str],
        plan: &mut SelectPlan<'src>,
        stage: usize,
    ) {
        let fields = nav
            .keys
            .iter()
            .map(|k| (k.target, params.arg(parent_path, k.local)))
            .collect();

        plan.stage_at(stage).steps.push(SelectStep {
            query: Select::Synthesize {
                fields,
                cardinality: nav.cardinality.clone().into(),
            },
            result: path.to_vec(),
        });
    }

    /// Emit a single SQL step for `nav` targeting SQLite-backed `target` at `stage`,
    /// attaching any route fields the target carries (shard fields included) onto its rows.
    fn select_nav<'src>(
        nav: &'src NavigationField<'src>,
        target: &'src Model<'src>,
        backing: &'src ModelBacking<'src>,
        params: &Params<'src>,
        parent_path: &[&'src str],
        path: &[&'src str],
        plan: &mut SelectPlan<'src>,
        stage: usize,
    ) {
        let is_shard = |t: &str| backing.fields.contains(&t);
        let is_route = |t: &str| target.route_fields.iter().any(|f| f.name == t) && !is_shard(t);

        let shard = nav
            .keys
            .iter()
            .filter(|k| is_shard(k.target))
            .map(|k| (k.target, params.arg(parent_path, k.local)))
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
        let bindings = sql_keys
            .iter()
            .map(|k| params.arg(parent_path, k.local))
            .collect();

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
            .map(|k| (k.target, params.arg(parent_path, k.local)))
            .collect();

        plan.stage_at(stage).steps.push(SelectStep {
            query: Select::Sql {
                database: backing.into(),
                sql: select_sql(target, &predicates, None),
                arguments: bindings,
                shard,
                mapping: Mapping {
                    cardinality: nav.cardinality.clone().into(),
                    join,
                },
                route_fields,
            },
            result: path.to_vec(),
        });
    }
}

/// Build an ordered SQL `SELECT` over the model's columns. Ordered by
/// primary key column(s).
fn select_sql(model: &Model, preds: &[String], limit_placeholder: Option<usize>) -> String {
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

    if let Some(n) = limit_placeholder {
        // ... LIMIT ?N
        sql.push_str(&format!(" LIMIT ?{n}"));
    }

    sql
}

/// Maps a field on the current model to the [SelectArg] that fixes its value,
/// paired with the first stage at which that value is readable.
#[derive(Default)]
struct Params<'src> {
    map: HashMap<&'src str, (SelectArg<'src>, usize)>,
}

impl<'src> Params<'src> {
    /// Resolve `field`, owned by the model at `parent_path`, to its source.
    fn arg(&self, parent_path: &[&'src str], field: &'src str) -> SelectArg<'src> {
        self.map
            .get(field)
            .map(|(arg, _)| arg.clone())
            .unwrap_or_else(|| {
                let mut path = parent_path.to_vec();
                path.push(field);
                SelectArg::Result(path)
            })
    }

    /// The first stage at which `field`'s value is readable.
    fn min_stage(&self, parent_stage: usize, field: &'src str) -> usize {
        self.map
            .get(field)
            .map_or(parent_stage + 1, |(_, stage)| *stage)
    }
}
