use anyhow::{Context, Result};
use insta::assert_snapshot;

use common::{
    CidlSpec, CidlType, NavigationPropertyKind, builder::ModelBuilder, wrangler::WranglerFormat,
};
use d1::D1Generator;

use std::path::PathBuf;

#[test]
fn test_generate_sql_from_cidl() -> Result<()> {
    // Arrange
    let cidl = {
        let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CidlSpec>(&cidl_contents)?
    };

    let wrangler = {
        let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.toml");
        WranglerFormat::from_path(&wrangler_path).context("Failed to open wrangler file")?
    };

    let d1gen = D1Generator::new(cidl, wrangler.as_spec()?);

    // Act
    let generated_sqlite = d1gen.sql()?;
    let updated_wrangler = d1gen.wrangler();

    // Assert
    assert_snapshot!("generated_d1_snapshot_from_json", generated_sqlite);
    assert_snapshot!(
        "updated_wrangler",
        format!("{}", toml::to_string_pretty(&updated_wrangler).unwrap())
    );

    Ok(())
}

// TODO: Remove this once extractor can do FK's
#[test]
fn test_generate_sql_from_models() -> Result<()> {
    // Arrange
    let models = vec![
        // Basic User with attributes
        ModelBuilder::new("User")
            .id()
            .attribute("name", CidlType::Text, true, None)
            .attribute("age", CidlType::Integer, false, None)
            .build(),
        // One-to-One via attribute only
        ModelBuilder::new("Profile")
            .id()
            .attribute("userId", CidlType::Integer, false, Some("User".into()))
            .build(),
        // One-to-One via attribute + nav property
        ModelBuilder::new("Passport")
            .id()
            .attribute("userId", CidlType::Integer, false, Some("User".into()))
            .nav_p(
                "user",
                CidlType::Model("User".into()),
                false,
                NavigationPropertyKind::OneToOne {
                    reference: "userId".into(),
                },
            )
            .build(),
        // One-to-Many: Person -> Dog
        ModelBuilder::new("Person")
            .id()
            .attribute("bossId", CidlType::Integer, false, Some("Boss".into()))
            .nav_p(
                "dogs",
                CidlType::Array(Box::new(CidlType::Model("Dog".into()))),
                false,
                NavigationPropertyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
        // One-to-Many: Boss -> Person
        ModelBuilder::new("Boss")
            .id()
            .nav_p(
                "people",
                CidlType::Array(Box::new(CidlType::Model("Person".into()))),
                false,
                NavigationPropertyKind::OneToMany {
                    reference: "bossId".into(),
                },
            )
            .build(),
        // Dogs belong to a Person
        ModelBuilder::new("Dog")
            .id()
            .attribute("name", CidlType::Text, false, None)
            .attribute("personId", CidlType::Integer, false, Some("Person".into()))
            .build(),
        // Cats belong to a Person
        ModelBuilder::new("Cat")
            .id()
            .attribute("breed", CidlType::Text, true, None)
            .attribute("personId", CidlType::Integer, false, Some("Person".into()))
            .build(),
        // Many-to-Many: Student <-> Course
        ModelBuilder::new("Student")
            .id()
            .nav_p(
                "courses",
                CidlType::Array(Box::new(CidlType::Model("Course".into()))),
                false,
                NavigationPropertyKind::ManyToMany {
                    unique_id: "StudentsCourses".into(),
                },
            )
            .build(),
        ModelBuilder::new("Course")
            .id()
            .nav_p(
                "students",
                CidlType::Array(Box::new(CidlType::Model("Student".into()))),
                false,
                NavigationPropertyKind::ManyToMany {
                    unique_id: "StudentsCourses".into(),
                },
            )
            .build(),
        // Nullable FK: optional Car -> Garage
        ModelBuilder::new("Garage").id().build(),
        ModelBuilder::new("Car")
            .id()
            .attribute("garageId", CidlType::Integer, true, Some("Garage".into()))
            .build(),
        // Multi-FK model: Order -> User + Product
        ModelBuilder::new("Product")
            .id()
            .attribute("name", CidlType::Text, false, None)
            .build(),
        ModelBuilder::new("Order")
            .id()
            .attribute("userId", CidlType::Integer, false, Some("User".into()))
            .attribute(
                "productId",
                CidlType::Integer,
                false,
                Some("Product".into()),
            )
            .build(),
    ];

    let cidl = common::builder::create_cidl(models);
    let wrangler = common::builder::create_wrangler();
    let d1gen = D1Generator::new(cidl, wrangler);

    // Act
    let generated_sqlite = d1gen.sql()?;

    // Assert
    assert_snapshot!("generate_d1_snapshot_from_models", generated_sqlite);
    Ok(())
}
