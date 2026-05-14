use std::ops::Not;

use crate::{
    EnvBindingKind, LocalSymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    resolve_cidl_type, resolve_validator_tags,
};
use ast::{ApiMethod, CidlType, HttpVerb, MediaType, ValidatedField};
use frontend::{ApiBlockMethod, ApiBlockMethodParamKind, SpdSlice, Tag};

#[derive(Default)]
pub struct ApiAnalysis<'src, 'p> {
    sink: ErrorSink<'src, 'p>,
}

impl<'src, 'p> ApiAnalysis<'src, 'p> {
    pub fn analyze(
        mut self,
        table: &SymbolTable<'src, 'p>,
    ) -> BatchResult<'src, 'p, Vec<(&'src str, Vec<ApiMethod<'src>>)>> {
        let mut result = Vec::new();

        for api_block in &table.apis {
            // Validate the model reference
            let namespace = match (
                table.models.get(api_block.symbol.name),
                table.services.get(api_block.symbol.name),
            ) {
                (Some(model), _) => model.symbol.name,
                (_, Some(symbol)) => symbol.name,
                _ => {
                    self.sink.push(SemanticError::ApiUnknownNamespaceReference {
                        api: &api_block.symbol,
                    });
                    continue;
                }
            };

            let mut methods = Vec::new();
            for method in api_block.methods.inners() {
                if let Some(api_method) = self.method(namespace, method, table) {
                    methods.push(api_method);
                }
            }
            result.push((namespace, methods));
        }

        self.sink.finish()?;
        Ok(result)
    }

    fn method(
        &mut self,
        namespace: &'src str,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
    ) -> Option<ApiMethod<'src>> {
        // Validate return type
        let (return_type, return_media) = self.return_type(method, table);

        // Validate parameters
        let (parameters, parameters_media, is_static, data_source_name) =
            self.parameters(namespace, method, table);

        // Validate method-level tags (only `[inject ...]` is permitted here)
        let injected = self.method_injects(method, table);

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
        })
    }

    /// Validates method-level tags. Only `[inject ...]` tags are valid here.
    /// Returns the flattened list of injected symbol names declared on this method.
    fn method_injects(
        &mut self,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
    ) -> Vec<&'src str> {
        let mut injected: Vec<&'src str> = Vec::new();

        for tag in &method.symbol.tags {
            let Tag::Inject { bindings } = &tag.inner else {
                self.sink.push(SemanticError::TagInvalidInContext {
                    tag,
                    symbol: &method.symbol,
                });
                continue;
            };

            for binding in bindings {
                let name = binding.name;

                let is_env_binding = [EnvBindingKind::D1, EnvBindingKind::Kv, EnvBindingKind::R2]
                    .iter()
                    .any(|kind| {
                        table.local.contains_key(&LocalSymbolKind::EnvBinding {
                            kind: kind.clone(),
                            name,
                        })
                    });

                let is_env_var = table.local.contains_key(&LocalSymbolKind::EnvVar(name));

                let is_inject_block_symbol = table
                    .injects
                    .iter()
                    .flat_map(|i| i.symbols.iter())
                    .any(|s| s.name == name);

                let is_service = table.services.contains_key(name);

                if !is_env_binding && !is_env_var && !is_inject_block_symbol && !is_service {
                    self.sink
                        .push(SemanticError::UnresolvedSymbol { symbol: binding });
                    continue;
                }

                if injected.contains(&name) {
                    // Duplicate within the same method, silently de-dupe.
                    continue;
                }
                injected.push(name);
            }
        }

        injected
    }

    fn return_type(
        &mut self,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
    ) -> (CidlType<'src>, MediaType) {
        let err = SemanticError::ApiInvalidReturn {
            method: &method.symbol,
        };

        let resolved_type = match resolve_cidl_type(&method.symbol, &method.return_type, table) {
            Ok(t) => t,
            Err(e) => {
                self.sink.push(e);
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
                matches!(method.return_type, CidlType::Stream),
                self.sink,
                err.clone()
            );
        }

        (CidlType::http(resolved_type), return_media)
    }

    fn parameters(
        &mut self,
        namespace: &'src str,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
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
                            self.sink.push(SemanticError::TagInvalidInContext {
                                tag,
                                symbol: self_sym,
                            });
                            continue;
                        };

                        data_source = Some(name.inner);

                        // Check that the data source exists on this namespace
                        let ds_exists =
                            table.local.contains_key(&LocalSymbolKind::DataSourceDecl {
                                model: namespace,
                                name: name.inner,
                            });

                        ensure!(
                            ds_exists,
                            self.sink,
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
                    self.sink
                        .push(SemanticError::TagInvalidInContext { tag, symbol: param });
                }
            }

            let resolved_type = match resolve_cidl_type(param, &param.cidl_type, table) {
                Ok(t) => t,
                Err(e) => {
                    self.sink.push(e);
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
                        self.sink,
                        invalid_type_err
                    );
                }

                CidlType::R2Object => {
                    // GET requests do not support R2Object parameters
                    ensure!(
                        matches!(method.http_verb, HttpVerb::Get).not(),
                        self.sink,
                        invalid_type_err
                    );
                }

                CidlType::Stream => {
                    // GET requests do not have any body, so Stream parameters
                    // cannot be used
                    ensure!(
                        matches!(method.http_verb, HttpVerb::Get).not(),
                        self.sink,
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
                        self.sink,
                        invalid_type_err
                    );
                }

                _ => {}
            }

            let validators = match resolve_validator_tags(param) {
                Ok(v) => v,
                Err(errs) => {
                    self.sink.extend(errs);
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
}
