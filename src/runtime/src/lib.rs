#![allow(clippy::missing_safety_doc)]

mod methods;

use common::Model;

use serde_json::Map;

use std::cell::RefCell;
use std::collections::HashMap;
use std::slice;
use std::str;

type D1Result = Vec<Map<String, serde_json::Value>>;
type ModelMeta = HashMap<String, Model>;
type IncludeTree = Map<String, serde_json::Value>;

/// The result length of the last call to [object_relational_mapping]
static mut RETURN_LEN: usize = 0;

/// User space function to get the [RETURN_LEN]
#[unsafe(no_mangle)]
pub extern "C" fn get_return_len() -> usize {
    unsafe { RETURN_LEN }
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

/// Maps ORM friendly SQL rows to a [Model]. Requires a previous call to [set_meta_ptr].
///
/// Panics on any error.
///
/// Returns a pointer to a JSON result which needs a subsequent [dealloc] call to free.
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
) -> *const u8 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let rows_json = unsafe { str::from_utf8(slice::from_raw_parts(rows_ptr, rows_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let rows = serde_json::from_str::<D1Result>(rows_json).unwrap();
    let include_tree = serde_json::from_str::<Option<IncludeTree>>(include_tree_json).unwrap();

    let res = META
        .with(|meta| {
            methods::orm::object_relational_mapping(
                model_name,
                &meta.borrow(),
                &rows,
                &include_tree,
            )
        })
        .unwrap();

    let json_str = serde_json::to_string(&res).unwrap();

    yield_result(json_str.into_bytes())
}

/// Creates an insert statement for the given model. Requires a previous call to [set_meta_ptr].
///
/// Panics on any error.
///
/// Returns a pointer to a JSON result which needs a subsequent [dealloc] call to free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn insert_model(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // New Model
    new_model_ptr: *const u8,
    new_model_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> *const u8 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let new_model_json =
        unsafe { str::from_utf8(slice::from_raw_parts(new_model_ptr, new_model_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let new_model = serde_json::from_str::<Map<String, serde_json::Value>>(new_model_json).unwrap();
    let include_tree = serde_json::from_str::<Option<IncludeTree>>(include_tree_json).unwrap();

    let res = META
        .with(|meta| {
            methods::insert::insert_model(
                model_name,
                &meta.borrow(),
                new_model,
                include_tree.as_ref(),
            )
        })
        .unwrap();

    yield_result(res.into_bytes())
}

/// Creates an update statement for the given model. Requires a previous call to [set_meta_ptr].
///
/// Panics on any error.
///
/// Returns a pointer to a JSON result which needs a subsequent [dealloc] call to free.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn update_model(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // New Model
    updated_model_ptr: *const u8,
    updated_model_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> *const u8 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let new_model_json = unsafe {
        str::from_utf8(slice::from_raw_parts(updated_model_ptr, updated_model_len)).unwrap()
    };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let updated_model =
        serde_json::from_str::<Map<String, serde_json::Value>>(new_model_json).unwrap();
    let include_tree = serde_json::from_str::<Option<IncludeTree>>(include_tree_json).unwrap();

    let res = META
        .with(|meta| {
            methods::update::update_model(
                model_name,
                &meta.borrow(),
                updated_model,
                include_tree.as_ref(),
            )
        })
        .unwrap();

    yield_result(res.into_bytes())
}

fn yield_result(mut bytes: Vec<u8>) -> *mut u8 {
    // Shrink capacity to match length so dealloc() receives the correct allocation size
    bytes.shrink_to_fit();
    let ptr = bytes.as_mut_ptr();
    unsafe {
        RETURN_LEN = bytes.len();
        std::mem::forget(bytes); // leak so frontend can read
    }

    ptr
}
