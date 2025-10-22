use common::{CidlType, Model};

pub trait ClientLanguageTypeMapper {
    fn type_name(&self, ty: &CidlType, model: Option<&Model>) -> String;
}

pub struct TypeScriptMapper;
impl ClientLanguageTypeMapper for TypeScriptMapper {
    fn type_name(&self, ty: &CidlType, model: Option<&Model>) -> String {
        match ty {
            CidlType::Integer => "number".to_string(),
            CidlType::Real => "number".to_string(),
            CidlType::Text => "string".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Object(name) => name.clone(),
            CidlType::Nullable(inner) => {
                if matches!(inner.as_ref(), CidlType::Void) {
                    return "null".to_string();
                }

                let inner_ts = self.type_name(inner, model);
                format!("{} | null", inner_ts)
            }
            CidlType::Array(inner) => {
                let inner_ts = self.type_name(inner, model);
                format!("{}[]", inner_ts)
            }
            CidlType::HttpResult(inner) => self.type_name(inner, model),
            CidlType::Void => "void".to_string(),
            CidlType::Partial(name) => format!("DeepPartial<{name}>"),
            CidlType::DataSource => {
                let mut ds = model
                    .unwrap()
                    .data_sources
                    .keys()
                    .map(|k| format!("\"{k}\""))
                    .collect::<Vec<_>>();

                ds.push("\"none\"".to_string());
                format!("{} = \"none\"", ds.join(" |")) // default to none
            }
            _ => panic!("Invalid type {:?}", ty),
        }
    }
}
