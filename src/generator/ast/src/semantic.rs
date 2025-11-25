use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::err::{GeneratorErrorKind, Result};
use crate::{
    ApiMethod, CidlType, CloesceAst, HttpVerb, Model, NamedTypedValue, NavigationPropertyKind,
    ensure, fail,
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
    pub fn analyze(ast: &mut CloesceAst) -> Result<()> {
        // Validate models
        Self::models(ast)?;

        // Validate Plain Old Objects
        Self::poos(ast)?;

        // Validate Services
        Self::services(ast)?;

        Ok(())
    }

    fn poos(ast: &mut CloesceAst) -> Result<()> {
        // Topo sort and cycle detection
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

        // Sort
        let rank = kahns(graph, in_degree, ast.poos.len())?;
        ast.poos.sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

        Ok(())
    }

    fn models(ast: &mut CloesceAst) -> Result<()> {
        let ensure_valid_sql_type = |model: &Model, value: &NamedTypedValue| {
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
        let mut model_reference_to_fk_model = HashMap::<(&str, &str), &str>::new();
        let mut unvalidated_navs = Vec::new();

        // Maps a m2m unique id to the models that reference the id
        let mut m2m = HashMap::<&String, Vec<&String>>::new();

        // Validate Models
        for (model_name, model) in &ast.models {
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
                    let Some(fk_model) = ast.models.get(fk_model_name.as_str()) else {
                        fail!(
                            GeneratorErrorKind::UnknownObject,
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

                    model_reference_to_fk_model
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
                    ast.models.contains_key(nav.model_name.as_str()),
                    GeneratorErrorKind::UnknownObject,
                    "{} => {}?",
                    model.name,
                    nav.model_name
                );

                match &nav.kind {
                    NavigationPropertyKind::OneToOne { reference } => {
                        // Validate the nav prop's reference is consistent
                        if let Some(&fk_model) =
                            model_reference_to_fk_model.get(&(&model.name, reference))
                        {
                            ensure!(
                                fk_model == nav.model_name,
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
                                nav.model_name,
                                reference,
                                model.name
                            );
                        }
                    }
                    NavigationPropertyKind::OneToMany { reference: _ } => {
                        unvalidated_navs.push((&model.name, &nav.model_name, nav));
                    }
                    NavigationPropertyKind::ManyToMany { unique_id } => {
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
                            .map(|nav| &nav.model_name)
                        else {
                            fail!(
                                GeneratorErrorKind::UnknownIncludeTreeReference,
                                "{}.{}",
                                model.name,
                                var_name
                            );
                        };

                        let Some(child_model) = ast.models.get(model_name) else {
                            fail!(
                                GeneratorErrorKind::UnknownObject,
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
            let NavigationPropertyKind::OneToMany { reference } = &nav.kind else {
                continue;
            };

            // Validate the nav props reference is consistent to an attribute
            // on another model
            let Some(&fk_model) = model_reference_to_fk_model.get(&(nav_model, reference)) else {
                fail!(
                    GeneratorErrorKind::InvalidNavigationPropertyReference,
                    "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                    model_name,
                    nav.var_name,
                    nav_model,
                    reference,
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
                reference,
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
        let rank = kahns(graph, in_degree, ast.models.len())?;
        ast.models.sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

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
                if !ast.services.contains_key(&attr.injected) {
                    continue;
                }

                graph
                    .entry(attr.injected.as_str())
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
    ast.models.contains_key(o) || ast.poos.contains_key(o)
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
        CidlType::Object(o) | CidlType::Partial(o) => ensure!(
            is_valid_object_ref(ast, o),
            GeneratorErrorKind::UnknownObject,
            "{}.{}",
            namespace,
            method.name
        ),

        CidlType::DataSource(o) => ensure!(
            ast.models.contains_key(o),
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
                ast.models.contains_key(model_name),
                GeneratorErrorKind::UnknownDataSourceReference,
                "{}.{} data source references {}",
                namespace,
                method.name,
                model_name
            );

            if *model_name == namespace {
                ds += 1;
            }
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
            CidlType::Stream => {
                ensure!(
                    method.parameters.len() == 1 && matches!(param.cidl_type, CidlType::Stream),
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
