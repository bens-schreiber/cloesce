use std::{
    collections::{HashSet, VecDeque},
    ops::Not,
};

use frontend::{ParsedIncludeTree, Spd, Symbol, Tag};
use idl::{
    CidlType, CloesceIdl, DataSource, DataSourceGetMethod, DataSourceGetMethodParam,
    DataSourceMethod, IncludeTree, Model, NavigationFieldKind, ValidatedField, Validator,
};
use indexmap::IndexMap;
use orm::select::SelectModel;

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    resolve_injects, resolve_validator_tags,
};

pub struct DataSourceAnalysis;
impl<'src, 'p> DataSourceAnalysis {
    pub fn analyze(
        models: &IndexMap<&'src str, Model>,
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

            let list = ds.list.as_ref().map(|method| {
                let parameters = method
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

                DataSourceMethod {
                    parameters,
                    injected: resolve_injects(&method.inner.method, table, sink),
                }
            });

            let get = ds.get.as_ref().map(|method| {
                let parameters = method
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

                        DataSourceGetMethodParam {
                            parameter: ValidatedField {
                                name: p.name.into(),
                                cidl_type: p.cidl_type.clone(),
                                validators,
                            },
                            instance_field: instance_tag.is_some(),
                        }
                    })
                    .collect::<Vec<_>>();

                DataSourceGetMethod {
                    parameters,
                    injected: resolve_injects(&method.inner.method, table, sink),
                }
            });

            let save = ds.save.as_ref().map(|method| {
                let parameters = method
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

                DataSourceMethod {
                    parameters,
                    injected: resolve_injects(&method.inner.method, table, sink),
                }
            });

            res.push((
                model_sym.name,
                DataSource {
                    name: ds.symbol.name,
                    tree: parsed_include_tree_to_ast(&ds.tree),
                    list,
                    get,
                    save,
                    include_query: String::new(),
                    is_internal,
                },
            ));
        }

        res
    }
}

// TODO: When GET requests can accept object parameters,
// this can be removed
#[derive(Clone, Copy)]
enum DsMethodKind {
    Scalar,
    Body,
}

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
        DsMethodKind::Scalar => {
            let inner = match &param.cidl_type {
                CidlType::Nullable(inner) => inner.as_ref(),
                other => other,
            };
            matches!(
                inner,
                CidlType::Int
                    | CidlType::Real
                    | CidlType::String
                    | CidlType::Blob
                    | CidlType::Boolean
                    | CidlType::DateIso
            )
        }
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

pub struct DataSourceExpansion;
impl<'src> DataSourceExpansion {
    pub fn expand(idl: &mut CloesceIdl<'src>) {
        let to_default = idl
            .models
            .values()
            .filter(|m| m.backing_binding.is_some() && m.default_data_source().is_none())
            .map(|m| m.name)
            .collect::<Vec<&str>>();

        for name in to_default {
            let tree = Self::include_dfs(&idl.models, name, &mut HashSet::new());
            let ds = DataSource {
                name: "Default",
                tree,
                list: None,
                get: None,
                save: None,
                include_query: String::new(),
                is_internal: false,
            };
            idl.models
                .get_mut(name)
                .unwrap()
                .data_sources
                .insert(ds.name, ds);
        }

        let queries = idl
            .models
            .values()
            .filter(|m| m.backing_binding.is_some())
            .flat_map(|model| {
                model.data_sources.iter().map(|(ds_name, ds)| {
                    let query = SelectModel::query(model.name, None, Some(&ds.tree), idl)
                        .unwrap_or_default();
                    (model.name, *ds_name, query)
                })
            })
            .collect::<Vec<_>>();

        for (model_name, ds_name, query) in queries {
            if let Some(ds) = idl
                .models
                .get_mut(model_name)
                .and_then(|m| m.data_sources.get_mut(ds_name))
            {
                ds.include_query = query;
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
                NavigationFieldKind::OneToMany { .. } | NavigationFieldKind::ManyToMany => {
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
}

fn parsed_include_tree_to_ast<'src>(tree: &ParsedIncludeTree<'src>) -> IncludeTree<'src> {
    IncludeTree(
        tree.0
            .iter()
            .map(|(sym, child)| (sym.name.into(), parsed_include_tree_to_ast(child)))
            .collect(),
    )
}
