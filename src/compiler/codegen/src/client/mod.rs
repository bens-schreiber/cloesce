use askama::Template;
use idl::{
    ApiMethod, CidlType, CloesceIdl, DataSourceGetMethodParam, HttpVerb, MediaType, Model,
    NavigationField, NavigationFieldKind,
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
    idl: &'src CloesceIdl<'src>,
    worker_url: &'src str,
    mapper: TypeScriptMapper,
}

impl ClientTemplate<'_> {
    fn map_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty, self.idl)
    }

    fn map_root_type(&self, ty: &CidlType<'_>) -> String {
        self.mapper.cidl_type(ty.root_type(), self.idl)
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
        self.mapper.cidl_type(&cidl_type, self.idl)
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
            CidlType::Object { name } => name,
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

    fn is_get_request(&self, verb: &HttpVerb) -> bool {
        matches!(verb, HttpVerb::Get)
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

    fn ds_get<'t>(
        &self,
        model: &'t Model<'t>,
        api: &ApiMethod<'_>,
    ) -> &'t [DataSourceGetMethodParam<'t>] {
        api.data_source
            .and_then(|n| model.data_sources.get(n))
            .and_then(|ds| ds.get.as_ref())
            .map(|g| g.parameters.as_slice())
            .unwrap_or(&[])
    }
}

pub struct ClientGenerator;
impl ClientGenerator {
    pub fn generate(idl: &CloesceIdl, worker_url: &str) -> String {
        let tmpl = ClientTemplate {
            idl,
            worker_url,
            mapper: TypeScriptMapper::client(),
        };
        tmpl.render().unwrap()
    }
}
