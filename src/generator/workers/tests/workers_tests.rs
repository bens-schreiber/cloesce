use workers::WorkersGenerator;

use common::CidlSpec;

use anyhow::Result;
use insta::assert_snapshot;

use std::path::PathBuf;

#[test]
fn test_generate_workers_snapshot() -> Result<()> {
    // Arrange
    let cidl = {
        let cidl_path = PathBuf::from("../../test_fixtures/cidl.json");
        let cidl_contents = std::fs::read_to_string(cidl_path)?;
        serde_json::from_str::<CidlSpec>(&cidl_contents)?
    };

    let workers_path = PathBuf::from("root/workers.snap.new");

    // Act
    let workers = WorkersGenerator.create(
        cidl,
        String::from("http://cloesce.com/foo/api"),
        &workers_path,
    )?;

    // Assert
    assert_snapshot!("generated_workers", workers);
    Ok(())
}
