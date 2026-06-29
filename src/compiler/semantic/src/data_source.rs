use std::collections::HashSet;

use idl::{IncludeTree, Model, NavigationCardinality};
use indexmap::IndexMap;

pub mod analysis {
    use std::ops::Not;

    use frontend::{ParsedIncludeTree, Spd, Symbol, Tag};
    use idl::{
        CidlType, DataSource, DataSourceGetMethod, DataSourceGetMethodParam, DataSourceMethod,
        ValidatedField, Validator,
    };

    use crate::{
        SymbolTable,
        err::{ErrorSink, SemanticError},
        is_valid_sql_type, resolve_inject, resolve_validator_tags,
    };

    use super::{IncludeTree, IndexMap, Model, include_dfs};

    enum DsMethodKind {
        Scalar,
        Body,
    }

    /// Validates every [DataSource], returning a list of Model namespaces and their associated [DataSource]s.
    pub fn analyze<'src, 'p>(
        models: &IndexMap<&'src str, Model<'src>>,
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Vec<(&'src str, DataSource<'src>)> {
        let mut res = Vec::new();

        for ds in &table.data_sources {
            // Validate tags
            let mut is_internal = false;
            for tag in &ds.symbol.tags {
                if matches!(&tag.inner, Tag::Internal).not() {
                    sink.push(SemanticError::TagInvalidInContext {
                        tag,
                        symbol: &ds.symbol,
                    });
                    continue;
                }
                is_internal = true;
            }

            // Validate the model reference
            let Some(model_sym) = table.models.get(ds.model.name).map(|m| &m.symbol) else {
                sink.push(SemanticError::DataSourceUnknownModelReference { source: &ds.symbol });
                continue;
            };

            let Some(model) = models.get(model_sym.name) else {
                // Model must be invalid for some reason, skip.
                continue;
            };

            // Validate include tree via BFS. A missing tree falls back to the
            // generated default during expansion and needs no validation.
            let mut q = std::collections::VecDeque::new();
            if let Some(tree) = &ds.tree {
                q.push_back((tree, model));
            }
            while let Some((node, parent_model)) = q.pop_front() {
                for (field, child) in &node.0 {
                    // Check navigation properties
                    let nav = parent_model
                        .navigation_fields
                        .iter()
                        .find(|nav| nav.field.name == field.name);

                    if let Some(nav) = nav {
                        // Navigate into the adjacent model
                        if let Some(adj_model) = models.get(nav.model_reference) {
                            q.push_back((child, adj_model));
                        }
                        continue;
                    }

                    // Check KV properties
                    if parent_model
                        .kv_fields
                        .iter()
                        .any(|kv| kv.field.name == field.name)
                    {
                        continue;
                    }

                    // Check R2 properties
                    if parent_model
                        .r2_fields
                        .iter()
                        .any(|r2| r2.field.name == field.name)
                    {
                        continue;
                    }

                    sink.push(SemanticError::DataSourceInvalidIncludeTreeReference {
                        source: &ds.symbol,
                        model: model_sym,
                        field,
                    });
                }
            }

            // For each verb: if the user declared a stub, validate and capture it.
            // Otherwise a default-valued method is left in place for the expansion pass to fill.
            let list = ds
                .list
                .as_ref()
                .map(|method| {
                    let mut parameters = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                validate_ds_param(p, &ds.symbol, DsMethodKind::Scalar, sink);
                            if let Some(tag) = instance_tag {
                                // Not allowed on list methods
                                sink.push(SemanticError::TagInvalidInContext { tag, symbol: p });
                            }
                            ValidatedField {
                                name: p.name.into(),
                                cidl_type: p.cidl_type.clone(),
                                validators,
                            }
                        })
                        .collect::<Vec<_>>();

                    let (injected, durable_target) =
                        resolve_inject(&method.inner.method, &mut parameters, table, sink);

                    DataSourceMethod {
                        parameters,
                        injected,
                        is_stub: true,
                        durable_target,
                    }
                })
                .unwrap_or_default();

            let get = ds
                .get
                .as_ref()
                .map(|method| {
                    let (mut fields, instance_fields): (Vec<_>, Vec<_>) = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                validate_ds_param(p, &ds.symbol, DsMethodKind::Scalar, sink);

                            if let Some(tag) = instance_tag {
                                let is_field = model.columns.iter().any(|c| c.field.name == p.name)
                                    || model
                                        .primary_columns
                                        .iter()
                                        .any(|pk| pk.field.name == p.name);
                                if !is_field {
                                    sink.push(SemanticError::InstanceTagOnNonField {
                                        tag,
                                        source: &ds.symbol,
                                        param: p,
                                    });
                                }
                            }

                            (
                                ValidatedField {
                                    name: p.name.into(),
                                    cidl_type: p.cidl_type.clone(),
                                    validators,
                                },
                                instance_tag.is_some(),
                            )
                        })
                        .unzip();

                    let (injected, durable_target) =
                        resolve_inject(&method.inner.method, &mut fields, table, sink);

                    let parameters = fields
                        .into_iter()
                        .zip(instance_fields)
                        .map(|(parameter, instance_field)| DataSourceGetMethodParam {
                            parameter,
                            instance_field,
                        })
                        .collect();

                    DataSourceGetMethod {
                        parameters,
                        injected,
                        is_stub: true,
                        durable_target,
                    }
                })
                .unwrap_or_default();

            let save = ds
                .save
                .as_ref()
                .map(|method| {
                    let mut parameters = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                validate_ds_param(p, &ds.symbol, DsMethodKind::Body, sink);
                            if let Some(tag) = instance_tag {
                                // Not allowed on save methods
                                sink.push(SemanticError::TagInvalidInContext { tag, symbol: p });
                            }
                            ValidatedField {
                                name: p.name.into(),
                                cidl_type: p.cidl_type.clone(),
                                validators,
                            }
                        })
                        .collect::<Vec<_>>();

                    let (injected, durable_target) =
                        resolve_inject(&method.inner.method, &mut parameters, table, sink);

                    DataSourceMethod {
                        parameters,
                        injected,
                        is_stub: true,
                        durable_target,
                    }
                })
                .unwrap_or_default();

            res.push((
                model_sym.name,
                DataSource {
                    name: ds.symbol.name,
                    tree: match &ds.tree {
                        Some(tree) => parsed_include_tree_to_idl(tree),
                        None => include_dfs(
                            models,
                            model_sym.name,
                            &mut std::collections::HashSet::new(),
                        ),
                    },
                    list,
                    get,
                    save,
                    include_query: String::new(),
                    get_query: String::new(),
                    list_query: String::new(),
                    is_internal,
                },
            ));
        }

        res
    }

    /// Validates that a parameter has a sensible type for its method kind and only
    /// carries tags valid in a data source method parameter.
    ///
    /// Returns the resolved validators and the `instance` tag if present.
    fn validate_ds_param<'src, 'p>(
        param: &'p Symbol<'src>,
        source_sym: &'p Symbol<'src>,
        kind: DsMethodKind,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> (Vec<Validator<'src>>, Option<&'p Spd<Tag<'src>>>) {
        let mut instance_tag = None;

        for tag in param.tags.iter() {
            match &tag.inner {
                Tag::Instance => instance_tag = Some(tag),
                Tag::Validator { .. } => {}
                _ => sink.push(SemanticError::TagInvalidInContext { tag, symbol: param }),
            }
        }

        let valid_type = match kind {
            DsMethodKind::Scalar => is_valid_sql_type(&param.cidl_type),
            DsMethodKind::Body => !matches!(param.cidl_type.root_type(), CidlType::Stream),
        };
        if !valid_type {
            sink.push(SemanticError::DataSourceInvalidMethodParam {
                source: source_sym,
                param,
            });
        }

        match resolve_validator_tags(param) {
            Ok(v) => (v, instance_tag),
            Err(errs) => {
                sink.extend(errs);
                (vec![], None)
            }
        }
    }

    fn parsed_include_tree_to_idl<'src>(tree: &ParsedIncludeTree<'src>) -> IncludeTree<'src> {
        IncludeTree(
            tree.0
                .iter()
                .map(|(sym, child)| (sym.name.into(), parsed_include_tree_to_idl(child)))
                .collect(),
        )
    }
}

pub mod expansion {
    use idl::{
        BackingKind, CidlType, CloesceIdl, DataSource, DataSourceGetMethod,
        DataSourceGetMethodParam, DataSourceMethod, DurableTarget, ModelBacking, Number,
        ValidatedField, Validator, model_bindings,
    };
    use orm::select::SelectModel;

    use super::{HashSet, Model, include_dfs};

    #[derive(Default)]
    struct GeneratedDataSource<'src> {
        include_query: String,
        get_query: String,
        list_query: String,
        get: Option<DataSourceGetMethod<'src>>,
        list: Option<DataSourceMethod<'src>>,
        save: Option<DataSourceMethod<'src>>,
    }

    /// Adds a `Default` [DataSource] to every model that doesn't have one.
    ///
    /// The default include tree contains all KV, R2, 1:1, 1:N and M:N relationships,
    /// but stops recursing after a 1:N or M:N to avoid join explosion.
    pub fn expand(idl: &mut CloesceIdl) {
        let needs_default = idl
            .models
            .values()
            .filter(|m| m.default_data_source().is_none() && m.has_data())
            .map(|m| m.name)
            .collect::<Vec<&str>>();

        for name in needs_default {
            let tree = include_dfs(&idl.models, name, &mut HashSet::new());
            idl.models.get_mut(name).unwrap().data_sources.insert(
                "Default",
                DataSource {
                    name: "Default",
                    tree,
                    list: DataSourceMethod::default(),
                    get: DataSourceGetMethod::default(),
                    save: DataSourceMethod::default(),
                    include_query: String::new(),
                    get_query: String::new(),
                    list_query: String::new(),
                    is_internal: false,
                },
            );
        }

        let generated =
            idl.models
                .values()
                .flat_map(|model| {
                    model.data_sources.iter().map(|(ds_name, ds)| {
                        (model.name, *ds_name, generate_source(idl, model, ds))
                    })
                })
                .collect::<Vec<_>>();

        for (model_name, ds_name, generated) in generated {
            let Some(ds) = idl
                .models
                .get_mut(model_name)
                .and_then(|m| m.data_sources.get_mut(ds_name))
            else {
                continue;
            };

            ds.include_query = generated.include_query;
            ds.get_query = generated.get_query;
            ds.list_query = generated.list_query;

            // Only replace a verb the user did not declare as a stub.
            if let Some(get) = generated.get.filter(|_| !ds.get.is_stub) {
                ds.get = get;
            }
            if let Some(list) = generated.list.filter(|_| !ds.list.is_stub) {
                ds.list = list;
            }
            if let Some(save) = generated.save.filter(|_| !ds.save.is_stub) {
                ds.save = save;
            }
        }
    }

    fn generate_source<'src>(
        idl: &CloesceIdl<'src>,
        model: &Model<'src>,
        ds: &DataSource<'src>,
    ) -> GeneratedDataSource<'src> {
        // If a model is Durable Object backed, by default data source methods
        // will include each shard field, and execute in that DO's context
        let (shard_fields, durable_target) = if let Some(ModelBacking {
            kind: BackingKind::DurableObject,
            fields,
            ..
        }) = &model.backing
        {
            let shard_fields = model
                .route_fields
                .iter()
                .filter(|f| fields.iter().any(|bf| *bf == f.name))
                .map(|f| (*f).clone())
                .collect::<Vec<_>>();

            let durable_target = DurableTarget {
                binding: model.backing.as_ref().unwrap().binding,
                shard_args: shard_fields
                    .iter()
                    .map(|f| &f.name)
                    .cloned()
                    .collect::<Vec<_>>(),
            };

            (shard_fields, Some(durable_target))
        } else {
            (vec![], None)
        };

        let save_model_param = || ValidatedField {
            name: "model".into(),
            cidl_type: CidlType::Partial {
                object_name: model.name,
            },
            validators: vec![],
        };

        // Seek-pagination params: `lastSeen_<pk>...`, `limit`.
        let list_params = || {
            model
                .primary_columns
                .iter()
                .map(|pk| ValidatedField {
                    name: format!("lastSeen_{}", pk.field.name).into(),
                    ..pk.field.clone()
                })
                .chain(std::iter::once(ValidatedField {
                    name: "limit".into(),
                    cidl_type: CidlType::Int,
                    validators: vec![Validator::GreaterThan(Number::Int(0))],
                }))
        };

        let injected = model_bindings(idl, model, Some(&ds.tree));

        if model.uses_sqlite() {
            let include_query =
                SelectModel::query(model.name, None, Some(&ds.tree), idl).unwrap_or_default();

            let get_params = shard_fields
                .iter()
                .cloned()
                .chain(model.primary_columns.iter().map(|pk| pk.field.clone()))
                .map(|parameter| DataSourceGetMethodParam {
                    parameter,
                    instance_field: true,
                })
                .collect::<Vec<_>>();

            return GeneratedDataSource {
                get_query: build_get_query(model, &include_query),
                list_query: build_list_query(model, &include_query),
                include_query,
                get: Some(DataSourceGetMethod {
                    parameters: get_params,
                    injected: injected.clone(),
                    is_stub: false,
                    durable_target: durable_target.clone(),
                }),
                list: Some(DataSourceMethod {
                    parameters: shard_fields.iter().cloned().chain(list_params()).collect(),
                    injected: injected.clone(),
                    is_stub: false,
                    durable_target: durable_target.clone(),
                }),
                save: Some(DataSourceMethod {
                    parameters: shard_fields
                        .iter()
                        .cloned()
                        .chain(std::iter::once(save_model_param()))
                        .collect(),
                    injected,
                    is_stub: false,
                    durable_target,
                }),
            };
        }

        GeneratedDataSource {
            get: Some(DataSourceGetMethod {
                // All route fields (which includes shard fields)
                parameters: model
                    .route_fields
                    .iter()
                    .map(|f| DataSourceGetMethodParam {
                        parameter: f.clone(),
                        instance_field: true,
                    })
                    .collect(),
                injected: injected.clone(),
                is_stub: false,
                durable_target: durable_target.clone(),
            }),
            // Save does not need all route fields, just the shard fields to populate the
            // DO context.
            //
            // TODO: Could we populate the DO from the `model` param object instead
            // (like we do with the route fields)? Should `save` just be an instance method
            // on the model?
            save: Some(DataSourceMethod {
                parameters: shard_fields
                    .iter()
                    .cloned()
                    .chain(std::iter::once(save_model_param()))
                    .collect(),
                injected: injected.clone(),
                is_stub: false,
                durable_target: durable_target.clone(),
            }),
            ..GeneratedDataSource::default()
        }
    }

    /// Basic primary key fetch SELECT, e.g. `{include_query} WHERE "Model"."pk" = ?1`.
    fn build_get_query(model: &Model, include_query: &str) -> String {
        let cols = model
            .primary_columns
            .iter()
            .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
            .collect::<Vec<_>>();
        let placeholders = (1..=cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>();

        if cols.len() == 1 {
            format!("{include_query} WHERE {} = {}", cols[0], placeholders[0])
        } else {
            format!(
                "{include_query} WHERE ({}) = ({})",
                cols.join(", "),
                placeholders.join(", ")
            )
        }
    }

    /// Seek-pagination SELECT keyed off the primary key:
    /// `{include_query} WHERE pk > ?1 ORDER BY pk ASC LIMIT ?N`.
    fn build_list_query(model: &Model, include_query: &str) -> String {
        let cols = model
            .primary_columns
            .iter()
            .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
            .collect::<Vec<_>>();
        let placeholders = (1..=cols.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>();
        let limit = format!("?{}", cols.len() + 1);
        let order = cols
            .iter()
            .map(|c| format!("{c} ASC"))
            .collect::<Vec<_>>()
            .join(", ");

        let where_clause = if cols.len() == 1 {
            format!("{} > {}", cols[0], placeholders[0])
        } else {
            format!("({}) > ({})", cols.join(", "), placeholders.join(", "))
        };

        format!("{include_query} WHERE {where_clause} ORDER BY {order} LIMIT {limit}")
    }
}

pub fn include_dfs<'src>(
    models: &IndexMap<&'src str, Model<'src>>,
    current_model: &'src str,
    visited: &mut HashSet<&'src str>,
) -> IncludeTree<'src> {
    if !visited.insert(current_model) {
        return IncludeTree::default();
    }

    let mut current_node = IncludeTree::default();

    let model = models.get(current_model).unwrap();
    for nav in &model.navigation_fields {
        match nav.cardinality {
            NavigationCardinality::One => {
                if nav.model_reference == current_model {
                    // Self-referencing 1:1. Include but don't recurse.
                    current_node
                        .0
                        .insert(nav.field.name.clone(), IncludeTree::default());
                    continue;
                }

                if visited.contains(&nav.model_reference) {
                    // Skip to avoid circular reference
                    continue;
                }
                let new_node = include_dfs(models, nav.model_reference, visited);
                current_node.0.insert(nav.field.name.clone(), new_node);
            }
            NavigationCardinality::Many => {
                // Include the related model as a leaf, but don't recurse.
                current_node
                    .0
                    .insert(nav.field.name.clone(), IncludeTree::default());
            }
        }
    }

    for kv in &model.kv_fields {
        current_node
            .0
            .insert(kv.field.name.clone(), IncludeTree::default());
    }

    for r2 in &model.r2_fields {
        current_node
            .0
            .insert(r2.field.name.clone(), IncludeTree::default());
    }

    visited.remove(current_model);
    current_node
}
