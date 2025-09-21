use common::CidlType;

use crate::ClientLanguageTypeMapper;

pub struct TypeScriptMapper;
impl ClientLanguageTypeMapper for TypeScriptMapper {
    fn type_name(&self, ty: &CidlType, nullable: bool) -> String {
        let base = match ty {
            CidlType::Integer => "number".to_string(),
            CidlType::Real => "number".to_string(),
            CidlType::Text => "string".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Model(name) => name.clone(),
            CidlType::Array(inner) => {
                let inner_ts = self.type_name(inner, nullable);
                format!("{}[]", inner_ts)
            }
            CidlType::HttpResult(inner) => {
                if let Some(inner) = inner.as_deref() {
                    self.type_name(inner, nullable)
                } else {
                    "void".to_string()
                }
            }
            invalid => panic!("Invalid TypeScript type, {:?}", invalid),
        };

        if nullable {
            format!("{base} | null")
        } else {
            base
        }
    }
}
