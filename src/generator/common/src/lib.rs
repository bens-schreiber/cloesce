pub mod builder;
pub mod wrangler;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

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

    /// A Cloesce model, containing it's name
    Model(String),

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
    ///
    /// Option as the type could be null
    pub fn root_type(&self) -> Option<&CidlType> {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.root_type(),
            CidlType::Nullable(inner) => inner.root_type(),
            t => Some(t),
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
pub struct IncludeTree(pub Vec<(NamedTypedValue, IncludeTree)>);

impl IncludeTree {
    pub fn to_lookup(&self) -> HashMap<&NamedTypedValue, &IncludeTree> {
        self.0
            .iter()
            .map(|(tv, tree)| (tv, tree))
            .collect::<HashMap<_, _>>()
    }
}

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
    pub value: NamedTypedValue,
    pub kind: NavigationPropertyKind,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Model {
    pub name: String,
    pub attributes: Vec<ModelAttribute>,
    pub primary_key: NamedTypedValue,
    pub navigation_properties: Vec<NavigationProperty>,
    pub methods: Vec<ModelMethod>,
    pub data_sources: Vec<DataSource>,
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

#[derive(Serialize, Deserialize)]
pub struct CidlSpec {
    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub wrangler_env: WranglerEnv,
    pub models: Vec<Model>,
}
