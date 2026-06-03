use idl::{ApiMethod, CidlType, CloesceIdl, CrudKind, DataSource, HttpVerb, MediaType, Model};

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

    /// Returns a list of API methods for the given [CrudKind].
    ///
    /// Each CRUD verb produces one route per DS (e.g. `$get_WithKv`, `$save_Foo`, etc).
    /// The route is named by combining the verb with the DS name, except in the case
    /// of the `Default` DS, which omits the suffix (e.g. `$get` instead of `$get_Default`).
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
                .map(|ds| ApiMethod {
                    name: format_name(ds),
                    is_static: true,
                    data_source: None,
                    http_verb: HttpVerb::Get,
                    return_type: CidlType::Object { name: model.name },
                    return_media: MediaType::Json,
                    parameters_media: MediaType::Json,
                    parameters: ds
                        .get
                        .parameters
                        .iter()
                        .map(|p| p.parameter.clone())
                        .collect(),
                    injected: ds.get.injected.clone(),
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
                    parameters: ds.list.parameters.clone(),
                    injected: ds.list.injected.clone(),
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
                    parameters: ds.save.parameters.clone(),
                    injected: ds.save.injected.clone(),
                })
                .collect(),
        }
    }
}
