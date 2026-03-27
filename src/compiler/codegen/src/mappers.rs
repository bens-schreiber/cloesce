use std::sync::Arc;

use ast::{CidlType, CloesceAst, MediaType};
use handlebars::Handlebars;

pub fn make_mapper_helper<'a, F>(
    mapper: Arc<dyn LanguageTypeMapper + Send + Sync + 'a>,
    ast: &'a CloesceAst,
    f: F,
) -> Box<dyn handlebars::HelperDef + Send + Sync + 'a>
where
    F: Fn(serde_json::Value, &dyn LanguageTypeMapper, &CloesceAst) -> String + Send + Sync + 'a,
{
    Box::new(
        move |h: &handlebars::Helper<'_>,
              _hb: &Handlebars<'_>,
              _ctx: &handlebars::Context,
              _rc: &mut handlebars::RenderContext<'_, '_>,
              out: &mut dyn handlebars::Output| {
            let value = h.param(0).unwrap().value().clone();
            let rendered = f(value, mapper.as_ref(), ast);
            out.write(&rendered)?;
            Ok(())
        },
    )
}

pub trait LanguageTypeMapper {
    fn cidl_type(&self, ty: &CidlType, ast: &CloesceAst) -> String;
    fn media_type(&self, ty: &MediaType) -> String;
}

pub struct TypeScriptMapper {
    use_namespaces: bool,
}
impl TypeScriptMapper {
    pub fn new() -> Self {
        Self {
            use_namespaces: false,
        }
    }
    pub fn with_namespaces() -> Self {
        Self {
            use_namespaces: true,
        }
    }

    fn namespace(&self, ast: &CloesceAst, name: &str) -> String {
        if !self.use_namespaces {
            return name.to_string();
        }

        if ast.models.contains_key(name) {
            format!("Cloesce.Models.{name}")
        } else if ast.services.contains_key(name) {
            format!("Cloesce.Services.{name}")
        } else if ast.poos.contains_key(name) {
            format!("Cloesce.PlainOldObjects.{name}")
        } else {
            panic!(
                "Type {} not found in models, services, or plain old objects",
                name
            );
        }
    }
}
impl LanguageTypeMapper for TypeScriptMapper {
    fn cidl_type(&self, ty: &CidlType, ast: &CloesceAst) -> String {
        match ty {
            CidlType::Json => "unknown".to_string(),
            CidlType::Integer => "number".to_string(),
            CidlType::Double => "number".to_string(),
            CidlType::String => "string".to_string(),
            CidlType::Boolean => "boolean".to_string(),
            CidlType::DateIso => "Date".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Object { name, .. } => self.namespace(ast, name),
            CidlType::Nullable(inner) => {
                if matches!(inner.as_ref(), CidlType::Void) {
                    return "null".to_string();
                }

                let inner_ts = self.cidl_type(inner, ast);
                format!("{} | null", inner_ts)
            }
            CidlType::Array(inner) => {
                let inner_ts = self.cidl_type(inner, ast);
                format!("{}[]", inner_ts)
            }
            CidlType::HttpResult(inner) => self.cidl_type(inner, ast),
            CidlType::Void => "void".to_string(),
            CidlType::Partial { object_name, .. } => {
                format!("DeepPartial<{}>", self.namespace(ast, object_name))
            }
            CidlType::DataSource { model_name, .. } => {
                let ds = &ast
                    .models
                    .get(model_name)
                    .expect("Model to exist")
                    .data_sources;

                let joined = ds
                    .iter()
                    .filter_map(|d| (!d.is_private).then_some(format!("\"{}\"", d.name)))
                    .collect::<Vec<_>>()
                    .join(" | ");

                format!("{joined} = \"default\"")
            }
            CidlType::Stream => "Uint8Array".to_string(),
            CidlType::KvObject(inner) => {
                let inner_ts = self.cidl_type(inner, ast);
                format!("KValue<{inner_ts}>")
            }
            CidlType::Paginated(inner) => {
                let inner_ts = self.cidl_type(inner, ast);
                format!("Paginated<{inner_ts}>")
            }
            CidlType::R2Object => "R2Object".to_string(),
            CidlType::Inject { name } => self.namespace(ast, name),
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }
}
