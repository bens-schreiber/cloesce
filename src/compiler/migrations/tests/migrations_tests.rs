use std::collections::HashMap;

use idl::{CloesceIdl, ModelBacking};

use compiler_test::{expected_str, src_to_idl};
use migrations::{
    MigrationsDilemma, MigrationsGenerator, MigrationsIdl, MigrationsIntent, MigrationsModel,
};

use indexmap::IndexMap;
use sqlx::SqlitePool;

async fn exists_in_db(db: &SqlitePool, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)
         FROM sqlite_master
         WHERE type='table' AND name=?1",
    )
    .bind(name)
    .fetch_one(db)
    .await
    .expect("Failed to check object existence")
        > 0
}

async fn query(db: &SqlitePool, sql: &str) -> Result<(), sqlx::Error> {
    let tx = db.begin().await?;
    sqlx::query(sql).execute(db).await?;
    tx.commit().await?;
    Ok(())
}

fn as_migration(idl: CloesceIdl) -> MigrationsIdl {
    let CloesceIdl { hash, models, .. } = idl;

    // Convert each full Model -> MigrationsModel
    let migrations_models = models
        .into_iter()
        .map(|(name, model)| {
            let m = MigrationsModel {
                hash: model.hash,
                name: model.name.to_string(),
                backing: Some(ModelBacking {
                    kind: idl::BackingKind::D1,
                    binding: "db",
                    fields: vec![],
                }),
                primary_columns: model.primary_columns,
                columns: model.columns,
            };
            (name.to_string(), m)
        })
        .collect::<IndexMap<_, _>>();

    MigrationsIdl {
        hash,
        models: migrations_models,
    }
}

fn empty_migration() -> MigrationsIdl<'static> {
    let mut empty_idl = CloesceIdl::default();
    empty_idl.set_merkle_hash();
    as_migration(empty_idl)
}

fn src_to_migration(src: &'static str) -> MigrationsIdl<'static> {
    let mut idl = src_to_idl(src);
    idl.set_merkle_hash();
    as_migration(idl)
}

#[derive(Default)]
struct MockMigrationsIntent {
    answers: HashMap<(String, Option<String>), Option<String>>,
}

impl MigrationsIntent for MockMigrationsIntent {
    fn ask(&self, dilemma: MigrationsDilemma) -> Option<usize> {
        let (key, opts) = match &dilemma {
            MigrationsDilemma::RenameOrDropModel {
                model_name,
                options,
            } => (
                (model_name.to_string(), None),
                options.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            ),
            MigrationsDilemma::RenameOrDropColumn {
                model_name,
                options,
                column_name: attribute_name,
            } => (
                ((*model_name).to_string(), Some(attribute_name.to_string())),
                options.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            ),
        };

        let ans = self.answers.get(&key).unwrap().clone();
        ans.map(|a| opts.iter().enumerate().find(|(_, o)| **o == a).unwrap().0)
    }
}

#[sqlx::test]
async fn migrate_models_scalars(db: SqlitePool) {
    let empty_idl = empty_migration();

    // Create
    let idl = {
        // Arrange
        let idl = src_to_migration(
            r#"
            d1 { db }

            model User for db {
                primary {
                    id: int
                }

                column {
                    name: option<string>
                    age: int
                    address: string
                }
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&idl, Some(&empty_idl), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "CREATE TABLE IF NOT EXISTS");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
        expected_str!(sql, "\"address\" text NOT NULL");

        query(&db, &sql).await.expect("Insert table query to work");
        assert!(exists_in_db(&db, "User").await);

        idl
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_idl, Some(&idl), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "DROP TABLE IF EXISTS \"User\"");

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "User").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_one(db: SqlitePool) {
    let empty_idl = empty_migration();

    // Create
    let idl = {
        // Arrange
        let idl = src_to_migration(
            r#"
            d1 { db }

            model Dog for db {
                primary {
                    id: int
                }
            }

            model Person for db {
                primary {
                    id: int
                }

                foreign Dog::id {
                    dogId
                }

                one Dog::id(dogId) { dog }
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&idl, Some(&empty_idl), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);

        idl
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_idl, Some(&idl), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop query to work");

        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_many(db: SqlitePool) {
    let empty_idl = empty_migration();

    // Create
    let idl = {
        // Arrange
        let idl = src_to_migration(
            r#"
            d1 { db }

            model Boss for db {
                primary {
                    id: int
                }

                many Person::bossId(id) {
                    persons
                }
            }

            model Person for db {
                primary {
                    id: int
                }

                foreign Boss::id {
                    bossId
                }

                many Dog::personId(id) {
                    dogs
                }

                many Cat::personId(id) {
                    cats
                }
            }

            model Dog for db {
                primary {
                    id: int
                }

                foreign Person::id {
                    personId
                }
            }

            model Cat for db {
                primary {
                    id: int
                }

                foreign Person::id {
                    personId
                }
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&idl, Some(&empty_idl), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Boss" ( "id" integer PRIMARY KEY );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Person" ( "id" integer PRIMARY KEY, "bossId" integer NOT NULL, FOREIGN KEY ("bossId") REFERENCES "Boss" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Dog" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Cat" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Boss").await);
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);
        assert!(exists_in_db(&db, "Cat").await);

        idl
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_idl, Some(&idl), &MockMigrationsIntent::default());

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "Boss").await);
        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Cat").await);
    }
}

#[sqlx::test]
async fn migrate_with_rebuild(db: SqlitePool) {
    let mut base_ast = {
        let idl = src_to_migration(
            r#"
            d1 { db }

            model User for db {
                primary {
                    id: int
                }

                column {
                    name: option<string>
                    age: int
                    address: string
                }
            }
        "#,
        );

        let sql = MigrationsGenerator::migrate(&idl, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        idl
    };

    // Changes without Rebuild
    base_ast = {
        // Arrange
        let new = src_to_migration(
            r#"
            d1 { db }

            model User for db {
                primary {
                    id: int
                }

                column {
                    first_name: option<string>
                    age: string
                    favorite_color: string
                }
            }
        "#,
        );

        let mut intent = MockMigrationsIntent::default();
        intent.answers.insert(
            ("User".into(), Some("name".into())),
            Some("first_name".into()),
        );
        intent
            .answers
            .insert(("User".into(), Some("address".into())), None);

        // Act
        let sql = MigrationsGenerator::migrate(&new, Some(&base_ast), &intent);

        // Assert
        expected_str!(
            sql,
            "ALTER TABLE \"User\" RENAME COLUMN \"name\" TO \"first_name\""
        );
        expected_str!(
            sql,
            r#"
ALTER TABLE "User" DROP COLUMN "age";
ALTER TABLE "User" ADD COLUMN "age" text"#
        );
        expected_str!(sql, "ALTER TABLE \"User\" DROP COLUMN \"address\"");

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);

        new
    };

    // Rebuild: Primary Key
    base_ast = {
        // Arrange
        let new = src_to_migration(
            r#"
            d1 { db }

            model User for db {
                primary {
                    id: string
                }

                column {
                    first_name: option<string>
                    age: string
                    favorite_color: string
                }
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "User_"#);
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "User" ( "id" text PRIMARY KEY, "first_name" text, "age" text NOT NULL, "favorite_color" text NOT NULL );"#
        );
        expected_str!(
            sql,
            r#"INSERT INTO "User" ("first_name", "age", "favorite_color", "id") SELECT "first_name", "age", "favorite_color", CAST("id" AS text) FROM "User"#
        );
        expected_str!(sql, r#"DROP TABLE "User_"#);

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);

        new
    };

    // Rebuild: Foreign Key
    {
        // Arrange
        let new = src_to_migration(
            r#"
            d1 { db }

            model Dog for db {
                primary {
                    id: int
                }
            }

            model User for db {
                primary {
                    id: string
                }

                column {
                    first_name: option<string>
                    age: string
                    favorite_color: string
                }

                foreign Dog::id {
                    dog_id
                }
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "User_"#);
        expected_str!(
            sql,
            r#"INSERT INTO "User" ("first_name", "age", "favorite_color", "dog_id", "id") SELECT "first_name", "age", "favorite_color", 0, "id" FROM "User_"#
        );
        expected_str!(sql, r#"DROP TABLE "User_"#);

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);
        assert!(exists_in_db(&db, "Dog").await);
    }

    // Rebuild: Unique Constraints
    {
        let empty_idl = empty_migration();
        let mut base_unique_ast = {
            let migration = src_to_migration(
                r#"
                d1 { db }

                model UniqueUser for db {
                    primary {
                        id: int
                    }

                    column {
                        email: string
                        first_name: string
                        last_name: string
                        age: int
                    }

                    unique (email)
                    unique (first_name, last_name)
                }
            "#,
            );

            let sql = MigrationsGenerator::migrate(
                &migration,
                Some(&empty_idl),
                &MockMigrationsIntent::default(),
            );

            expected_str!(sql, r#""email" text UNIQUE NOT NULL"#);
            expected_str!(sql, r#"UNIQUE ("first_name", "last_name")"#);

            query(&db, &sql)
                .await
                .expect("Create table queries to work");
            assert!(exists_in_db(&db, "UniqueUser").await);

            migration
        };

        // Add a unique constraint => rebuild table
        {
            let with_age_unique_ast = src_to_migration(
                r#"
                d1 { db }

                model UniqueUser for db {
                    primary {
                        id: int
                    }

                    column {
                        email: string
                        first_name: string
                        last_name: string
                        age: int
                    }

                    unique (email)
                    unique (first_name, last_name)
                    unique (age)
                }
            "#,
            );

            // Act
            let sql = MigrationsGenerator::migrate(
                &with_age_unique_ast,
                Some(&base_unique_ast),
                &MockMigrationsIntent::default(),
            );

            // Assert
            expected_str!(sql, r#"ALTER TABLE "UniqueUser" RENAME TO "UniqueUser_"#);
            expected_str!(sql, r#""age" integer UNIQUE NOT NULL"#);
            expected_str!(sql, r#"DROP TABLE "UniqueUser_"#);

            query(&db, &sql).await.expect("Rebuild query to work");
            assert!(exists_in_db(&db, "UniqueUser").await);

            base_unique_ast = with_age_unique_ast;
        }

        // Drop a unique constraint => rebuild table
        {
            let without_age_unique_ast = src_to_migration(
                r#"
                d1 { db }

                model UniqueUser for db {
                    primary {
                        id: int
                    }

                    column {
                        email: string
                        first_name: string
                        last_name: string
                        age: int
                    }

                    unique (email)
                    unique (first_name, last_name)
                }
            "#,
            );

            // Act
            let sql = MigrationsGenerator::migrate(
                &without_age_unique_ast,
                Some(&base_unique_ast),
                &MockMigrationsIntent::default(),
            );

            // Assert
            expected_str!(sql, r#"ALTER TABLE "UniqueUser" RENAME TO "UniqueUser_"#);
            expected_str!(sql, r#"DROP TABLE "UniqueUser_"#);

            query(&db, &sql).await.expect("Rebuild query to work");
            assert!(exists_in_db(&db, "UniqueUser").await);
        }
    }
}

#[sqlx::test]
async fn migrate_with_rename(db: SqlitePool) {
    // Arrange
    let base_ast = {
        let migration = src_to_migration(
            r#"
            d1 { db }

            model User for db {
                primary {
                    id: int
                }

                column {
                    name: option<string>
                    age: int
                    address: string
                }
            }

            model UserSettings for db {
                primary {
                    id: int
                }

                foreign User::id {
                    userId
                }
            }
        "#,
        );

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        migration
    };

    let new = src_to_migration(
        r#"
        d1 { db }

        model AppUser for db {
            primary {
                id: int
            }

            column {
                name: option<string>
                age: int
                address: string
            }
        }

        model UserSettings for db {
            primary {
                id: int
            }

            foreign AppUser::id {
                userId
            }
        }
    "#,
    );

    let mut intent = MockMigrationsIntent::default();
    intent
        .answers
        .insert(("User".into(), None), Some("AppUser".into()));

    // Act
    let sql = MigrationsGenerator::migrate(&new, Some(&base_ast), &intent);

    // Assert
    expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "AppUser""#);
    assert!(sql.matches('\n').count() < 5); // should have less than 5 newlines (i.e., should be a single ALTER TABLE statement)

    query(&db, &sql).await.expect("Alter table queries to work");
    assert!(exists_in_db(&db, "AppUser").await);
    assert!(!exists_in_db(&db, "User").await);
}

#[sqlx::test]
async fn migrate_models_composite_pk_and_fk(db: SqlitePool) {
    let empty_idl = empty_migration();

    let idl = src_to_migration(
        r#"
        d1 { db }

        model Parent for db {
            primary {
                orgId: int
                userId: int
            }

            column {
                name: string
            }
        }

        model Child for db {
            primary {
                id: int
            }

            foreign Parent::{orgId, userId} {
                orgId
                userId
            }
        }
    "#,
    );

    let sql =
        MigrationsGenerator::migrate(&idl, Some(&empty_idl), &MockMigrationsIntent::default());

    expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "Parent""#);
    expected_str!(sql, r#"PRIMARY KEY ("orgId", "userId")"#);
    expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "Child""#);
    expected_str!(
        sql,
        r#"FOREIGN KEY ("orgId", "userId") REFERENCES "Parent" ("orgId", "userId") ON DELETE RESTRICT ON UPDATE CASCADE"#
    );

    query(&db, &sql).await.expect("Create tables query to work");
    assert!(exists_in_db(&db, "Parent").await);
    assert!(exists_in_db(&db, "Child").await);
}
