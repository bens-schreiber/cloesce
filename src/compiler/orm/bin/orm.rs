//! WASM bindings for the Cloesce ORM
//!
//! # Overview
//! This module provides a set of unsafe extern "C" functions that can be called in foreign environments.
//! In order for the ORM to work properly, the [IDL] must be set by calling [set_idl_ptr] with a pointer to a JSON string representing the IDL.
//! (TODO: Try to bake the IDL directly into the memory of the WASM module at compile time to avoid this step)
//!
//! Each function returns 0 on success and 1 on failure, with the result or error message stored in [RETURN_PTR] and its length in [RETURN_LEN].
//!
//! ## Safety
//! All functions in this module are unsafe because they involve raw pointer manipulation and require adherence to specific
//! data formats. Callers must ensure that all pointers passed to these functions are valid and that the data they point to is
//! correctly formatted as UTF-8 encoded strings or JSON, as specified in each function's documentation.

use idl::ValidatedField;
use idl::{CloesceIdl, IncludeTree};
use orm::OrmErrorKind;
use orm::query::save;
use orm::query::select;
use orm::query::select::planner::SelectOperation;
use orm::validate::validate_cidl_type;

use std::cell::RefCell;
use std::slice;
use std::str;

fn serde_err(e: serde_json::Error) -> OrmErrorKind {
    OrmErrorKind::SerializeError {
        message: e.to_string(),
    }
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
///
/// # Safety
/// `ptr` must be a pointer returned from [alloc] and `cap` must be
/// the same capacity that was passed to [alloc] when the pointer was created.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, cap: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, cap);
    }
}

thread_local! {
    /// intended to be imported once at WASM initializaton
    pub static IDL: RefCell<CloesceIdl<'static>> = RefCell::new(CloesceIdl::default());
}

/// Sets the [IDL] global variable, returning 0 on success.
///
/// # Safety
/// `ptr` must be a pointer to a UTF-8 encoded JSON string representing the IDL
/// and `cap` must be the length of the JSON string in bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_idl_ptr(ptr: *mut u8, cap: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts(ptr, cap) };

    let parsed: CloesceIdl = match serde_json::from_slice(slice) {
        Ok(val) => val,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    IDL.with(|idl| {
        *idl.borrow_mut() = parsed;
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

/// Reads a UTF-8 string from WASM memory.
///
/// # Safety
/// `ptr` must point to `len` bytes of valid UTF-8.
unsafe fn read_str<'a>(ptr: *const u8, len: usize) -> &'a str {
    unsafe { str::from_utf8(slice::from_raw_parts(ptr, len)).unwrap() }
}

/// Plans a select (get or list) operation, returning a `SelectPlan` as JSON for
/// the runtime executor.
///
/// Requires a previous call to [set_idl_ptr].
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
///
/// # Safety
/// `model_name_ptr` must be a pointer to a UTF-8 encoded string representing the model name,
/// `operation_ptr` must be a pointer to a UTF-8 encoded string of either `get` or `list`,
/// and `include_tree_ptr` must be a pointer to a UTF-8 encoded JSON string representing the include tree.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plan_select(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // Operation ("get" | "list")
    operation_ptr: *const u8,
    operation_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> i32 {
    let model_name = unsafe { read_str(model_name_ptr, model_name_len) };
    let operation_raw = unsafe { read_str(operation_ptr, operation_len) };
    let include_tree_json = unsafe { read_str(include_tree_ptr, include_tree_len) };

    let operation = match operation_raw {
        "get" => SelectOperation::Get,
        "list" => SelectOperation::List,
        other => {
            yield_error(OrmErrorKind::SerializeError {
                message: format!("Unknown select operation '{other}'"),
            });
            return 1;
        }
    };

    let tree = match serde_json::from_str::<Option<IncludeTree>>(include_tree_json) {
        Ok(tree) => tree.unwrap_or_default(),
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let json = IDL.with(|idl| {
        let idl = idl.borrow();
        let plan = select::planner::plan(operation, model_name, &idl, &tree);
        serde_json::to_string(&plan).unwrap()
    });

    yield_result(json.into_bytes());
    0
}

/// Plans a save (upsert) operation from a payload, returning a `SavePlan` as JSON
/// for the runtime executor.
///
/// Requires a previous call to [set_idl_ptr].
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
///
/// # Safety
/// `model_name_ptr` must be a pointer to a UTF-8 encoded string representing the model name,
/// `include_tree_ptr` must be a pointer to a UTF-8 encoded JSON string representing the include tree,
/// and `payload_ptr` must be a pointer to a UTF-8 encoded JSON string representing the save payload.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn plan_save(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,

    // Payload
    payload_ptr: *const u8,
    payload_len: usize,
) -> i32 {
    let model_name = unsafe { read_str(model_name_ptr, model_name_len) };
    let include_tree_json = unsafe { read_str(include_tree_ptr, include_tree_len) };
    let payload_json = unsafe { read_str(payload_ptr, payload_len) };

    let tree = match serde_json::from_str::<Option<IncludeTree>>(include_tree_json) {
        Ok(tree) => tree.unwrap_or_default(),
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let payload = match serde_json::from_str::<serde_json::Value>(payload_json) {
        Ok(payload) => payload,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let res = IDL.with(|idl| {
        let idl = idl.borrow();
        save::planner::plan(model_name, &idl, &tree, &payload)
            .map(|plan| serde_json::to_string(&plan).unwrap())
    });

    match res {
        Ok(json) => {
            yield_result(json.into_bytes());
            0
        }
        Err(e) => {
            yield_error(e);
            1
        }
    }
}

/// Validates a value against a ValidatedField
///
/// Requires a previous call to [set_idl_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
///
/// # Safety
/// `cidl_type_ptr` must be a pointer to a UTF-8 encoded JSON string representing the CidlType
///  and `value_ptr` must be a pointer to a UTF-8 encoded JSON string representing the value to be validated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn validate_type(
    // Validated Field
    validated_field_ptr: *const u8,
    validated_field_len: usize,

    // Value
    value_ptr: *const u8,
    value_len: usize,
) -> i32 {
    let validated_field_raw = unsafe { read_str(validated_field_ptr, validated_field_len) };
    let value_raw = unsafe { read_str(value_ptr, value_len) };

    let validated_field = match serde_json::from_str::<ValidatedField>(validated_field_raw) {
        Ok(res) => res,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let value = match serde_json::from_str::<Option<serde_json::Value>>(value_raw) {
        Ok(res) => res,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let res = IDL.with(|idl| validate_cidl_type(&validated_field, value, &idl.borrow(), false));
    match res {
        Ok(value) => {
            let bytes = serde_json::to_string(&value).unwrap().into_bytes();
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

fn yield_error(e: OrmErrorKind) {
    let bytes = e.to_string().into_bytes();
    yield_result(bytes);
}

fn main() {}
