use serde::{Deserialize, Serialize};
use std::fs;

#[repr(u32)]
#[derive(Serialize, Deserialize)]
pub enum CidlType {
    Integer = 0,
    Real = 1,
    Text = 2,
    Blob = 3,
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

impl CidlSpec {
    pub fn from_file_path(file_path: &String) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(file_path)?;
        let spec = serde_json::from_str::<CidlSpec>(&contents)?;
        Ok(spec)
    }
}
