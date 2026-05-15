use codegen::client::ClientGenerator;
use compiler_test::{COMPREHENSIVE_SRC, src_to_idl};

#[test]
fn client_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let idl = src_to_idl(COMPREHENSIVE_SRC);

    let client_code = ClientGenerator::generate(&idl, WORKERS_URL);
    insta::assert_snapshot!(client_code);
}
