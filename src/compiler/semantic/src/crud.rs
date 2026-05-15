use idl::{
    ApiMethod, CidlType, CloesceIdl, CrudKind, DataSource, HttpVerb, MediaType, Model,
    ValidatedField,
};

pub struct CrudExpansion;
impl CrudExpansion {
    /// Expands a [Model]'s [CrudKind]s into actual API methods on the model.
    pub fn expand(idl: &mut CloesceIdl) {
        for model in idl.models.values_mut() {
            let mut crud_methods = vec![];
            for crud in &model.cruds {
                crud_methods.extend(Self::methods(crud, model));
            }

            model.apis.extend(crud_methods);
        }
    }

    /// Returns a list of API methods for the given [CrudKind]
    ///
    /// - [CrudKind::Get] generates a method for each public data source with a `get` block
    ///   with the name `$get_{data_source_name}`.
    ///
    /// - [CrudKind::List] generates a method for each public data source with a `list` block
    ///   with the name `$list_{data_source_name}`.
    ///
    /// - [CrudKind::Save] generates a method for each public data source with the name `$save_{data_source_name}`.
    ///
    /// The `Default` data source is treated as a special case and does not have the data source name appended to the method.
    fn methods<'src>(crud: &CrudKind, model: &Model<'src>) -> Vec<ApiMethod<'src>> {
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
                .filter(|ds| ds.get.is_some() || ds.name == "Default")
                .map(|ds| {
                    let parameters = ds
                        .get
                        .as_ref()
                        .map(|g| g.parameters.iter().map(|p| &p.parameter).cloned().collect())
                        .unwrap_or_else(Vec::new)
                        .into_iter()
                        .chain(model.key_fields.iter().cloned())
                        .collect();

                    ApiMethod {
                        name: format_name(ds),
                        is_static: true,
                        data_source: None,
                        http_verb: HttpVerb::Get,
                        return_type: CidlType::Object { name: model.name },
                        return_media: MediaType::Json,
                        parameters_media: MediaType::Json,
                        parameters,
                        injected: vec![],
                    }
                })
                .collect(),
            CrudKind::List => sources
                .filter(|ds| ds.list.is_some())
                .map(|ds| {
                    let parameters = ds.list.as_ref().unwrap().parameters.clone();

                    ApiMethod {
                        name: format_name(ds),
                        is_static: true,
                        data_source: None,
                        http_verb: HttpVerb::Get,
                        return_type: CidlType::array(CidlType::Object { name: model.name }),
                        return_media: MediaType::Json,
                        parameters_media: MediaType::Json,
                        parameters,
                        injected: vec![],
                    }
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
                    parameters: vec![ValidatedField {
                        name: "model".into(),
                        cidl_type: CidlType::Partial {
                            object_name: model.name,
                        },
                        validators: vec![],
                    }],
                    injected: vec![],
                })
                .collect(),
        }
    }
}
