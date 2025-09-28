pub mod builder;
pub mod wrangler;

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[macro_export]
macro_rules! match_cidl {
    // Base: simple variant without inner
    ($value:expr, $variant:ident => $body:expr) => {
        if let CidlType::$variant = $value { $body } else { false }
    };

    // Base: Model(_)
    ($value:expr, Model(_) => $body:expr) => {
        if let CidlType::Model(_) = $value { $body } else { false }
    };

    // Recursive: HttpResult(inner)
    ($value:expr, HttpResult($($inner:tt)+) => $body:expr) => {
        if let CidlType::HttpResult(Some(inner)) = $value {
            match_cidl!(inner.as_ref(), $($inner)+ => $body)
        } else {
            false
        }
    };

    // Recursive: Array(inner)
    ($value:expr, Array($($inner:tt)+) => $body:expr) => {
        if let CidlType::Array(inner) = $value {
            match_cidl!(inner.as_ref(), $($inner)+ => $body)
        } else {
            false
        }
    };
}

#[macro_export]
macro_rules! matches_cidl {
    ($value:expr, $($pattern:tt)+) => {
        $crate::match_cidl!($value, $($pattern)+ => true)
    };
}

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
    /// Returns the inner part of an array if the type is an array
    pub fn unwrap_array(&self) -> Option<&CidlType> {
        match self {
            CidlType::Array(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns the root most CidlType, being any non Model/Array/Result.
    ///
    /// Option as `HttpResult` can wrap None
    pub fn root_type(&self) -> Option<&CidlType> {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.as_ref().map(|i| i.root_type())?,
            t => Some(t),
        }
    }

    pub fn array(cidl_type: CidlType) -> CidlType {
        CidlType::Array(Box::new(cidl_type))
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
    pub nullable: bool,
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
pub struct CidlSpec {
    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub models: Vec<Model>,
}
