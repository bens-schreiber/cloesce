use codegen::backend::BackendGenerator;
use compiler_test::src_to_ast;

mod shared;

#[test]
fn backend_code_generation_snapshot() {
    let ast = src_to_ast(shared::COMPREHENSIVE_SRC);

    let backend_code = BackendGenerator::generate(&ast);
    insta::assert_snapshot!(backend_code);
}
