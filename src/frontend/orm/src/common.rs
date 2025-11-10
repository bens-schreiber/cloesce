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
    sql: String,
    db: sqlx::SqlitePool,
) -> Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> {
    use d1::D1Generator;

    let migration_ast = {
        use ast::{CloesceAst, MigrationsAst, MigrationsModel, builder::create_ast};

        let mut ast = create_ast(models.drain().map(|(_, m)| m).collect::<Vec<_>>());
        ast.semantic_analysis().unwrap();

        let CloesceAst { hash, models, .. } = ast;

        // Convert each full Model -> MigrationsModel
        let migrations_models = models
            .into_iter()
            .map(|(name, model)| {
                let m = MigrationsModel {
                    hash: model.hash,
                    name: model.name,
                    primary_key: model.primary_key,
                    attributes: model.attributes,
                    navigation_properties: model.navigation_properties,
                    data_sources: model.data_sources,
                };
                (name, m)
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

    sqlx::query(&sql).execute(&db).await
}
