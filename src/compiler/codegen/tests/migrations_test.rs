use std::collections::HashMap;

use ast::{CloesceAst, MigrationsAst, MigrationsModel};

use codegen::migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};
use compiler_test::{expected_str, src_to_ast};

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

fn as_migration(ast: CloesceAst) -> MigrationsAst {
    let CloesceAst { hash, models, .. } = ast;

    // Convert each full Model → MigrationsModel
    let migrations_models: IndexMap<String, MigrationsModel> = models
        .into_iter()
        .map(|(name, model)| {
            let m = MigrationsModel {
                hash: model.hash,
                name: model.name,
                d1_binding: Some("db".into()),
                primary_columns: model.primary_columns,
                columns: model.columns,
                navigation_fields: model.navigation_fields,
            };
            (name, m)
        })
        .collect();

    MigrationsAst {
        hash,
        models: migrations_models,
    }
}

fn empty_migration() -> MigrationsAst {
    let mut empty_ast = CloesceAst::default();
    empty_ast.set_merkle_hash();
    as_migration(empty_ast)
}

fn src_to_migration(src: &str) -> MigrationsAst {
    let mut ast = src_to_ast(src);
    ast.set_merkle_hash();
    as_migration(ast)
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
            } => ((model_name.clone(), None), options),
            MigrationsDilemma::RenameOrDropColumn {
                model_name,
                options,
                column_name: attribute_name,
            } => ((model_name.clone(), Some(attribute_name.clone())), options),
        };

        let ans = self.answers.get(&key).unwrap().clone();
        ans.map(|a| opts.iter().enumerate().find(|(_, o)| ***o == a).unwrap().0)
    }
}

#[sqlx::test]
async fn migrate_models_scalars(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model User {
                [primary id]
                id: int

                name: Option<string>
                age: int
                address: string
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "CREATE TABLE IF NOT EXISTS");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
        expected_str!(sql, "\"address\" text NOT NULL");

        query(&db, &sql).await.expect("Insert table query to work");
        assert!(exists_in_db(&db, "User").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "DROP TABLE IF EXISTS \"User\"");

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "User").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_one(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model Person {
                [primary id]
                id: int

                [foreign dogId -> Dog::id]
                [nav dog -> dogId]
                dogId: int
                dog: Dog
            }

            @d1(db)
            model Dog {
                [primary id]
                id: int
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop query to work");

        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_many(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model Dog {
                [primary id]
                id: int

                [foreign personId -> Person::id]
                personId: int
            }

            @d1(db)
            model Cat {
                [primary id]
                id: int

                [foreign personId -> Person::id]
                personId: int
            }

            @d1(db)
            model Person {
                [primary id]
                id: int

                [foreign bossId -> Boss::id]
                bossId: int

                [nav dogs -> Dog::personId]
                dogs: Array<Dog>

                [nav cats -> Cat::personId]
                cats: Array<Cat>
            }

            @d1(db)
            model Boss {
                [primary id]
                id: int

                [nav persons -> Person::bossId]
                persons: Array<Person>
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

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

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "Boss").await);
        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Cat").await);
    }
}

#[sqlx::test]
async fn migrate_models_many_to_many(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model Student {
                [primary id]
                id: int

                [nav courses <> Course::students]
                courses: Array<Course>
            }

            @d1(db)
            model Course {
                [primary id]
                id: int

                [nav students <> Student::courses]
                students: Array<Student>
            }
        "#,
        );

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "CourseStudent""#);
        expected_str!(sql, r#""left" integer NOT NULL"#);
        expected_str!(sql, r#""right" integer NOT NULL"#);
        expected_str!(sql, r#"PRIMARY KEY ("left", "right")"#);
        expected_str!(
            sql,
            r#"FOREIGN KEY ("left") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );
        expected_str!(
            sql,
            r#"FOREIGN KEY ("right") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );

        query(&db, &sql).await.expect("Insert tables query to work");
        assert!(exists_in_db(&db, "CourseStudent").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "StudentsCourses").await);
    }
}

#[sqlx::test]
async fn migrate_with_rebuild(db: SqlitePool) {
    let mut base_ast = {
        let ast = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model User {
                [primary id]
                id: int

                name: Option<string>
                age: int
                address: string
            }
        "#,
        );

        let sql = MigrationsGenerator::migrate(&ast, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        ast
    };

    // Changes without Rebuild
    base_ast = {
        // Arrange
        let new = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model User {
                [primary id]
                id: int

                first_name: Option<string>
                age: string
                favorite_color: string
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
            env { db: d1 }

            @d1(db)
            model User {
                [primary id]
                id: string

                first_name: Option<string>
                age: string
                favorite_color: string
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
            env { db: d1 }

            @d1(db)
            model Dog {
                [primary id]
                id: int
            }

            @d1(db)
            model User {
                [primary id]
                id: string

                first_name: Option<string>
                age: string
                favorite_color: string

                [foreign dog_id -> Dog::id]
                dog_id: int
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
        let empty_ast = empty_migration();
        let mut base_unique_ast = {
            let migration = src_to_migration(
                r#"
                env { db: d1 }

                @d1(db)
                model UniqueUser {
                    [primary id]
                    id: int

                    [unique email]
                    email: string

                    [unique first_name, last_name]
                    first_name: string
                    last_name: string

                    age: int
                }
            "#,
            );

            let sql = MigrationsGenerator::migrate(
                &migration,
                Some(&empty_ast),
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
                env { db: d1 }

                @d1(db)
                model UniqueUser {
                    [primary id]
                    id: int

                    [unique email]
                    email: string

                    [unique first_name, last_name]
                    first_name: string
                    last_name: string

                    [unique age]
                    age: int
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
                env { db: d1 }

                @d1(db)
                model UniqueUser {
                    [primary id]
                    id: int

                    [unique email]
                    email: string

                    [unique first_name, last_name]
                    first_name: string
                    last_name: string

                    age: int
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
            env { db: d1 }

            @d1(db)
            model User {
                [primary id]
                id: int

                name: Option<string>
                age: int
                address: string
            }

            @d1(db)
            model UserSettings {
                [primary id]
                id: int

                [foreign userId -> User::id]
                userId: int
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
        env { db: d1 }

        @d1(db)
        model AppUser {
            [primary id]
            id: int

            name: Option<string>
            age: int
            address: string
        }

        @d1(db)
        model UserSettings {
            [primary id]
            id: int

            [foreign userId -> AppUser::id]
            userId: int
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
async fn migrate_alter_drop_m2m(db: SqlitePool) {
    // Arrange
    let m2m_ast = {
        let migration = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model Student {
                [primary id]
                id: int

                [nav courses <> Course::students]
                courses: Array<Course>
            }

            @d1(db)
            model Course {
                [primary id]
                id: int

                [nav students <> Student::courses]
                students: Array<Student>
            }
        "#,
        );

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");
        assert!(exists_in_db(&db, "CourseStudent").await);

        migration
    };

    let no_m2m_ast = src_to_migration(
        r#"
        env { db: d1 }

        @d1(db)
        model Student {
            [primary id]
            id: int
        }

        @d1(db)
        model Course {
            [primary id]
            id: int
        }
    "#,
    );

    // Act
    let sql = MigrationsGenerator::migrate(
        &no_m2m_ast,
        Some(&m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(!exists_in_db(&db, "CourseStudent").await)
}

#[sqlx::test]
async fn migrate_alter_add_m2m(db: SqlitePool) {
    // Arrange
    let no_m2m_ast = {
        let migration = src_to_migration(
            r#"
            env { db: d1 }

            @d1(db)
            model Student {
                [primary id]
                id: int
            }

            @d1(db)
            model Course {
                [primary id]
                id: int
            }
        "#,
        );

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        migration
    };

    let m2m_ast = src_to_migration(
        r#"
        env { db: d1 }

        @d1(db)
        model Student {
            [primary id]
            id: int

            [nav courses <> Course::students]
            courses: Array<Course>
        }

        @d1(db)
        model Course {
            [primary id]
            id: int

            [nav students <> Student::courses]
            students: Array<Student>
        }
    "#,
    );

    // Act
    let sql = MigrationsGenerator::migrate(
        &m2m_ast,
        Some(&no_m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(exists_in_db(&db, "CourseStudent").await)
}

#[sqlx::test]
async fn migrate_models_composite_pk_and_fk(db: SqlitePool) {
    let empty_ast = empty_migration();

    let ast = src_to_migration(
        r#"
        env { db: d1 }

        @d1(db)
        model Parent {
            [primary orgId, userId]
            orgId: int
            userId: int

            name: string
        }

        @d1(db)
        model Child {
            [primary id]
            id: int

            [foreign (orgId, userId) -> (Parent::orgId, Parent::userId)]
            orgId: int
            userId: int
        }
    "#,
    );

    let sql =
        MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

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

#[sqlx::test]
async fn migrate_models_many_to_many_composite_pk(db: SqlitePool) {
    let empty_ast = empty_migration();

    let ast = src_to_migration(
        r#"
        env { db: d1 }

        @d1(db)
        model Student {
            [primary schoolId, studentId]
            schoolId: int
            studentId: int

            [nav courses <> Course::students]
            courses: Array<Course>
        }

        @d1(db)
        model Course {
            [primary deptId, courseId]
            deptId: int
            courseId: int

            [nav students <> Student::courses]
            students: Array<Student>
        }
    "#,
    );

    let sql =
        MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

    expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "CourseStudent""#);
    expected_str!(sql, r#""left_deptId" integer NOT NULL"#);
    expected_str!(sql, r#""left_courseId" integer NOT NULL"#);
    expected_str!(sql, r#""right_schoolId" integer NOT NULL"#);
    expected_str!(sql, r#""right_studentId" integer NOT NULL"#);
    expected_str!(
        sql,
        r#"PRIMARY KEY ("left_deptId", "left_courseId", "right_schoolId", "right_studentId")"#
    );
    expected_str!(
        sql,
        r#"FOREIGN KEY ("left_deptId", "left_courseId") REFERENCES "Course" ("deptId", "courseId") ON DELETE RESTRICT ON UPDATE CASCADE"#
    );
    expected_str!(
        sql,
        r#"FOREIGN KEY ("right_schoolId", "right_studentId") REFERENCES "Student" ("schoolId", "studentId") ON DELETE RESTRICT ON UPDATE CASCADE"#
    );

    query(&db, &sql).await.expect("Create tables query to work");
    assert!(exists_in_db(&db, "CourseStudent").await);
}
