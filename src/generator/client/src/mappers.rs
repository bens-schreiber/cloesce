use ast::{CidlType, CloesceAst, MediaType};

pub trait ClientLanguageTypeMapper {
    fn cidl_type(&self, ty: &CidlType, ast: &CloesceAst) -> String;
    fn media_type(&self, ty: &MediaType) -> String;
}

pub struct TypeScriptMapper;
impl ClientLanguageTypeMapper for TypeScriptMapper {
    fn cidl_type(&self, ty: &CidlType, ast: &CloesceAst) -> String {
        match ty {
            CidlType::JsonValue => "unknown".to_string(),
            CidlType::Integer => "number".to_string(),
            CidlType::Real => "number".to_string(),
            CidlType::Text => "string".to_string(),
            CidlType::Boolean => "boolean".to_string(),
            CidlType::DateIso => "Date".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Object(name) => name.clone(),
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
            CidlType::Partial(name) => format!("DeepPartial<{name}>"),
            CidlType::DataSource(model_name) => {
                let ds = &ast
                    .models
                    .get(model_name)
                    .expect("Model to exist")
                    .data_sources;

                let joined = ds
                    .iter()
                    .filter_map(|(k, v)| (!v.is_private).then_some(format!("\"{k}\"")))
                    .collect::<Vec<_>>()
                    .join(" | ");

                format!("{joined} = \"default\"")
            }
            CidlType::Stream => "Uint8Array".to_string(),
            CidlType::KvObject(inner) => {
                let inner_ts = self.cidl_type(inner, ast);
                format!("KValue<{inner_ts}>")
            }
            CidlType::R2Object => "R2Object".to_string(),
            _ => panic!("Invalid type {:?}", ty),
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }
}
