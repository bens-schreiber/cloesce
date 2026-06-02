use std::{
    collections::{HashSet, VecDeque},
    ops::Not,
};

use frontend::{ParsedIncludeTree, Spd, Symbol, Tag};
use idl::{
    CidlType, CloesceIdl, DataSource, DataSourceGetMethod, DataSourceGetMethodParam,
    DataSourceMethod, IncludeTree, Model, NavigationFieldKind, Number, ValidatedField, Validator,
    model_bindings,
};
use indexmap::IndexMap;
use orm::select::SelectModel;

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    is_valid_sql_type, resolve_injects, resolve_validator_tags,
};

enum DsMethodKind {
    Scalar,
    Body,
}

pub struct DataSourceAnalysis;
impl<'src, 'p> DataSourceAnalysis {
    pub fn analyze(
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

            // Validate include tree via BFS
            let mut q = VecDeque::new();
            q.push_back((&ds.tree, model));
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
                    let parameters = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                Self::validate_ds_param(p, &ds.symbol, DsMethodKind::Scalar, sink);
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
                        .collect();

                    DataSourceMethod {
                        parameters,
                        injected: resolve_injects(&method.inner.method, table, sink),
                        is_stub: true,
                    }
                })
                .unwrap_or_default();

            let get = ds
                .get
                .as_ref()
                .map(|method| {
                    let parameters = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                Self::validate_ds_param(p, &ds.symbol, DsMethodKind::Scalar, sink);

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

                            DataSourceGetMethodParam {
                                parameter: ValidatedField {
                                    name: p.name.into(),
                                    cidl_type: p.cidl_type.clone(),
                                    validators,
                                },
                                instance_field: instance_tag.is_some(),
                            }
                        })
                        .collect();

                    DataSourceGetMethod {
                        parameters,
                        injected: resolve_injects(&method.inner.method, table, sink),
                        is_stub: true,
                    }
                })
                .unwrap_or_default();

            let save = ds
                .save
                .as_ref()
                .map(|method| {
                    let parameters = method
                        .inner
                        .parameters
                        .iter()
                        .map(|p| {
                            let (validators, instance_tag) =
                                Self::validate_ds_param(p, &ds.symbol, DsMethodKind::Body, sink);
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
                        .collect();

                    DataSourceMethod {
                        parameters,
                        injected: resolve_injects(&method.inner.method, table, sink),
                        is_stub: true,
                    }
                })
                .unwrap_or_default();

            res.push((
                model_sym.name,
                DataSource {
                    name: ds.symbol.name,
                    tree: parsed_include_tree_to_idl(&ds.tree),
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
    fn validate_ds_param(
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
}

pub struct DataSourceExpansion;
impl<'src> DataSourceExpansion {
    /// Adds a `Default` [DataSource] to every D1-backed model that doesn't have one,
    /// then for every DS: precomputes the include/get/list SQL and synthesizes default
    /// get/list/save methods for any verb the user didn't declare as a stub.
    ///
    /// The default include tree contains all KV, R2, 1:1, 1:N and M:N relationships,
    /// but stops recursing after a 1:N or M:N to avoid join explosion.
    pub fn expand(idl: &mut CloesceIdl<'src>) {
        let needs_default = idl
            .models
            .values()
            .filter(|m| m.backing_binding.is_some() && m.default_data_source().is_none())
            .map(|m| m.name)
            .collect::<Vec<&str>>();

        for name in needs_default {
            let tree = Self::include_dfs(&idl.models, name, &mut HashSet::new());
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

        let pending = idl
            .models
            .values()
            .filter(|m| m.backing_binding.is_some())
            .flat_map(|model| {
                model.data_sources.iter().map(|(ds_name, ds)| {
                    let include_query = SelectModel::query(model.name, None, Some(&ds.tree), idl)
                        .unwrap_or_default();
                    let bindings = model_bindings(idl, model, Some(&ds.tree));

                    let get_query = Self::build_get_query(model, &include_query);
                    let list_query = Self::build_list_query(model, &include_query);

                    // One PK-instance parameter per primary key column.
                    let get = ds.get.is_stub.not().then(|| DataSourceGetMethod {
                        parameters: model
                            .primary_columns
                            .iter()
                            .map(|pk| DataSourceGetMethodParam {
                                parameter: pk.field.clone(),
                                instance_field: true,
                            })
                            .collect(),
                        injected: bindings.clone(),
                        is_stub: false,
                    });

                    // Seek-pagination params: `lastSeen_<pk>...`, `limit`.
                    let list = ds.list.is_stub.not().then(|| DataSourceMethod {
                        parameters: model
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
                            .collect(),
                        injected: bindings.clone(),
                        is_stub: false,
                    });

                    // Single `model: partial<Model>` parameter.
                    let save = ds.save.is_stub.not().then(|| DataSourceMethod {
                        parameters: vec![ValidatedField {
                            name: "model".into(),
                            cidl_type: CidlType::Partial {
                                object_name: model.name,
                            },
                            validators: vec![],
                        }],
                        injected: bindings,
                        is_stub: false,
                    });

                    (
                        model.name,
                        *ds_name,
                        include_query,
                        get_query,
                        list_query,
                        get,
                        list,
                        save,
                    )
                })
            })
            .collect::<Vec<_>>();

        for (model_name, ds_name, include_query, get_query, list_query, get, list, save) in pending
        {
            let Some(ds) = idl
                .models
                .get_mut(model_name)
                .and_then(|m| m.data_sources.get_mut(ds_name))
            else {
                continue;
            };
            ds.include_query = include_query;
            ds.get_query = get_query;
            ds.list_query = list_query;
            if let Some(g) = get {
                ds.get = g;
            }
            if let Some(l) = list {
                ds.list = l;
            }
            if let Some(s) = save {
                ds.save = s;
            }
        }
    }

    fn include_dfs<'m>(
        models: &'m IndexMap<&'src str, Model<'src>>,
        current_model: &'src str,
        visited: &mut HashSet<&'src str>,
    ) -> IncludeTree<'src> {
        if !visited.insert(current_model) {
            return IncludeTree::default();
        }

        let mut current_node = IncludeTree::default();

        let model = models.get(current_model).unwrap();
        for nav in &model.navigation_fields {
            match nav.kind {
                NavigationFieldKind::OneToOne { .. } => {
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
                    let new_node = Self::include_dfs(models, nav.model_reference, visited);
                    current_node.0.insert(nav.field.name.clone(), new_node);
                }
                NavigationFieldKind::OneToMany { .. } => {
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

fn parsed_include_tree_to_idl<'src>(tree: &ParsedIncludeTree<'src>) -> IncludeTree<'src> {
    IncludeTree(
        tree.0
            .iter()
            .map(|(sym, child)| (sym.name.into(), parsed_include_tree_to_idl(child)))
            .collect(),
    )
}
