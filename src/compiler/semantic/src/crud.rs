use ast::{ApiMethod, CidlType, CloesceAst, CrudKind, HttpVerb, MediaType, ValidatedField};

pub struct CrudExpansion;
impl CrudExpansion {
    /// Expands a [Model]'s [CrudKind]s into actual API methods on the model.
    pub fn expand(ast: &mut CloesceAst) {
        for model in ast.models.values_mut() {
            let mut crud_methods = vec![];
            for crud in &model.cruds {
                let method = match crud {
                    CrudKind::Get => {
                        let mut parameters = vec![];

                        // Include all key fields
                        for field in &model.key_fields {
                            parameters.push(ValidatedField {
                                name: field.name.clone(),
                                cidl_type: CidlType::nullable(field.cidl_type.clone()),
                                validators: vec![],
                            });
                        }

                        // Include parameters from each data source, prefixed by source name
                        for (&ds_name, ds) in &model.data_sources {
                            if ds.is_internal {
                                continue;
                            }
                            if let Some(get) = &ds.get {
                                for param in &get.parameters {
                                    parameters.push(ValidatedField {
                                        name: format!("{}_{}", ds_name, param.name).into(),
                                        cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                        validators: param.validators.clone(),
                                    });
                                }
                            }
                        }

                        // Last parameter is always the data source
                        parameters.push(ValidatedField {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name,
                            },
                            validators: vec![],
                        });

                        ApiMethod {
                            name: "$get",
                            is_static: true,
                            http_verb: HttpVerb::Get,
                            return_type: CidlType::http(CidlType::Object { name: model.name }),
                            parameters,
                            parameters_media: MediaType::Json,
                            return_media: MediaType::Json,
                            data_source: None,
                        }
                    }
                    CrudKind::List => {
                        let mut parameters = vec![];

                        // Include parameters from each data source, prefixed by source name
                        for (&ds_name, ds) in &model.data_sources {
                            if ds.is_internal {
                                continue;
                            }
                            if let Some(list) = &ds.list {
                                for param in &list.parameters {
                                    parameters.push(ValidatedField {
                                        name: format!("{}_{}", ds_name, param.name).into(),
                                        cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                        validators: param.validators.clone(),
                                    });
                                }
                            }
                        }

                        // Last parameter is always the data source
                        parameters.push(ValidatedField {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name,
                            },
                            validators: vec![],
                        });

                        ApiMethod {
                            name: "$list",
                            is_static: true,
                            http_verb: HttpVerb::Get,
                            return_type: CidlType::http(CidlType::array(CidlType::Object {
                                name: model.name,
                            })),
                            parameters,
                            parameters_media: MediaType::Json,
                            return_media: MediaType::Json,
                            data_source: None,
                        }
                    }
                    CrudKind::Save => ApiMethod {
                        name: "$save",
                        is_static: true,
                        http_verb: HttpVerb::Post,
                        return_type: CidlType::http(CidlType::Object { name: model.name }),
                        parameters: vec![
                            ValidatedField {
                                name: "model".into(),
                                cidl_type: CidlType::Partial {
                                    object_name: model.name,
                                },
                                validators: vec![],
                            },
                            ValidatedField {
                                name: "__datasource".into(),
                                cidl_type: CidlType::DataSource {
                                    model_name: model.name,
                                },
                                validators: vec![],
                            },
                        ],
                        parameters_media: MediaType::Json,
                        return_media: MediaType::Json,
                        data_source: None,
                    },
                };

                crud_methods.push(method);
            }

            model.apis.extend(crud_methods);
        }
    }
}
