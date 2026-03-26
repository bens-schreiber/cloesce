use ast::{Api, ApiMethod, CidlType, Field, HttpVerb, MediaType};
use frontend::{ApiBlock, ApiBlockMethod, ParseAst};

use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
};

#[derive(Default)]
pub struct ApiAnalysis {
    sink: ErrorSink,
}

impl ApiAnalysis {
    pub fn analyze(
        mut self,
        apis: &[ApiBlock],
        parse: &ParseAst,
        table: &SymbolTable,
    ) -> BatchResult<Vec<(String, Api)>> {
        let mut result = Vec::new();

        for api_block in apis {
            // Validate the model reference
            let namespace = match (
                table.resolve(&api_block.model, SymbolKind::ModelDecl, None),
                table.resolve(&api_block.model, SymbolKind::ServiceDecl, None),
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

            let mut api = Api {
                name: api_block.symbol.name.clone(),
                methods: Vec::new(),
            };
            for method in &api_block.methods {
                if let Some(api_method) = self.method(&namespace.name, method, parse, table) {
                    api.methods.push(api_method);
                }
            }
            result.push((namespace.name.clone(), api));
        }

        self.sink.finish()?;
        Ok(result)
    }

    fn method(
        &mut self,
        namespace: &str,
        method: &ApiBlockMethod,
        parse: &ParseAst,
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
        self.return_type(method, table);

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
            return_type: method.return_type.clone(),
            parameters_media: MediaType::default(),
            parameters,
        })
    }

    fn return_type(&mut self, method: &ApiBlockMethod, table: &SymbolTable) {
        let err = || CompilerErrorKind::ApiInvalidReturn {
            method: method.symbol.clone(),
        };

        match method.return_type.root_type() {
            CidlType::Object { name, .. } | CidlType::Partial { name, .. } => {
                let valid = table
                    .resolve(name, SymbolKind::ModelDecl, None)
                    .or_else(|| table.resolve(name, SymbolKind::PlainOldObjectDecl, None))
                    .is_some();
                ensure!(valid, self.sink, err());
            }

            CidlType::DataSource { name, .. } => {
                let valid = table.resolve(name, SymbolKind::ModelDecl, None).is_some();
                ensure!(valid, self.sink, err());
            }

            CidlType::Inject { .. } => {
                self.sink.push(err());
            }

            CidlType::Stream => {
                // Stream is only valid as bare Stream or HttpResult<Stream>
                let valid = matches!(method.return_type, CidlType::Stream)
                    || matches!(
                        &method.return_type,
                        CidlType::HttpResult(boxed) if matches!(**boxed, CidlType::Stream)
                    );
                ensure!(valid, self.sink, err());
            }

            _ => {}
        }
    }

    fn parameters(&mut self, method: &ApiBlockMethod, table: &SymbolTable) -> Vec<Field> {
        let mut params = Vec::new();

        for param in &method.parameters {
            let err = || CompilerErrorKind::ApiInvalidParam {
                method: method.symbol.clone(),
                param: param.clone(),
            };

            // DataSource parameters validated separately
            if let CidlType::DataSource { name, .. } = &param.cidl_type {
                let valid = table.resolve(name, SymbolKind::ModelDecl, None).is_some();
                ensure!(valid, self.sink, err());
                params.push(Field {
                    name: param.name.clone(),
                    cidl_type: param.cidl_type.clone(),
                });
                continue;
            }

            match param.cidl_type.root_type() {
                CidlType::Void => {
                    self.sink.push(err());
                }

                CidlType::Object { name, .. } | CidlType::Partial { name, .. } => {
                    let valid = table
                        .resolve(name, SymbolKind::ModelDecl, None)
                        .or_else(|| table.resolve(name, SymbolKind::PlainOldObjectDecl, None))
                        .is_some();
                    ensure!(valid, self.sink, err());

                    // GET requests do not support Object parameters
                    if method.http_verb == HttpVerb::Get {
                        self.sink.push(err());
                    }
                }

                CidlType::R2Object => {
                    // GET requests do not support R2Object parameters
                    if method.http_verb == HttpVerb::Get {
                        self.sink.push(err());
                    }
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
                        err()
                    );
                }

                _ => {}
            }

            params.push(Field {
                name: param.name.clone(),
                cidl_type: param.cidl_type.clone(),
            });
        }

        params
    }
}
