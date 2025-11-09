pub mod builder;
pub mod err;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::PathBuf;

use err::GeneratorErrorKind;
use err::Result;
use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;
use serde::Serialize;
use serde_with::MapPreventDuplicates;
use serde_with::serde_as;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CidlType {
    /// No type
    Void,

    /// SQLite integer
    Integer,

    /// SQLite floating point number
    Real,

    /// SQLite string
    Text,

    /// (SQL equivalent to Integer)
    Boolean,

    /// An ISO Date string (SQL equivalent to Text)
    DateIso,

    /// A dependency injected instance, containing a type name.
    Inject(String),

    /// A model, or plain old object, containing the name of the class.
    Object(String),

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial(String),

    /// A data source of some model
    DataSource(String),

    /// An array of any type
    Array(Box<CidlType>),

    /// A REST API response, which can contain any type or nothing.
    HttpResult(Box<CidlType>),

    /// A wrapper denoting the type within can be null.
    /// If the inner value is void, represents just null.
    Nullable(Box<CidlType>),
}

impl CidlType {
    /// Returns the inner part of an array if the type is an array
    pub fn unwrap_array(&self) -> Option<&CidlType> {
        match self {
            CidlType::Array(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns the root most CidlType, being any non Model/Array/Result.
    pub fn root_type(&self) -> &CidlType {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.root_type(),
            CidlType::Nullable(inner) => inner.root_type(),
            t => t,
        }
    }

    pub fn is_nullable(&self) -> bool {
        matches!(self, CidlType::Nullable(_))
    }

    pub fn array(cidl_type: CidlType) -> CidlType {
        CidlType::Array(Box::new(cidl_type))
    }

    pub fn nullable(cidl_type: CidlType) -> CidlType {
        CidlType::Nullable(Box::new(cidl_type))
    }

    pub fn null() -> CidlType {
        CidlType::Nullable(Box::new(CidlType::Void))
    }

    pub fn http(cidl_type: CidlType) -> CidlType {
        CidlType::HttpResult(Box::new(cidl_type))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpVerb {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct NamedTypedValue {
    pub name: String,
    pub cidl_type: CidlType,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModelAttribute {
    #[serde(default)]
    pub hash: u64,

    pub value: NamedTypedValue,
    pub foreign_key_reference: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModelMethod {
    pub name: String,
    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub return_type: CidlType,
    pub parameters: Vec<NamedTypedValue>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

#[derive(Serialize, Deserialize, Debug)]
pub struct DataSource {
    #[serde(default)]
    pub hash: u64,

    pub name: String,
    pub tree: IncludeTree,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum NavigationPropertyKind {
    OneToOne { reference: String },
    OneToMany { reference: String },
    ManyToMany { unique_id: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NavigationProperty {
    #[serde(default)]
    pub hash: u64,

    pub var_name: String,
    pub model_name: String,
    pub kind: NavigationPropertyKind,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Model {
    #[serde(default)]
    pub hash: u64,

    pub name: String,
    pub primary_key: NamedTypedValue,
    pub attributes: Vec<ModelAttribute>,
    pub navigation_properties: Vec<NavigationProperty>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub methods: BTreeMap<String, ModelMethod>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub data_sources: BTreeMap<String, DataSource>,

    pub cruds: Vec<CrudKind>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlainOldObject {
    pub name: String,
    pub attributes: Vec<NamedTypedValue>,
    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub enum InputLanguage {
    TypeScript,
}

#[derive(Serialize, Deserialize)]
pub struct WranglerEnv {
    pub name: String,
    pub source_path: PathBuf,
    pub db_binding: String,
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct CloesceAst {
    #[serde(default)]
    pub hash: u64,

    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub wrangler_env: WranglerEnv,

    // TODO: MapPreventDuplicates is not supported for IndexMap
    pub models: IndexMap<String, Model>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub poos: BTreeMap<String, PlainOldObject>,

    pub app_source: Option<PathBuf>,
}

impl CloesceAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self> {
        let cidl_contents = std::fs::read_to_string(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        serde_json::from_str::<Self>(&cidl_contents).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })
    }

    pub fn to_json(&self) -> String {
        assert!(self.hash != 0u64);
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }

    pub fn to_migrations_json(self) -> String {
        assert!(self.hash != 0u64);
        let Self { hash, models, .. } = self;

        let migrations_models: IndexMap<String, MigrationsModel> = models
            .into_iter()
            .map(|(name, model)| {
                let m = MigrationsModel {
                    hash: model.hash,
                    name: model.name,
                    primary_key: model.primary_key,
                    attributes: model.attributes,
                    navigation_properties: model.navigation_properties,
                    data_sources: model.data_sources,
                };
                (name, m)
            })
            .collect();

        let migrations_ast = MigrationsAst {
            hash,
            models: migrations_models,
        };

        serde_json::to_string_pretty(&migrations_ast).expect("serialize migrations ast to work")
    }

    /// Analyzes the grammar of the AST, yielding a [GeneratorErrorKind] on failure.
    ///
    /// Sorts models topologically in SQL insertion order.
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
    pub fn semantic_analysis(&mut self) -> Result<()> {
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

        let is_valid_object_ref = |o| self.models.contains_key(o) || self.poos.contains_key(o);

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
        for (model_name, model) in &self.models {
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
                    let Some(fk_model) = self.models.get(fk_model_name.as_str()) else {
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
                    self.models.contains_key(nav.model_name.as_str()),
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

                        let Some(child_model) = self.models.get(model_name) else {
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

            // Validate methods
            for (method_name, method) in &model.methods {
                // Validate record
                ensure!(
                    *method_name == method.name,
                    GeneratorErrorKind::InvalidMapping,
                    "Method record key did not match it's method name? {}: {}",
                    method_name,
                    method.name
                );

                // Validate return type
                match &method.return_type {
                    CidlType::Object(o) => {
                        ensure!(
                            is_valid_object_ref(o),
                            GeneratorErrorKind::UnknownObject,
                            "{}.{}",
                            model.name,
                            method.name
                        );
                    }
                    CidlType::Partial(_) => {
                        fail!(
                            GeneratorErrorKind::UnexpectedPartialReturn,
                            "{}.{}",
                            model.name,
                            method.name,
                        )
                    }
                    _ => {}
                }

                // Validate method params
                let mut ds = 0;
                for param in &method.parameters {
                    if let CidlType::DataSource(model_name) = &param.cidl_type {
                        ensure!(
                            self.models.contains_key(model_name),
                            GeneratorErrorKind::UnknownDataSourceReference,
                            "{}.{} data source references {}",
                            model.name,
                            method.name,
                            model_name
                        );

                        if *model_name == model.name {
                            ds += 1;
                        }
                    }

                    let root_type = param.cidl_type.root_type();

                    match root_type {
                        CidlType::Void => {
                            fail!(
                                GeneratorErrorKind::UnexpectedVoid,
                                "{}.{}.{}",
                                model.name,
                                method.name,
                                param.name
                            )
                        }
                        CidlType::Object(o) | CidlType::Partial(o) => {
                            ensure!(
                                is_valid_object_ref(o),
                                GeneratorErrorKind::UnknownObject,
                                "{}.{}.{}",
                                model.name,
                                method.name,
                                param.name
                            );

                            // TODO: remove this
                            if method.http_verb == HttpVerb::GET {
                                fail!(
                                    GeneratorErrorKind::NotYetSupported,
                                    "GET Requests currently do not support model parameters {}.{}.{}",
                                    model.name,
                                    method.name,
                                    param.name
                                )
                            }
                        }
                        _ => {
                            // Ignore
                        }
                    }
                }

                if !method.is_static {
                    ensure!(
                        ds == 1,
                        GeneratorErrorKind::MissingOrExtraneousDataSource,
                        "Instantiated methods require one data source: {}.{}",
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

        // Kahn's algorithm
        let rank = {
            let mut queue = in_degree
                .iter()
                .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
                .collect::<VecDeque<_>>();

            let mut rank = HashMap::with_capacity(self.models.len());
            let mut counter = 0usize;

            while let Some(model_name) = queue.pop_front() {
                rank.insert(model_name.to_string(), counter);
                counter += 1;

                if let Some(adjs) = graph.get(model_name) {
                    for adj in adjs {
                        let deg = in_degree.get_mut(adj).expect("model names to be validated");
                        *deg -= 1;

                        if *deg == 0 {
                            queue.push_back(adj);
                        }
                    }
                }
            }

            if rank.len() != self.models.len() {
                let cyclic: Vec<&str> = in_degree
                    .iter()
                    .filter_map(|(&n, &d)| (d > 0).then_some(n))
                    .collect();
                fail!(
                    GeneratorErrorKind::CyclicalModelDependency,
                    "{}",
                    cyclic.join(", ")
                );
            }

            rank
        };

        // Sort models by SQL insertion order
        self.models
            .sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

        Ok(())
    }

    /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    pub fn set_merkle_hash(&mut self) {
        if self.hash != 0u64 {
            // If the root is hashed, it's safe to assume all children are hashed.
            // No work to be done.
            return;
        }

        let mut root_h = FxHasher::default();
        for model in self.models.values_mut() {
            let mut model_h = FxHasher::default();
            model_h.write(b"Model");
            model.primary_key.hash(&mut model_h);
            model.name.hash(&mut model_h);

            for attr in model.attributes.iter_mut() {
                let attr_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelAttribute");
                    attr.value.hash(&mut h);
                    attr.foreign_key_reference.hash(&mut h);
                    h.finish()
                };

                attr.hash = attr_h;
                model_h.write_u64(attr_h);
            }

            for ds in model.data_sources.values_mut() {
                let ds_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelDataSource");
                    ds.name.hash(&mut h);

                    fn dfs(node: &IncludeTree, h: &mut FxHasher) {
                        for (k, v) in &node.0 {
                            dfs(v, h);
                            k.hash(h);
                        }
                    }

                    dfs(&ds.tree, &mut h);
                    h.finish()
                };

                ds.hash = ds_h;
                model_h.write_u64(ds_h);
            }

            for nav in model.navigation_properties.iter_mut() {
                let nav_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelNavigationProperty");
                    nav.model_name.hash(&mut h);
                    nav.var_name.hash(&mut h);
                    nav.kind.hash(&mut h);
                    h.finish()
                };

                nav.hash = nav_h;
                model_h.write_u64(nav_h);
            }

            let model_h_finished = model_h.finish();
            model.hash = model_h_finished;
            root_h.write_u64(model_h_finished);
        }

        self.hash = root_h.finish();
    }
}

/// A subset of [Model] suited for migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsModel {
    pub hash: u64,
    pub name: String,
    pub primary_key: NamedTypedValue,
    pub attributes: Vec<ModelAttribute>,
    pub navigation_properties: Vec<NavigationProperty>,
    pub data_sources: BTreeMap<String, DataSource>,
}

/// A subset of [CloesceAst] suited for migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsAst {
    pub hash: u64,
    pub models: IndexMap<String, MigrationsModel>,
}

impl MigrationsAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        serde_json::from_str::<Self>(&contents).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }
}
