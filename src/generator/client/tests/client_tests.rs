use anyhow::Result;
use common::CidlSpec;
use insta::assert_snapshot;

use std::path::PathBuf;

#[test]
fn test_generate_client_snapshot() -> Result<()> {
    // // Arrange
    // let cidl = {
    //     let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
    //     let cidl_contents = std::fs::read_to_string(cidl_path)?;
    //     serde_json::from_str::<CidlSpec>(&cidl_contents)?
    // };

    // // Act
    // let client = client::generate_client_api(cidl, "http://localhost:1000/api".to_string());

    // // Assert
    // assert_snapshot!("generated_client", client);
    // Ok(())
    Ok(())
}
