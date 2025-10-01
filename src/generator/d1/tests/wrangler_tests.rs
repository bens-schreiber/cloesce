use common::{CloesceAst, wrangler::WranglerFormat};
use d1::D1Generator;

use anyhow::Result;
use insta::assert_snapshot;

use std::path::PathBuf;

#[test]
fn test_serialize_wrangler_spec() {
    // Filled TOML
    {
        let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.toml");
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
        let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.json");
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
fn test_generate_empty_wrangler_snap() -> Result<()> {
    // Arrange
    let ast = {
        let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CloesceAst>(&cidl_contents)?
    };

    let wrangler = WranglerFormat::Toml(toml::from_str("").unwrap());

    let d1gen = D1Generator::new(ast, wrangler.as_spec()?);

    // Act
    let updated_wrangler = d1gen.wrangler();

    // Assert
    assert_snapshot!(
        "updated_wrangler_from_empty",
        format!("{}", toml::to_string_pretty(&updated_wrangler).unwrap())
    );

    Ok(())
}
