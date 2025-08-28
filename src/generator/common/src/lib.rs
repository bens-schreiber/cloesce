use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum CidlType {
    Integer,
    Real,
    Text,
    Blob,
}

#[derive(Serialize, Deserialize)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Serialize, Deserialize)]
pub struct TypedValue {
    pub name: String,
    pub cidl_type: CidlType,
    pub nullable: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Attribute {
    pub value: TypedValue,
    pub primary_key: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Method {
    pub name: String,
    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub parameters: Vec<TypedValue>,
}

#[derive(Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub methods: Vec<Method>,
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

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WranglerSpec {
    #[serde(default)]
    pub d1_databases: Vec<D1Database>,
}
