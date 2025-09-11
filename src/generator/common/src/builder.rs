use std::path::PathBuf;

use crate::{
    Attribute, CidlForeignKey, CidlForeignKeyKind, CidlSpec, CidlType, DataSource, HttpVerb,
    IncludeTree, InputLanguage, Method, Model, TypedValue, WranglerSpec,
};

pub fn create_cidl(models: Vec<Model>) -> CidlSpec {
    CidlSpec {
        version: "1.0".to_string(),
        project_name: "test".to_string(),
        language: InputLanguage::TypeScript,
        models,
    }
}

pub fn create_wrangler() -> WranglerSpec {
    WranglerSpec {
        d1_databases: vec![],
    }
}

/// A builder pattern for tests to create models easily
pub struct ModelBuilder {
    name: String,
    attributes: Vec<Attribute>,
    methods: Vec<Method>,
    data_sources: Vec<DataSource>,
    source_path: Option<PathBuf>,
}

impl ModelBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            methods: Vec::new(),
            data_sources: Vec::new(),
            source_path: None,
        }
    }

    pub fn attribute(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        nullable: bool,
    ) -> Self {
        self.attributes.push(Attribute {
            value: TypedValue {
                name: name.into(),
                cidl_type,
                nullable,
            },
            primary_key: false,
            foreign_key: None,
        });
        self
    }

    pub fn pk(mut self, name: impl Into<String>, cidl_type: CidlType) -> Self {
        self.attributes.push(Attribute {
            value: TypedValue {
                name: name.into(),
                cidl_type,
                nullable: false,
            },
            primary_key: true,
            foreign_key: None,
        });
        self
    }

    pub fn id(self) -> Self {
        self.pk("id", CidlType::Integer)
    }

    pub fn fk(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        kind: CidlForeignKeyKind,
        model_name: impl Into<String>,
        nullable: bool,
    ) -> Self {
        self.attributes.push(Attribute {
            value: TypedValue {
                name: name.into(),
                cidl_type,
                nullable,
            },
            primary_key: false,
            foreign_key: Some(CidlForeignKey {
                kind,
                model_name: model_name.into(),
                navigation_property_name: None, // TODO: hardcoding for now
            }),
        });
        self
    }

    pub fn method(
        mut self,
        name: impl Into<String>,
        http_verb: HttpVerb,
        is_static: bool,
        parameters: Vec<TypedValue>,
    ) -> Self {
        self.methods.push(Method {
            name: name.into(),
            is_static,
            http_verb,
            parameters,
        });
        self
    }

    pub fn data_source(mut self, name: impl Into<String>, tree: IncludeTree) -> Self {
        self.data_sources.push(DataSource {
            name: name.into(),
            tree,
        });
        self
    }

    pub fn source_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    pub fn build(self) -> Model {
        Model {
            name: self.name,
            attributes: self.attributes,
            methods: self.methods,
            data_sources: self.data_sources,
            source_path: self.source_path.unwrap_or_default(),
        }
    }
}
