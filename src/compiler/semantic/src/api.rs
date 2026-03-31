use ast::{ApiMethod, CidlType, Field, HttpVerb, MediaType};
use frontend::{ApiBlockMethod, ParseAst};

use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, CompilerError, ErrorSink},
    resolve_cidl_type,
};

#[derive(Default)]
pub struct ApiAnalysis {
    sink: ErrorSink,
}

impl ApiAnalysis {
    pub fn analyze(
        mut self,
        parse: &ParseAst,
        table: &SymbolTable,
    ) -> BatchResult<Vec<(String, Vec<ApiMethod>)>> {
        let mut result = Vec::new();

        for api_block in &parse.apis {
            // Validate the model reference
            let namespace = match (
                table.resolve(&api_block.namespace, SymbolKind::ModelDecl, None),
                table.resolve(&api_block.namespace, SymbolKind::ServiceDecl, None),
            ) {
                (Some(model), _) => model,
                (_, Some(service)) => service,
                (None, None) => {
                    self.sink.push(CompilerError::ApiUnknownNamespaceReference {
                        api: Box::new(api_block.symbol.clone()),
                    });
                    continue;
                }
            };

            let mut methods = Vec::new();
            for method in &api_block.methods {
                if let Some(api_method) = self.method(&namespace.name, parse, method, table) {
                    methods.push(api_method);
                }
            }
            result.push((namespace.name.clone(), methods));
        }

        self.sink.finish()?;
        Ok(result)
    }

    fn method(
        &mut self,
        namespace: &str,
        parse: &ParseAst,
        method: &ApiBlockMethod,
        table: &SymbolTable,
    ) -> Option<ApiMethod> {
        // Generated API methods start with a '$'
        if method.symbol.name.starts_with('$') {
            self.sink.push(CompilerError::ApiReservedMethod {
                method: Box::new(method.symbol.clone()),
            });
        }

        // Validate data source reference
        let data_source_name = if let Some(ref ds) = method.data_source {
            ensure!(
                !method.is_static,
                self.sink,
                CompilerError::ApiStaticMethodWithDataSource {
                    method: Box::new(method.symbol.clone())
                }
            );

            // Check that the data source exists on this namespace
            let ds_exists = parse
                .sources
                .iter()
                .any(|s| s.symbol.name == *ds && s.model == namespace);

            ensure!(
                ds_exists,
                self.sink,
                CompilerError::ApiUnknownDataSourceReference {
                    method: Box::new(method.symbol.clone()),
                    data_source: ds.clone(),
                }
            );

            Some(ds.clone())
        } else {
            None
        };

        // Validate return type
        let (return_type, return_media) = self.return_type(method, table);

        // Validate parameters
        let (parameters, parameters_media) = self.parameters(method, table);

        let data_source = if method.is_static {
            None
        } else {
            data_source_name
        };

        Some(ApiMethod {
            name: method.symbol.name.clone(),
            is_static: method.is_static,
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
        method: &ApiBlockMethod,
        table: &SymbolTable,
    ) -> (CidlType, MediaType) {
        let err = CompilerError::ApiInvalidReturn {
            method: Box::new(method.symbol.clone()),
        };

        let resolved_type = match resolve_cidl_type(&method.symbol, &method.return_type, table) {
            Ok(t) => t,
            Err(e) => {
                self.sink.push(e);
                return (CidlType::Void, MediaType::default());
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
        method: &ApiBlockMethod,
        table: &SymbolTable,
    ) -> (Vec<Field>, MediaType) {
        let mut params = Vec::new();

        let mut has_stream = false;
        for param in &method.parameters {
            let err = CompilerError::ApiInvalidParam {
                method: Box::new(method.symbol.clone()),
                param: Box::new(param.clone()),
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
                    ensure!(method.http_verb != HttpVerb::Get, self.sink, err);
                }

                CidlType::R2Object => {
                    // GET requests do not support R2Object parameters
                    ensure!(method.http_verb != HttpVerb::Get, self.sink, err);
                }

                CidlType::Stream => {
                    has_stream = true;
                    let required_params = method
                        .parameters
                        .iter()
                        .filter(|p| {
                            !matches!(
                                p.cidl_type,
                                CidlType::Inject { .. } | CidlType::DataSource { .. }
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

            params.push(Field {
                name: param.name.clone(),
                cidl_type: resolved_type,
            });
        }

        (
            params,
            if has_stream {
                MediaType::Octet
            } else {
                MediaType::Json
            },
        )
    }
}
