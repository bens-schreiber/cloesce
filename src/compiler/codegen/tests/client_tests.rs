use codegen::client::ClientGenerator;
use compiler_test::src_to_ast;

mod common;

#[test]
fn client_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let ast = src_to_ast(common::COMPREHENSIVE_SRC);

    let client_code = ClientGenerator::generate(&ast, WORKERS_URL);
    insta::assert_snapshot!(client_code);
}
