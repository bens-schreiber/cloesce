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
                let ds = {
                    if let Some(d1_model) = ast.d1_models.get(model_name) {
                        &d1_model.data_sources
                    } else if let Some(kv_model) = ast.kv_models.get(model_name) {
                        &kv_model.data_sources
                    } else {
                        unreachable!("Model {model_name} not found for DataSource type");
                    }
                };

                let mut format_ds = ds.keys().map(|k| format!("\"{k}\"")).collect::<Vec<_>>();
                format_ds.push("\"none\"".to_string());
                format!("{} = \"none\"", format_ds.join(" |")) // default to none
            }
            CidlType::Stream => "ReadableStream<Uint8Array>".to_string(),
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
