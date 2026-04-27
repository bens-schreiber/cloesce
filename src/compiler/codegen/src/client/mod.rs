use askama::Template;
use ast::{
    ApiMethod, CidlType, CloesceAst, HttpVerb, MediaType, Model, NavigationField,
    NavigationFieldKind, ValidatedField,
};

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};

macro_rules! cidl_type_contains {
    ($value:expr, $pattern:pat) => {{
        let mut cur = $value;

        loop {
            match cur {
                CidlType::Array(inner)
                | CidlType::Nullable(inner)
                | CidlType::HttpResult(inner)
                | CidlType::KvObject(inner)
                | CidlType::Paginated(inner) => {
                    if matches!(cur, $pattern) {
                        break true;
                    }
                    cur = inner;
                }

                other => break matches!(other, $pattern),
            }
        }
    }};
}

#[derive(Template)]
#[template(path = "client.ts.jinja", escape = "none")]
struct ClientTemplate<'src> {
    ast: &'src CloesceAst<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl ClientTemplate<'_> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.ast)
    }

    fn map_root_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty.root_type(), self.ast)
    }

    fn map_media(&self, ty: &MediaType) -> String {
        self.mapper.media_type(ty)
    }

    fn nav_type(&self, nav: &NavigationField<'_>) -> String {
        let cidl_type = match &nav.kind {
            NavigationFieldKind::OneToOne { .. } => CidlType::Object {
                name: nav.model_reference,
            },
            NavigationFieldKind::OneToMany { .. } | NavigationFieldKind::ManyToMany => {
                CidlType::array(CidlType::Object {
                    name: nav.model_reference,
                })
            }
        };
        self.mapper.cidl_type(&cidl_type, self.ast)
    }

    fn kv_inner_type<'t>(&self, ty: &'t CidlType<'t>) -> &'t CidlType<'t> {
        match ty {
            CidlType::KvObject(inner) => inner.as_ref(),
            CidlType::Paginated(inner) => match inner.as_ref() {
                CidlType::KvObject(inner) => inner.as_ref(),
                _ => ty,
            },
            _ => ty,
        }
    }

    fn object_name<'t>(&self, ty: &'t CidlType<'t>) -> &'t str {
        match ty.root_type() {
            CidlType::Inject { name } | CidlType::Object { name } => name,
            CidlType::Partial { object_name } => object_name,
            _ => panic!("Expected object type, got {:?}", ty),
        }
    }

    fn needs_constructor(&self, ty: &CidlType<'_>) -> bool {
        matches!(
            ty.root_type(),
            CidlType::Object { .. } | CidlType::Blob | CidlType::DateIso | CidlType::Stream
        )
    }

    fn has_array(&self, ty: &CidlType<'_>) -> bool {
        cidl_type_contains!(ty, CidlType::Array(_))
    }

    fn is_blob(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty.root_type(), CidlType::Blob)
    }

    fn is_object(&self, ty: &CidlType<'_>) -> bool {
        matches!(
            ty.root_type(),
            CidlType::Object { .. } | CidlType::Partial { .. }
        )
    }

    fn is_serializable(&self, ty: &CidlType<'_>) -> bool {
        !matches!(ty.root_type(), CidlType::Inject { .. } | CidlType::Env)
    }

    fn is_get_request(&self, verb: &HttpVerb) -> bool {
        matches!(verb, HttpVerb::Get)
    }

    fn is_url_param(&self, ty: &CidlType<'_>, verb: &HttpVerb) -> bool {
        matches!(verb, HttpVerb::Get) || matches!(ty, CidlType::DataSource { .. })
    }

    fn is_stream(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty.root_type(), CidlType::Stream)
    }

    fn contains_stream(&self, ty: &CidlType<'_>) -> bool {
        cidl_type_contains!(ty, CidlType::Stream)
    }

    fn is_paginated(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty, CidlType::Paginated(_))
    }

    fn is_one_to_one(&self, nav: &NavigationField<'_>) -> bool {
        matches!(nav.kind, NavigationFieldKind::OneToOne { .. })
    }

    fn is_crud_method(&self, name: &str) -> bool {
        name == "$get" || name == "$save" || name == "$list"
    }

    fn strip_ds_prefix<'a>(&self, ds_name: &str, param_name: &'a str) -> &'a str {
        let prefix = format!("{}_", ds_name);
        param_name
            .strip_prefix(prefix.as_str())
            .unwrap_or(param_name)
    }

    /// CRUD method parameters that belong to `ds_name` (prefixed `Name_`).
    fn crud_ds_params<'t>(
        &self,
        api: &'t ApiMethod<'t>,
        ds_name: &str,
    ) -> Vec<&'t ValidatedField<'t>> {
        let prefix = format!("{}_", ds_name);
        api.parameters
            .iter()
            .filter(|p| p.name.starts_with(prefix.as_str()))
            .collect()
    }

    /// Parameters that are not part of the models fields or keys
    /// but are part of the data source's GET method are considered "extra",
    /// and must be explicitly passed when calling the CRUD method
    /// (as opposed to resolving from `this`)
    fn ds_extra_params<'t>(
        &self,
        model: &'t Model<'t>,
        api: &ApiMethod<'_>,
    ) -> Vec<&'t ValidatedField<'t>> {
        let Some(ds_name) = api.data_source else {
            return vec![];
        };
        let Some(get) = model
            .data_sources
            .get(ds_name)
            .and_then(|ds| ds.get.as_ref())
        else {
            return vec![];
        };

        let fields: Vec<&str> = model
            .primary_columns
            .iter()
            .map(|c| c.field.name.as_ref())
            .chain(model.columns.iter().map(|c| c.field.name.as_ref()))
            .chain(model.key_fields.iter().map(|k| k.name.as_ref()))
            .collect();

        get.parameters
            .iter()
            .filter(|p| !fields.contains(&p.name.as_ref()))
            .collect()
    }

    fn ds_get_params<'t>(
        &self,
        model: &'t Model<'t>,
        api: &ApiMethod<'_>,
    ) -> Vec<&'t ValidatedField<'t>> {
        let Some(ds_name) = api.data_source else {
            return vec![];
        };
        model
            .data_sources
            .get(ds_name)
            .and_then(|ds| ds.get.as_ref())
            .map(|get| get.parameters.iter().collect())
            .unwrap_or_default()
    }

    fn is_ds_extra_param(&self, model: &Model<'_>, api: &ApiMethod<'_>, param_name: &str) -> bool {
        self.ds_extra_params(model, api)
            .iter()
            .any(|p| p.name == param_name)
    }
}

pub struct ClientGenerator;
impl ClientGenerator {
    pub fn generate(ast: &CloesceAst, worker_url: &str) -> String {
        let tmpl = ClientTemplate {
            ast,
            worker_url,
            mapper: TypeScriptMapper::client(),
        };
        tmpl.render().unwrap()
    }
}
