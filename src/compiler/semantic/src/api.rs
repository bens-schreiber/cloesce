pub mod analysis {
    use std::ops::Not;

    use crate::{
        LocalSymbolKind, SymbolTable, ensure,
        err::{ErrorSink, SemanticError},
        resolve_cidl_type, resolve_inject, resolve_validator_tags,
    };
    use frontend::{ApiBlockMethod, SpdSlice, Tag};
    use idl::{
        ApiMethod, ApiMethodParam, CidlType, DEFAULT_DATA_SOURCE_NAME, HttpVerb, MediaType, Model,
        ParamSource, ValidatedField,
    };
    use indexmap::IndexMap;

    /// Validates every API method, returning a list of Model namespaces and their associated API methods.
    pub fn analyze<'src, 'p>(
        models: &IndexMap<&'src str, Model<'src>>,
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

            let model = models
                .get(model.symbol.name)
                .expect("unresolved symbols to have been caught earlier");

            let mut methods = Vec::new();
            for api_method in api_block.methods.inners() {
                if let Some(m) = method(model, api_method, table, sink) {
                    methods.push(m);
                }
            }
            result.push((model.name, methods));
        }

        result
    }

    fn method<'src, 'p>(
        model: &Model<'src>,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<ApiMethod<'src>> {
        // Validate return type
        let (return_type, return_media) = return_type(method, table, sink);

        // Validate parameters
        let (mut parameters, parameters_media, is_static, data_source_name) =
            parameters(model.name, method, table, sink);

        let mut fields = parameters
            .iter()
            .map(|p| p.field.clone())
            .collect::<Vec<_>>();
        let (injected, durable_target) = resolve_inject(&method.injects, &mut fields, table, sink);
        for (param, field) in parameters.iter_mut().zip(fields) {
            param.field = field;
        }

        let data_source = is_static
            .not()
            .then(|| data_source_name.unwrap_or("Default"));

        // An instantiated method runs inside the Durable Object its data source's
        // `get` resolves, so it inherits that `get`'s durable target during
        // expansion.
        if let Some(ds_name) = data_source {
            let ds_is_durable = match model.data_sources.get(ds_name) {
                Some(ds) => ds.get.durable_target.is_some(),
                // The Default source isn't synthesized yet; it is durable iff the
                // model is backed by a Durable Object.
                None => model.is_durable_backed(),
            };

            if ds_is_durable && durable_target.is_some() {
                sink.push(SemanticError::ApiInjectsDurableWhenSourceInjectsDurable {
                    method: &method.symbol,
                });
            }
        }

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

    fn return_type<'src, 'p>(
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> (CidlType<'src>, MediaType) {
        let err = SemanticError::ApiInvalidReturn {
            method: &method.symbol,
        };

        let resolved_type = match resolve_cidl_type(&method.symbol, &method.symbol.cidl_type, table)
        {
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
        model_name: &'src str,
        method: &'p ApiBlockMethod<'src>,
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> (
        Vec<ApiMethodParam<'src>>,
        MediaType,
        bool,
        Option<&'src str>,
    ) {
        let mut params = Vec::new();

        let mut has_stream = false;
        let mut data_source: Option<&'src str> = None;

        let is_static = method.source.is_none();
        if let Some(source) = &method.source {
            let source_name = match &source.inner.source {
                Some(s) => s.name,
                None => DEFAULT_DATA_SOURCE_NAME,
            };
            data_source = Some(source_name);

            if let Some(named) = source
                .inner
                .source
                .as_ref()
                .filter(|s| s.name != DEFAULT_DATA_SOURCE_NAME)
            {
                let ds_exists = table.local.contains_key(&LocalSymbolKind::DataSourceDecl {
                    model: model_name,
                    name: named.name,
                });

                ensure!(
                    ds_exists,
                    sink,
                    SemanticError::ApiUnknownDataSourceReference {
                        method: &method.symbol,
                        data_source: named,
                    }
                );
            }
        }

        for param in &method.parameters {
            // Validate tags
            let mut source = ParamSource::Body;
            for tag in &param.tags {
                match &tag.inner {
                    Tag::Validator { .. } => {}
                    Tag::Header => source = ParamSource::Header,
                    _ => sink.push(SemanticError::TagInvalidInContext { tag, symbol: param }),
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
                    let required_params = method.parameters.len();

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

            params.push(ApiMethodParam {
                field: ValidatedField {
                    name: param.name.into(),
                    cidl_type: resolved_type,
                    validators,
                },
                source,
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

pub mod expansion {
    use idl::{
        ApiMethod, ApiMethodParam, CidlType, CloesceIdl, CrudKind, DataSource, HttpVerb, MediaType,
        Model, ParamSource, ValidatedField,
    };

    fn body_params<'src>(
        fields: impl IntoIterator<Item = ValidatedField<'src>>,
    ) -> Vec<ApiMethodParam<'src>> {
        fields
            .into_iter()
            .map(|field| ApiMethodParam {
                field,
                source: ParamSource::Body,
            })
            .collect()
    }

    /// Expands a [Model]'s [CrudKind]s into actual API methods on the model.
    ///
    /// Additionally, instantiated methods inherit the durable target of their data source's `get` method.
    pub fn expand(idl: &mut CloesceIdl) {
        for model in idl.models.values_mut() {
            for api in &mut model.apis {
                let Some(ds_name) = api.data_source else {
                    continue;
                };

                let Some(get_target) = model
                    .data_sources
                    .get(ds_name)
                    .and_then(|ds| ds.get.durable_target.as_ref())
                else {
                    continue;
                };

                // An instantiated method runs inside the Durable Object its data
                // source's `get` resolves, so it inherits that `get`'s durable target.
                api.durable_target = Some(get_target.clone());
            }

            let mut crud_methods = vec![];
            for crud in &model.cruds {
                crud_methods.extend(generate_crud_methods(crud, model));
            }

            model.apis.extend(crud_methods);
        }
    }

    /// Returns a list of API methods for the given [CrudKind].
    ///
    /// Each CRUD verb produces one route per DS (e.g. `$get_WithKv`, `$save_Foo`, etc).
    /// The route is named by combining the verb with the DS name, except in the case
    /// of the `Default` DS, which omits the suffix (e.g. `$get` instead of `$get_Default`).
    ///
    /// All generated methods inherit the [ApiMethod::durable_target] of their associated
    /// data source method.
    fn generate_crud_methods<'src>(crud: &CrudKind, model: &Model<'src>) -> Vec<ApiMethod<'src>> {
        let sources = model.data_sources.values().filter(|ds| !ds.is_internal);
        let format_name = |ds: &DataSource<'src>| {
            let verb = match crud {
                CrudKind::Get => "get",
                CrudKind::List => "list",
                CrudKind::Save => "save",
            };
            if ds.name == "Default" {
                format!("${verb}").into()
            } else {
                format!("${verb}_{}", ds.name).into()
            }
        };

        match crud {
            CrudKind::Get => sources
                .map(|ds| ApiMethod {
                    name: format_name(ds),
                    is_static: true,
                    data_source: None,
                    http_verb: HttpVerb::Get,
                    return_type: CidlType::Object { name: model.name },
                    return_media: MediaType::Json,
                    parameters_media: MediaType::Json,
                    parameters: body_params(ds.get.parameters.iter().map(|p| p.parameter.clone())),
                    injected: ds.get.injected.clone(),
                    durable_target: ds.get.durable_target.clone(),
                })
                .collect(),
            CrudKind::List => sources
                .map(|ds| ApiMethod {
                    name: format_name(ds),
                    is_static: true,
                    data_source: None,
                    http_verb: HttpVerb::Get,
                    return_type: CidlType::array(CidlType::Object { name: model.name }),
                    return_media: MediaType::Json,
                    parameters_media: MediaType::Json,
                    parameters: body_params(ds.list.parameters.iter().cloned()),
                    injected: ds.list.injected.clone(),
                    durable_target: ds.list.durable_target.clone(),
                })
                .collect(),
            CrudKind::Save => sources
                .map(|ds| ApiMethod {
                    name: format_name(ds),
                    is_static: true,
                    data_source: None,
                    http_verb: HttpVerb::Post,
                    return_type: CidlType::Object { name: model.name },
                    return_media: MediaType::Json,
                    parameters_media: MediaType::Json,
                    parameters: body_params(ds.save.parameters.iter().cloned()),
                    injected: ds.save.injected.clone(),
                    durable_target: ds.save.durable_target.clone(),
                })
                .collect(),
        }
    }
}
