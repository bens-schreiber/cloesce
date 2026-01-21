use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use indexmap::IndexMap;

use ast::{
    ApiMethod, CidlType, CloesceAst, D1Column, DataSource, HttpVerb, IncludeTree, KeyValue,
    MediaType, Model, NamedTypedValue, NavigationProperty, NavigationPropertyKind, R2Object,
    WranglerEnv, WranglerSpec,
};
use wrangler::WranglerDefault;

pub fn create_ast(models: Vec<Model>) -> CloesceAst {
    let model_map = models
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect::<IndexMap<String, Model>>();

    CloesceAst {
        project_name: "test".to_string(),
        models: model_map,
        poos: BTreeMap::default(),
        services: IndexMap::default(),
        wrangler_env: Some(WranglerEnv {
            name: "TestEnv".to_string(),
            source_path: PathBuf::default(),
            d1_binding: Some("TEST_DB".to_string()),
            r2_bindings: vec!["r2_namespace".to_string()],
            kv_bindings: vec!["kv_namespace".to_string()],
            vars: HashMap::new(),
        }),
        main_source: None,
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

pub struct ModelBuilder {
    name: String,
    primary_key: Option<NamedTypedValue>,
    columns: Vec<D1Column>,
    navigation_properties: Vec<NavigationProperty>,
    key_params: Vec<String>,
    kv_objects: Vec<KeyValue>,
    r2_objects: Vec<R2Object>,
    methods: BTreeMap<String, ApiMethod>,
    data_sources: BTreeMap<String, DataSource>,
}

impl ModelBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            primary_key: None,
            columns: Vec::new(),
            navigation_properties: Vec::new(),
            key_params: Vec::new(),
            kv_objects: Vec::new(),
            r2_objects: Vec::new(),
            methods: BTreeMap::new(),
            data_sources: BTreeMap::new(),
        }
    }

    pub fn col(
        mut self,
        name: impl Into<String>,
        cidl_type: CidlType,
        foreign_key: Option<String>,
    ) -> Self {
        self.columns.push(D1Column {
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
        model_reference: impl Into<String>,
        foreign_key: NavigationPropertyKind,
    ) -> Self {
        self.navigation_properties.push(NavigationProperty {
            var_name: var_name.into(),
            model_reference: model_reference.into(),
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

    pub fn key_param(mut self, name: impl Into<String>) -> Self {
        self.key_params.push(name.into());
        self
    }

    pub fn kv_object(
        mut self,
        format: impl Into<String>,
        namespace_binding: impl Into<String>,
        name: impl Into<String>,
        list_prefix: bool,
        cidl_type: CidlType,
    ) -> Self {
        self.kv_objects.push(KeyValue {
            format: format.into(),
            namespace_binding: namespace_binding.into(),
            value: NamedTypedValue {
                name: name.into(),
                cidl_type,
            },
            list_prefix,
        });
        self
    }

    pub fn r2_object(
        mut self,
        format: impl Into<String>,
        bucket_binding: impl Into<String>,
        var_name: impl Into<String>,
        list_prefix: bool,
    ) -> Self {
        self.r2_objects.push(R2Object {
            format: format.into(),
            bucket_binding: bucket_binding.into(),
            var_name: var_name.into(),
            list_prefix,
        });
        self
    }

    pub fn id_pk(self) -> Self {
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

    pub fn build(self) -> Model {
        Model {
            name: self.name,
            primary_key: self.primary_key,
            columns: self.columns,
            navigation_properties: self.navigation_properties,
            key_params: self.key_params,
            kv_objects: self.kv_objects,
            r2_objects: self.r2_objects,
            methods: self.methods,
            data_sources: self.data_sources,
            hash: 0,
            cruds: vec![],
            source_path: PathBuf::default(),
        }
    }
}
