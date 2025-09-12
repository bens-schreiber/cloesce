use anyhow::{Context, Result};
use common::{
    CidlForeignKeyKind, CidlSpec, CidlType, InputLanguage, WranglerFormat, WranglerSpec,
    builder::ModelBuilder,
};
use d1::D1Generator;
use insta::assert_snapshot;

use std::path::PathBuf;

#[test]
fn test_serialize_wrangler_spec() {
    // Filled TOML
    {
        let wrangler_path = PathBuf::from("../fixtures/wrangler.toml");
        WranglerFormat::from_path(&wrangler_path).expect("Wrangler file to serialize");
    }

    // Empty TOML
    {
        WranglerFormat::Toml(toml::from_str("").unwrap())
            .as_spec()
            .expect("Wrangler file to serialize");
    }

    // Filled JSON
    {
        let wrangler_path = PathBuf::from("../fixtures/wrangler.json");
        WranglerFormat::from_path(&wrangler_path).expect("Wrangler file to serialize");
    }

    // Empty JSON
    {
        WranglerFormat::Json(serde_json::from_str("{}").unwrap())
            .as_spec()
            .expect("Wrangler file to serialize");
    }
}

#[test]
fn test_generate_d1_snapshot_from_json() -> Result<()> {
    // Arrange
    let cidl = {
        let cidl_path = PathBuf::from("../fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CidlSpec>(&cidl_contents)?
    };

    let wrangler = {
        let wrangler_path = PathBuf::from("../fixtures/wrangler.toml");
        WranglerFormat::from_path(&wrangler_path).context("Failed to open wrangler file")?
    };

    let d1gen = D1Generator::new(cidl, wrangler.as_spec()?);

    // Act
    let generated_sqlite = d1gen.sqlite()?;
    let updated_wrangler = d1gen.wrangler();

    // Assert
    assert_snapshot!("generated_d1_snapshot_from_json", generated_sqlite);
    assert_snapshot!(
        "updated_wrangler",
        format!("{}", toml::to_string_pretty(&updated_wrangler).unwrap())
    );

    Ok(())
}

// TODO: Can remove this once the CIDL is capable of supporting FK's
#[test]
fn test_generate_d1_snapshot_from_models() -> Result<()> {
    // Arrange
    let (cidl, wrangler) = {
        // Shoutout CHATGPT heres a huge list of crap
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
                    CidlForeignKeyKind::OneToOne("userId".into()),
                )
                .build(),
            // One-to-Many: Person -> Dog
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".into()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
            // One-to-Many: Boss -> Person
            ModelBuilder::new("Boss")
                .id()
                .nav_p(
                    "people",
                    CidlType::Array(Box::new(CidlType::Model("Person".into()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
            // Dogs belong to a Person
            ModelBuilder::new("Dog")
                .id()
                .attribute("name", CidlType::Text, false, None)
                .build(),
            // Cats belong to a Person
            ModelBuilder::new("Cat")
                .id()
                .attribute("breed", CidlType::Text, true, None)
                .build(),
            // Many-to-Many: Student <-> Course
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    CidlType::Array(Box::new(CidlType::Model("Course".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany,
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    CidlType::Array(Box::new(CidlType::Model("Student".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany,
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

        let wrangler = WranglerSpec {
            d1_databases: vec![],
        };

        let cidl = CidlSpec {
            version: "0.0.1".to_string(),
            project_name: "People who own dogs, dogs have treats and collars".to_string(),
            language: InputLanguage::TypeScript,
            models,
        };

        (cidl, wrangler)
    };

    let d1gen = D1Generator::new(cidl, wrangler);

    // Act
    let generated_sqlite = d1gen.sqlite()?;

    // Assert
    assert_snapshot!("generate_d1_snapshot_from_models", generated_sqlite);
    Ok(())
}

#[test]
fn test_generate_d1_from_empty_wrangler_snapshot() -> Result<()> {
    // Arrange
    let cidl = {
        let cidl_path = PathBuf::from("../fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CidlSpec>(&cidl_contents)?
    };

    let wrangler = WranglerFormat::Toml(toml::from_str("").unwrap());

    let d1gen = D1Generator::new(cidl, wrangler.as_spec()?);

    // Act
    let updated_wrangler = d1gen.wrangler();

    // Assert
    assert_snapshot!(
        "updated_wrangler_from_empty",
        format!("{}", toml::to_string_pretty(&updated_wrangler).unwrap())
    );

    Ok(())
}
