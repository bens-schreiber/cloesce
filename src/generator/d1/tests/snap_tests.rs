use anyhow::{Context, Result};
use insta::assert_snapshot;

use common::{CidlSpec, wrangler::WranglerFormat};
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
