// workers_tests.rs
use anyhow::Result;
use common::CidlSpec;
use insta::assert_snapshot;
use workers::WorkersFactory;
use std::path::PathBuf;

fn load_cidl() -> Result<CidlSpec> {
    let cidl_path = PathBuf::from("../fixtures/cidl.json");
    let cidl_contents = std::fs::read_to_string(cidl_path)?;
    Ok(serde_json::from_str::<CidlSpec>(&cidl_contents)?)
}

#[test]
fn test_generate_client_snapshot() -> Result<()> {
    // Arrange
    let cidl = load_cidl()?;
    
    // Use the default domain here
    let workers = WorkersFactory::new().create(cidl);
    
    // Assert
    assert_snapshot!("generated_workers", workers);
    Ok(())
}

#[test]
fn test_generate_client_with_custom_domain() -> Result<()> {
    // Arrange
    let cidl = load_cidl()?;
    
    // Lets use a nested domain to try and see if our changes work 
    let workers = WorkersFactory::new()
        .with_domain("https://localhost:5000/foo/bar/baz".to_string())
        .create(cidl);
    
    // Assert - router should use "baz" as root
    assert!(workers.contains("const router = { baz:"));
    
    Ok(())
}

#[test]
fn test_domain_normalization() -> Result<()> {
    // Test various domain formats
    let test_cases = vec![
        ("localhost:5000/myapi", "myapi"),
        ("http://example.com/v1/api", "api"),
        ("https://api.example.com/services", "services"),
        ("example.com", "api"), // no path, defaults to "api"
    ];
    
    for (domain, expected_root) in test_cases {
        // Arrange
        let cidl = load_cidl()?;
        
        // Act
        let workers = WorkersFactory::new()
            .with_domain(domain.to_string())
            .create(cidl);
        
        // Assert
        let expected = format!("const router = {{ {}:", expected_root);
        assert!(
            workers.contains(&expected),
            "Domain '{}' should produce root '{}', but got:\n{}",
            domain,
            expected_root,
            workers.lines()
                .find(|l| l.contains("const router"))
                .unwrap_or("router not found")
        );
    }
    
    Ok(())
}