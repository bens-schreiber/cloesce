pub mod builder;
pub mod err;

use std::collections::BTreeMap;
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

    /// SQLite large structured data
    Blob,

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
    pub value: NamedTypedValue,
    pub foreign_key_reference: Option<String>,
    pub hash: Option<u64>,
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
    pub name: String,
    pub tree: IncludeTree,
    pub hash: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum NavigationPropertyKind {
    OneToOne { reference: String },
    OneToMany { reference: String },
    ManyToMany { unique_id: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NavigationProperty {
    pub var_name: String,
    pub model_name: String,
    pub kind: NavigationPropertyKind,
    pub hash: Option<u64>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Model {
    pub name: String,
    pub primary_key: NamedTypedValue,
    pub attributes: Vec<ModelAttribute>,
    pub navigation_properties: Vec<NavigationProperty>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub methods: BTreeMap<String, ModelMethod>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub data_sources: BTreeMap<String, DataSource>,

    pub source_path: PathBuf,
    pub hash: Option<u64>,
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
}

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct CloesceAst {
    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub wrangler_env: WranglerEnv,

    // TODO: MapPreventDuplicates is not supported for IndexMap
    pub models: IndexMap<String, Model>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub poos: BTreeMap<String, PlainOldObject>,

    pub app_source: Option<PathBuf>,
    pub hash: Option<u64>,
}

impl CloesceAst {
    pub fn from_json(path: &std::path::Path) -> Result<CloesceAst> {
        let cidl_contents = std::fs::read_to_string(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        serde_json::from_str::<CloesceAst>(&cidl_contents).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })
    }

    fn is_valid_object_ref(&self, o: &str) -> bool {
        self.models.contains_key(o) || self.poos.contains_key(o)
    }

    /// Ensures all `CidlTypes` are logically correct for the area, essentially doing
    /// the first level of semantic analysis for the generator.
    ///
    /// Returns error on
    /// - Model attributes with invalid SQL types
    /// - Primary keys with invalid SQL types
    /// - Invalid Model or Method map K/V
    /// - Unknown navigation property model references
    /// - Unknown model references in method parameters
    /// - Invalid method parameter types
    pub fn validate_types(&self) -> Result<()> {
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
                    CidlType::Integer | CidlType::Real | CidlType::Text | CidlType::Blob
                ),
                GeneratorErrorKind::InvalidSqlType,
                "{}.{}",
                model.name,
                value.name
            );

            Ok(())
        };

        for (model_name, model) in &self.models {
            // Validate record
            ensure!(
                *model_name == model.name,
                GeneratorErrorKind::InvalidMapping,
                "Model record key did not match it's model name? {} : {}",
                model_name,
                model.name
            );

            // Validate PK
            ensure_valid_sql_type(model, &model.primary_key)?;

            // Validate attributes
            for a in &model.attributes {
                ensure_valid_sql_type(model, &a.value)?;

                if let Some(fk_model) = &a.foreign_key_reference {
                    // Validate the fk's model exists
                    ensure!(
                        self.models.contains_key(fk_model.as_str()),
                        GeneratorErrorKind::UnknownObject,
                        "{}.{} => {}?",
                        model.name,
                        a.value.name,
                        fk_model
                    );
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
                            self.is_valid_object_ref(o),
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
                                self.is_valid_object_ref(o),
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

        Ok(())
    }

    /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    ///
    /// TODO: It would be neat if this could be done while deserializing.
    /// It could also be combined with [Self::validate_types] for less O(n) traversals.
    pub fn set_merkle_hash(&mut self) {
        if self.hash.is_some() {
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

                attr.hash = Some(attr_h);
                model_h.write_u64(attr_h);
            }

            for ds in model.data_sources.values_mut() {
                let ds_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelDataSource");
                    ds.name.hash(&mut h);

                    let mut q = VecDeque::new();
                    q.push_back(&ds.tree);
                    while let Some(n) = q.pop_front() {
                        for (k, v) in &n.0 {
                            k.hash(&mut h);
                            q.push_back(v);
                        }
                    }

                    h.finish()
                };

                ds.hash = Some(ds_h);
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

                nav.hash = Some(nav_h);
                model_h.write_u64(nav_h);
            }

            let model_h_finished = model_h.finish();
            model.hash = Some(model_h_finished);
            root_h.write_u64(model_h_finished);
        }

        self.hash = Some(root_h.finish())
    }
}
