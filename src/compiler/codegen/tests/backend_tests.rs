use codegen::backend::BackendGenerator;
use compiler_test::src_to_ast;

mod common;

#[test]
fn backend_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let ast = src_to_ast(common::COMPREHENSIVE_SRC);

    let backend_code = BackendGenerator::generate(&ast, WORKERS_URL);
    insta::assert_snapshot!(backend_code);
}
