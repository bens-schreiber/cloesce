use std::{
    collections::{HashSet, VecDeque},
    ops::Not,
};

use frontend::{DataSourceBlockMethod, ParsedIncludeTree, Symbol, Tag};
use idl::{
    CidlType, CloesceIdl, DataSource, DataSourceGetMethod, DataSourceGetMethodParam,
    DataSourceListMethod, IncludeTree, Model, NavigationFieldKind, ValidatedField, Validator,
};
use indexmap::IndexMap;
use orm::select::SelectModel;

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    is_valid_sql_type, resolve_validator_tags,
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
                validate_raw_sql(&ds.symbol, &method.inner, sink);

                let params = method
                    .inner
                    .parameters
                    .iter()
                    .map(|p| {
                        let (validators, instance_tag) = validate_ds_param(p, &ds.symbol, sink);

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

                DataSourceListMethod {
                    parameters: params,
                    raw_sql: method.inner.raw_sql.to_string(),
                }
            });

            let get = ds.get.as_ref().map(|method| {
                validate_raw_sql(&ds.symbol, &method.inner, sink);

                let params = method
                    .inner
                    .parameters
                    .iter()
                    .map(|p| {
                        let (validators, instance_tag) = validate_ds_param(p, &ds.symbol, sink);

                        if let Some(tag) = instance_tag {
                            // Parameter must exist as a field on the model
                            if !model.columns.iter().any(|c| c.field.name == p.name)
                                && !model
                                    .primary_columns
                                    .iter()
                                    .any(|pk| pk.field.name == p.name)
                                && !model.key_fields.iter().any(|k| k.name == p.name)
                            {
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
                    parameters: params,
                    raw_sql: method.inner.raw_sql.to_string(),
                }
            });

            res.push((
                model_sym.name,
                DataSource {
                    name: ds.symbol.name,
                    tree: parsed_include_tree_to_ast(&ds.tree),
                    list,
                    get,
                    is_internal,
                },
            ));
        }

        return res;

        /// Validates that the raw SQL only references parameters that are defined
        /// on the method.
        fn validate_raw_sql<'src, 'p>(
            source_sym: &'p Symbol<'src>,
            method: &DataSourceBlockMethod,
            sink: &mut ErrorSink<'src, 'p>,
        ) {
            let mut chars = method.raw_sql.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '$' {
                    let name: String =
                        std::iter::from_fn(|| chars.next_if(|c| c.is_alphanumeric() || *c == '_'))
                            .collect();

                    if !name.is_empty()
                        && name != "include"
                        && !method.parameters.iter().any(|p| p.name == name)
                    {
                        sink.push(SemanticError::DataSourceUnknownSqlParam {
                            source: source_sym,
                            name,
                        });
                    }
                }
            }
        }

        /// Validates that a parameter is a valid SQL type and has valid tags for
        /// a data source method parameter.
        ///
        /// Returns a list of resolved validators and whether the parameter is an instance field.
        fn validate_ds_param<'src, 'p>(
            param: &'p Symbol<'src>,
            source_sym: &'p Symbol<'src>,
            sink: &mut ErrorSink<'src, 'p>,
        ) -> (Vec<Validator<'src>>, Option<&'p frontend::Spd<Tag<'src>>>) {
            let mut instance_tag = None;

            // Validate tags
            for tag in param.tags.iter() {
                match &tag.inner {
                    Tag::Instance => instance_tag = Some(tag),
                    Tag::Validator { .. } => {
                        // Allowed
                    }
                    _ => sink.push(SemanticError::TagInvalidInContext { tag, symbol: param }),
                }
            }

            // Validate type
            if !is_valid_sql_type(&param.cidl_type) {
                sink.push(SemanticError::DataSourceInvalidMethodParam {
                    source: source_sym,
                    param,
                });
            }

            // Resolve all validator tags
            match resolve_validator_tags(param) {
                Ok(v) => (v, instance_tag),
                Err(errs) => {
                    sink.extend(errs);
                    (vec![], None)
                }
            }
        }
    }
}

pub struct DataSourceExpansion;
impl<'src> DataSourceExpansion {
    fn default_data_source(
        model: &Model<'src>,
        tree: IncludeTree<'src>,
        idl: &CloesceIdl,
    ) -> DataSource<'src> {
        let Ok(include_sql) = SelectModel::query(model.name, None, Some(&tree), idl) else {
            // Model doesn't have any D1 fields, no SQL needed.
            return DataSource {
                name: "Default",
                tree,
                is_internal: false,
                list: None,
                get: None,
            };
        };

        DataSource {
            name: "Default",
            tree,
            is_internal: false,
            list: Some(Self::build_default_list(model, &include_sql)),
            get: Some(Self::build_default_get(model, &include_sql)),
        }
    }

    // Simple get by primary key
    fn build_default_get(model: &Model<'src>, include_sql: &String) -> DataSourceGetMethod<'src> {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| DataSourceGetMethodParam {
                parameter: pk.field.clone(),
                instance_field: true,
            })
            .collect();

        let where_clause = if model.primary_columns.len() == 1 {
            let pk = &model.primary_columns[0];
            format!(r#""{}"."{}""#, model.name, pk.field.name)
        } else {
            model
                .primary_columns
                .iter()
                .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
                .collect::<Vec<String>>()
                .join(", ")
        };

        let params = (0..model.primary_columns.len())
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<String>>()
            .join(", ");

        let raw_sql = if model.primary_columns.len() == 1 {
            format!("{include_sql} WHERE {where_clause} = ?1")
        } else {
            format!("{include_sql} WHERE ({where_clause}) = ({params})")
        };

        DataSourceGetMethod {
            parameters,
            raw_sql,
        }
    }

    // Seek pagination based on primary keys with a limit, ordered by primary key ascending.
    fn build_default_list(model: &Model<'src>, include_sql: &String) -> DataSourceListMethod<'src> {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| ValidatedField {
                name: format!("lastSeen_{}", pk.field.name).into(),
                ..pk.field.clone()
            })
            .chain(vec![ValidatedField {
                name: "limit".into(),
                cidl_type: CidlType::Int,
                validators: vec![idl::Validator::GreaterThan(idl::Number::Int(0))],
            }])
            .collect();

        let where_clause = if model.primary_columns.len() == 1 {
            let pk = &model.primary_columns[0];
            format!(r#""{}"."{}""#, model.name, pk.field.name)
        } else {
            model
                .primary_columns
                .iter()
                .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
                .collect::<Vec<String>>()
                .join(", ")
        };

        let params = (0..model.primary_columns.len())
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<String>>()
            .join(", ");

        let where_expr = if model.primary_columns.len() == 1 {
            format!("{where_clause} > ?1")
        } else {
            format!("({where_clause}) > ({params})")
        };

        let order = model
            .primary_columns
            .iter()
            .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
            .collect::<Vec<String>>()
            .join(" ASC, ")
            + " ASC";

        let limit_param = model.primary_columns.len() + 1;
        let raw_sql =
            format!("{include_sql} WHERE {where_expr} ORDER BY {order} LIMIT ?{limit_param}");

        DataSourceListMethod {
            parameters,
            raw_sql,
        }
    }

    /// Normalizes input and resolves `$parameterName` placeholders
    /// to positional `?N` syntax for prepared statements.
    pub fn resolve_sql_params(
        raw_sql: &str,
        params: &[&ValidatedField<'_>],
        include_sql: &str,
    ) -> String {
        // Normalize whitespace to keep generated SQL stable.
        let normalized = raw_sql.split_whitespace().collect::<Vec<_>>().join(" ");

        let mut result = String::with_capacity(normalized.len());
        let mut chars = normalized.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                // Collect an identifier following '$'.
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }

                if name.is_empty() {
                    // Standalone '$', keep as-is.
                    result.push('$');
                    continue;
                }

                if name == "include" {
                    // Expand $include to the full include SQL.
                    result.push_str(include_sql);
                    continue;
                }

                // Look up the parameter index by exact name match.
                if let Some((idx, _param)) = params
                    .iter()
                    .enumerate()
                    .find(|(_, p)| p.name.as_ref() == name.as_str())
                {
                    // Replace with positional parameter ?N.
                    result.push('?');
                    result.push_str(&(idx + 1).to_string());
                } else {
                    // Unknown placeholder here: keep it literal.
                    result.push('$');
                    result.push_str(&name);
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Creates a default [DataSource] for any model that doesn't have one,
    /// including default get/list SQL queries for models with D1 fields.
    ///
    /// Each data source has a default [IncludeTree] with all KV, R2, 1:1, 1:N and M:N relationships by default.
    /// Does not include relationships after a 1:N or M:N to avoid explosion (of sql joins not the computer).
    pub fn expand(idl: &mut CloesceIdl<'src>) {
        // For each model without a default DS, build one
        {
            let models_to_process = idl
                .models
                .iter()
                .filter(|(_, model)| model.default_data_source().is_none())
                .map(|(_, model)| model.name)
                .collect::<Vec<&str>>();

            for model_name in models_to_process {
                let data_source = {
                    let tree = Self::include_dfs(&idl.models, model_name, &mut HashSet::new());
                    let model = idl.models.get(&model_name).unwrap();
                    Self::default_data_source(model, tree, idl)
                };

                idl.models
                    .get_mut(model_name)
                    .unwrap()
                    .data_sources
                    .insert(data_source.name, data_source);
            }
        }

        // Fill in any missing get/list with the default implementation
        let pending = idl
            .models
            .values()
            .filter(|m| m.has_d1())
            .map(|model| {
                let defaults = model
                    .data_sources
                    .iter()
                    .map(|(ds_name, ds)| {
                        let sql =
                            SelectModel::query(model.name, None, Some(&ds.tree), idl).unwrap();

                        let get = ds
                            .get
                            .is_none()
                            .then(|| Self::build_default_get(model, &sql));
                        let list = ds
                            .list
                            .is_none()
                            .then(|| Self::build_default_list(model, &sql));
                        (*ds_name, get, list, sql)
                    })
                    .collect();
                (model.name, defaults)
            })
            .collect::<Vec<(&str, Vec<_>)>>();

        for (name, defaults) in pending {
            let model = idl.models.get_mut(name).unwrap();
            for (ds_name, get, list, include_sql) in defaults {
                // Update the existing data source with missing get/list methods
                let ds = model.data_sources.get_mut(ds_name).unwrap();
                if let Some(g) = get {
                    ds.get = Some(g);
                }
                if let Some(l) = list {
                    ds.list = Some(l);
                }

                // Resolve $paramName -> ?N
                if let Some(m) = &mut ds.get {
                    let params = m
                        .parameters
                        .iter()
                        .map(|p| &p.parameter)
                        .collect::<Vec<_>>();
                    m.raw_sql = Self::resolve_sql_params(&m.raw_sql, &params, &include_sql);
                }
                if let Some(m) = &mut ds.list {
                    let params = m.parameters.iter().collect::<Vec<_>>();
                    m.raw_sql = Self::resolve_sql_params(&m.raw_sql, &params, &include_sql);
                }
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
