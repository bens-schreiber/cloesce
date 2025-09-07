use anyhow::{Context, Result};
use common::{CidlSpec, WranglerFormat};
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
fn test_generate_d1_snapshot() -> Result<()> {
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
    assert_snapshot!("generated_sqlite", generated_sqlite);
    assert_snapshot!(
        "updated_wrangler",
        format!("{}", toml::to_string_pretty(&updated_wrangler).unwrap())
    );

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
