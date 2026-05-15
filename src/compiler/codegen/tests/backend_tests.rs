use codegen::backend::BackendGenerator;
use compiler_test::{COMPREHENSIVE_SRC, src_to_idl};

#[test]
fn backend_code_generation_snapshot() {
    const WORKERS_URL: &str = "http://example.com/path/to/api";
    let idl = src_to_idl(COMPREHENSIVE_SRC);

    let backend_code = BackendGenerator::generate(&idl, WORKERS_URL);
    insta::assert_snapshot!(backend_code);
}
