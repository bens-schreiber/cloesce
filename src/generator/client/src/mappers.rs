use common::CidlType;

use crate::LanguageTypeMapper;

pub struct TypeScriptMapper;
impl LanguageTypeMapper for TypeScriptMapper {
    fn type_name(&self, ty: &CidlType, nullable: bool) -> String {
        let base = match ty {
            CidlType::Integer => "number",
            CidlType::Real => "number",
            CidlType::Text => "string",
            CidlType::Blob => "Uint8Array",
        };
        if nullable {
            format!("{} | null", base)
        } else {
            base.to_string()
        }
    }
}
