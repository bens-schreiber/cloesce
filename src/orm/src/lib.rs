#![allow(clippy::missing_safety_doc)]
mod methods;

use ast::CidlType;
use ast::CloesceAst;

use methods::map::map_sql;
use methods::select::SelectModel;
use methods::upsert::UpsertModel;
use methods::validate::validate_cidl_type;

use serde_json::Map;
use std::cell::RefCell;
use std::slice;
use std::str;

type IncludeTreeJson = Map<String, serde_json::Value>;
type D1Result = Vec<Map<String, serde_json::Value>>;

/// WASM memory allocation handler. A subsequent [dealloc] must be called to prevent memory leaks.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// WASM free memory handler.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, cap: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, cap);
    }
}

thread_local! {
    /// intended to be imported once at WASM initializaton
    pub static AST: RefCell<CloesceAst> = RefCell::new(CloesceAst::default());
}

/// Sets the [AST] global variable, returning 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_ast_ptr(ptr: *mut u8, cap: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, cap) };

    let parsed: CloesceAst = match serde_json::from_slice(slice) {
        Ok(val) => val,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    AST.with(|ast| {
        *ast.borrow_mut() = parsed;
    });

    0
}

static mut RETURN_PTR: *const u8 = std::ptr::null();
static mut RETURN_LEN: usize = 0;

/// User space function to get the [RETURN_LEN]
#[unsafe(no_mangle)]
pub extern "C" fn get_return_len() -> usize {
    unsafe { RETURN_LEN }
}

/// User space function to get the [RETURN_PTR]
#[unsafe(no_mangle)]
pub extern "C" fn get_return_ptr() -> *const u8 {
    unsafe { RETURN_PTR }
}

/// Creates a series of insert, update and upsert statements, finally selecting the model.
///
/// Requires a previous call to [set_ast_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn upsert_model(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // New Model
    new_model_ptr: *const u8,
    new_model_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let new_model_json =
        unsafe { str::from_utf8(slice::from_raw_parts(new_model_ptr, new_model_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let new_model = match serde_json::from_str::<Map<String, serde_json::Value>>(new_model_json) {
        Ok(new_model) => new_model,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res =
        AST.with(|ast| UpsertModel::query(model_name, &ast.borrow(), new_model, include_tree));
    match res {
        Ok(res) => {
            let bytes = serde_json::to_string(&res).unwrap().into_bytes();
            yield_result(bytes);
            0
        }
        Err(e) => {
            yield_error(e);
            1
        }
    }
}

/// Creates a series of joins to select a model.
///
/// Requires a previous call to [set_ast_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn select_model(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // From
    from_ptr: *const u8,
    from_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let from_raw = unsafe { str::from_utf8(slice::from_raw_parts(from_ptr, from_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let from = serde_json::from_str::<Option<String>>(from_raw).unwrap();
    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = AST.with(|ast| SelectModel::query(model_name, from, include_tree, &ast.borrow()));
    match res {
        Ok(res) => {
            let bytes = res.into_bytes();
            yield_result(bytes);
            0
        }
        Err(e) => {
            yield_error(e);
            1
        }
    }
}

/// Maps D1 results to a Cloesce model structure.
///
/// Requires a previous call to [set_ast_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn map(
    // Model name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // D1 Results
    d1_results_ptr: *const u8,
    d1_results_len: usize,

    // Include tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let d1_results_raw =
        unsafe { str::from_utf8(slice::from_raw_parts(d1_results_ptr, d1_results_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let d1_results = match serde_json::from_str::<D1Result>(d1_results_raw) {
        Ok(res) => res,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = AST.with(|ast| map_sql(model_name, d1_results, include_tree, &ast.borrow()));
    match res {
        Ok(res) => {
            let bytes = serde_json::to_string(&res).unwrap().into_bytes();
            yield_result(bytes);
            0
        }
        Err(e) => {
            yield_error(e);
            1
        }
    }
}

/// Validates a value against a CidlType.
///
/// Requires a previous call to [set_ast_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_type(
    // Cidl Type
    cidl_type_ptr: *const u8,
    cidl_type_len: usize,

    // Value
    value_ptr: *const u8,
    value_len: usize,
) -> i32 {
    let cidl_type_raw =
        unsafe { str::from_utf8(slice::from_raw_parts(cidl_type_ptr, cidl_type_len)).unwrap() };
    let value_raw = unsafe { str::from_utf8(slice::from_raw_parts(value_ptr, value_len)).unwrap() };

    let value = match serde_json::from_str::<Option<serde_json::Value>>(value_raw) {
        Ok(res) => res,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let cidl_type = match serde_json::from_str::<CidlType>(cidl_type_raw) {
        Ok(res) => res,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = AST.with(|ast| validate_cidl_type(cidl_type, value, &ast.borrow(), false));
    match res {
        Ok(value) => {
            let bytes = serde_json::to_string(&value).unwrap().into_bytes();
            yield_result(bytes);
            0
        }
        Err(e) => {
            yield_error(serde_json::to_string(&e).unwrap());
            1
        }
    }
}

fn yield_result(mut bytes: Vec<u8>) {
    // Shrink capacity to match length so dealloc() receives the correct allocation size
    bytes.shrink_to_fit();
    let ptr = bytes.as_mut_ptr();
    unsafe {
        RETURN_LEN = bytes.len();
        std::mem::forget(bytes); // leak so frontend can read

        RETURN_PTR = ptr;
    }
}

fn yield_error(e: impl ToString) {
    let bytes = format!("Encountered an issue in the WASM ORM: {}", e.to_string()).into_bytes();
    yield_result(bytes);
}
