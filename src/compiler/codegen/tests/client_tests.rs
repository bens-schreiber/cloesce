use codegen::{client::ClientGenerator, workers::WorkersGenerator};
use compiler_test::src_to_ast;

mod shared;

#[test]
fn client_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let mut ast = src_to_ast(shared::COMPREHENSIVE_SRC);
    WorkersGenerator::generate(&mut ast, WORKERS_URL);

    let client_code = ClientGenerator::generate(&ast, WORKERS_URL);
    insta::assert_snapshot!(client_code);
}
