use codegen::{backend::BackendGenerator, workers::WorkersGenerator};
use compiler_test::src_to_ast;

mod shared;

#[test]
fn backend_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let mut ast = src_to_ast(shared::COMPREHENSIVE_SRC);
    WorkersGenerator::generate(&mut ast, WORKERS_URL);

    let backend_code = BackendGenerator::generate(&ast);
    insta::assert_snapshot!(backend_code);
}
