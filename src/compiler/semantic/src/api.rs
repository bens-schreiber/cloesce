use std::ops::Not;

use crate::{
    LocalSymbolKind, SymbolTable, ensure,
    err::{ErrorSink, SemanticError},
    resolve_cidl_type, resolve_injects, resolve_validator_tags,
};
use frontend::{ApiBlockMethod, ApiBlockMethodParamKind, InjectEntry, SpdSlice, Symbol, Tag};
use idl::{ApiMethod, CidlType, DurableTarget, HttpVerb, MediaType, ValidatedField};

pub fn analyze<'src, 'p>(
    sink: &mut ErrorSink<'src, 'p>,
    table: &SymbolTable<'src, 'p>,
) -> Vec<(&'src str, Vec<ApiMethod<'src>>)> {
    let mut result = Vec::new();

    for api_block in &table.apis {
        let Some(model) = table.models.get(api_block.symbol.name) else {
            sink.push(SemanticError::ApiUnknownNamespaceReference {
                api: &api_block.symbol,
            });
            continue;
        };
        let namespace = model.symbol.name;

        let mut methods = Vec::new();
        for api_method in api_block.methods.inners() {
            if let Some(m) = method(namespace, api_method, table, sink) {
                methods.push(m);
            }
        }
        result.push((namespace, methods));
    }

    result
}

fn method<'src, 'p>(
    namespace: &'src str,
    method: &'p ApiBlockMethod<'src>,
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> Option<ApiMethod<'src>> {
    // Validate return type
    let (return_type, return_media) = return_type(method, table, sink);

    // Validate parameters
    let (mut parameters, parameters_media, is_static, data_source_name) =
        parameters(namespace, method, table, sink);

    let mut injected = resolve_injects(&method.symbol, table, sink);

    let durable_target = context(method, &mut parameters, table, sink);
    if durable_target.is_some() {
        injected.push(idl::CONTEXT_INJECT_KEY);
    }

    let data_source = if table.models.contains_key(namespace) && !is_static {
        Some(data_source_name.unwrap_or("Default"))
    } else {
        None
    };

    Some(ApiMethod {
        name: method.symbol.name.into(),
        is_static,
        data_source,
        http_verb: method.http_verb,
        return_media,
        return_type,
        parameters_media,
        parameters,
        injected,
        durable_target,
    })
}

/// Resolves a method's `[inject Do(args)]` context entries into a [DurableTarget].
fn context<'src, 'p>(
    method: &'p ApiBlockMethod<'src>,
    parameters: &mut [ValidatedField<'src>],
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> Option<DurableTarget<'src>> {
    let mut target: Option<DurableTarget<'src>> = None;

    for tag in &method.symbol.tags {
        let Tag::Inject { entries } = &tag.inner else {
            continue;
        };

        for (binding, args) in entries.iter().filter_map(|entry| {
            if let InjectEntry::Context { symbol, args } = entry {
                Some((symbol, args))
            } else {
                None
            }
        }) {
            if target.is_some() {
                // at most one DO instantiation per method.
                sink.push(SemanticError::TagInvalidInContext {
                    tag,
                    symbol: &method.symbol,
                });
                continue;
            }

            let Some(durable) = table.durable_bindings.get(binding.name) else {
                sink.push(SemanticError::UnresolvedSymbol { symbol: binding });
                continue;
            };

            let shard_fields: Vec<&Symbol> = durable
                .shard_blocks
                .inners()
                .flat_map(|s| &s.fields)
                .collect();

            if shard_fields.len() != args.len() {
                sink.push(SemanticError::ArgCountMismatch {
                    field: binding,
                    expected: shard_fields.len(),
                    got: args.len(),
                });
                continue;
            }

            let mut shard_args = Vec::with_capacity(args.len());
            for (arg, shard_field) in args.iter().zip(&shard_fields) {
                let Some(param) = parameters.iter_mut().find(|p| p.name == arg.name) else {
                    // a shard argument must name a parameter of the method.
                    sink.push(SemanticError::UnresolvedSymbol { symbol: arg });
                    continue;
                };

                let shard_type = match resolve_cidl_type(shard_field, &shard_field.cidl_type, table)
                {
                    Ok(t) => t,
                    Err(e) => {
                        sink.push(e);
                        continue;
                    }
                };
                if param.cidl_type != shard_type {
                    sink.push(SemanticError::ArgTypeMismatch {
                        field: binding,
                        arg,
                    });
                    continue;
                }

                // Inherit the shard field's validators
                match resolve_validator_tags(shard_field) {
                    Ok(validators) => param.validators.extend(validators),
                    Err(errs) => sink.extend(errs),
                }

                shard_args.push(arg.name);
            }

            target = Some(DurableTarget {
                binding: binding.name,
                shard_args,
            });
        }
    }

    target
}

fn return_type<'src, 'p>(
    method: &'p ApiBlockMethod<'src>,
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> (CidlType<'src>, MediaType) {
    let err = SemanticError::ApiInvalidReturn {
        method: &method.symbol,
    };

    let resolved_type = match resolve_cidl_type(&method.symbol, &method.symbol.cidl_type, table) {
        Ok(t) => t,
        Err(e) => {
            sink.push(e);
            return (CidlType::Void, MediaType::Json);
        }
    };

    let return_media = match resolved_type.root_type() {
        CidlType::Stream => MediaType::Octet,
        _ => MediaType::Json,
    };

    if matches!(resolved_type.root_type(), CidlType::Stream) {
        // Stream is only valid as a return type if it's the root type
        ensure!(
            matches!(method.symbol.cidl_type, CidlType::Stream),
            sink,
            err.clone()
        );
    }

    (resolved_type, return_media)
}

fn parameters<'src, 'p>(
    namespace: &'src str,
    method: &'p ApiBlockMethod<'src>,
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> (
    Vec<ValidatedField<'src>>,
    MediaType,
    bool,
    Option<&'src str>,
) {
    let mut params = Vec::new();

    let mut has_stream = false;
    let mut data_source: Option<&'src str> = None;
    let mut is_static = true;
    for param in method.parameters.inners() {
        let param = match param {
            ApiBlockMethodParamKind::SelfParam(self_sym) => {
                is_static = false;

                // Validate tags
                for tag in &self_sym.tags {
                    let Tag::Source { name } = &tag.inner else {
                        sink.push(SemanticError::TagInvalidInContext {
                            tag,
                            symbol: self_sym,
                        });
                        continue;
                    };

                    data_source = Some(name.inner);

                    // Check that the data source exists on this namespace
                    let ds_exists = table.local.contains_key(&LocalSymbolKind::DataSourceDecl {
                        model: namespace,
                        name: name.inner,
                    });

                    ensure!(
                        ds_exists,
                        sink,
                        SemanticError::ApiUnknownDataSourceReference {
                            method: &method.symbol,
                            data_source: name,
                        }
                    );
                }

                // No further validation is needed for `self`.
                continue;
            }
            ApiBlockMethodParamKind::Param(symbol) => symbol,
        };

        // Validate tags
        for tag in &param.tags {
            if !matches!(tag.inner, Tag::Validator { .. }) {
                sink.push(SemanticError::TagInvalidInContext { tag, symbol: param });
            }
        }

        let resolved_type = match resolve_cidl_type(param, &param.cidl_type, table) {
            Ok(t) => t,
            Err(e) => {
                sink.push(e);
                continue;
            }
        };
        let invalid_type_err = SemanticError::ApiInvalidParam {
            method: &method.symbol,
            param,
        };
        match resolved_type.root_type() {
            CidlType::Object { .. } | CidlType::Partial { .. } => {
                // GET requests do not support Object parameters
                ensure!(
                    matches!(method.http_verb, HttpVerb::Get).not(),
                    sink,
                    invalid_type_err
                );
            }

            CidlType::R2Object => {
                // GET requests do not support R2Object parameters
                ensure!(
                    matches!(method.http_verb, HttpVerb::Get).not(),
                    sink,
                    invalid_type_err
                );
            }

            CidlType::Stream => {
                // GET requests do not have any body, so Stream parameters
                // cannot be used
                ensure!(
                    matches!(method.http_verb, HttpVerb::Get).not(),
                    sink,
                    invalid_type_err.clone()
                );

                has_stream = true;
                let required_params = method
                    .parameters
                    .inners()
                    .filter(|p| matches!(p, ApiBlockMethodParamKind::Param(_)))
                    .count();

                // Only one Stream parameter is allowed, and it must be the
                // only non-injected parameter
                ensure!(
                    required_params == 1 && matches!(param.cidl_type, CidlType::Stream),
                    sink,
                    invalid_type_err
                );
            }

            _ => {}
        }

        let validators = match resolve_validator_tags(param) {
            Ok(v) => v,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        };

        params.push(ValidatedField {
            name: param.name.into(),
            cidl_type: resolved_type,
            validators,
        });
    }

    (
        params,
        if has_stream {
            MediaType::Octet
        } else {
            MediaType::Json
        },
        is_static,
        data_source,
    )
}
