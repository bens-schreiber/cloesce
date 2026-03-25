use std::ops::Not;

use ast::{ApiMethod, CidlType, HttpVerb, MediaType, ModelApi, SymbolKind, SymbolRef, SymbolTable};
use frontend::{ApiBlock, ApiBlockMethod, ParseAst};

use crate::{
    ensure,
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
    ) -> BatchResult<Vec<(SymbolRef, ModelApi)>> {
        let mut result = Vec::new();

        for api in apis {
            // Validate the model reference
            let Some(model_sym) = table.lookup(api.model) else {
                self.sink
                    .push(CompilerErrorKind::ApiUnknownModelReference { api: api.id });
                continue;
            };

            if matches!(model_sym.kind, SymbolKind::ModelDecl).not() {
                self.sink
                    .push(CompilerErrorKind::ApiUnknownModelReference { api: api.id });
                continue;
            }

            let mut model_api = ModelApi {
                symbol: api.model,
                methods: Vec::new(),
            };
            for method in &api.methods {
                if let Some(api_method) = self.method(api.model, method, parse, table) {
                    model_api.methods.push(api_method);
                }
            }
            result.push((api.model, model_api));
        }

        self.sink.finish()?;
        Ok(result)
    }

    fn method(
        &mut self,
        model: SymbolRef,
        method: &ApiBlockMethod,
        parse: &ParseAst,
        table: &SymbolTable,
    ) -> Option<ApiMethod> {
        // Validate data source reference
        if let Some(ds) = method.data_source_name {
            ensure!(
                !method.is_static,
                self.sink,
                CompilerErrorKind::ApiStaticMethodWithDataSource { method: method.id }
            );

            // Check that the data source exists on this model
            let ds_exists = parse.sources.iter().any(|s| s.id == ds && s.model == model);

            ensure!(
                ds_exists,
                self.sink,
                CompilerErrorKind::ApiUnknownDataSourceReference {
                    method: method.id,
                    data_source: ds,
                }
            );
        }

        // Validate return type
        self.return_type(method, table);

        // Validate parameters
        let parameters = self.parameters(method, table);

        let data_source = if method.is_static {
            None
        } else {
            method.data_source_name
        };

        Some(ApiMethod {
            name: table.name(method.id).to_string(),
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
        let err = || CompilerErrorKind::ApiInvalidReturn { method: method.id };

        match method.return_type.root_type() {
            CidlType::Object(o) | CidlType::Partial(o) => {
                let valid = table.lookup(*o).is_some_and(|s| {
                    matches!(
                        s.kind,
                        SymbolKind::ModelDecl | SymbolKind::PlainOldObjectDecl
                    )
                });
                ensure!(valid, self.sink, err());
            }

            CidlType::DataSource(model_ref) => {
                let valid = table
                    .lookup(*model_ref)
                    .is_some_and(|s| matches!(s.kind, SymbolKind::ModelDecl));
                ensure!(valid, self.sink, err());
            }

            CidlType::Inject(_) => {
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

    fn parameters(&mut self, method: &ApiBlockMethod, table: &SymbolTable) -> Vec<SymbolRef> {
        let mut param_refs = Vec::new();

        for param in &method.parameters {
            let err = || CompilerErrorKind::ApiInvalidParam {
                method: method.id,
                param: param.id,
            };

            let Some(param_sym) = table.lookup(param.id) else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { symbol: param.id });
                continue;
            };

            // DataSource parameters validated separately
            if let CidlType::DataSource(model_ref) = &param_sym.cidl_type {
                let valid = table
                    .lookup(*model_ref)
                    .is_some_and(|s| matches!(s.kind, SymbolKind::ModelDecl));
                ensure!(valid, self.sink, err());
                param_refs.push(param.id);
                continue;
            }

            match param_sym.cidl_type.root_type() {
                // TODO: data sources
                CidlType::Void => {
                    self.sink.push(err());
                }

                CidlType::Object(o) | CidlType::Partial(o) => {
                    let valid = table.lookup(*o).is_some_and(|s| {
                        matches!(
                            s.kind,
                            SymbolKind::ModelDecl | SymbolKind::PlainOldObjectDecl
                        )
                    });
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
                            !matches!(p.cidl_type, CidlType::Inject(_) | CidlType::DataSource(_))
                        })
                        .count();

                    ensure!(
                        required_params == 1 && matches!(param_sym.cidl_type, CidlType::Stream),
                        self.sink,
                        err()
                    );
                }

                _ => {}
            }

            param_refs.push(param.id);
        }

        param_refs
    }
}
