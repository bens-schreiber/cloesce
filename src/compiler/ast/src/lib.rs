use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;
use std::usize;

use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Default)]
pub enum CidlType {
    #[default]
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

    /// Cloudflare Wrangler Environment
    Env,

    /// A dependency injected instance, containing a type name.
    Inject {
        name: String,
    },

    /// A model, or plain old object, containing the name of the class.
    Object {
        name: String,
    },

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial {
        object_name: String,
    },

    /// A data source of some model
    DataSource {
        model_name: String,
    },

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

    /// A reference to an object or injected type that is not yet resolved by the parser
    UnresolvedReference {
        name: String,
    },
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct Field {
    pub name: String,
    pub cidl_type: CidlType,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct IncludeTree(pub BTreeMap<String, IncludeTree>);

/// A D1 Navigation field, representing a relationship to another model
/// through a foreign key or composite foreign key.
#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub enum NavigationFieldKind {
    OneToOne {
        /// The columns on the current model that reference the other model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<String>,
    },
    OneToMany {
        /// The columns on the other model that reference the current model's primary key.
        /// Multiple columns indicate a composite foreign key.
        columns: Vec<String>,
    },

    /// A many to many relationship expressed through a join table,
    /// consisting of the two models primary keys (be they composite or not).
    ManyToMany,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NavigationField {
    #[serde(default)]
    pub hash: u64,

    pub field: Field,

    /// Referenced model name.
    pub model_reference: String,
    pub kind: NavigationFieldKind,
}

impl NavigationField {
    pub fn many_to_many_table_name(&self, parent_model_name: &str) -> String {
        let mut names = [parent_model_name, &self.model_reference];
        names.sort();
        format!("{}{}", names[0], names[1])
    }
}

#[derive(Serialize, Deserialize, Debug, Hash)]
pub struct ForeignKeyReference {
    pub model_name: String,
    pub column_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Column {
    #[serde(default)]
    pub hash: u64,

    pub field: Field,

    /// If the attribute is a foreign key, the referenced model and column.
    pub foreign_key_reference: Option<ForeignKeyReference>,

    /// IDs of unique constraints that this column participates in.
    pub unique_ids: Vec<usize>,

    /// An ID indicating which composite key this column belongs to, if any.
    /// Columns with the same composite_id belong to the same composite key.
    ///
    /// A primary key, will not fill this slot as a composite key as it's already identified as
    /// a key by being in the primary_key_columns list. Thus, a column that makes up
    /// a primary key can be apart of a composite foreign key.
    pub composite_id: Option<usize>,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug, Clone)]
pub enum CrudKind {
    Get,
    List,
    Save,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataSourceMethod {
    pub parameters: Vec<Field>,
    pub raw_sql: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct DataSource {
    pub name: String,
    pub tree: IncludeTree,
    pub list: Option<DataSourceMethod>,
    pub get: Option<DataSourceMethod>,
    pub is_private: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct KvR2Field {
    pub field: Field,
    pub format: String,
    pub binding: String,
    pub list_prefix: bool,
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
pub struct ApiMethod {
    pub name: String,

    /// If true, the method is static (instantiated on a class, not an instance).
    /// Static methods require no hydration or data source.
    pub is_static: bool,
    pub data_source: Option<String>,

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
pub struct Model {
    #[serde(default)]
    pub hash: u64,

    pub name: String,

    pub d1_binding: Option<String>,
    pub primary_columns: Vec<Column>,
    pub columns: Vec<Column>,

    pub kv_fields: Vec<KvR2Field>,
    pub r2_fields: Vec<KvR2Field>,
    pub navigation_fields: Vec<NavigationField>,
    pub key_fields: Vec<String>,

    pub apis: Vec<ApiMethod>,
    pub data_sources: Vec<DataSource>,
    pub cruds: Vec<CrudKind>,
}

impl Model {
    pub fn has_d1(&self) -> bool {
        self.d1_binding.is_some()
    }

    pub fn has_kv(&self) -> bool {
        !self.kv_fields.is_empty()
    }

    pub fn has_r2(&self) -> bool {
        !self.r2_fields.is_empty()
    }

    /// Returns the data source with the symbol name "Default", if it exists.
    pub fn default_data_source(&self) -> Option<&DataSource> {
        self.data_sources.iter().find(|ds| ds.name == "Default")
    }

    pub fn has_composite_pk(&self) -> bool {
        self.primary_columns.len() > 1
    }

    /// Returns all columns, including primary key columns, as a single list.
    /// The boolean indicates whether the column is a primary key column.
    pub fn all_columns(&self) -> impl Iterator<Item = (&Column, bool)> {
        self.columns
            .iter()
            .map(|c| (c, false))
            .chain(self.primary_columns.iter().map(|c| (c, true)))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ServiceField {
    pub name: String,

    /// Injected symbol name
    pub inject_reference: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Service {
    pub name: String,
    pub fields: Vec<ServiceField>,
    pub apis: Vec<ApiMethod>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PlainOldObject {
    pub name: String,
    pub fields: Vec<Field>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WranglerEnv {
    pub d1_bindings: Vec<String>,
    pub kv_bindings: Vec<String>,
    pub r2_bindings: Vec<String>,
    pub vars: Vec<Field>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CloesceAst {
    #[serde(default)]
    pub hash: u64,

    pub wrangler_env: Option<WranglerEnv>,
    pub models: IndexMap<String, Model>,
    pub services: IndexMap<String, Service>,
    pub poos: BTreeMap<String, PlainOldObject>,
}

impl CloesceAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self, String> {
        let cidl_contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Self>(&cidl_contents)
            .map_err(|e| format!("failed to parse ast json: {e}"))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }

    pub fn to_migrations_json(self) -> String {
        let Self { hash, models, .. } = self;

        let migrations_models: IndexMap<String, MigrationsModel> = models
            .into_iter()
            .filter_map(|(name, model)| {
                if !model.has_d1() {
                    return None;
                }

                let m = MigrationsModel {
                    hash: model.hash,
                    name: model.name,
                    d1_binding: model.d1_binding,
                    primary_columns: model.primary_columns,
                    columns: model.columns,
                    navigation_fields: model.navigation_fields,
                };
                Some((name, m))
            })
            .collect();

        let migrations_ast = MigrationsAst {
            hash,
            models: migrations_models,
        };

        serde_json::to_string_pretty(&migrations_ast).expect("serialize migrations ast to work")
    }

    /// Traverses the AST setting the `hash` field as a merkle hash, meaning a parents hash depends on it's childrens hashes.
    pub fn set_merkle_hash(&mut self) {
        if self.hash != 0u64 {
            // If the root is hashed, it's safe to assume all children are hashed.
            // No work to be done.
            return;
        }

        let mut root_h = FxHasher::default();
        for model in self.models.values_mut() {
            let mut model_h = FxHasher::default();
            model_h.write(b"Model");
            model.name.hash(&mut model_h);
            model.d1_binding.hash(&mut model_h);

            for pk in model.primary_columns.iter_mut() {
                let pk_col_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelPrimaryKeyColumn");
                    pk.field.hash(&mut h);
                    pk.foreign_key_reference.hash(&mut h);
                    pk.unique_ids.hash(&mut h);
                    h.finish()
                };

                pk.hash = pk_col_h;
                model_h.write_u64(pk_col_h);
            }

            for col in model.columns.iter_mut() {
                let col_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelColumn");
                    col.field.hash(&mut h);
                    col.foreign_key_reference.hash(&mut h);
                    col.unique_ids.hash(&mut h);
                    h.finish()
                };

                col.hash = col_h;
                model_h.write_u64(col_h);
            }

            for nav in model.navigation_fields.iter_mut() {
                let nav_h = {
                    let mut h = FxHasher::default();
                    h.write(b"ModelNavigationProperty");
                    nav.model_reference.hash(&mut h);
                    nav.field.hash(&mut h);
                    nav.kind.hash(&mut h);
                    h.finish()
                };

                nav.hash = nav_h;
                model_h.write_u64(nav_h);
            }

            let model_h_finished = model_h.finish();
            model.hash = model_h_finished;
            root_h.write_u64(model_h_finished);
        }

        self.hash = root_h.finish();
    }
}

/// A subset of [Model] suited for migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsModel {
    pub hash: u64,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub d1_binding: Option<String>,

    pub primary_columns: Vec<Column>,
    pub columns: Vec<Column>,
    pub navigation_fields: Vec<NavigationField>,
}

impl MigrationsModel {
    pub fn all_columns(&self) -> impl Iterator<Item = (&Column, bool)> {
        self.columns
            .iter()
            .map(|c| (c, false))
            .chain(self.primary_columns.iter().map(|c| (c, true)))
    }
}

// /// A subset of [CloesceAst] suited for D1 migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsAst {
    pub hash: u64,

    #[serde(deserialize_with = "skip_if_not_d1")]
    pub models: IndexMap<String, MigrationsModel>,
}

impl MigrationsAst {
    pub fn from_json(path: &std::path::Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Self>(&contents).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }
}

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

fn skip_if_not_d1<'de, D>(
    deserializer: D,
) -> std::result::Result<IndexMap<String, MigrationsModel>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Temp {
        hash: u64,
        name: String,
        d1_binding: Option<String>,
        primary_columns: Vec<Column>,
        columns: Vec<Column>,
        navigation_fields: Vec<NavigationField>,
    }

    let temps: IndexMap<String, Temp> = Deserialize::deserialize(deserializer)?;

    Ok(temps
        .into_iter()
        .filter_map(|(key, t)| {
            (!t.columns.is_empty() || !t.primary_columns.is_empty()).then_some({
                let m = MigrationsModel {
                    hash: t.hash,
                    name: t.name,
                    d1_binding: t.d1_binding,
                    primary_columns: t.primary_columns,
                    columns: t.columns,
                    navigation_fields: t.navigation_fields,
                };
                (key, m)
            })
        })
        .collect())
}
