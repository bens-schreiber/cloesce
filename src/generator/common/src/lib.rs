pub mod builder;
pub mod err;

use std::collections::BTreeMap;
use std::path::PathBuf;

use err::GeneratorErrorKind;
use err::Result;
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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub models: BTreeMap<String, Model>,

    #[serde_as(as = "MapPreventDuplicates<_, _>")]
    pub poos: BTreeMap<String, PlainOldObject>,
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
                for param in &method.parameters {
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
            }
        }

        Ok(())
    }
}
