pub mod err;

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[macro_export]
macro_rules! cidl_type_contains {
    ($value:expr, $pattern:pat) => {{
        let mut cur = $value;

        loop {
            match cur {
                $pattern => break true,

                CidlType::Array(inner)
                | CidlType::Nullable(inner)
                | CidlType::HttpResult(inner)
                | CidlType::Paginated(inner) => {
                    cur = inner;
                }

                _ => break false,
            }
        }
    }};
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CidlType {
    Void,
    Integer,
    Double,
    String,
    Blob,
    Boolean,

    /// An ISO Date string
    DateIso,

    /// A Binary Large Object that is not to be buffered in memory,
    /// but rather piped to some destination.
    Stream,

    /// Any valid JSON value
    Json,

    /// A Cloudflare R2 object (HEAD object response)
    R2Object,

    /// A dependency injected instance, containing a type name.
    Inject(String),

    /// A model, or plain old object, containing the name of the class.
    Object(String),

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial(String),

    /// A data source of some model
    DataSource(String),

    /// An array of any type
    Array(Box<CidlType>),

    /// A REST API response, which can contain any type or nothing.
    HttpResult(Box<CidlType>),

    /// A wrapper denoting the type within can be null.
    /// If the inner value is void, represents just null.
    Nullable(Box<CidlType>),

    /// A paginated response containing list metadata and a page of results.
    Paginated(Box<CidlType>),

    /// A Cloudflare Workers KV object (GET value response)
    KvObject(Box<CidlType>),
}

impl CidlType {
    pub fn root_type(&self) -> &CidlType {
        match self {
            CidlType::Array(inner) => inner.root_type(),
            CidlType::HttpResult(inner) => inner.root_type(),
            CidlType::Nullable(inner) => inner.root_type(),
            CidlType::KvObject(inner) => inner.root_type(),
            CidlType::Paginated(inner) => inner.root_type(),
            t => t,
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

    pub fn http(cidl_type: CidlType) -> CidlType {
        CidlType::HttpResult(Box::new(cidl_type))
    }

    pub fn paginated(cidl_type: CidlType) -> CidlType {
        CidlType::Paginated(Box::new(cidl_type))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
pub struct Field {
    pub symbol: Symbol,
    pub name: String,
    pub cidl_type: CidlType,
}

/// The expected media type for request/response bodies.
/// An API endpoint may expect data in some format, and return data in some format.
/// Defaults to JSON.
#[derive(Serialize, Deserialize, Debug, Default)]
pub enum MediaType {
    #[default]
    Json,

    Octet,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Api {
    pub symbol: Symbol,
    pub model_symbol: Symbol,
    pub cruds: Vec<CrudKind>,
    pub methods: Vec<ApiMethod>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApiMethod {
    pub symbol: Symbol,
    pub name: String,

    /// If true, the method is static (instantiated on a class, not an instance).
    /// Static methods require no hydration or data source.
    pub is_static: bool,
    pub data_source: Option<Symbol>,

    pub http_verb: HttpVerb,

    /// The media format the client should use to read the response body.
    #[serde(default)]
    pub return_media: MediaType,
    pub return_type: CidlType,

    /// The media format the client should use to send the request body.
    #[serde(default)]
    pub parameters_media: MediaType,
    pub parameters: Vec<Field>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

#[derive(Serialize, Deserialize, Debug, Clone, Hash, PartialEq, Eq)]
pub enum CrudListParam {
    LastSeen,
    Limit,
    Offset,
}

/// A tree of model symbol names to include when hydrating a data source.
#[derive(Serialize, Deserialize, Debug)]
pub struct DataSource {
    pub name: String,
    pub tree: IncludeTree,

    /// If true, the data source will not be generated on the client.
    pub is_private: bool,

    /// List pagination parameter names for the LIST method
    pub list_params: Vec<CrudListParam>,
}

/// A D1 Navigation property, representing a relationship to another model
/// through a foreign key or composite foreign key.
#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum D1NavigationPropertyKind {
    OneToOne {
        /// The columns on the current model that reference the other model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<Symbol>,
    },
    OneToMany {
        /// The columns on the other model that reference the current model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<Symbol>,
    },

    /// A many to many relationship expressed through a join table,
    /// consisting of the two models primary keys (be they composite or not).
    ManyToMany { column: Symbol },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct D1NavigationProperty {
    pub hash: u64,

    /// The field on the current model that represents the relationship
    pub field: Symbol,

    /// The model that this this navigation property points to
    pub to_model: Symbol,

    /// The kind of navigation property, which encodes the relationship and foreign key structure.
    pub kind: D1NavigationPropertyKind,
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct ForeignKey {
    pub hash: u64,
    pub to_model: Symbol,
    pub columns: Vec<Symbol>,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug)]
pub enum CrudKind {
    GET,
    LIST,
    SAVE,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KvNavigationProperty {
    pub namespace_binding: Symbol,
    pub field: Field,
    pub format: String,

    /// If true, treat the key as a prefix for listing multiple keys.
    pub list_prefix: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct R2NavigationProperty {
    pub name: String,
    pub symbol: Symbol,
    pub format: String,

    pub bucket_binding: Symbol,

    /// If true, treat the key as a prefix for listing multiple keys.
    pub list_prefix: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Binding {
    pub symbol: Symbol,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Model {
    pub hash: u64,
    pub symbol: Symbol,
    pub name: String,
    pub d1_binding: Option<Binding>,
    pub columns: Vec<Field>,
    pub primary_key_columns: Vec<Symbol>,
    pub navigation_properties: Vec<D1NavigationProperty>,
    pub foreign_keys: Vec<ForeignKey>,

    /// Each inner Vec represents a unique constraint, containing the column names that make up the constraint.
    pub unique_constraints: Vec<Vec<Symbol>>,

    pub key_params: Vec<Symbol>,
    pub kv_navigation_properties: Vec<KvNavigationProperty>,
    pub r2_navigation_properties: Vec<R2NavigationProperty>,
}

impl Model {
    pub fn has_d1(&self) -> bool {
        self.d1_binding.is_some()
    }

    pub fn has_kv(&self) -> bool {
        !self.kv_navigation_properties.is_empty()
    }

    pub fn has_r2(&self) -> bool {
        !self.r2_navigation_properties.is_empty()
    }

    pub fn has_composite_pk(&self) -> bool {
        self.primary_key_columns.len() > 1
    }

    pub fn primary_keys(&self) -> impl Iterator<Item = &Field> {
        self.columns
            .iter()
            .filter(move |col| self.primary_key_columns.contains(&col.symbol))
    }

    pub fn key_params(&self) -> impl Iterator<Item = &Field> {
        self.columns
            .iter()
            .filter(move |col| self.key_params.contains(&col.symbol))
    }

    pub fn kv_objects(&self) -> impl Iterator<Item = &Field> {
        self.kv_navigation_properties.iter().map(|kv| &kv.field)
    }

    pub fn r2_objects(&self) -> impl Iterator<Item = &Field> {
        self.r2_navigation_properties.iter().filter_map(|r2| {
            self.columns
                .iter()
                .find(|col| col.symbol == r2.symbol)
                .or_else(|| self.columns.iter().find(|col| col.name == r2.name))
        })
    }

    pub fn navigation_properties(
        &self,
    ) -> impl Iterator<Item = (&D1NavigationProperty, Vec<&Field>)> {
        self.navigation_properties.iter().map(|nav| {
            let key_fields = match &nav.kind {
                D1NavigationPropertyKind::OneToOne { columns }
                | D1NavigationPropertyKind::OneToMany { columns } => self
                    .columns
                    .iter()
                    .filter(|col| columns.contains(&col.symbol))
                    .collect(),

                D1NavigationPropertyKind::ManyToMany { column } => self
                    .columns
                    .iter()
                    .filter(|col| &col.symbol == column)
                    .collect(),
            };
            (nav, key_fields)
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServiceAttribute {
    /// Symbol name of the class field.
    pub var_name: String,

    /// Symbol of the injected class.
    pub inject_reference: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    /// The symbol that defines the service in the source code.
    pub name: String,

    /// Class fields which are all injected dependencies.
    pub attributes: Vec<ServiceAttribute>,

    /// Injected symbols required to initialize the service.
    pub initializer: Option<Vec<String>>,

    /// API definitions.
    pub methods: BTreeMap<Symbol, ApiMethod>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlainOldObject {
    /// The symbol that defines the POO in the source code.
    pub symbol: Symbol,

    /// The name of the POO.
    pub name: String,

    /// Class fields of any serializable type.
    pub attributes: Vec<Field>,

    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub struct WranglerEnv {
    pub symbol: Symbol,
    pub d1_bindings: Vec<Binding>,
    pub kv_bindings: Vec<Binding>,
    pub r2_bindings: Vec<Binding>,
    pub vars: HashMap<Symbol, Field>,
}

/// A unique symbol for a model, service, or plain old object.
#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Symbol(pub u32);

#[derive(Serialize, Deserialize, Default)]
pub struct CloesceAst {
    pub hash: u64,
    pub project_name: String,
    pub wrangler_env: Vec<WranglerEnv>,

    /// Maps a model symbol to the model definition
    pub models: IndexMap<Symbol, Model>,

    /// Maps an API block symbol (`api::<model_name>`) to an API
    pub apis: IndexMap<Symbol, Api>,

    /// Maps a Model symbol to all of its data sources
    pub sources: IndexMap<Symbol, Vec<DataSource>>,

    /// Maps a service symbol to the service definition
    pub services: IndexMap<Symbol, Service>,
    pub poos: BTreeMap<Symbol, PlainOldObject>,
}

impl CloesceAst {
    // pub fn from_json(path: &std::path::Path) -> Result<Self> {
    //     let cidl_contents = std::fs::read_to_string(path).map_err(|e| {
    //         GeneratorErrorKind::InvalidInputFile
    //             .to_error()
    //             .with_context(e.to_string())
    //     })?;
    //     serde_json::from_str::<Self>(&cidl_contents).map_err(|e| {
    //         GeneratorErrorKind::InvalidInputFile
    //             .to_error()
    //             .with_context(e.to_string())
    //     })
    // }

    // pub fn to_json(&self) -> String {
    //     serde_json::to_string_pretty(self).expect("serialize self to work")
    // }

    // pub fn to_migrations_json(self) -> String {
    //     let Self { hash, models, .. } = self;

    //     let migrations_models: IndexMap<String, MigrationsModel> = models
    //         .into_iter()
    //         .filter_map(|(name, model)| {
    //             if !model.has_d1() {
    //                 return None;
    //             }

    //             let m = MigrationsModel {
    //                 hash: model.hash,
    //                 name: model.name,
    //                 d1_binding: model.d1_binding,
    //                 primary_key_columns: model.primary_key_columns,
    //                 columns: model.columns,
    //                 navigation_properties: model.navigation_properties,
    //             };
    //             Some((name, m))
    //         })
    //         .collect();

    //     let migrations_ast = MigrationsAst {
    //         hash,
    //         models: migrations_models,
    //     };

    //     serde_json::to_string_pretty(&migrations_ast).expect("serialize migrations ast to work")
    // }

    // /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    // pub fn set_merkle_hash(&mut self) {
    //     if self.hash != 0u64 {
    //         // If the root is hashed, it's safe to assume all children are hashed.
    //         // No work to be done.
    //         return;
    //     }

    //     let mut root_h = FxHasher::default();
    //     for model in self.models.values_mut() {
    //         let mut model_h = FxHasher::default();
    //         model_h.write(b"Model");
    //         model.name.hash(&mut model_h);
    //         model.d1_binding.hash(&mut model_h);

    //         for pk_col in model.primary_key_columns.iter_mut() {
    //             let pk_col_h = {
    //                 let mut h = FxHasher::default();
    //                 h.write(b"ModelPrimaryKeyColumn");
    //                 pk_col.value.hash(&mut h);
    //                 pk_col.foreign_key_reference.hash(&mut h);
    //                 pk_col.unique_ids.hash(&mut h);
    //                 h.finish()
    //             };

    //             pk_col.hash = pk_col_h;
    //             model_h.write_u64(pk_col_h);
    //         }

    //         for col in model.columns.iter_mut() {
    //             let col_h = {
    //                 let mut h = FxHasher::default();
    //                 h.write(b"ModelColumn");
    //                 col.value.hash(&mut h);
    //                 col.foreign_key_reference.hash(&mut h);
    //                 col.unique_ids.hash(&mut h);
    //                 h.finish()
    //             };

    //             col.hash = col_h;
    //             model_h.write_u64(col_h);
    //         }

    //         for nav in model.navigation_properties.iter_mut() {
    //             let nav_h = {
    //                 let mut h = FxHasher::default();
    //                 h.write(b"ModelNavigationProperty");
    //                 nav.model_reference.hash(&mut h);
    //                 nav.field_name.hash(&mut h);
    //                 nav.kind.hash(&mut h);
    //                 h.finish()
    //             };

    //             nav.hash = nav_h;
    //             model_h.write_u64(nav_h);
    //         }

    //         let model_h_finished = model_h.finish();
    //         model.hash = model_h_finished;
    //         root_h.write_u64(model_h_finished);
    //     }

    //     self.hash = root_h.finish();
    // }
}

/// A subset of [Model] suited for migrations.
///
/// Assumed that the tree is semantically valid.
// #[derive(Serialize, Deserialize)]
// pub struct MigrationsModel {
//     pub hash: u64,
//     pub name: String,

//     #[serde(skip_serializing_if = "Option::is_none")]
//     pub d1_binding: Option<String>,

//     pub primary_key_columns: Vec<D1Column>,
//     pub columns: Vec<D1Column>,
//     pub navigation_properties: Vec<NavigationProperty>,
// }

// impl MigrationsModel {
//     pub fn all_columns(&self) -> impl Iterator<Item = (&D1Column, bool)> {
//         self.columns
//             .iter()
//             .map(|c| (c, false))
//             .chain(self.primary_key_columns.iter().map(|c| (c, true)))
//     }
// }

/// A subset of [CloesceAst] suited for D1 migrations.
///
/// Assumed that the tree is semantically valid.
// #[derive(Serialize, Deserialize)]
// pub struct MigrationsAst {
//     pub hash: u64,

//     #[serde(deserialize_with = "skip_if_not_d1")]
//     pub models: IndexMap<String, MigrationsModel>,
// }

// impl MigrationsAst {
//     pub fn from_json(path: &std::path::Path) -> Result<Self> {
//         let contents = std::fs::read_to_string(path).map_err(|e| {
//             GeneratorErrorKind::InvalidInputFile
//                 .to_error()
//                 .with_context(e.to_string())
//         })?;
//         serde_json::from_str::<Self>(&contents).map_err(|e| {
//             GeneratorErrorKind::InvalidInputFile
//                 .to_error()
//                 .with_context(e.to_string())
//         })
//     }

//     pub fn to_json(&self) -> String {
//         serde_json::to_string_pretty(self).expect("serialize self to work")
//     }
// }

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
    pub migrations_dir: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct KVNamespace {
    pub binding: Option<String>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct R2Bucket {
    pub binding: Option<String>,
    pub bucket_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WranglerSpec {
    pub name: Option<String>,
    pub compatibility_date: Option<String>,
    pub main: Option<String>,

    #[serde(default)]
    pub d1_databases: Vec<D1Database>,

    #[serde(default)]
    pub kv_namespaces: Vec<KVNamespace>,

    #[serde(default)]
    pub r2_buckets: Vec<R2Bucket>,

    #[serde(default)]
    pub vars: HashMap<String, Value>,
}

// fn skip_if_not_d1<'de, D>(
//     deserializer: D,
// ) -> std::result::Result<IndexMap<String, MigrationsModel>, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     struct Temp {
//         hash: u64,
//         name: String,
//         d1_binding: Option<String>,
//         primary_key_columns: Vec<D1Column>,
//         columns: Vec<D1Column>,
//         navigation_properties: Vec<NavigationProperty>,
//     }

//     let temps: IndexMap<String, Temp> = Deserialize::deserialize(deserializer)?;

//     Ok(temps
//         .into_iter()
//         .filter_map(|(key, t)| {
//             (!t.columns.is_empty() || !t.primary_key_columns.is_empty()).then_some({
//                 let m = MigrationsModel {
//                     hash: t.hash,
//                     name: t.name,
//                     d1_binding: t.d1_binding,
//                     primary_key_columns: t.primary_key_columns,
//                     columns: t.columns,
//                     navigation_properties: t.navigation_properties,
//                 };
//                 (key, m)
//             })
//         })
//         .collect())
// }
