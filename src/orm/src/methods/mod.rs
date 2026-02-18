pub mod map;
pub mod select;
pub mod upsert;
pub mod validate;

use std::fmt::Display;

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

#[derive(Debug)]
pub struct OrmError {
    pub kind: OrmErrorKind,
    pub context: String,
}

impl OrmError {
    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        let ctx = ctx.into();
        if self.context.is_empty() {
            self.context = ctx;
        } else {
            // Prepend new context
            self.context = format!("{ctx}: {}", self.context);
        }
        self
    }
}

impl Display for OrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Kind: {:?} Context: {} ({})",
            self.kind, self.context, self.kind as u32
        )
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u32)]
pub enum OrmErrorKind {
    UnknownModel,
    ModelMissingD1,
    MissingPrimaryKey,
    MissingAttribute,
    MissingKeyParameter,
    TypeMismatch,
}

impl OrmErrorKind {
    pub fn to_error(self) -> OrmError {
        let context = String::new();
        OrmError {
            kind: self,
            context,
        }
    }
}

#[macro_export]
macro_rules! fail {
    ($kind:expr) => {
        return Err($kind.to_error())
    };
    ($kind:expr, $($arg:tt)*) => {
        return Err($kind.to_error().with_context(format!($($arg)*)))
    };
}

#[macro_export]
macro_rules! ensure {
    ($cond:expr, $kind:expr) => {
        if !($cond) {
            fail!($kind)
        }
    };
    ($cond:expr, $kind:expr, $($arg:tt)*) => {
        if !($cond) {
            fail!($kind, $($arg)*)
        }
    };
}

pub type Result<T> = std::result::Result<T, OrmError>;

#[cfg(test)]
use sqlx::sqlite::SqliteRow;

#[cfg(test)]
use crate::ModelMeta;

#[cfg(test)]
pub async fn test_sql(
    mut models: ModelMeta,
    stmts: Vec<(String, Vec<serde_json::Value>)>,
    db: sqlx::SqlitePool,
) -> std::result::Result<Vec<Vec<SqliteRow>>, sqlx::Error> {
    use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};

    // Generate and run schema migration
    let migration_ast = {
        use ast::{CloesceAst, MigrationsAst, MigrationsModel};
        use generator_test::{create_ast, create_spec};
        use semantic::SemanticAnalysis;

        let mut ast = create_ast(models.drain().map(|(_, m)| m).collect::<Vec<_>>());
        let spec = create_spec(&ast);
        SemanticAnalysis::analyze(&mut ast, &spec).unwrap();

        let CloesceAst { hash, models, .. } = ast;
        let migrations_models = models
            .into_iter()
            .map(|(name, model)| {
                (
                    name,
                    MigrationsModel {
                        hash: model.hash,
                        name: model.name,
                        primary_key: model.primary_key.unwrap(),
                        columns: model.columns,
                        navigation_properties: model.navigation_properties,
                    },
                )
            })
            .collect();

        MigrationsAst {
            hash,
            models: migrations_models,
        }
    };

    struct MockMigrationsIntent;
    impl MigrationsIntent for MockMigrationsIntent {
        fn ask(&self, _: MigrationsDilemma) -> Option<usize> {
            panic!()
        }
    }

    let migration = MigrationsGenerator::migrate(&migration_ast, None, &MockMigrationsIntent);
    sqlx::query(&migration).execute(&db).await.unwrap();

    let mut tx = db.begin().await?;
    let mut results = Vec::new();
    for (sql, values) in stmts {
        let mut query = sqlx::query(&sql);
        for value in values.iter() {
            query = match value {
                serde_json::Value::Null => query.bind(None::<String>),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        query.bind(i)
                    } else if let Some(f) = n.as_f64() {
                        query.bind(f)
                    } else {
                        unimplemented!("Number type not implemented in test_sql")
                    }
                }
                serde_json::Value::String(s) => query.bind(s),
                _ => unimplemented!("Value type not implemented in test_sql"),
            };
        }
        let rows = query.fetch_all(&mut *tx).await?;
        results.push(rows);
    }
    tx.commit().await?;

    Ok(results)
}
