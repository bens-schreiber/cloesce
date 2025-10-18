#![allow(clippy::missing_safety_doc)]
mod orm;
mod upsert;

use common::Model;

use serde_json::Map;
use std::cell::RefCell;
use std::collections::HashMap;
use std::slice;
use std::str;
use upsert::UpsertModel;

type D1Result = Vec<Map<String, serde_json::Value>>;
type ModelMeta = HashMap<String, Model>;
type IncludeTree = Map<String, serde_json::Value>;

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
    /// Cloesce meta data AST, intended to be imported once at WASM initializaton
    pub static META: RefCell<ModelMeta> = RefCell::new(HashMap::new());
}

/// Sets the [META] global variable, returning 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_meta_ptr(ptr: *mut u8, cap: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, cap) };

    let parsed: ModelMeta = match serde_json::from_slice(slice) {
        Ok(val) => val,
        Err(_) => return 1,
    };

    META.with(|meta| {
        *meta.borrow_mut() = parsed;
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

/// Maps ORM friendly SQL rows to a [Model]. Requires a previous call to [set_meta_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn object_relational_mapping(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // SQL result rows
    rows_ptr: *const u8,
    rows_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let rows_json = unsafe { str::from_utf8(slice::from_raw_parts(rows_ptr, rows_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let rows = match serde_json::from_str::<D1Result>(rows_json) {
        Ok(rows) => rows,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTree>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = META.with(|meta| {
        orm::object_relational_mapping(model_name, &meta.borrow(), &rows, &include_tree)
    });
    match res {
        Ok(res) => {
            let json_str = serde_json::to_string(&res).unwrap();
            yield_result(json_str.into_bytes());
            0
        }
        Err(e) => {
            yield_error(e);
            1
        }
    }
}

/// Creates an insert statement for the given model. Requires a previous call to [set_meta_ptr].
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

    let include_tree = match serde_json::from_str::<Option<IncludeTree>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = META.with(|meta| {
        UpsertModel::query(model_name, &meta.borrow(), new_model, include_tree.as_ref())
    });
    match res {
        Ok(res) => {
            yield_result(res.into_bytes());
            0
        }
        Err(e) => {
            yield_result(e.into_bytes());
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
    let bytes = format!(
        "Encountered an issue in the WASM ORM runtime: {}",
        e.to_string()
    )
    .into_bytes();
    yield_result(bytes);
}
