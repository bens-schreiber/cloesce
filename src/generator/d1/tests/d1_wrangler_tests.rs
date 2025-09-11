use anyhow::{Context, Result};
use common::{CidlSpec, InputLanguage, WranglerFormat, WranglerSpec, builder::ModelBuilder};
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
        let collar = ModelBuilder::new("Collar").id().build();
        let treat = ModelBuilder::new("Treat").id().build();
        let dog = ModelBuilder::new("Dog")
            .id()
            .fk(
                "Treat",
                common::CidlType::Integer,
                common::CidlForeignKeyKind::OneToOne,
                "Treat",
                false,
            )
            .fk(
                "Collar",
                common::CidlType::Integer,
                common::CidlForeignKeyKind::OneToOne,
                "Collar",
                false,
            )
            .build();
        let person = ModelBuilder::new("Person").id().build();

        let wrangler = WranglerSpec {
            d1_databases: vec![],
        };

        let cidl = CidlSpec {
            version: "0.0.1".to_string(),
            project_name: "People who own dogs, dogs have treats and collars".to_string(),
            language: InputLanguage::TypeScript,
            models: vec![collar, treat, dog, person],
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
