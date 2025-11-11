#[cfg(test)]
use crate::ModelMeta;

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

#[cfg(test)]
#[macro_export]
macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let got_val = &$got;
        let expected_val = &$expected;
        assert!(
            got_val.to_string().contains(&expected_val.to_string()),
            "Expected: \n`{}`, \n\ngot:\n{:?}",
            expected_val,
            got_val
        );
    }};
}

#[cfg(test)]
pub async fn test_sql(
    mut models: ModelMeta,
    stmts: Vec<(String, Vec<serde_json::Value>)>,
    db: sqlx::SqlitePool,
) -> Result<(), sqlx::Error> {
    use d1::D1Generator;

    // Generate and run schema migration
    let migration_ast = {
        use ast::{CloesceAst, MigrationsAst, MigrationsModel, builder::create_ast};

        let mut ast = create_ast(models.drain().map(|(_, m)| m).collect::<Vec<_>>());
        ast.semantic_analysis().unwrap();

        let CloesceAst { hash, models, .. } = ast;
        let migrations_models = models
            .into_iter()
            .map(|(name, model)| {
                (
                    name,
                    MigrationsModel {
                        hash: model.hash,
                        name: model.name,
                        primary_key: model.primary_key,
                        attributes: model.attributes,
                        navigation_properties: model.navigation_properties,
                        data_sources: model.data_sources,
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
    impl d1::MigrationsIntent for MockMigrationsIntent {
        fn ask(&self, _: d1::MigrationsDilemma) -> Option<usize> {
            panic!()
        }
    }

    let migration = D1Generator::migrate(&migration_ast, None, &MockMigrationsIntent);
    sqlx::query(&migration).execute(&db).await.unwrap();

    let mut tx = db.begin().await?;
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
        query.execute(&mut *tx).await?;
    }
    tx.commit().await?;

    Ok(())
}
