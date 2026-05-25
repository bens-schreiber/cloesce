use frontend::Symbol;
use idl::{
    Field, KvBinding, KvBindingField, R2Binding, R2BindingField, ValidatedField, WranglerEnv,
};

use crate::{
    SymbolTable, ensure,
    err::{ErrorSink, SemanticError},
    resolve_cidl_type, resolve_validator_tags,
};

/// Builds the [WranglerEnv] from the symbol table, resolving and validating
/// KV/R2 binding fields and their parameters along the way.
///
/// Returns [None] if there is nothing to put in the env block and no models
/// require one.
pub fn build_wrangler_env<'src, 'p>(
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> Option<WranglerEnv<'src>> {
    let d1_bindings: Vec<&'src str> = table
        .d1_bindings
        .iter()
        .flat_map(|b| b.bindings.iter().map(|s| s.name))
        .collect();

    let vars = table
        .vars_blocks
        .iter()
        .flat_map(|b| b.vars.iter())
        .map(|s| Field {
            name: s.name.into(),
            cidl_type: s.cidl_type.clone(),
        })
        .collect::<Vec<_>>();

    let mut kv_bindings = Vec::new();
    for block in table.kv_bindings.values() {
        let mut fields = Vec::new();
        for spd in &block.fields {
            let bf = &spd.inner;

            let resolved_type = match resolve_cidl_type(&bf.symbol, &bf.symbol.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    sink.push(err);
                    continue;
                }
            };

            let params = resolve_binding_params(&bf.params, table, sink);
            validate_key_format(&bf.symbol, bf.key_format, &bf.params, sink);

            fields.push(KvBindingField {
                name: bf.symbol.name,
                cidl_type: resolved_type,
                params,
                key_format: bf.key_format,
            });
        }

        kv_bindings.push(KvBinding {
            name: block.symbol.name,
            fields,
        });
    }

    let mut r2_bindings = Vec::new();
    for block in table.r2_bindings.values() {
        let mut fields = Vec::new();
        for spd in &block.fields {
            let bf = &spd.inner;
            let params = resolve_binding_params(&bf.params, table, sink);
            validate_key_format(&bf.symbol, bf.key_format, &bf.params, sink);
            fields.push(R2BindingField {
                name: bf.symbol.name,
                params,
                key_format: bf.key_format,
            });
        }
        r2_bindings.push(R2Binding {
            name: block.symbol.name,
            fields,
        });
    }

    if d1_bindings.is_empty() && kv_bindings.is_empty() && r2_bindings.is_empty() && vars.is_empty()
    {
        let needs_env = table.models.values().any(|m| !m.blocks.is_empty());
        ensure!(!needs_env, sink, SemanticError::MissingWranglerEnvBlock);
        return None;
    };

    Some(WranglerEnv {
        d1_bindings,
        r2_bindings,
        kv_bindings,
        vars,
    })
}

/// Validates that every `{var}` referenced in a binding field's key format
/// corresponds to a declared param on that field.
///
/// Also flags malformed key formats (e.g. nested or unclosed braces).
fn validate_key_format<'src, 'p>(
    field: &'p Symbol<'src>,
    key_format: &'src str,
    params: &'p [Symbol<'src>],
    sink: &mut ErrorSink<'src, 'p>,
) {
    let vars = match extract_braced(key_format) {
        Ok(v) => v,
        Err(reason) => {
            sink.push(SemanticError::KvR2InvalidKeyFormat { field, reason });
            return;
        }
    };

    for var in vars {
        if !params.iter().any(|p| p.name == var) {
            sink.push(SemanticError::KvR2UnknownKeyVariable {
                field,
                variable: var,
            });
        }
    }
}

/// Extracts braced variables from a format string.
/// e.g. "users/{userId}/posts/{postId}" => ["userId", "postId"].
///
/// Returns an error string if the format string is invalid (e.g. nested or
/// unclosed braces).
fn extract_braced(s: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut current = None;
    for (i, c) in s.char_indices() {
        match (current.is_some(), c) {
            (false, '{') => current = Some(i + 1),
            (true, '{') => return Err("nested brace in key".to_string()),
            (true, '}') => {
                let start_idx = current.take().unwrap();
                out.push(&s[start_idx..i]);
            }
            (true, _) => {}
            _ => {}
        }
    }
    if current.is_some() {
        return Err("unclosed brace in key".to_string());
    }
    Ok(out)
}

/// Resolves a binding field's parameter symbols into [ValidatedField]s.
fn resolve_binding_params<'src, 'p>(
    params: &'p [Symbol<'src>],
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> Vec<ValidatedField<'src>> {
    let mut out = Vec::new();
    for param in params {
        let resolved = match resolve_cidl_type(param, &param.cidl_type, table) {
            Ok(t) => t,
            Err(err) => {
                sink.push(err);
                continue;
            }
        };

        let validators = match resolve_validator_tags(param) {
            Ok(v) => v,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        };

        out.push(ValidatedField {
            name: param.name.into(),
            cidl_type: resolved,
            validators,
        });
    }
    out
}
