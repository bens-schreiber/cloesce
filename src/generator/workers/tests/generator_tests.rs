use std::path::PathBuf;

use anyhow::{Context, Result};
use common::{CloesceAst, wrangler::WranglerFormat};
use insta::assert_snapshot;
use workers::WorkersGenerator;

#[test]
fn test_generate_sql_from_cidl() -> Result<()> {
    // Arrange
    let ast = {
        let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CloesceAst>(&cidl_contents)?
    };

    let wrangler = {
        let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.toml");
        WranglerFormat::from_path(&wrangler_path)
            .context("Failed to open wrangler file")?
            .as_spec()?
    };

    // Act
    let res = WorkersGenerator::create(
        ast,
        wrangler,
        "http://foo.com/api".into(),
        &PathBuf::from("src/models.ts"),
    )?;

    // Assert
    assert_snapshot!("generated_workers_from_json", res);

    Ok(())
}
