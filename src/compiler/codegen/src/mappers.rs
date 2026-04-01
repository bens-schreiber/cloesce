use ast::{CidlType, CloesceAst, MediaType};

pub trait LanguageTypeMapper {
    fn cidl_type(&self, ty: &CidlType, ast: &CloesceAst) -> String;
    fn media_type(&self, ty: &MediaType) -> String;
}

pub enum TypescriptMapperKind {
    BackendTypes,
    ClientApi,
}

pub struct TypeScriptMapper {
    kind: TypescriptMapperKind,
}
impl TypeScriptMapper {
    pub fn backend() -> Self {
        Self {
            kind: TypescriptMapperKind::BackendTypes,
        }
    }

    pub fn client() -> Self {
        Self {
            kind: TypescriptMapperKind::ClientApi,
        }
    }

    fn namespace(&self, ast: &CloesceAst, name: &str) -> String {
        if matches!(self.kind, TypescriptMapperKind::ClientApi) {
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
                    .filter_map(|d| (!d.is_internal).then_some(format!("\"{}\"", d.name)))
                    .collect::<Vec<_>>()
                    .join(" | ");

                if matches!(self.kind, TypescriptMapperKind::ClientApi) {
                    format!("{joined} = \"default\"")
                } else {
                    joined
                }
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
            CidlType::Env => "Cloesce.Env".to_string(),
            CidlType::UnresolvedReference { name } => {
                unreachable!("Unresolved reference should have been resolved by this point: {name}")
            }
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }
}
