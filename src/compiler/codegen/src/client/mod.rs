use std::collections::BTreeMap;

use askama::Template;
use ast::{
    ApiMethod, CidlType, CloesceAst, DataSource, HttpVerb, MediaType, Model, NavigationField,
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
    fn fmt_verb(&self, verb: &HttpVerb) -> &'static str {
        match verb {
            HttpVerb::Get => "GET",
            HttpVerb::Post => "POST",
            HttpVerb::Put => "PUT",
            HttpVerb::Patch => "PATCH",
            HttpVerb::Delete => "DELETE",
        }
    }

    fn fmt_content_type(&self, media: &MediaType) -> &'static str {
        match media {
            MediaType::Json => "application/json",
            MediaType::Octet => "application/octet-stream",
        }
    }

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
            _ => "",
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

    fn is_object_array(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty.root_type(), CidlType::Object { .. })
            && cidl_type_contains!(ty, CidlType::Array(_))
    }

    fn is_blob_array(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty.root_type(), CidlType::Blob) && cidl_type_contains!(ty, CidlType::Array(_))
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

    fn is_datasource(&self, ty: &CidlType<'_>) -> bool {
        matches!(ty, CidlType::DataSource { .. })
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

    /// Returns the un-prefixed parameter name given a data source name and a prefixed param name.
    /// e.g. ds_name="Default", param_name="Default_id" → "id"
    fn strip_ds_prefix<'a>(&self, ds_name: &str, param_name: &'a str) -> &'a str {
        let prefix = format!("{}_", ds_name);
        param_name
            .strip_prefix(prefix.as_str())
            .unwrap_or(param_name)
    }

    /// Returns the CRUD method parameters (from the ApiMethod) that belong to `ds_name`.
    /// These are params whose name starts with `"{ds_name}_"`.
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

    fn crud_args_type(
        &self,
        method_name: &str,
        model_name: &str,
        data_sources: &BTreeMap<&str, DataSource<'_>>,
    ) -> String {
        if method_name == "$save" {
            format!("DataSources.{}.{}", model_name, method_name)
        } else {
            let parts: Vec<String> = data_sources
                .values()
                .filter(|ds| !ds.is_internal)
                .map(|ds| format!("DataSources.{}.{}.{}", model_name, method_name, ds.name))
                .collect();
            parts.join(" | ")
        }
    }

    fn ds_kind_union(&self, data_sources: &BTreeMap<&str, DataSource<'_>>) -> String {
        data_sources
            .values()
            .filter(|d| !d.is_internal)
            .map(|d| format!("\"{}\"", d.name))
            .collect::<Vec<_>>()
            .join(" | ")
    }

    fn ds_get_params<'t>(
        &self,
        model: &'t Model<'t>,
        api: &ApiMethod<'_>,
    ) -> Vec<&'t ValidatedField<'t>> {
        let ds_name = match api.data_source {
            Some(name) => name,
            None => return vec![],
        };
        let ds = match model.data_sources.get(ds_name) {
            Some(ds) => ds,
            None => return vec![],
        };
        match &ds.get {
            Some(get) => get.parameters.iter().collect(),
            None => vec![],
        }
    }

    /// Returns DS get params that are NOT fields (primary columns, columns, or key_fields) of the model.
    /// These must be passed explicitly as method parameters.
    fn ds_extra_params<'t>(
        &self,
        model: &'t Model<'t>,
        api: &ApiMethod<'_>,
    ) -> Vec<&'t ValidatedField<'t>> {
        let all_field_names: std::collections::HashSet<&str> = model
            .primary_columns
            .iter()
            .map(|c| c.field.name.as_ref())
            .chain(model.columns.iter().map(|c| c.field.name.as_ref()))
            .chain(model.key_fields.iter().map(|k| k.name.as_ref()))
            .collect();

        self.ds_get_params(model, api)
            .into_iter()
            .filter(|p| !all_field_names.contains(p.name.as_ref()))
            .collect()
    }

    /// Returns true if a DS get parameter name is an "extra" param (not a field of the model).
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
