use anyhow::Result;
use common::CidlSpec;
use insta::assert_snapshot;
use workers::WorkersFactory;

use std::path::PathBuf;

#[test]
fn test_generate_client_snapshot() -> Result<()> {
    // Arrange
    let cidl = {
        let cidl_path = PathBuf::from("../fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CidlSpec>(&cidl_contents)?
    };

    // Act
    let workers = WorkersFactory.create(cidl);

    // Assert
    assert_snapshot!("generated_workers", workers);
    Ok(())
}
