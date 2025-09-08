use common::CidlType;

use crate::ClientLanguageTypeMapper;

pub struct TypeScriptMapper;
impl ClientLanguageTypeMapper for TypeScriptMapper {
    fn type_name(&self, ty: &CidlType, nullable: bool) -> String {
        let base = match ty {
            CidlType::Integer => "number",
            CidlType::Real => "number",
            CidlType::Text => "string",
            CidlType::Blob => "Uint8Array",
            _ => panic!("Non SQL types are not supported in the client"),
        };
        if nullable {
            format!("{} | null", base)
        } else {
            base.to_string()
        }
    }
}
