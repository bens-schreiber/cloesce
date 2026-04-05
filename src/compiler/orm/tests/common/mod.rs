use ast::{CloesceAst, MigrationsAst, MigrationsModel};

pub async fn test_sql(
    ast: CloesceAst<'_>,
    stmts: Vec<(String, Vec<serde_json::Value>)>,
    db: sqlx::SqlitePool,
) -> std::result::Result<Vec<Vec<sqlx::sqlite::SqliteRow>>, sqlx::Error> {
    // Generate and run schema migration
    let migration_ast = {
        let CloesceAst { models, hash, .. } = ast;
        let migrations_models = models
            .into_iter()
            .map(|(name, model)| {
                (
                    name.to_string(),
                    MigrationsModel {
                        hash: model.hash,
                        name: model.name.to_string(),
                        d1_binding: None, // Not used in test
                        primary_columns: model.primary_columns,
                        columns: model.columns,
                        navigation_fields: model.navigation_fields,
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
    impl migrations::MigrationsIntent for MockMigrationsIntent {
        fn ask(&self, _: migrations::MigrationsDilemma) -> Option<usize> {
            panic!()
        }
    }

    let migration =
        migrations::MigrationsGenerator::migrate(&migration_ast, None, &MockMigrationsIntent);
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
