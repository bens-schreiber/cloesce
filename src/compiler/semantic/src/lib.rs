//! Semantic Analysis + Expansion phase.
//!
//! Semantic analysis is responsible for validating the [Ast] produced by the parser and converting it into the [CloesceIdl],
//! a HIR that describes the full semantics of the program. This includes:
//!
//! - Resolving type references to produce fully resolved [CidlType]s
//! - Tying APIs to their respective model namespaces
//! - Validating that all symbols are uniquely defined and correctly used
//! - Validating the Wrangler environment configuration (Cloudflares infrastructure bindings)
//! - Various other semantic checks (see the [SemanticError] enum)
//!
//! Additionally, after semantic analysis, the IDL is expanded with synthetic APIs and data sources based on the presence of models
//! and the configuration of existing data sources.
//!
//! ## Error Sink
//!
//! No single error halts the entire analysis process. Instead, errors are collected in an [ErrorSink] and reported together at the end.
//! Some errors may cause a certain structure to be escaped or treated as if it were not present, but will be reported in the final error list.

use frontend::{
    ApiBlock, ApiBlockMethodParamKind, ArgumentLiteral, Ast, AstBlockKind, D1BindingBlock,
    DataSourceBlock, DurableBindingBlock, InjectBlock, InjectEntry, KvBindingBlock, ModelBlock,
    PlainOldObjectBlock, R2BindingBlock, Spd, SpdSlice, Symbol, Tag, VarsBlock,
};
use idl::{CidlType, CloesceIdl, Number, PlainOldObject, ValidatedField, Validator};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap};

use crate::{
    err::{BatchResult, ErrorSink, SemanticError},
    model::ModelAnalysis,
};

mod api;
mod crud;
mod data_source;
mod env;
pub mod err;
mod model;

/// Undergoes semantic analysis and expansion on the provided [Ast],
/// returning either a valid [CloesceIdl] or a list of [SemanticError]s.
pub fn analyze<'src, 'p>(
    ast: &'p Ast<'src>,
) -> Result<CloesceIdl<'src>, Vec<SemanticError<'src, 'p>>> {
    let mut sink = ErrorSink::new();
    let table = SymbolTable::from_ast(ast, &mut sink);

    let wrangler_env = env::analyze(&table, &mut sink);

    let mut models = match ModelAnalysis::new(&wrangler_env).analyze(&table) {
        Ok(models) => models,
        Err(errs) => {
            sink.extend(errs);
            IndexMap::default()
        }
    };

    let data_source_map = data_source::analysis::analyze(&models, &table, &mut sink);
    let poos = analyze_poos(&table, &mut sink);

    let api_map = api::analyze(&mut sink, &table);

    for (namespace, apis) in api_map {
        if let Some(model) = models.get_mut(&namespace) {
            model.apis.extend(apis);
        }
    }

    for (model_name, ds) in data_source_map {
        if let Some(model) = models.get_mut(&model_name) {
            model.data_sources.insert(ds.name, ds);
        }
    }

    let injects = table
        .injects
        .iter()
        .flat_map(|i| i.symbols.iter().map(|f| f.name))
        .collect();

    let mut idl = CloesceIdl {
        hash: 0,
        wrangler_env,
        models,
        poos,
        injects,
    };
    let errs = sink.drain();
    if !errs.is_empty() {
        return Err(errs);
    }

    data_source::expansion::expand(&mut idl);
    crud::expand(&mut idl);
    idl.set_merkle_hash();

    Ok(idl)
}

fn analyze_poos<'src, 'p>(
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> BTreeMap<&'src str, PlainOldObject<'src>> {
    let mut res = BTreeMap::new();

    for poo in table.poos.values() {
        let poo_name = poo.symbol.name;
        let mut fields = Vec::new();

        for field in &poo.fields {
            let resolved_type = match resolve_cidl_type(field, &field.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    sink.push(err);
                    continue;
                }
            };

            match resolved_type.root_type() {
                CidlType::Stream => {
                    sink.push(SemanticError::PlainOldObjectInvalidFieldType { field });
                }
                _ => {
                    // All other types are valid
                }
            }

            let validators = match resolve_validator_tags(field) {
                Ok(v) => v,
                Err(errs) => {
                    sink.extend(errs);
                    Vec::new()
                }
            };

            fields.push(ValidatedField {
                name: field.name.into(),
                cidl_type: resolved_type,
                validators,
            });
        }

        res.insert(
            poo_name,
            PlainOldObject {
                name: poo_name,
                fields,
            },
        );
    }

    res
}

/// Scopes for any symbol that is nested within some other symbol,
/// (called a local symbol) e.g. a field within a model or a parameter within an API method.
#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
enum LocalSymbolKind<'src> {
    BindingTemplate {
        binding: &'src str,
        name: &'src str,
    },
    BindingTemplateParam {
        binding: &'src str,
        template: &'src str,
        name: &'src str,
    },
    ShardField {
        binding: &'src str,
        name: &'src str,
    },
    ModelField {
        model: &'src str,
        name: &'src str,
    },
    PlainOldObjectField {
        poo: &'src str,
        name: &'src str,
    },
    ApiMethodDecl {
        namespace: &'src str,
        name: &'src str,
    },
    ApiMethodParam {
        namespace: &'src str,
        method: &'src str,
        name: &'src str,
    },
    DataSourceDecl {
        model: &'src str,
        name: &'src str,
    },
    DataSourceMethodParam {
        data_source: &'src str,
        method: &'src str,
        name: &'src str,
    },
}

/// A table that maps a symbol name to its definition in the [Ast].
#[derive(Default)]
struct SymbolTable<'src, 'p> {
    // Globals
    models: BTreeMap<&'src str, &'p ModelBlock<'src>>,
    poos: BTreeMap<&'src str, &'p PlainOldObjectBlock<'src>>,
    d1_bindings: Vec<&'p D1BindingBlock<'src>>,
    kv_bindings: BTreeMap<&'src str, &'p KvBindingBlock<'src>>,
    r2_bindings: BTreeMap<&'src str, &'p R2BindingBlock<'src>>,
    durable_bindings: BTreeMap<&'src str, &'p DurableBindingBlock<'src>>,
    vars_blocks: Vec<&'p VarsBlock<'src>>,
    injects: Vec<&'p InjectBlock<'src>>,
    apis: Vec<&'p ApiBlock<'src>>,
    data_sources: Vec<&'p DataSourceBlock<'src>>,

    // Locals
    local: BTreeMap<LocalSymbolKind<'src>, &'p Symbol<'src>>,
}

impl<'src, 'p> SymbolTable<'src, 'p> {
    /// Creates a [SymbolTable] by walking the [Ast].
    ///
    /// Catches [SemanticError::DuplicateSymbol] errors.
    fn from_ast(ast: &'p Ast<'src>, sink: &mut ErrorSink<'src, 'p>) -> Self {
        let mut st = SymbolTable::default();
        let mut global_names = HashMap::new();

        // Insert a symbol into the global namespace, returning false if it was a duplicate.
        let mut insert_global = |sink: &mut ErrorSink<'src, 'p>, symbol: &'p Symbol<'src>| {
            if let Some(first) = global_names.insert(symbol.name, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first,
                    second: symbol,
                });
                return false;
            }
            true
        };

        // Insert a symbol into the local namespace, returning false if it was a duplicate.
        let mut insert_local = |sink: &mut ErrorSink<'src, 'p>,
                                symbol: &'p Symbol<'src>,
                                kind: LocalSymbolKind<'src>| {
            if let Some(first) = st.local.insert(kind, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first,
                    second: symbol,
                });
                return false;
            }
            true
        };

        for block in ast.blocks.inners() {
            match block {
                AstBlockKind::Model(model_block) => {
                    insert_global(sink, &model_block.symbol);
                    st.models.insert(model_block.symbol.name, model_block);

                    for symbol in model_block.blocks.iter().flat_map(|b| b.inner.symbols()) {
                        insert_local(
                            sink,
                            symbol,
                            LocalSymbolKind::ModelField {
                                model: model_block.symbol.name,
                                name: symbol.name,
                            },
                        );
                    }

                    for arg in model_block.shard_args.iter().flatten() {
                        insert_local(
                            sink,
                            arg,
                            LocalSymbolKind::ModelField {
                                model: model_block.symbol.name,
                                name: arg.name,
                            },
                        );
                    }
                }
                AstBlockKind::PlainOldObject(plain_old_object_block) => {
                    insert_global(sink, &plain_old_object_block.symbol);
                    st.poos
                        .insert(plain_old_object_block.symbol.name, plain_old_object_block);

                    for field in &plain_old_object_block.fields {
                        insert_local(
                            sink,
                            field,
                            LocalSymbolKind::PlainOldObjectField {
                                poo: plain_old_object_block.symbol.name,
                                name: field.name,
                            },
                        );
                    }
                }
                AstBlockKind::Api(api_block) => {
                    st.apis.push(api_block);
                    for method in api_block.methods.inners() {
                        insert_local(
                            sink,
                            &method.symbol,
                            LocalSymbolKind::ApiMethodDecl {
                                namespace: api_block.symbol.name,
                                name: method.symbol.name,
                            },
                        );

                        for param in method.parameters.inners() {
                            let (symbol, name) = match param {
                                ApiBlockMethodParamKind::SelfParam(symbol) => (symbol, "self"),
                                ApiBlockMethodParamKind::Param(symbol) => (symbol, symbol.name),
                            };

                            insert_local(
                                sink,
                                symbol,
                                LocalSymbolKind::ApiMethodParam {
                                    namespace: api_block.symbol.name,
                                    method: method.symbol.name,
                                    name,
                                },
                            );
                        }
                    }
                }
                AstBlockKind::DataSource(data_source_block) => {
                    st.data_sources.push(data_source_block);
                    insert_local(
                        sink,
                        &data_source_block.symbol,
                        LocalSymbolKind::DataSourceDecl {
                            model: data_source_block.model.name,
                            name: data_source_block.symbol.name,
                        },
                    );

                    for (method_name, method) in [
                        ("list", &data_source_block.list),
                        ("get", &data_source_block.get),
                    ]
                    .into_iter()
                    .filter_map(|(n, m)| m.as_ref().map(|spd| (n, &spd.inner)))
                    {
                        for param in &method.parameters {
                            insert_local(
                                sink,
                                param,
                                LocalSymbolKind::DataSourceMethodParam {
                                    data_source: data_source_block.symbol.name,
                                    method: method_name,
                                    name: param.name,
                                },
                            );
                        }
                    }
                }
                AstBlockKind::D1Binding(block) => {
                    st.d1_bindings.push(block);
                    for symbol in &block.bindings {
                        insert_global(sink, symbol);
                    }
                }
                AstBlockKind::KvBinding(block) => {
                    if insert_global(sink, &block.symbol) {
                        st.kv_bindings.insert(block.symbol.name, block);

                        for field in block.templates.inners() {
                            insert_local(
                                sink,
                                &field.symbol,
                                LocalSymbolKind::BindingTemplate {
                                    binding: block.symbol.name,
                                    name: field.symbol.name,
                                },
                            );

                            for param in &field.params {
                                insert_local(
                                    sink,
                                    param,
                                    LocalSymbolKind::BindingTemplateParam {
                                        binding: block.symbol.name,
                                        template: field.symbol.name,
                                        name: param.name,
                                    },
                                );
                            }
                        }
                    }
                }
                AstBlockKind::R2Binding(block) => {
                    if insert_global(sink, &block.symbol) {
                        st.r2_bindings.insert(block.symbol.name, block);

                        for field in block.templates.inners() {
                            insert_local(
                                sink,
                                &field.symbol,
                                LocalSymbolKind::BindingTemplate {
                                    binding: block.symbol.name,
                                    name: field.symbol.name,
                                },
                            );

                            for param in &field.params {
                                insert_local(
                                    sink,
                                    param,
                                    LocalSymbolKind::BindingTemplateParam {
                                        binding: block.symbol.name,
                                        template: field.symbol.name,
                                        name: param.name,
                                    },
                                );
                            }
                        }
                    }
                }
                AstBlockKind::DurableBinding(block) => {
                    if insert_global(sink, &block.symbol) {
                        st.durable_bindings.insert(block.symbol.name, block);

                        for field in block.shard_blocks.inners().flat_map(|s| &s.fields) {
                            insert_local(
                                sink,
                                field,
                                LocalSymbolKind::ShardField {
                                    binding: block.symbol.name,
                                    name: field.name,
                                },
                            );
                        }

                        for field in block.templates.inners() {
                            insert_local(
                                sink,
                                &field.symbol,
                                LocalSymbolKind::BindingTemplate {
                                    binding: block.symbol.name,
                                    name: field.symbol.name,
                                },
                            );

                            for param in &field.params {
                                insert_local(
                                    sink,
                                    param,
                                    LocalSymbolKind::BindingTemplateParam {
                                        binding: block.symbol.name,
                                        template: field.symbol.name,
                                        name: param.name,
                                    },
                                );
                            }
                        }
                    }
                }
                AstBlockKind::Vars(block) => {
                    st.vars_blocks.push(block);
                    for symbol in &block.vars {
                        insert_global(sink, symbol);
                    }
                }
                AstBlockKind::Inject(inject_block) => {
                    st.injects.push(inject_block);
                    for symbol in &inject_block.symbols {
                        insert_global(sink, symbol);
                    }
                }
            }
        }

        st
    }
}

/// Resolves references inside of [CidlType::Object] and [CidlType::Partial] to ensure they point to a valid model or POO.
///
/// Returns an error if the type cannot be resolved or is invalid.
fn resolve_cidl_type<'src, 'p>(
    symbol: &'p Symbol<'src>,
    cidl_type: &CidlType<'src>,
    table: &SymbolTable<'src, 'p>,
) -> Result<CidlType<'src>, SemanticError<'src, 'p>> {
    match cidl_type {
        CidlType::Object { name } => {
            if table.models.contains_key(name) || table.poos.contains_key(name) {
                return Ok(cidl_type.clone());
            }
            Err(SemanticError::UnresolvedSymbol { symbol })
        }
        CidlType::Partial { object_name } => {
            if table.models.contains_key(object_name) || table.poos.contains_key(object_name) {
                return Ok(cidl_type.clone());
            }
            Err(SemanticError::UnresolvedSymbol { symbol })
        }
        CidlType::Nullable(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::Nullable(Box::new(resolved_inner)))
        }
        CidlType::Array(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::Array(Box::new(resolved_inner)))
        }
        CidlType::KvObject(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::KvObject(Box::new(resolved_inner)))
        }
        _ => Ok(cidl_type.clone()),
    }
}

/// Resolves validators for a given symbol, returning an error if
/// any validator is invalid.
fn resolve_validator_tags<'src, 'p>(
    symbol: &'p Symbol<'src>,
) -> BatchResult<'src, 'p, Vec<Validator<'src>>> {
    use frontend::Keyword::*;

    let mut resolved = Vec::new();
    let mut sink = ErrorSink::new();

    for spd in &symbol.tags {
        let Tag::Validator {
            name: kw,
            argument: arg,
        } = &spd.inner
        else {
            continue;
        };

        let root = symbol.cidl_type.root_type();
        let type_ok = match kw {
            GreaterThan | GreaterThanOrEqual | LessThan | LessThanOrEqual | Step => {
                matches!(root, CidlType::Int | CidlType::Real)
            }
            Len | MinLen | MaxLen | Regex => matches!(root, CidlType::String),
            _ => unreachable!(
                "Tag::Validator only carries validator keywords as parsed in the frontend"
            ),
        };
        if !type_ok {
            sink.push(SemanticError::ValidatorInvalidForType {
                symbol,
                validator: spd,
            });
            continue;
        }

        let validator = match kw {
            GreaterThan => parse_number(arg, symbol, spd, &mut sink).map(Validator::GreaterThan),
            GreaterThanOrEqual => {
                parse_number(arg, symbol, spd, &mut sink).map(Validator::GreaterThanOrEqual)
            }
            LessThan => parse_number(arg, symbol, spd, &mut sink).map(Validator::LessThan),
            LessThanOrEqual => {
                parse_number(arg, symbol, spd, &mut sink).map(Validator::LessThanOrEqual)
            }
            Step => parse_i64(arg, symbol, spd, &mut sink).map(Validator::Step),
            Len => parse_usize(arg, symbol, spd, &mut sink).map(Validator::Length),
            MinLen => parse_usize(arg, symbol, spd, &mut sink).map(Validator::MinLength),
            MaxLen => parse_usize(arg, symbol, spd, &mut sink).map(Validator::MaxLength),
            Regex => match arg {
                ArgumentLiteral::Regex(s) => match regex::Regex::new(s) {
                    Ok(_) => Some(Validator::Regex(std::borrow::Cow::Borrowed(s))),
                    Err(e) => {
                        sink.push(SemanticError::ValidatorInvalidArgument {
                            symbol,
                            validator: spd,
                            reason: format!("invalid regex pattern: {e}"),
                        });
                        None
                    }
                },
                _ => {
                    sink.push(invalid_arg(
                        symbol,
                        spd,
                        "expected a regex argument (e.g. /pattern/)",
                    ));
                    None
                }
            },
            _ => unreachable!("non-validator keywords are filtered out above"),
        };

        if let Some(v) = validator {
            resolved.push(v);
        }
    }

    sink.finish()?;
    return Ok(resolved);

    fn invalid_arg<'src, 'p>(
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        reason: impl Into<std::string::String>,
    ) -> SemanticError<'src, 'p> {
        SemanticError::ValidatorInvalidArgument {
            symbol,
            validator: spd,
            reason: reason.into(),
        }
    }

    fn parse_number<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<Number> {
        match lit {
            ArgumentLiteral::Int(s) => match s.parse() {
                Ok(n) => return Some(Number::Int(n)),
                Err(_) => sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid integer"),
                )),
            },
            ArgumentLiteral::Real(s) => match s.parse() {
                Ok(n) => return Some(Number::Float(n)),
                Err(_) => sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid number"),
                )),
            },
            _ => sink.push(invalid_arg(symbol, spd, "expected a numeric argument")),
        }
        None
    }

    fn parse_usize<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<usize> {
        let ArgumentLiteral::Int(s) = lit else {
            sink.push(invalid_arg(symbol, spd, "expected an integer argument"));
            return None;
        };
        match s.parse() {
            Ok(n) => Some(n),
            Err(_) => {
                sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid non-negative integer"),
                ));
                None
            }
        }
    }

    fn parse_i64<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<i64> {
        let ArgumentLiteral::Int(s) = lit else {
            sink.push(invalid_arg(symbol, spd, "expected an integer argument"));
            return None;
        };
        match s.parse() {
            Ok(n) => Some(n),
            Err(_) => {
                sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid integer"),
                ));
                None
            }
        }
    }
}

/// Resolves the `[inject ...]` tags on a method's symbol into a deduplicated list of binding names.
///
/// Each binding name must resolve to one of:
/// - an env binding (D1 / KV / R2 / Durable Object),
/// - an env var,
/// - an `inject { ... }` block symbol.
///
/// Context entries (`Do(args)`) are skipped here; they are resolved separately.
fn resolve_injects<'src, 'p>(
    method: &'p Symbol<'src>,
    table: &SymbolTable<'src, 'p>,
    sink: &mut ErrorSink<'src, 'p>,
) -> Vec<&'src str> {
    let mut injected: Vec<&'src str> = Vec::new();

    for tag in &method.tags {
        let Tag::Inject { entries } = &tag.inner else {
            sink.push(SemanticError::TagInvalidInContext {
                tag,
                symbol: method,
            });
            continue;
        };

        for entry in entries {
            let InjectEntry::Binding(binding) = entry else {
                continue;
            };
            let name = binding.name;

            let is_d1 = table
                .d1_bindings
                .iter()
                .flat_map(|b| b.bindings.iter())
                .any(|s| s.name == name);
            let is_kv = table.kv_bindings.contains_key(name);
            let is_r2 = table.r2_bindings.contains_key(name);
            let is_durable = table.durable_bindings.contains_key(name);
            let is_env_var = table
                .vars_blocks
                .iter()
                .flat_map(|v| v.vars.iter())
                .any(|s| s.name == name);

            let is_inject_block_symbol = table
                .injects
                .iter()
                .flat_map(|i| i.symbols.iter())
                .any(|s| s.name == name);

            if !is_d1 && !is_kv && !is_r2 && !is_durable && !is_env_var && !is_inject_block_symbol {
                sink.push(SemanticError::UnresolvedSymbol { symbol: binding });
                continue;
            }

            if !injected.contains(&name) {
                injected.push(name);
            }
        }
    }

    injected
}

/// Returns if a column in a D1 model is a valid SQLite type
fn is_valid_sql_type(cidl_type: &CidlType) -> bool {
    let inner = match cidl_type {
        CidlType::Nullable(inner) => inner.as_ref(),
        other => other,
    };

    matches!(
        inner,
        CidlType::Int
            | CidlType::Real
            | CidlType::String
            | CidlType::Blob
            | CidlType::Boolean
            | CidlType::DateIso
    )
}
