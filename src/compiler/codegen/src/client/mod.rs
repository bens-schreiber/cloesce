use askama::Template;
use ast::{
    CidlType, CloesceAst, CrudKind, DataSource, HttpVerb, MediaType, NavigationField,
    NavigationFieldKind,
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
struct ClientTemplate<'a> {
    ast: &'a CloesceAst<'a>,
    worker_url: &'a str,
    mapper: TypeScriptMapper,
}

impl<'a> ClientTemplate<'a> {
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

    fn kv_inner_type<'b>(&self, ty: &'b CidlType<'b>) -> &'b CidlType<'b> {
        match ty {
            CidlType::KvObject(inner) => inner.as_ref(),
            CidlType::Paginated(inner) => match inner.as_ref() {
                CidlType::KvObject(inner) => inner.as_ref(),
                _ => ty,
            },
            _ => ty,
        }
    }

    fn map_kv_inner_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(self.kv_inner_type(ty), self.ast)
    }

    fn object_name<'b>(&self, ty: &'b CidlType<'b>) -> &'b str {
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

    fn is_crud_get(&self, crud: &CrudKind) -> bool {
        matches!(crud, CrudKind::Get)
    }

    fn is_crud_list(&self, crud: &CrudKind) -> bool {
        matches!(crud, CrudKind::List)
    }

    fn is_crud_save(&self, crud: &CrudKind) -> bool {
        matches!(crud, CrudKind::Save)
    }

    fn crud_args_type(
        &self,
        method_name: &str,
        model_name: &str,
        data_sources: &[DataSource<'_>],
    ) -> String {
        if method_name == "$save" {
            format!("DataSources.{}.{}", model_name, method_name)
        } else {
            let parts: Vec<String> = data_sources
                .iter()
                .filter(|ds| !ds.is_internal)
                .map(|ds| format!("DataSources.{}.{}.{}", model_name, method_name, ds.name))
                .collect();
            parts.join(" | ")
        }
    }

    fn ds_kind_union(&self, data_sources: &[DataSource<'_>]) -> String {
        let parts: Vec<String> = data_sources
            .iter()
            .filter(|ds| !ds.is_internal)
            .map(|ds| format!("\"{}\"", ds.name))
            .collect();
        parts.join(" | ")
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
