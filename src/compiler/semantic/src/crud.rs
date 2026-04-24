use std::collections::HashSet;

use ast::{ApiMethod, CidlType, CloesceAst, CrudKind, Field, HttpVerb, MediaType, ValidatedField};

pub struct CrudExpansion;
impl CrudExpansion {
    /// Expands a [Model]'s [CrudKind]s into actual API methods on the model.
    pub fn expand(ast: &mut CloesceAst) {
        for model in ast.models.values_mut() {
            let mut crud_methods = vec![];
            for crud in &model.cruds {
                let method = match crud {
                    CrudKind::Get => {
                        let mut seen = HashSet::new();
                        let mut parameters = vec![];

                        // Include all key fields
                        for &name in &model.key_fields {
                            if seen.insert(name.into()) {
                                parameters.push(ValidatedField {
                                    name: name.into(),
                                    cidl_type: CidlType::String,
                                    validators: todo!(),
                                });
                            }
                        }

                        // Include parameters from all data sources, ensuring no duplicates
                        for ds in model.data_sources.values() {
                            if let Some(get) = &ds.get {
                                for param in &get.parameters {
                                    if seen.insert(param.name.clone()) {
                                        parameters.push(ValidatedField {
                                            name: param.name.clone(),
                                            cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                            validators: todo!(),
                                        });
                                    }
                                }
                            }
                        }

                        // Last parameter is always the data source
                        parameters.push(ValidatedField {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name,
                            },
                            validators: todo!(),
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
                        let mut seen = HashSet::new();
                        let mut parameters = vec![];

                        // Include parameters from all data sources, ensuring no duplicates
                        for ds in model.data_sources.values() {
                            if let Some(list) = &ds.list {
                                for param in &list.parameters {
                                    if seen.insert(param.name.clone()) {
                                        parameters.push(ValidatedField {
                                            name: param.name.clone(),
                                            cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                            validators: todo!(),
                                        });
                                    }
                                }
                            }
                        }

                        // Last parameter is always the data source
                        parameters.push(ValidatedField {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name,
                            },
                            validators: todo!(),
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
                                validators: todo!(),
                            },
                            ValidatedField {
                                name: "__datasource".into(),
                                cidl_type: CidlType::DataSource {
                                    model_name: model.name,
                                },
                                validators: todo!(),
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
