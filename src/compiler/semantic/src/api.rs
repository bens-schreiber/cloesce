use ast::{ApiMethod, CidlType, Field, HttpVerb, MediaType};
use frontend::{ApiBlockMethod, ParseAst};

use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
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
                (Some(model), _) => &model,
                (_, Some(service)) => service,
                (None, None) => {
                    self.sink
                        .push(CompilerErrorKind::ApiUnknownNamespaceReference {
                            api: api_block.symbol.clone(),
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
        // Validate data source reference
        let data_source_name = if let Some(ref ds) = method.data_source {
            ensure!(
                !method.is_static,
                self.sink,
                CompilerErrorKind::ApiStaticMethodWithDataSource {
                    method: method.symbol.clone(),
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
                CompilerErrorKind::ApiUnknownDataSourceReference {
                    method: method.symbol.clone(),
                    data_source: ds.clone(),
                }
            );

            Some(ds.clone())
        } else {
            None
        };

        // Validate return type
        let return_type = self.return_type(method, table);

        // Validate parameters
        let parameters = self.parameters(method, table);

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
            return_media: MediaType::default(),
            return_type,
            parameters_media: MediaType::default(),
            parameters,
        })
    }

    fn return_type(&mut self, method: &ApiBlockMethod, table: &SymbolTable) -> CidlType {
        let err = CompilerErrorKind::ApiInvalidReturn {
            method: method.symbol.clone(),
        };

        let resolved_type = match resolve_cidl_type(&method.symbol, &method.return_type, table) {
            Ok(t) => t,
            Err(e) => {
                self.sink.push(e);
                return CidlType::Void;
            }
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

        CidlType::http(resolved_type)
    }

    fn parameters(&mut self, method: &ApiBlockMethod, table: &SymbolTable) -> Vec<Field> {
        let mut params = Vec::new();

        for param in &method.parameters {
            let err = CompilerErrorKind::ApiInvalidParam {
                method: method.symbol.clone(),
                param: param.clone(),
            };

            let resolved_type = match resolve_cidl_type(&param, &param.cidl_type, table) {
                Ok(t) => t,
                Err(e) => {
                    self.sink.push(e);
                    continue;
                }
            };
            match resolved_type.root_type() {
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

        params
    }
}
