use frontend::{SpdSlice, Symbol};
use idl::{Binding, BindingTemplate, CidlType, Field, ValidatedField, WranglerEnv};

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    resolve_cidl_type, resolve_validator_tags,
};

pub struct WranglerAnalysis;
impl WranglerAnalysis {
    /// Builds the [WranglerEnv] from the symbol table, resolving and validating
    /// KV/R2 binding templates and their parameters along the way.
    pub fn analyze<'src, 'p>(
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> WranglerEnv<'src> {
        let d1_bindings = table
            .d1_bindings
            .iter()
            .flat_map(|b| b.bindings.iter().map(|s| s.name))
            .collect::<Vec<_>>();

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
            let mut templates = Vec::new();
            for bf in block.templates.inners() {
                let Some(mut field) = validate_symbol(&bf.symbol, sink, table) else {
                    continue;
                };

                // KV templates always return `KvObject<T>`
                // (or `paginated<KvObject<T>>` if the template is paginated).
                field.cidl_type = match &field.cidl_type {
                    CidlType::Paginated(inner) => {
                        CidlType::paginated(CidlType::KvObject(inner.clone()))
                    }
                    other => CidlType::KvObject(Box::new(other.clone())),
                };

                let params = bf
                    .params
                    .iter()
                    .filter_map(|p| validate_symbol(p, sink, table))
                    .collect::<Vec<_>>();

                if !validate_key_format(&bf.symbol, bf.key_format, &bf.params, sink)
                    || params.len() != bf.params.len()
                {
                    continue;
                }

                templates.push(BindingTemplate {
                    field,
                    key_format: bf.key_format,
                    params,
                });
            }

            kv_bindings.push(Binding {
                name: block.symbol.name,
                templates,
            });
        }

        let mut r2_bindings = Vec::new();
        for block in table.r2_bindings.values() {
            let mut templates = Vec::new();
            for bf in block.templates.inners() {
                let Some(mut field) = validate_symbol(&bf.symbol, sink, table) else {
                    continue;
                };

                // R2 templates always return `R2Object` (or `paginated<R2Object>` if the template is paginated).
                field.cidl_type = if bf.is_paginated {
                    CidlType::Paginated(Box::new(CidlType::R2Object))
                } else {
                    CidlType::R2Object
                };

                let params = bf
                    .params
                    .iter()
                    .filter_map(|p| validate_symbol(p, sink, table))
                    .collect::<Vec<_>>();

                if !validate_key_format(&bf.symbol, bf.key_format, &bf.params, sink)
                    || params.len() != bf.params.len()
                {
                    continue;
                }
                templates.push(BindingTemplate {
                    field,
                    key_format: bf.key_format,
                    params,
                });
            }

            r2_bindings.push(Binding {
                name: block.symbol.name,
                templates,
            });
        }

        WranglerEnv {
            d1_bindings,
            r2_bindings,
            kv_bindings,
            vars,
        }
    }
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
) -> bool {
    let vars = match extract_braced(key_format) {
        Ok(v) => v,
        Err(reason) => {
            sink.push(SemanticError::TemplateInvalidFormat { field, reason });
            return false;
        }
    };

    for var in vars {
        if !params.iter().any(|p| p.name == var) {
            sink.push(SemanticError::TemplateUnknownVariable {
                field,
                variable: var,
            });
            return false;
        }
    }

    true
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

fn validate_symbol<'src, 'p>(
    symbol: &'p Symbol<'src>,
    sink: &mut ErrorSink<'src, 'p>,
    table: &SymbolTable<'src, 'p>,
) -> Option<ValidatedField<'src>> {
    let cidl_type = match resolve_cidl_type(symbol, &symbol.cidl_type, table) {
        Ok(t) => t,
        Err(err) => {
            sink.push(err);
            return None;
        }
    };

    let validators = match resolve_validator_tags(symbol) {
        Ok(tags) => tags,
        Err(errs) => {
            sink.extend(errs);
            return None;
        }
    };

    Some(ValidatedField {
        name: symbol.name.into(),
        cidl_type,
        validators,
    })
}
