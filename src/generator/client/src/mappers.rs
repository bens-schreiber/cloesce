use common::{CidlType, SqlType};

use crate::ClientLanguageTypeMapper;

pub struct TypeScriptMapper;
impl ClientLanguageTypeMapper for TypeScriptMapper {
    fn type_name(&self, ty: &CidlType, nullable: bool) -> String {
        let base = match ty {
            CidlType::Sql(sql_type) => match sql_type {
                SqlType::Integer => "number",
                SqlType::Real => "number",
                SqlType::Text => "string",
                SqlType::Blob => "Uint8Array",
            },
            _ => panic!("Non SQL types are not supported in the client"),
        };
        if nullable {
            format!("{} | null", base)
        } else {
            base.to_string()
        }
    }
}
