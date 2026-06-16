use codegen::{
    backend::BackendGenerator, client::ClientGenerator, wrangler::DurableMigrationGenerator,
};
use compiler_test::{COMPREHENSIVE_SRC, src_to_idl};

#[test]
fn backend_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let idl = src_to_idl(COMPREHENSIVE_SRC);

    let backend_code = BackendGenerator::generate(&idl, WORKERS_URL);
    insta::assert_snapshot!(backend_code);
}

#[test]
fn durable_migration_generation_snapshot() {
    const NAME: &str = "create_users_table";
    const TIMESTAMP: u64 = 1_700_000_000;
    const SQL: &str = r#"
            CREATE TABLE User (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE
            );
        "#;

    let migration_code = DurableMigrationGenerator::generate(NAME, TIMESTAMP, SQL);
    insta::assert_snapshot!(migration_code);
}

#[test]
fn client_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let idl = src_to_idl(COMPREHENSIVE_SRC);

    let client_code = ClientGenerator::generate(&idl, WORKERS_URL);
    insta::assert_snapshot!(client_code);
}
