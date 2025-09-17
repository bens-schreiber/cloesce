use std::path::PathBuf;

use crate::{
    Attribute, CidlForeignKeyKind, CidlSpec, CidlType, DataSource, HttpVerb, IncludeTree,
    InputLanguage, Method, Model, NavigationProperty, TypedValue, WranglerSpec,
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

#[derive(Default)]
pub struct IncludeTreeBuilder {
    nodes: Vec<(TypedValue, IncludeTree)>,
}

impl IncludeTreeBuilder {
    pub fn add(mut self, name: &str, cidl_type: CidlType) -> Self {
        self.nodes.push((
            TypedValue {
                name: name.into(),
                cidl_type,
                nullable: false,
            },
            IncludeTree(vec![]),
        ));
        self
    }

    pub fn add_with_children<F>(mut self, name: &str, cidl_type: CidlType, build: F) -> Self
    where
        F: FnOnce(IncludeTreeBuilder) -> IncludeTreeBuilder,
    {
        let subtree = build(IncludeTreeBuilder::default()).build();
        self.nodes.push((
            TypedValue {
                name: name.into(),
                cidl_type,
                nullable: false,
            },
            subtree,
        ));
        self
    }

    pub fn build(self) -> IncludeTree {
        IncludeTree(self.nodes)
    }
}

/// A builder pattern for tests to create models easily
pub struct ModelBuilder {
    name: String,
    attributes: Vec<Attribute>,
    navigation_properties: Vec<NavigationProperty>,
    methods: Vec<Method>,
    data_sources: Vec<DataSource>,
    source_path: Option<PathBuf>,
}

impl ModelBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            navigation_properties: Vec::new(),
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
        foreign_key: Option<String>,
    ) -> Self {
        self.attributes.push(Attribute {
            value: TypedValue {
                name: name.into(),
                cidl_type,
                nullable,
            },
            primary_key: false,
            foreign_key,
        });
        self
    }

    pub fn nav_p(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        nullable: bool,
        foreign_key: CidlForeignKeyKind,
    ) -> Self {
        self.navigation_properties.push(NavigationProperty {
            value: TypedValue {
                name: name.into(),
                cidl_type,
                nullable,
            },
            foreign_key,
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

    pub fn method(
        mut self,
        name: impl Into<String>,
        http_verb: HttpVerb,
        is_static: bool,
        parameters: Vec<TypedValue>,
        return_type: Option<CidlType>,
    ) -> Self {
        self.methods.push(Method {
            name: name.into(),
            is_static,
            http_verb,
            return_type,
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
            navigation_properties: self.navigation_properties,
            methods: self.methods,
            data_sources: self.data_sources,
            source_path: self.source_path.unwrap_or_default(),
        }
    }
}
