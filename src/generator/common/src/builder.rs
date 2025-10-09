use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    CidlType, CloesceAst, DataSource, HttpVerb, IncludeTree, InputLanguage, Middleware,
    MiddlewareMethod, Model, ModelAttribute, ModelMethod, NamedTypedValue, NavigationProperty,
    NavigationPropertyKind, WranglerEnv,
};

pub fn create_ast(mut models: Vec<Model>, middleware: Option<Middleware>) -> CloesceAst {
    let map = models
        .drain(..)
        .map(|m| (m.name.clone(), m))
        .collect::<BTreeMap<String, Model>>();
    CloesceAst {
        version: "1.0".to_string(),
        project_name: "test".to_string(),
        language: InputLanguage::TypeScript,
        models: map,
        poos: BTreeMap::default(), // TODO: if tests cover PlainOldObjects, add to `create_ast`
        wrangler_env: WranglerEnv {
            name: "Env".into(),
            source_path: "source.ts".into(),
        },
        middleware,
    }
}

pub fn create_middleware(
    class_name: impl Into<String>,
    method_name: impl Into<String>,
    parameters: Vec<NamedTypedValue>,
    source_path: impl Into<PathBuf>,
) -> Middleware {
    Middleware {
        class_name: class_name.into(),
        method: MiddlewareMethod {
            name: method_name.into(),
            parameters,
        },
        source_path: source_path.into(),
    }
}

#[derive(Default)]
pub struct IncludeTreeBuilder {
    nodes: BTreeMap<String, IncludeTree>,
}

impl IncludeTreeBuilder {
    pub fn add_node(mut self, name: impl Into<String>) -> Self {
        self.nodes
            .insert(name.into(), IncludeTree(BTreeMap::default()));
        self
    }

    pub fn add_with_children<F>(mut self, name: &str, build: F) -> Self
    where
        F: FnOnce(IncludeTreeBuilder) -> IncludeTreeBuilder,
    {
        let subtree = build(IncludeTreeBuilder::default()).build();
        self.nodes.insert(name.to_string(), subtree);
        self
    }

    pub fn build(self) -> IncludeTree {
        IncludeTree(self.nodes)
    }
}

/// A builder pattern for tests to create models easily
pub struct ModelBuilder {
    name: String,
    attributes: Vec<ModelAttribute>,
    navigation_properties: Vec<NavigationProperty>,
    primary_key: Option<NamedTypedValue>,
    methods: BTreeMap<String, ModelMethod>,
    data_sources: BTreeMap<String, DataSource>,
    source_path: Option<PathBuf>,
}

impl ModelBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            navigation_properties: Vec::new(),
            methods: BTreeMap::new(),
            data_sources: BTreeMap::new(),
            source_path: None,
            primary_key: None,
        }
    }

    pub fn attribute(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        foreign_key: Option<String>,
    ) -> Self {
        self.attributes.push(ModelAttribute {
            value: NamedTypedValue {
                name: name.into(),
                cidl_type,
            },
            foreign_key_reference: foreign_key,
        });
        self
    }

    pub fn nav_p(
        mut self,
        var_name: impl Into<String>,
        model_name: impl Into<String>,
        foreign_key: NavigationPropertyKind,
    ) -> Self {
        self.navigation_properties.push(NavigationProperty {
            var_name: var_name.into(),
            model_name: model_name.into(),
            kind: foreign_key,
        });
        self
    }

    pub fn pk(mut self, name: impl Into<String>, cidl_type: CidlType) -> Self {
        self.primary_key = Some(NamedTypedValue {
            name: name.into(),
            cidl_type,
        });
        self
    }

    pub fn id(self) -> Self {
        self.pk("id", CidlType::Integer)
    }

    pub fn method(
        mut self,
        name: impl Into<String> + Clone,
        http_verb: HttpVerb,
        is_static: bool,
        parameters: Vec<NamedTypedValue>,
        return_type: CidlType,
    ) -> Self {
        self.methods.insert(
            name.clone().into(),
            ModelMethod {
                name: name.into(),
                is_static,
                http_verb,
                return_type,
                parameters,
            },
        );
        self
    }

    pub fn data_source(mut self, name: impl Into<String> + Clone, tree: IncludeTree) -> Self {
        self.data_sources.insert(
            name.clone().into(),
            DataSource {
                name: name.into(),
                tree,
            },
        );
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
            primary_key: self.primary_key.unwrap(),
        }
    }
}
