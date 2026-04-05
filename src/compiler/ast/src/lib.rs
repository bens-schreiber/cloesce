use std::borrow::Cow;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::hash::Hash;
use std::hash::Hasher;

use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Deserialize, Serialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Default)]
pub enum CidlType<'src> {
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
        #[serde(borrow)]
        name: &'src str,
    },

    /// A model, or plain old object, containing the name of the class.
    Object {
        #[serde(borrow)]
        name: &'src str,
    },

    /// A part of a model or plain object, containing the name of the class.
    ///
    /// Only valid as a method argument.
    Partial {
        #[serde(borrow)]
        object_name: &'src str,
    },

    /// A data source of some model
    DataSource {
        #[serde(borrow)]
        model_name: &'src str,
    },

    /// An array of any type
    #[serde(borrow)]
    Array(Box<CidlType<'src>>),

    /// A REST API response, which can contain any type or nothing.
    #[serde(borrow)]
    HttpResult(Box<CidlType<'src>>),

    /// A wrapper denoting the type within can be null.
    /// If the inner value is void, represents just null.
    #[serde(borrow)]
    Nullable(Box<CidlType<'src>>),

    /// A paginated response containing list metadata and a page of results.
    #[serde(borrow)]
    Paginated(Box<CidlType<'src>>),

    /// A Cloudflare Workers KV object (GET value response)
    #[serde(borrow)]
    KvObject(Box<CidlType<'src>>),

    /// A reference to an object or injected type that is not yet resolved by the parser
    UnresolvedReference {
        #[serde(borrow)]
        name: &'src str,
    },
}

impl<'src> CidlType<'src> {
    pub fn root_type(&self) -> &CidlType<'src> {
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

    pub fn array(cidl_type: CidlType<'src>) -> CidlType<'src> {
        CidlType::Array(Box::new(cidl_type))
    }

    pub fn nullable(cidl_type: CidlType<'src>) -> CidlType<'src> {
        CidlType::Nullable(Box::new(cidl_type))
    }

    pub fn null() -> CidlType<'src> {
        CidlType::Nullable(Box::new(CidlType::Void))
    }

    pub fn http(cidl_type: CidlType<'src>) -> CidlType<'src> {
        CidlType::HttpResult(Box::new(cidl_type))
    }

    pub fn paginated(cidl_type: CidlType<'src>) -> CidlType<'src> {
        CidlType::Paginated(Box::new(cidl_type))
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone, Copy)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq, Hash, Clone)]
pub struct Field<'src> {
    pub name: Cow<'src, str>,

    #[serde(borrow)]
    pub cidl_type: CidlType<'src>,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct IncludeTree<'src>(#[serde(borrow)] pub BTreeMap<Cow<'src, str>, IncludeTree<'src>>);

/// A D1 Navigation field, representing a relationship to another model
/// through a foreign key or composite foreign key.
#[derive(Deserialize, Serialize, Debug, Clone, Hash)]
pub enum NavigationFieldKind<'src> {
    OneToOne {
        /// The columns on the current model that reference the other model's primary key.
        /// Multiple columns indicate a composite foreign key.
        #[serde(borrow)]
        columns: Vec<&'src str>,
    },
    OneToMany {
        /// The columns on the other model that reference the current model's primary key.
        /// Multiple columns indicate a composite foreign key.
        #[serde(borrow)]
        columns: Vec<&'src str>,
    },

    /// A many to many relationship expressed through a join table,
    /// consisting of the two models primary keys (be they composite or not).
    ManyToMany,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct NavigationField<'src> {
    #[serde(default)]
    pub hash: u64,

    #[serde(borrow)]
    pub field: Field<'src>,

    /// Referenced model name.
    #[serde(borrow)]
    pub model_reference: &'src str,

    #[serde(borrow)]
    pub kind: NavigationFieldKind<'src>,
}

impl<'src> NavigationField<'src> {
    pub fn many_to_many_table_name(&self, parent_model_name: &'src str) -> String {
        let mut names = [parent_model_name, self.model_reference];
        names.sort();
        format!("{}{}", names[0], names[1])
    }
}

#[derive(Deserialize, Serialize, Debug, Hash)]
pub struct ForeignKeyReference<'src> {
    #[serde(borrow)]
    pub model_name: &'src str,

    #[serde(borrow)]
    pub column_name: &'src str,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Column<'src> {
    #[serde(default)]
    pub hash: u64,

    #[serde(borrow)]
    pub field: Field<'src>,

    /// If the attribute is a foreign key, the referenced model and column.
    #[serde(borrow)]
    pub foreign_key_reference: Option<ForeignKeyReference<'src>>,

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

#[derive(Deserialize, Serialize, Hash, PartialEq, Eq, Debug, Clone)]
pub enum CrudKind {
    Get,
    List,
    Save,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DataSourceMethod<'src> {
    #[serde(borrow)]
    pub parameters: Vec<Field<'src>>,

    #[serde(skip)]
    pub raw_sql: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DataSource<'src> {
    #[serde(borrow)]
    pub name: &'src str,

    #[serde(skip)]
    pub tree: IncludeTree<'src>,

    #[serde(borrow)]
    pub list: Option<DataSourceMethod<'src>>,

    #[serde(borrow)]
    pub get: Option<DataSourceMethod<'src>>,

    pub is_internal: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct KvR2Field<'src> {
    #[serde(borrow)]
    pub field: Field<'src>,

    #[serde(borrow)]
    pub format: &'src str,

    #[serde(borrow)]
    pub binding: &'src str,

    pub list_prefix: bool,
}

#[derive(Deserialize, Serialize, Debug)]
pub enum MediaType {
    Json,
    Octet,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct ApiMethod<'src> {
    #[serde(borrow)]
    pub name: &'src str,

    /// If true, the method is static (instantiated on a class, not an instance).
    /// Static methods require no hydration or data source.
    pub is_static: bool,

    pub data_source: Option<&'src str>,

    pub http_verb: HttpVerb,

    /// The media format the client should use to read the response body.
    pub return_media: MediaType,

    #[serde(borrow)]
    pub return_type: CidlType<'src>,

    /// The media format the client should use to send the request body.
    pub parameters_media: MediaType,

    #[serde(borrow)]
    pub parameters: Vec<Field<'src>>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct Model<'src> {
    #[serde(default)]
    pub hash: u64,

    #[serde(borrow)]
    pub name: &'src str,

    #[serde(borrow)]
    pub d1_binding: Option<&'src str>,

    #[serde(borrow)]
    pub primary_columns: Vec<Column<'src>>,

    #[serde(borrow)]
    pub columns: Vec<Column<'src>>,

    #[serde(borrow)]
    pub kv_fields: Vec<KvR2Field<'src>>,

    #[serde(borrow)]
    pub r2_fields: Vec<KvR2Field<'src>>,

    #[serde(borrow)]
    pub navigation_fields: Vec<NavigationField<'src>>,

    #[serde(borrow)]
    pub key_fields: Vec<&'src str>,

    #[serde(borrow)]
    pub apis: Vec<ApiMethod<'src>>,

    #[serde(borrow)]
    pub data_sources: BTreeMap<&'src str, DataSource<'src>>,

    pub cruds: Vec<CrudKind>,
}

impl Model<'_> {
    pub fn has_d1(&self) -> bool {
        self.d1_binding.is_some()
    }

    pub fn has_kv(&self) -> bool {
        !self.kv_fields.is_empty()
    }

    pub fn has_r2(&self) -> bool {
        !self.r2_fields.is_empty()
    }

    /// Returns the data source with name "Default"
    pub fn default_data_source(&self) -> Option<&DataSource<'_>> {
        self.data_sources.get("Default")
    }

    pub fn has_composite_pk(&self) -> bool {
        self.primary_columns.len() > 1
    }

    /// Returns all columns, including primary key columns, as a single list.
    /// The boolean indicates whether the column is a primary key column.
    pub fn all_columns(&self) -> impl Iterator<Item = (&Column<'_>, bool)> {
        self.columns
            .iter()
            .map(|c| (c, false))
            .chain(self.primary_columns.iter().map(|c| (c, true)))
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Service<'src> {
    #[serde(borrow)]
    pub name: &'src str,

    #[serde(borrow)]
    pub fields: Vec<Field<'src>>,

    #[serde(borrow)]
    pub apis: Vec<ApiMethod<'src>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct PlainOldObject<'src> {
    #[serde(borrow)]
    pub name: &'src str,

    #[serde(borrow)]
    pub fields: Vec<Field<'src>>,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct WranglerEnv<'src> {
    #[serde(borrow)]
    pub d1_bindings: Vec<&'src str>,

    #[serde(borrow)]
    pub kv_bindings: Vec<&'src str>,

    #[serde(borrow)]
    pub r2_bindings: Vec<&'src str>,

    #[serde(borrow)]
    pub vars: Vec<Field<'src>>,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct CloesceAst<'src> {
    #[serde(default)]
    pub hash: u64,

    #[serde(borrow)]
    pub wrangler_env: Option<WranglerEnv<'src>>,

    #[serde(borrow)]
    pub models: IndexMap<&'src str, Model<'src>>,

    #[serde(borrow)]
    pub services: IndexMap<&'src str, Service<'src>>,

    #[serde(borrow)]
    pub poos: BTreeMap<&'src str, PlainOldObject<'src>>,

    #[serde(borrow)]
    pub injects: Vec<&'src str>,
}

impl CloesceAst<'_> {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }

    /// Traverses the AST setting the `hash` field as a merkle hash (a parents hash depends on it's childrens hashes)
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

/// A subset of [Model] suited for migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsModel<'src> {
    pub hash: u64,
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub d1_binding: Option<String>,

    #[serde(borrow)]
    pub primary_columns: Vec<Column<'src>>,

    #[serde(borrow)]
    pub columns: Vec<Column<'src>>,

    #[serde(borrow)]
    pub navigation_fields: Vec<NavigationField<'src>>,
}

impl<'src> MigrationsModel<'src> {
    /// Returns all columns, including primary key columns, as a single iterator.
    /// The boolean indicates whether the column is a primary key column.
    pub fn all_columns(&self) -> impl Iterator<Item = (&Column<'src>, bool)> {
        self.columns
            .iter()
            .map(|c| (c, false))
            .chain(self.primary_columns.iter().map(|c| (c, true)))
    }
}

/// A subset of [CloesceAst] suited for D1 migrations.
///
/// Assumed that the tree is semantically valid.
#[derive(Serialize, Deserialize)]
pub struct MigrationsAst<'src> {
    pub hash: u64,

    #[serde(borrow)]
    pub models: IndexMap<String, MigrationsModel<'src>>,
}

impl<'src> MigrationsAst<'src> {
    pub fn from_json(json: &'src str) -> std::result::Result<Self, String> {
        serde_json::from_str::<Self>(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("serialize self to work")
    }
}
