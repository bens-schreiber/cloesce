use std::ops::Not;

use crate::{
    LocalSymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    resolve_cidl_type, resolve_validators,
};
use ast::{ApiMethod, CidlType, HttpVerb, MediaType, ValidatedField};
use frontend::{ApiBlockMethod, ApiBlockMethodParamKind, SpdSlice};

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
                (_, Some(service)) => service.symbol.name,
                _ => {
                    self.sink.push(SemanticError::ApiUnknownNamespaceReference {
                        api: &api_block.symbol,
                    });
                    continue;
                }
            };

            let mut methods = Vec::new();
            for method in api_block.methods.blocks() {
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

        let data_source = if table.models.contains_key(namespace) && !is_static {
            Some(data_source_name.unwrap_or("Default"))
        } else {
            None
        };

        Some(ApiMethod {
            name: method.symbol.name,
            is_static,
            data_source,
            http_verb: method.http_verb,
            return_media,
            return_type,
            parameters_media,
            parameters,
        })
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

        match resolved_type.root_type() {
            CidlType::Inject { .. } | CidlType::Env => {
                self.sink.push(err);
            }

            CidlType::Stream => {
                // Stream is only valid as bare Stream
                ensure!(
                    matches!(method.return_type, CidlType::Stream),
                    self.sink,
                    err
                );
            }

            _ => {}
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
        let mut data_source_symbol = None;
        let mut is_static = true;
        for param in method.parameters.blocks() {
            let param = match param {
                ApiBlockMethodParamKind::SelfParam { data_source, .. } => {
                    is_static = false;
                    data_source_symbol = data_source.clone();
                    let Some(ds) = data_source else {
                        continue;
                    };

                    // Check that the data source exists on this namespace
                    let ds_exists = table.local.contains_key(&LocalSymbolKind::DataSourceDecl {
                        model: namespace,
                        name: ds.name,
                    });

                    ensure!(
                        ds_exists,
                        self.sink,
                        SemanticError::ApiUnknownDataSourceReference {
                            method: &method.symbol,
                            data_source: ds
                        }
                    );

                    continue;
                }
                ApiBlockMethodParamKind::Field(symbol) => symbol,
            };

            let err = SemanticError::ApiInvalidParam {
                method: &method.symbol,
                param,
            };

            let resolved_type = match resolve_cidl_type(param, &param.cidl_type, table) {
                Ok(t) => t,
                Err(e) => {
                    self.sink.push(e);
                    continue;
                }
            };
            match resolved_type.root_type() {
                CidlType::Inject { .. } => {
                    // Option, Array or any other wrapper types are not allowed to wrap Inject
                    ensure!(*resolved_type.root_type() == resolved_type, self.sink, err);
                }

                CidlType::Void => {
                    self.sink.push(err);
                }

                CidlType::Object { .. } | CidlType::Partial { .. } => {
                    // GET requests do not support Object parameters
                    ensure!(
                        matches!(method.http_verb, HttpVerb::Get).not(),
                        self.sink,
                        err
                    );
                }

                CidlType::R2Object => {
                    // GET requests do not support R2Object parameters
                    ensure!(
                        matches!(method.http_verb, HttpVerb::Get).not(),
                        self.sink,
                        err
                    );
                }

                CidlType::Stream => {
                    has_stream = true;
                    let required_params = method
                        .parameters
                        .blocks()
                        .filter(|p| {
                            let ApiBlockMethodParamKind::Field(symbol) = p else {
                                return false;
                            };

                            !matches!(
                                symbol.cidl_type,
                                CidlType::Inject { .. }
                                    | CidlType::DataSource { .. }
                                    | CidlType::Env
                            )
                        })
                        .count();

                    ensure!(
                        required_params == 1 && matches!(param.cidl_type, CidlType::Stream),
                        self.sink,
                        err
                    );
                }

                _ => {}
            }

            let validators = match resolve_validators(param) {
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
            data_source_symbol.map(|s| s.name),
        )
    }
}
