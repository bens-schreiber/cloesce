use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use indexmap::IndexMap;

use ast::{
    ApiMethod, CidlType, CloesceAst, D1Model, D1ModelAttribute, DataSource, HttpVerb, IncludeTree,
    InputLanguage, MediaType, NamedTypedValue, NavigationProperty, NavigationPropertyKind,
    WranglerEnv, WranglerSpec,
};
use wrangler::WranglerDefault;

pub fn create_ast(mut d1_models: Vec<D1Model>) -> CloesceAst {
    let map = d1_models
        .drain(..)
        .map(|m| (m.name.clone(), m))
        .collect::<IndexMap<String, D1Model>>();

    CloesceAst {
        version: "1.0".to_string(),
        project_name: "test".to_string(),
        language: InputLanguage::TypeScript,
        d1_models: map,
        kv_models: BTreeMap::default(),
        poos: IndexMap::default(),
        wrangler_env: Some(WranglerEnv {
            name: "TestEnv".to_string(),
            source_path: PathBuf::default(),
            d1_binding: Some("TEST_DB".to_string()),
            kv_bindings: Vec::default(),
            vars: HashMap::default(),
        }),
        services: IndexMap::default(),
        app_source: None,
        hash: 0,
    }
}

pub fn create_spec(ast: &CloesceAst) -> WranglerSpec {
    let mut spec = WranglerSpec::default();
    WranglerDefault::set_defaults(&mut spec, ast);
    spec
}

#[derive(Default)]
pub struct IncludeTreeBuilder {
    nodes: BTreeMap<String, IncludeTree>,
}

/// Compares two strings disregarding tabs, amount of spaces, and amount of newlines.
/// Ensures that some expr is present in another expr.
#[macro_export]
macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let clean = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert!(
            clean(&$got.to_string()).contains(&clean(&$expected.to_string())),
            "Expected:\n`{}`\n\ngot:\n`{}`",
            $expected,
            $got
        );
    }};
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

pub struct D1ModelBuilder {
    name: String,
    attributes: Vec<D1ModelAttribute>,
    navigation_properties: Vec<NavigationProperty>,
    primary_key: Option<NamedTypedValue>,
    methods: BTreeMap<String, ApiMethod>,
    data_sources: BTreeMap<String, DataSource>,
}

impl D1ModelBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            attributes: Vec::new(),
            navigation_properties: Vec::new(),
            methods: BTreeMap::new(),
            data_sources: BTreeMap::new(),
            primary_key: None,
        }
    }

    pub fn attribute(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        foreign_key: Option<String>,
    ) -> Self {
        self.attributes.push(D1ModelAttribute {
            value: NamedTypedValue {
                name: name.into(),
                cidl_type,
            },
            foreign_key_reference: foreign_key,
            hash: 0,
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
            hash: 0,
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
            ApiMethod {
                name: name.into(),
                is_static,
                http_verb,
                return_type,
                parameters,
                return_media: MediaType::default(),
                parameters_media: MediaType::default(),
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
                hash: 0,
            },
        );
        self
    }

    pub fn build(self) -> D1Model {
        D1Model {
            name: self.name,
            attributes: self.attributes,
            navigation_properties: self.navigation_properties,
            methods: self.methods,
            data_sources: self.data_sources,
            source_path: PathBuf::default(),
            primary_key: self.primary_key.unwrap(),
            cruds: vec![],
            hash: 0,
        }
    }
}
