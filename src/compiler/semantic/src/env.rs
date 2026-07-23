use std::borrow::Cow;

use frontend::{SpdSlice, Symbol};
use idl::{
    Binding, BindingTemplate, CidlType, DurableBinding, Field, TemplateSegment, ValidatedField,
    WranglerEnv,
};

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    resolve_cidl_type, resolve_validator_tags,
};

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

            // KV templates always return `KvObject<T>`.
            field.cidl_type = CidlType::KvObject(Box::new(field.cidl_type.clone()));

            let params = bf
                .params
                .iter()
                .filter_map(|p| validate_symbol(p, sink, table))
                .collect::<Vec<_>>();

            if params.len() != bf.params.len() {
                continue;
            }

            let Some(segments) = key_segments(&bf.symbol, bf.key_format, &bf.params, sink) else {
                continue;
            };
            templates.push(BindingTemplate {
                field,
                segments,
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

            // R2 templates always return `R2Object`.
            field.cidl_type = CidlType::R2Object;

            let params = bf
                .params
                .iter()
                .filter_map(|p| validate_symbol(p, sink, table))
                .collect::<Vec<_>>();

            if params.len() != bf.params.len() {
                continue;
            }

            let Some(segments) = key_segments(&bf.symbol, bf.key_format, &bf.params, sink) else {
                continue;
            };
            templates.push(BindingTemplate {
                field,
                segments,
                params,
            });
        }

        r2_bindings.push(Binding {
            name: block.symbol.name,
            templates,
        });
    }

    let mut durable_bindings = Vec::new();
    for block in table.durable_bindings.values() {
        let shard_fields = block
            .shard_blocks
            .inners()
            .flat_map(|s| &s.fields)
            .filter_map(|sf| validate_symbol(sf, sink, table))
            .collect::<Vec<_>>();

        let mut templates = Vec::new();
        for bf in block.templates.inners() {
            // DO storage stores values directly
            let Some(field) = validate_symbol(&bf.symbol, sink, table) else {
                continue;
            };

            let params = bf
                .params
                .iter()
                .filter_map(|p| validate_symbol(p, sink, table))
                .collect::<Vec<_>>();

            if params.len() != bf.params.len() {
                continue;
            }

            let Some(segments) = key_segments(&bf.symbol, bf.key_format, &bf.params, sink) else {
                continue;
            };
            templates.push(BindingTemplate {
                field,
                segments,
                params,
            });
        }

        durable_bindings.push(DurableBinding {
            name: block.symbol.name,
            shard_fields,
            templates,
        });
    }

    WranglerEnv {
        d1_bindings,
        r2_bindings,
        kv_bindings,
        durable_bindings,
        vars,
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

/// Builds a template's key [TemplateSegment]s. An explicit `Some(fmt)` is
/// validated and parsed; a body-less `None` yields the generated default key.
fn key_segments<'src, 'p>(
    symbol: &'p Symbol<'src>,
    key_format: Option<&'src str>,
    params: &'p [Symbol<'src>],
    sink: &mut ErrorSink<'src, 'p>,
) -> Option<Vec<TemplateSegment<'src, &'src str>>> {
    match key_format {
        Some(fmt) => {
            if !validate_key_format(symbol, fmt, params, sink) {
                return None;
            }
            Some(TemplateSegment::parse(fmt, |name| name))
        }
        None => Some(default_segments(symbol.name, params)),
    }
}

/// Builds the default key for a body-less template: `name` for zero params, else
/// `[Literal("name/p0/"), Value(p0), Literal("/p1/"), Value(p1), …]`.
fn default_segments<'src>(
    name: &'src str,
    params: &[Symbol<'src>],
) -> Vec<TemplateSegment<'src, &'src str>> {
    if params.is_empty() {
        return vec![TemplateSegment::Literal(Cow::Borrowed(name))];
    }
    params
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            let literal = if i == 0 {
                format!("{name}/{}/", p.name)
            } else {
                format!("/{}/", p.name)
            };
            [
                TemplateSegment::Literal(Cow::Owned(literal)),
                TemplateSegment::Value(p.name),
            ]
        })
        .collect()
}
