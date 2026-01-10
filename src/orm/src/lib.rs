#![allow(clippy::missing_safety_doc)]
mod methods;

use ast::Model;

use methods::json::select_as_json;
use methods::upsert::UpsertModel;

use serde_json::Map;
use std::cell::RefCell;
use std::collections::HashMap;
use std::slice;
use std::str;

type ModelMeta = HashMap<String, Model>;
type IncludeTreeJson = Map<String, serde_json::Value>;

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
    /// Cloesce meta data, intended to be imported once at WASM initializaton
    pub static META: RefCell<ModelMeta> = RefCell::new(HashMap::new());
}

/// Sets the [META] global variable, returning 0 on success.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_meta_ptr(ptr: *mut u8, cap: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, cap) };

    let parsed: ModelMeta = match serde_json::from_slice(slice) {
        Ok(val) => val,
        Err(e) => {
            yield_error(e);
            return 1;
        }
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

/// Creates a series of insert, update and upsert statements, finally selecting the model.
///
/// Requires a previous call to [set_meta_ptr].
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
        META.with(|meta| UpsertModel::query(model_name, &meta.borrow(), new_model, include_tree));
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

/// Generates a SQL query for a models D1 columns and navigation properties with respect to an include tree,
/// yielding a JSON array of results.
///
/// Requires a previous call to [set_meta_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn as_json(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let include_tree = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(e);
            return 1;
        }
    };

    let res = META.with(|meta| select_as_json(model_name, include_tree, &meta.borrow()));
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
