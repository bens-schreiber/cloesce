use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use ast::err::{GeneratorErrorKind, Result};
use ast::{
    ApiMethod, CidlType, CloesceAst, D1Model, D1NavigationPropertyKind, HttpVerb,
    KVNavigationProperty, NamedTypedValue, WranglerSpec, ensure, fail,
};

type AdjacencyList<'a> = BTreeMap<&'a str, Vec<&'a str>>;

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    /// Analyzes the grammar of the AST, yielding a [GeneratorErrorKind] on failure.
    ///
    /// Sorts models topologically in SQL insertion order.
    ///
    /// Sorts services and plain old objects in constructor order.
    ///
    /// Determines the MediaType of all [ApiMethod]'s.
    ///
    /// Returns a set of all objects that have blobs (be it a direct attribute or composition)
    ///
    /// Returns error on
    /// - Missing WranglerEnv when models are defined
    /// - Inconsistent WranglerEnv bindings with WranglerSpec
    /// - Missing WranglerEnv vars in WranglerSpec
    /// - Model attributes with invalid SQL types
    /// - Primary keys with invalid SQL types
    /// - Invalid Model or Method map K/V
    /// - Unknown navigation property model references
    /// - Unknown model references in method parameters
    /// - Invalid method parameter types
    /// - Unknown or invalid foreign key references
    /// - Missing navigation property attributes
    /// - Cyclical dependencies
    /// - Invalid data source type
    /// - Invalid data source reference
    pub fn analyze(ast: &mut CloesceAst, spec: &WranglerSpec) -> Result<()> {
        // Wrangler must be validated first so that the env can be used in later validations
        Self::wrangler(ast, spec)?;

        Self::d1_models(ast)?;
        Self::kv_models(ast)?;
        Self::poos(ast)?;
        Self::services(ast)?;
        Ok(())
    }

    fn wrangler(ast: &CloesceAst, spec: &WranglerSpec) -> Result<()> {
        let env = match &ast.wrangler_env {
            Some(env) => env,

            // No models => no wrangler needed
            None if ast.d1_models.is_empty() && ast.kv_models.is_empty() => return Ok(()),

            _ => fail!(
                GeneratorErrorKind::MissingWranglerEnv,
                "The AST is missing a WranglerEnv but models are defined"
            ),
        };

        for var in env.vars.keys() {
            ensure!(
                spec.vars.contains_key(var),
                GeneratorErrorKind::MissingWranglerVariable,
                "A variable is defined in the WranglerEnv but not in the Wrangler config ({})",
                var
            )
        }

        // If D1 models are defined, ensure a D1 database binding exists
        ensure!(
            !spec.d1_databases.is_empty() || ast.d1_models.is_empty(),
            GeneratorErrorKind::MissingWranglerD1Binding,
            "No D1 database binding is defined, but D1 models are defined in the WranglerEnv ({})",
            env.source_path.display()
        );

        // TODO: multiple databases
        if let Some(db) = spec.d1_databases.first() {
            ensure!(
                env.d1_binding == db.binding,
                GeneratorErrorKind::InconsistentWranglerBinding,
                "The Wrangler configs D1 binding does not match the WranglerEnv binding ({}.{:?} != {} in {})",
                env.name,
                env.d1_binding,
                db.binding.as_ref().unwrap(),
                env.source_path.display()
            );
        }

        // If KV models are defined, ensure a KV namespace binding exists
        ensure!(
            !spec.kv_namespaces.is_empty() || ast.kv_models.is_empty(),
            GeneratorErrorKind::MissingWranglerKVNamespace,
            "No KV namespace binding is defined, but KV models are defined in the WranglerEnv ({})",
            env.source_path.display()
        );

        for kv in &env.kv_bindings {
            ensure!(
                spec.kv_namespaces
                    .iter()
                    .any(|ns| ns.binding.as_ref().is_some_and(|b| b == kv)),
                GeneratorErrorKind::InconsistentWranglerBinding,
                "A Wrangler config KV binding was missing or did not match the WranglerEnv binding ({} {})",
                kv,
                env.source_path.display()
            )
        }

        Ok(())
    }

    fn poos(ast: &mut CloesceAst) -> Result<()> {
        // Cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for (name, poo) in &ast.poos {
            graph.entry(&poo.name).or_default();
            in_degree.entry(&poo.name).or_insert(0);

            ensure!(
                *name == poo.name,
                GeneratorErrorKind::InvalidMapping,
                "Plain Old Object record key did not match it's Plain Old Object name? {} : {}",
                name,
                poo.name
            );

            for attr in &poo.attributes {
                match &attr.cidl_type.root_type() {
                    CidlType::Void => {
                        fail!(
                            GeneratorErrorKind::UnexpectedVoid,
                            "{}.{}",
                            poo.name,
                            attr.name
                        )
                    }
                    CidlType::Object(o) | CidlType::Partial(o) => {
                        ensure!(
                            is_valid_object_ref(ast, o),
                            GeneratorErrorKind::UnknownObject,
                            "{}.{} => {}?",
                            poo.name,
                            attr.name,
                            o
                        );

                        if ast.poos.contains_key(o) {
                            graph.entry(o.as_str()).or_default().push(&poo.name);
                            in_degree.entry(&poo.name).and_modify(|d| *d += 1);
                        }
                    }
                    CidlType::Inject(o) => {
                        fail!(
                            GeneratorErrorKind::UnexpectedInject,
                            "{}.{} => {}?",
                            poo.name,
                            attr.name,
                            o
                        )
                    }
                    CidlType::DataSource(reference) => ensure!(
                        is_valid_data_source_ref(ast, reference),
                        GeneratorErrorKind::InvalidModelReference,
                        "{}.{} => {}?",
                        poo.name,
                        attr.name,
                        reference
                    ),
                    CidlType::Stream => {
                        fail!(
                            GeneratorErrorKind::InvalidStream,
                            "{}.{}",
                            poo.name,
                            attr.name,
                        )
                    }
                    _ => {}
                }
            }
        }

        // Detect cycles
        kahns(graph, in_degree, ast.poos.len())?;

        Ok(())
    }

    fn d1_models(ast: &mut CloesceAst) -> Result<()> {
        // TODO: Use env to check binding on each model (multiple databases)
        let Some(_env) = &ast.wrangler_env else {
            return Ok(()); // No D1 models
        };

        let ensure_valid_sql_type = |model: &D1Model, value: &NamedTypedValue| {
            let inner = match &value.cidl_type {
                CidlType::Nullable(inner) if matches!(inner.as_ref(), CidlType::Void) => {
                    fail!(GeneratorErrorKind::NullSqlType)
                }
                CidlType::Nullable(inner) => inner.as_ref(),
                other => other,
            };

            ensure!(
                matches!(
                    inner,
                    CidlType::Integer
                        | CidlType::Real
                        | CidlType::Text
                        | CidlType::Blob
                        | CidlType::Boolean
                        | CidlType::DateIso
                ),
                GeneratorErrorKind::InvalidSqlType,
                "{}.{}",
                model.name,
                value.name
            );

            Ok(())
        };

        // Topo sort and cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        // Maps a model name and a foreign key reference to the model it is referencing
        // Ie, Person.dogId => { (Person, dogId): "Dog" }
        let mut model_attr_ref_to_fk_model = HashMap::<(&str, &str), &str>::new();
        let mut unvalidated_navs = Vec::new();

        // Maps a m2m unique id to the models that reference the id
        let mut m2m = HashMap::<&String, Vec<&String>>::new();

        // Validate Models
        for (model_name, model) in &ast.d1_models {
            ensure!(
                *model_name == model.name,
                GeneratorErrorKind::InvalidMapping,
                "Model record key did not match it's model name? {} : {}",
                model_name,
                model.name
            );

            graph.entry(&model.name).or_default();
            in_degree.entry(&model.name).or_insert(0);

            // Validate PK
            ensure!(
                !model.primary_key.cidl_type.is_nullable(),
                GeneratorErrorKind::NullPrimaryKey,
                "{}.{}",
                model.name,
                model.primary_key.name
            );
            ensure_valid_sql_type(model, &model.primary_key)?;

            // Validate attributes
            for a in &model.attributes {
                ensure_valid_sql_type(model, &a.value)?;

                if let Some(fk_model_name) = &a.foreign_key_reference {
                    let Some(fk_model) = ast.d1_models.get(fk_model_name.as_str()) else {
                        fail!(
                            GeneratorErrorKind::InvalidModelReference,
                            "{}.{} => {}?",
                            model.name,
                            a.value.name,
                            fk_model_name
                        );
                    };

                    // Validate the types are equal
                    ensure!(
                        *a.value.cidl_type.root_type() == fk_model.primary_key.cidl_type,
                        GeneratorErrorKind::MismatchedForeignKeyTypes,
                        "{}.{} ({:?}) != {}.{} ({:?})",
                        model.name,
                        a.value.name,
                        a.value.cidl_type,
                        fk_model_name,
                        fk_model.primary_key.name,
                        fk_model.primary_key.cidl_type
                    );

                    model_attr_ref_to_fk_model
                        .insert((&model.name, a.value.name.as_str()), fk_model_name);

                    // Nullable FK's do not constrain table creation order, and thus
                    // can be left out of the topo sort
                    if !a.value.cidl_type.is_nullable() {
                        // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                        // Dog must come before Person
                        graph.entry(fk_model_name).or_default().push(&model.name);
                        in_degree.entry(&model.name).and_modify(|d| *d += 1);
                    }
                }
            }

            // Validate navigation props
            for nav in &model.navigation_properties {
                ensure!(
                    ast.d1_models.contains_key(nav.model_reference.as_str()),
                    GeneratorErrorKind::InvalidModelReference,
                    "{} => {}?",
                    model.name,
                    nav.model_reference
                );

                match &nav.kind {
                    D1NavigationPropertyKind::OneToOne {
                        attribute_reference,
                    } => {
                        // Validate the nav prop's reference is consistent
                        if let Some(&fk_model) =
                            model_attr_ref_to_fk_model.get(&(&model.name, attribute_reference))
                        {
                            ensure!(
                                fk_model == nav.model_reference,
                                GeneratorErrorKind::MismatchedNavigationPropertyTypes,
                                "({}.{}) does not match type ({})",
                                model.name,
                                nav.var_name,
                                fk_model
                            );
                        } else {
                            fail!(
                                GeneratorErrorKind::InvalidNavigationPropertyReference,
                                "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                                model.name,
                                nav.var_name,
                                nav.model_reference,
                                attribute_reference,
                                model.name
                            );
                        }
                    }
                    D1NavigationPropertyKind::OneToMany {
                        attribute_reference: _,
                    } => {
                        unvalidated_navs.push((&model.name, &nav.model_reference, nav));
                    }
                    D1NavigationPropertyKind::ManyToMany { unique_id } => {
                        m2m.entry(unique_id).or_default().push(&model.name);
                    }
                }
            }

            // Validate Data Sources (BFS)
            for ds in model.data_sources.values() {
                let mut q = VecDeque::new();
                q.push_back((&ds.tree, model));

                while let Some((node, parent_model)) = q.pop_front() {
                    for (var_name, child) in &node.0 {
                        let Some(model_name) = parent_model
                            .navigation_properties
                            .iter()
                            .find(|nav| nav.var_name == *var_name)
                            .map(|nav| &nav.model_reference)
                        else {
                            fail!(
                                GeneratorErrorKind::UnknownIncludeTreeReference,
                                "{}.{}",
                                model.name,
                                var_name
                            );
                        };

                        let Some(child_model) = ast.d1_models.get(model_name) else {
                            fail!(
                                GeneratorErrorKind::InvalidModelReference,
                                "{} => {}?",
                                model.name,
                                model_name
                            );
                        };

                        q.push_back((child, child_model));
                    }
                }
            }

            // Validate Methods
            for (method_name, method) in &model.methods {
                let ds = validate_methods(&model.name, method_name, method, ast)?;

                if !method.is_static {
                    ensure!(
                        ds == 1,
                        GeneratorErrorKind::MissingOrExtraneousDataSource,
                        "Instantiated model methods require one data source: {}.{}",
                        model.name,
                        method.name,
                    )
                }
            }
        }

        // Validate 1:M nav props
        for (model_name, nav_model, nav) in unvalidated_navs {
            let D1NavigationPropertyKind::OneToMany {
                attribute_reference,
            } = &nav.kind
            else {
                continue;
            };

            // Validate the nav props reference is consistent to an attribute
            // on another model
            let Some(&fk_model) = model_attr_ref_to_fk_model.get(&(nav_model, attribute_reference))
            else {
                fail!(
                    GeneratorErrorKind::InvalidNavigationPropertyReference,
                    "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                    model_name,
                    nav.var_name,
                    nav_model,
                    attribute_reference,
                    model_name
                );
            };

            // The types should reference one another
            // ie, Person has many dogs, personId on dog should be an fk to Person
            ensure!(
                model_name == fk_model,
                GeneratorErrorKind::MismatchedNavigationPropertyTypes,
                "({}.{}) does not match type ({}.{})",
                model_name,
                nav.var_name,
                nav_model,
                attribute_reference,
            );

            // One To Many: Person has many Dogs (sql)=> Dog has an fk to  Person
            // Person must come before Dog in topo order
            graph.entry(model_name).or_default().push(nav_model);
            *in_degree.entry(nav_model).or_insert(0) += 1;
        }

        // Validate M:M
        for (unique_id, jcts) in m2m {
            if jcts.len() < 2 {
                fail!(
                    GeneratorErrorKind::MissingManyToManyReference,
                    "Missing junction table for many to many table {}",
                    unique_id
                );
            }

            if jcts.len() > 2 {
                let joined = jcts
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                fail!(
                    GeneratorErrorKind::ExtraneousManyToManyReferences,
                    "Many To Many Table {unique_id} {joined}",
                );
            }
        }

        // Sort models by SQL insertion order
        let rank = kahns(graph, in_degree, ast.d1_models.len())?;
        ast.d1_models
            .sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

        Ok(())
    }

    fn kv_models(ast: &CloesceAst) -> Result<()> {
        let Some(env) = &ast.wrangler_env else {
            return Ok(()); // No KV models
        };

        // Tree validation
        let mut in_degree = BTreeMap::<&str, usize>::new();

        let kv_binding_set = env.kv_bindings.iter().collect::<HashSet<&String>>();

        // Validate models
        for (model_name, model) in &ast.kv_models {
            ensure!(
                *model_name == model.name,
                GeneratorErrorKind::InvalidMapping,
                "KV Model record key did not match it's model name? {} : {}",
                model_name,
                model.name
            );

            ensure!(
                kv_binding_set.contains(&model.binding),
                GeneratorErrorKind::InconsistentWranglerBinding,
                "KV Model {} binding {} not found in WranglerEnv bindings",
                model.name,
                model.binding
            );

            ensure!(
                !matches!(model.cidl_type, CidlType::Inject(_)),
                GeneratorErrorKind::UnexpectedInject,
                r#"KV Model "{}"'s type cannot be an injected instance."#,
                model.name
            );

            // Attributes
            for attr in &model.navigation_properties {
                match attr {
                    KVNavigationProperty::KValue(ntv) => match ntv.cidl_type.root_type() {
                        CidlType::Inject(_) => fail!(
                            GeneratorErrorKind::UnexpectedInject,
                            r#"KV Model attribute "{}.{}"'s type cannot be an injected instance."#,
                            model.name,
                            ntv.name
                        ),
                        CidlType::Object(o) | CidlType::Partial(o) => {
                            ensure!(
                                is_valid_object_ref(ast, o),
                                GeneratorErrorKind::UnknownObject,
                                "{}.{} => {}?",
                                model.name,
                                ntv.name,
                                o
                            )
                        }
                        CidlType::DataSource(reference) => ensure!(
                            is_valid_data_source_ref(ast, reference),
                            GeneratorErrorKind::InvalidModelReference,
                            "{}.{} => {}?",
                            model.name,
                            ntv.name,
                            reference
                        ),
                        _ => {}
                    },
                    KVNavigationProperty::Model {
                        model_reference,
                        var_name,
                        many,
                    } => {
                        let Some(ref_model) = ast.kv_models.get(model_reference.as_str()) else {
                            fail!(
                                GeneratorErrorKind::InvalidModelReference,
                                "{}.{} => {}?",
                                model.name,
                                var_name,
                                model_reference
                            );
                        };

                        // namespaces must be equal
                        ensure!(
                            ref_model.binding == model.binding,
                            GeneratorErrorKind::MismatchedKVModelNamespaces,
                            "{}.{} ({}) != {}.{} ({})",
                            model.name,
                            var_name,
                            model.binding,
                            model_reference,
                            var_name,
                            ref_model.binding
                        );

                        ensure!(
                            !*many || !ref_model.params.is_empty(),
                            GeneratorErrorKind::InvalidKVTree,
                            r#"KV Model "{}" is referenced as many in "{}.{}", but has no key parameters."#,
                            model_reference,
                            model.name,
                            var_name
                        );

                        in_degree
                            .entry(model_reference.as_str())
                            .and_modify(|d| *d += 1)
                            .or_insert(1);
                    }
                }
            }

            // Data Sources
            for ds in model.data_sources.values() {
                let mut q = VecDeque::new();
                q.push_back((&ds.tree, model));

                while let Some((node, parent_model)) = q.pop_front() {
                    for (var_name, child) in &node.0 {
                        let found_match =
                            parent_model
                                .navigation_properties
                                .iter()
                                .find(|attr| match attr {
                                    KVNavigationProperty::Model { var_name: v, .. } => {
                                        *v == *var_name
                                    }
                                    KVNavigationProperty::KValue(named_typed_value) => {
                                        named_typed_value.name == *var_name
                                    }
                                });

                        let Some(found_match) = found_match else {
                            fail!(
                                GeneratorErrorKind::UnknownIncludeTreeReference,
                                "{}.{}",
                                model.name,
                                var_name
                            );
                        };

                        match found_match {
                            KVNavigationProperty::KValue(_) => {
                                // KValues do not have attributes to traverse
                            }
                            KVNavigationProperty::Model {
                                model_reference, ..
                            } => {
                                let Some(child_model) = ast.kv_models.get(model_reference.as_str())
                                else {
                                    unreachable!(
                                        "Model references should be validated before data sources"
                                    )
                                };

                                q.push_back((child, child_model));
                            }
                        }
                    }
                }
            }

            // Methods
            for (method_name, method) in &model.methods {
                validate_methods(model_name, method_name, method, ast)?;
            }
        }

        // KV Models must be a tree (in degree <= 1)
        if let Some((name, deg)) = in_degree.iter().find(|(_, deg)| **deg > 1) {
            fail!(
                GeneratorErrorKind::InvalidKVTree,
                r#"KV Model "{}" has an in degree of {}."#,
                name,
                deg
            )
        }

        Ok(())
    }

    fn services(ast: &mut CloesceAst) -> Result<()> {
        // Topo sort and cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for (service_name, service) in &ast.services {
            graph.entry(&service.name).or_default();
            in_degree.entry(&service.name).or_insert(0);

            // Validate record
            ensure!(
                *service_name == service.name,
                GeneratorErrorKind::InvalidMapping,
                "Method record key did not match it's method name? {}: {}",
                service_name,
                service.name
            );

            // Assemble graph
            for attr in &service.attributes {
                if !ast.services.contains_key(&attr.inject_reference) {
                    continue;
                }

                graph
                    .entry(attr.inject_reference.as_str())
                    .or_default()
                    .push(&service.name);
                in_degree.entry(&service.name).and_modify(|d| *d += 1);
            }

            // Validate methods
            for (method_name, method) in &service.methods {
                validate_methods(service_name, method_name, method, ast)?;
            }
        }

        // Sort
        let rank = kahns(graph, in_degree, ast.services.len())?;
        ast.services
            .sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

        Ok(())
    }
}

fn is_valid_object_ref(ast: &CloesceAst, o: &String) -> bool {
    ast.d1_models.contains_key(o) || ast.poos.contains_key(o) || ast.kv_models.contains_key(o)
}

fn is_valid_data_source_ref(ast: &CloesceAst, o: &String) -> bool {
    ast.d1_models.contains_key(o) || ast.kv_models.contains_key(o)
}

/// Returns how many data sources to the namespace are in the method.
fn validate_methods(
    namespace: &str,
    method_name: &str,
    method: &ApiMethod,
    ast: &CloesceAst,
) -> Result<i32> {
    // Validate record
    ensure!(
        *method_name == method.name,
        GeneratorErrorKind::InvalidMapping,
        "Method record key did not match it's method name? {}: {}",
        method_name,
        method.name
    );

    // Validate return type
    match &method.return_type.root_type() {
        CidlType::Object(o) | CidlType::Partial(o) => {
            ensure!(
                is_valid_object_ref(ast, o),
                GeneratorErrorKind::UnknownObject,
                "{}.{}",
                namespace,
                method.name
            );
        }

        CidlType::DataSource(model_name) => ensure!(
            is_valid_data_source_ref(ast, model_name),
            GeneratorErrorKind::InvalidModelReference,
            "{}.{}",
            namespace,
            method.name,
        ),

        CidlType::Inject(o) => fail!(
            GeneratorErrorKind::UnexpectedInject,
            "{}.{} => {}?",
            namespace,
            method.name,
            o
        ),
        CidlType::Stream => ensure!(
            matches!(method.return_type, CidlType::Stream),
            GeneratorErrorKind::InvalidStream,
            "{}.{}",
            namespace,
            method.name
        ),
        _ => {}
    }

    // Validate method params
    let mut ds = 0;
    for param in &method.parameters {
        if let CidlType::DataSource(model_name) = &param.cidl_type {
            ensure!(
                is_valid_data_source_ref(ast, model_name),
                GeneratorErrorKind::InvalidModelReference,
                "{}.{} data source references {}",
                namespace,
                method.name,
                model_name
            );

            if *model_name == namespace {
                ds += 1;
            }

            continue;
        }

        let root_type = param.cidl_type.root_type();

        match root_type {
            CidlType::Void => {
                fail!(
                    GeneratorErrorKind::UnexpectedVoid,
                    "{}.{}.{}",
                    namespace,
                    method.name,
                    param.name
                )
            }
            CidlType::Object(o) | CidlType::Partial(o) => {
                ensure!(
                    is_valid_object_ref(ast, o),
                    GeneratorErrorKind::UnknownObject,
                    "{}.{}.{}",
                    namespace,
                    method.name,
                    param.name
                );

                // TODO: remove this
                if method.http_verb == HttpVerb::GET {
                    fail!(
                        GeneratorErrorKind::NotYetSupported,
                        "GET Requests currently do not support object parameters {}.{}.{}",
                        namespace,
                        method.name,
                        param.name
                    )
                }
            }
            CidlType::DataSource(model_name) => {
                ensure!(
                    ast.d1_models.contains_key(model_name),
                    GeneratorErrorKind::InvalidModelReference,
                    "{}.{} data source references {}",
                    namespace,
                    method.name,
                    ds
                )
            }
            CidlType::Stream => {
                let valid_params_len = if method.is_static {
                    // There should only be the stream param
                    method.parameters.len() == 1
                } else {
                    // There should be a data source and the stream param
                    method.parameters.len() < 3
                };

                ensure!(
                    valid_params_len && matches!(param.cidl_type, CidlType::Stream),
                    GeneratorErrorKind::InvalidStream,
                    "{}.{}",
                    namespace,
                    method.name
                )
            }
            _ => {
                // Ignore
            }
        }
    }

    Ok(ds)
}

// Kahns algorithm for topological sort + cycle detection.
// If no cycles, returns a map of id to position used for sorting the original collection.
fn kahns<'a>(
    graph: AdjacencyList<'a>,
    mut in_degree: BTreeMap<&'a str, usize>,
    len: usize,
) -> Result<HashMap<String, usize>> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(model_name) = queue.pop_front() {
        rank.insert(model_name.to_string(), counter);
        counter += 1;

        if let Some(adjs) = graph.get(model_name) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if rank.len() != len {
        let cyclic: Vec<&str> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();
        fail!(
            GeneratorErrorKind::CyclicalDependency,
            "{}",
            cyclic.join(", ")
        );
    }

    Ok(rank)
}
