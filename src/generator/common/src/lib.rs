pub mod builder;
pub mod wrangler;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CidlType {
    Integer,
    Real,
    Text,
    Blob,
    D1Database,
    Model(String),
    Array(Box<CidlType>),
    HttpResult(Option<Box<CidlType>>),
}

impl CidlType {
    pub fn unwrap_array(&self) -> Option<&CidlType> {
        match self {
            CidlType::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn array_type(&self) -> &CidlType {
        match self {
            CidlType::Array(inner) => inner.array_type(),
            _ => self,
        }
    }

    pub fn array(cidl_type: CidlType) -> CidlType {
        CidlType::Array(Box::new(cidl_type))
    }
}

#[derive(Serialize, Deserialize, Debug)]
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
    pub nullable: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModelAttribute {
    pub value: NamedTypedValue,
    pub is_primary_key: bool,
    pub foreign_key_reference: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModelMethod {
    pub name: String,
    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub return_type: Option<CidlType>,
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
    pub navigation_properties: Vec<NavigationProperty>,
    pub methods: Vec<ModelMethod>,
    pub data_sources: Vec<DataSource>,
    pub source_path: PathBuf,
}

impl Model {
    /// Linear searches over attributes to find the primary key
    ///
    /// TODO: The CIDL should ensure PK's are always placed first in the list
    /// ensuring this is O(1).
    pub fn find_primary_key(&self) -> Option<&NamedTypedValue> {
        self.attributes
            .iter()
            .find_map(|a| a.is_primary_key.then_some(&a.value))
    }
}

#[derive(Serialize, Deserialize)]
pub enum InputLanguage {
    TypeScript,
}

#[derive(Serialize, Deserialize)]
pub struct CidlSpec {
    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub models: Vec<Model>,
}
