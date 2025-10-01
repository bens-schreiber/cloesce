use anyhow::Result;
use common::CloesceAst;
use insta::assert_snapshot;

use std::path::PathBuf;

#[test]
fn test_generate_client_snapshot() -> Result<()> {
    // Arrange
    let ast = {
        let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CloesceAst>(&cidl_contents)?
    };

    // Act
    let client = client::generate_client_api(ast, "http://localhost:1000/api".to_string());

    // Assert
    assert_snapshot!("generated_client", client);
    Ok(())
}
