use idl::ValidatedField;
use idl::{CloesceIdl, IncludeTree};
use orm::OrmErrorKind;
use orm::map::map_sql;
use orm::select::SelectModel;
use orm::upsert::UpsertModel;
use orm::validate::validate_cidl_type;

use serde_json::Map;
use std::cell::RefCell;
use std::slice;
use std::str;

type IncludeTreeJson = Map<String, serde_json::Value>;
type D1Result = Vec<Map<String, serde_json::Value>>;

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

/// Creates a series of insert, update and upsert statements, finally selecting the model.
///
/// Requires a previous call to [set_ast_ptr].
///
/// Panics on any error.
///
/// Returns 0 on pass 1 on fail. Stores result in [RETURN_PTR].
///
/// # Safety
/// `model_name_ptr` must be a pointer to a UTF-8 encoded string representing the
/// model name, `new_model_ptr` must be a pointer to a UTF-8 encoded JSON string
/// representing the new model, and `include_tree_ptr` must be a pointer to a UTF-8 encoded JSON string representing the include tree.
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
            yield_error(serde_err(e));
            return 1;
        }
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let res =
        IDL.with(|idl| UpsertModel::query(model_name, &idl.borrow(), new_model, include_tree));
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
///
/// # Safety
/// `model_name_ptr` must be a pointer to a UTF-8 encoded string representing the
/// model name, `from_ptr` must be a pointer to a UTF-8 encoded string representing the "from" clause,
/// and `include_tree_ptr` must be a pointer to a UTF-8 encoded JSON string representing the include tree.  
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
    let include_tree = match serde_json::from_str::<Option<IncludeTree>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let res =
        IDL.with(|idl| SelectModel::query(model_name, from, include_tree.as_ref(), &idl.borrow()));
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
///
/// # Safety
/// `model_name_ptr` must be a pointer to a UTF-8 encoded string representing the
/// model name, `d1_results_ptr` must be a pointer to a UTF-8 encoded JSON string
/// representing the D1 results and `include_tree_ptr` must be a pointer to a UTF-8 encoded JSON string
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
            yield_error(serde_err(e));
            return 1;
        }
    };

    let include_tree = match serde_json::from_str::<Option<IncludeTreeJson>>(include_tree_json) {
        Ok(include_tree) => include_tree,
        Err(e) => {
            yield_error(serde_err(e));
            return 1;
        }
    };

    let res = IDL.with(|idl| map_sql(model_name, d1_results, include_tree, &idl.borrow()));
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

/// Validates a value against a ValidatedField
///
/// Requires a previous call to [set_ast_ptr].
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
    let validated_field_raw = unsafe {
        str::from_utf8(slice::from_raw_parts(
            validated_field_ptr,
            validated_field_len,
        ))
        .unwrap()
    };
    let value_raw = unsafe { str::from_utf8(slice::from_raw_parts(value_ptr, value_len)).unwrap() };

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
