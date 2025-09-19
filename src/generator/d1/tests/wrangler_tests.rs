use common::WranglerFormat;
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
