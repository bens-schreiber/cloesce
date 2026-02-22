use std::collections::{BTreeMap, HashMap, VecDeque};

use ast::err::{GeneratorErrorKind, Result};
use ast::{
    ApiMethod, CidlType, CloesceAst, CrudKind, HttpVerb, Model, NamedTypedValue,
    NavigationPropertyKind, WranglerSpec, cidl_type_contains, ensure, fail,
};

type AdjacencyList<'a> = BTreeMap<&'a str, Vec<&'a str>>;

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    /// Analyzes the grammar of the AST, yielding a [GeneratorErrorKind] on failure.
    ///
    /// Sorts models topologically in SQL insertion order, and
    /// sorts services + plain old objects in constructor order.
    ///
    /// Returns a [GeneratorErrorKind] on failure.
    pub fn analyze(ast: &mut CloesceAst, spec: &WranglerSpec) -> Result<()> {
        // Wrangler must be validated first so that the env can be
        // used in subsequent calls
        Self::wrangler(ast, spec)?;

        Self::models(ast)?;
        Self::poos(ast)?;
        Self::services(ast)?;
        Ok(())
    }

    fn wrangler(ast: &CloesceAst, spec: &WranglerSpec) -> Result<()> {
        let env = match &ast.wrangler_env {
            Some(env) => env,

            // No models => no wrangler needed
            None if ast.models.is_empty() => return Ok(()),

            _ => fail!(
                GeneratorErrorKind::MissingWranglerEnv,
                "The AST is missing a WranglerEnv but models are defined"
            ),
        };

        let (has_d1, has_r2, has_kv) = ast
            .models
            .iter()
            .fold((false, false, false), |(d1, r2, kv), (_, m)| {
                (d1 || m.has_d1(), r2 || m.has_r2(), kv || m.has_kv())
            });

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
            !spec.d1_databases.is_empty() || !has_d1,
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
            !spec.kv_namespaces.is_empty() || !has_kv,
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

        // If R2 models are defined, ensure an R2 bucket binding exists
        ensure!(
            !spec.r2_buckets.is_empty() || !has_r2,
            GeneratorErrorKind::MissingWranglerKVNamespace,
            "No R2 bucket binding is defined, but R2 models are defined in the WranglerEnv ({})",
            env.source_path.display()
        );

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

    fn models(ast: &mut CloesceAst) -> Result<()> {
        if ast.wrangler_env.is_none() {
            return Ok(());
        }

        let mut d1_models = Vec::new();

        for (model_name, model) in &ast.models {
            ensure!(
                *model_name == model.name,
                GeneratorErrorKind::InvalidMapping,
                "{} : {}",
                model_name,
                model.name
            );

            if model.has_d1() {
                d1_models.push(model);
            }

            if model.has_kv() || model.has_r2() {
                Self::kv_r2_models(ast, model)?;
            }

            // Validate Data Sources (BFS)
            for ds in model.data_sources.values() {
                let mut q = VecDeque::new();
                q.push_back((&ds.tree, model));

                while let Some((node, parent_model)) = q.pop_front() {
                    for (var_name, child) in &node.0 {
                        let Some(model_name) =
                            valid_include_tree_reference(parent_model, var_name.clone())?
                        else {
                            continue;
                        };

                        let Some(child_model) = ast.models.get(model_name) else {
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
                validate_methods(&model.name, method_name, method, ast)?;
            }

            // Validate CRUD
            for crud in &model.cruds {
                if matches!(crud, CrudKind::LIST) && !model.has_d1() {
                    fail!(
                        GeneratorErrorKind::UnsupportedCrudOperation,
                        "{} has LIST CRUD but is not a D1 backed model",
                        model.name
                    );
                }
            }
        }

        // Sort models by SQL insertion order
        if !d1_models.is_empty() {
            let rank = Self::d1_models(ast, d1_models)?;
            ast.models
                .sort_by_key(|k, _| rank.get(k.as_str()).unwrap_or(&usize::MAX));
        }

        Ok(())
    }

    fn d1_models(ast: &CloesceAst, d1_models: Vec<&Model>) -> Result<HashMap<String, usize>> {
        // Topo sort and cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        // Maps a model name and a foreign key reference to the model it is referencing
        // Ie, Person.dogId => { (Person, dogId): "Dog" }
        let mut model_attr_ref_to_fk_model = HashMap::<(&str, &str), &str>::new();
        let mut unvalidated_navs = Vec::new();

        // Maps a m2m unique id to the models that reference the id
        let mut m2m = HashMap::<String, Vec<&String>>::new();

        // Validate Models D1 grammar
        for model in &d1_models {
            if !model.has_d1() {
                continue;
            }

            let Some(primary_key) = &model.primary_key else {
                fail!(GeneratorErrorKind::MissingPrimaryKey, "{}", model.name);
            };

            graph.entry(&model.name).or_default();
            in_degree.entry(&model.name).or_insert(0);

            // Validate PK
            ensure!(
                !primary_key.cidl_type.is_nullable(),
                GeneratorErrorKind::NullPrimaryKey,
                "{}.{}",
                model.name,
                primary_key.name
            );
            ensure_valid_sql_type(model, primary_key)?;

            // Validate columns
            for col in &model.columns {
                ensure_valid_sql_type(model, &col.value)?;

                if let Some(fk_model_name) = &col.foreign_key_reference {
                    let Some(fk_model) = ast.models.get(fk_model_name.as_str()) else {
                        fail!(
                            GeneratorErrorKind::InvalidModelReference,
                            "{}.{} => {}?",
                            model.name,
                            col.value.name,
                            fk_model_name
                        );
                    };

                    let Some(fk_model_pk) = fk_model.primary_key.as_ref() else {
                        fail!(
                            GeneratorErrorKind::InvalidModelReference,
                            "{}.{} => {} has no primary key?",
                            model.name,
                            col.value.name,
                            fk_model_name
                        );
                    };

                    // Validate the types are equal
                    ensure!(
                        *col.value.cidl_type.root_type() == fk_model_pk.cidl_type,
                        GeneratorErrorKind::MismatchedForeignKeyTypes,
                        "{}.{} ({:?}) != {}.{} ({:?})",
                        model.name,
                        col.value.name,
                        col.value.cidl_type,
                        fk_model_name,
                        fk_model_pk.name,
                        fk_model_pk.cidl_type
                    );

                    model_attr_ref_to_fk_model
                        .insert((&model.name, col.value.name.as_str()), fk_model_name);

                    // Nullable FK's do not constrain table creation order, and thus
                    // can be left out of the topo sort
                    if !col.value.cidl_type.is_nullable() {
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
                    ast.models.contains_key(nav.model_reference.as_str()),
                    GeneratorErrorKind::InvalidModelReference,
                    "{} => {}?",
                    model.name,
                    nav.model_reference
                );

                match &nav.kind {
                    NavigationPropertyKind::OneToOne { column_reference } => {
                        // Validate the nav prop's reference is consistent
                        if let Some(&fk_model) =
                            model_attr_ref_to_fk_model.get(&(&model.name, column_reference))
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
                                column_reference,
                                model.name
                            );
                        }
                    }
                    NavigationPropertyKind::OneToMany { .. } => {
                        unvalidated_navs.push((&model.name, &nav.model_reference, nav));
                    }
                    NavigationPropertyKind::ManyToMany => {
                        let id = nav.many_to_many_table_name(&model.name);
                        m2m.entry(id).or_default().push(&model.name);
                    }
                }
            }
        }

        // Validate 1:M nav props
        for (model_name, nav_model, nav) in unvalidated_navs {
            let NavigationPropertyKind::OneToMany { column_reference } = &nav.kind else {
                continue;
            };

            // Validate the nav props reference is consistent to an column
            // on another model
            let Some(&fk_model) = model_attr_ref_to_fk_model.get(&(nav_model, column_reference))
            else {
                fail!(
                    GeneratorErrorKind::InvalidNavigationPropertyReference,
                    "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                    model_name,
                    nav.var_name,
                    nav_model,
                    column_reference,
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
                column_reference,
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

        kahns(graph, in_degree, d1_models.len())
    }

    fn kv_r2_models(ast: &CloesceAst, model: &Model) -> Result<()> {
        // Validate KV key format
        for kv in &model.kv_objects {
            let vars = extract_braced(&kv.format)?;

            for var in vars {
                ensure!(
                    model.columns.iter().any(|col| col.value.name == var)
                        || model.key_params.contains(&var)
                        || model
                            .primary_key
                            .as_ref()
                            .map(|pk| pk.name == var)
                            .unwrap_or(false),
                    GeneratorErrorKind::UnknownKeyReference,
                    "{}.{} => {} missing key param for variable {}",
                    model.name,
                    kv.value.name,
                    kv.format,
                    var
                )
            }

            // Validate value type
            match &kv.value.cidl_type {
                CidlType::Object(o) | CidlType::Partial(o) => {
                    ensure!(
                        is_valid_object_ref(ast, o),
                        GeneratorErrorKind::UnknownObject,
                        "{}.{} => {}?",
                        model.name,
                        kv.value.name,
                        o
                    );
                }
                CidlType::Inject(o) => {
                    fail!(
                        GeneratorErrorKind::UnexpectedInject,
                        "{}.{} => {}?",
                        model.name,
                        kv.value.name,
                        o
                    )
                }
                CidlType::DataSource(reference) => ensure!(
                    is_valid_data_source_ref(ast, reference),
                    GeneratorErrorKind::InvalidModelReference,
                    "{}.{} => {}?",
                    model.name,
                    kv.value.name,
                    reference
                ),
                _ => {}
            }
        }

        // Validate R2 Key format
        for r2 in &model.r2_objects {
            let vars = extract_braced(&r2.format)?;

            for var in vars {
                ensure!(
                    model.columns.iter().any(|col| col.value.name == var)
                        || model.key_params.contains(&var)
                        || model
                            .primary_key
                            .as_ref()
                            .map(|pk| pk.name == var)
                            .unwrap_or(false),
                    GeneratorErrorKind::UnknownKeyReference,
                    "{}.{} => {} missing key param for variable {}",
                    model.name,
                    r2.var_name,
                    r2.format,
                    var
                )
            }
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

/// Extracts braced variables from a format string.
/// e.g, "users/{userId}/posts/{postId}" => ["userId", "postId"].
///
/// Returns a [GeneratorErrorKind] if the format string is invalid.
fn extract_braced(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut current = None;

    for c in s.chars() {
        match (current.as_mut(), c) {
            (None, '{') => current = Some(String::new()),
            (Some(_), '{') => {
                fail!(GeneratorErrorKind::InvalidKeyFormat, "nested brace in key");
            }
            (Some(buf), '}') => {
                out.push(std::mem::take(buf));
                current = None;
            }
            (Some(buf), c) => buf.push(c),
            _ => {}
        }
    }

    if current.is_some() {
        fail!(
            GeneratorErrorKind::InvalidKeyFormat,
            "unclosed brace in key"
        );
    }

    Ok(out)
}

/// Ensures the given [NamedTypedValue] can be mapped to a valid
/// SQLite type.
///
/// Returns a [GeneratorErrorKind] on failure.
fn ensure_valid_sql_type(model: &Model, value: &NamedTypedValue) -> Result<()> {
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
}

fn is_valid_object_ref(ast: &CloesceAst, o: &String) -> bool {
    ast.models.contains_key(o) || ast.poos.contains_key(o)
}

fn is_valid_data_source_ref(ast: &CloesceAst, o: &String) -> bool {
    ast.models.contains_key(o)
}

/// Validates an [ApiMethod]'s grammar.
///
/// Returns a [GeneratorErrorKind] on failure.
fn validate_methods(
    namespace: &str,
    method_name: &str,
    method: &ApiMethod,
    ast: &CloesceAst,
) -> Result<()> {
    // Validate record
    ensure!(
        *method_name == method.name,
        GeneratorErrorKind::InvalidMapping,
        "Method record key did not match it's method name? {}: {}",
        method_name,
        method.name
    );

    // Validate data source reference
    if let Some(ds) = &method.data_source {
        ensure!(
            !method.is_static,
            GeneratorErrorKind::InvalidDataSourceReference,
            "{}.{} has a data source but is a static method.",
            namespace,
            method.name
        );

        let Some(model) = ast.models.get(namespace) else {
            fail!(
                GeneratorErrorKind::InvalidModelReference,
                "{}.{} references a data source on an unknown model {}",
                namespace,
                method.name,
                namespace
            );
        };

        ensure!(
            model.data_sources.contains_key(ds),
            GeneratorErrorKind::UnknownDataSourceReference,
            "{}.{} references an unknown data source {} on model {}",
            namespace,
            method.name,
            ds,
            namespace
        );
    }

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
            GeneratorErrorKind::UnknownDataSourceReference,
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
            // Stream or HttpResult<Stream>
            matches!(method.return_type, CidlType::Stream)
                || matches!(&method.return_type, CidlType::HttpResult(boxed) if matches!(**boxed, CidlType::Stream)),
            GeneratorErrorKind::InvalidStream,
            "{}.{}",
            namespace,
            method.name
        ),
        _ => {}
    }

    // Validate method params
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

            continue;
        }

        ensure!(
            !cidl_type_contains!(&param.cidl_type, CidlType::HttpResult(_)),
            GeneratorErrorKind::NotYetSupported,
            "Requests currently do not support HttpResult parameters {}.{}.{}",
            namespace,
            method.name,
            param.name
        );

        // todo: remove this limitation
        ensure!(
            method.http_verb != HttpVerb::GET
                || !cidl_type_contains!(&param.cidl_type, CidlType::KvObject(_)),
            GeneratorErrorKind::NotYetSupported,
            "GET Requests currently do not support KV Object parameters {}.{}.{}",
            namespace,
            method.name,
            param.name
        );

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
            CidlType::R2Object => {
                // TODO: remove this
                if method.http_verb == HttpVerb::GET {
                    fail!(
                        GeneratorErrorKind::NotYetSupported,
                        "GET Requests currently do not support R2Object parameters {}.{}.{}",
                        namespace,
                        method.name,
                        param.name
                    )
                }
            }
            CidlType::DataSource(model_name) => {
                ensure!(
                    ast.models.contains_key(model_name),
                    GeneratorErrorKind::InvalidModelReference,
                    "{}.{} data source references {}",
                    namespace,
                    method.name,
                    model_name
                )
            }
            CidlType::Stream => {
                let required_params = method
                    .parameters
                    .iter()
                    .filter(|p| {
                        !matches!(p.cidl_type, CidlType::Inject(_) | CidlType::DataSource(_))
                    })
                    .count();

                ensure!(
                    required_params == 1 && matches!(param.cidl_type, CidlType::Stream),
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

    Ok(())
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

/// Ensures that a reference within an include tree exists within the given model.
///
/// Returns the referenced model name if the reference is a navigation property,
/// or None if the reference is a KV or R2 object.
fn valid_include_tree_reference(model: &Model, var_name: String) -> Result<Option<&str>> {
    if let Some(nav) = model
        .navigation_properties
        .iter()
        .find(|nav| nav.var_name == var_name)
    {
        return Ok(Some(&nav.model_reference));
    }

    if model.kv_objects.iter().any(|kv| kv.value.name == var_name) {
        return Ok(None);
    }

    if model.r2_objects.iter().any(|r2| r2.var_name == var_name) {
        return Ok(None);
    }

    fail!(
        GeneratorErrorKind::UnknownIncludeTreeReference,
        "{}.{}",
        model.name,
        var_name
    );
}
