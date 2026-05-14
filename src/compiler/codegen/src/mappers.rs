use idl::{CidlType, CloesceIdl, Field, IncludeTree, MediaType};

pub trait LanguageTypeMapper {
    /// Maps a [CidlType] to a type in the target language
    fn cidl_type(&self, ty: &CidlType, idl: &CloesceIdl) -> String;

    /// Maps a [MediaType] to a type in the target language
    fn media_type(&self, ty: &MediaType) -> String;

    /// Converts a format string to the target languages string interpolation syntax,
    /// using the provided parameters to identify placeholders
    fn interpolate_format(&self, format: &str, parameters: &[Field]) -> String;

    /// Converts an [IncludeTree] to a string representation in the target language
    fn include_tree(&self, tree: &IncludeTree) -> String;
}

pub enum TypeScriptMapperKind {
    BackendTypes,
    ClientApi,
}

pub struct TypeScriptMapper {
    kind: TypeScriptMapperKind,
}
impl TypeScriptMapper {
    pub fn backend() -> Self {
        Self {
            kind: TypeScriptMapperKind::BackendTypes,
        }
    }

    pub fn client() -> Self {
        Self {
            kind: TypeScriptMapperKind::ClientApi,
        }
    }

    fn namespace(&self, idl: &CloesceIdl, name: &str) -> String {
        if matches!(self.kind, TypeScriptMapperKind::ClientApi) {
            return name.to_string();
        }

        if idl.models.contains_key(name) || idl.services.contains_key(name) {
            format!("{name}.Self")
        } else {
            name.to_string()
        }
    }
}
impl LanguageTypeMapper for TypeScriptMapper {
    fn cidl_type(&self, ty: &CidlType, idl: &CloesceIdl) -> String {
        match ty {
            CidlType::Json => "unknown".to_string(),
            CidlType::Int | CidlType::Real => "number".to_string(),
            CidlType::String => "string".to_string(),
            CidlType::Boolean => "boolean".to_string(),
            CidlType::DateIso => "Date".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Object { name, .. } => self.namespace(idl, name),
            CidlType::Nullable(inner) => format!("{} | null", self.cidl_type(inner, idl)),
            CidlType::Array(inner) => format!("{}[]", self.cidl_type(inner, idl)),
            CidlType::Void => "void".to_string(),
            CidlType::Partial { object_name, .. } => {
                format!("DeepPartial<{}>", self.namespace(idl, object_name))
            }
            CidlType::Stream => match self.kind {
                TypeScriptMapperKind::BackendTypes => "CfReadableStream".to_string(),
                TypeScriptMapperKind::ClientApi => "Uint8Array".to_string(),
            },
            CidlType::KvObject(inner) => format!("KValue<{}>", self.cidl_type(inner, idl)),
            CidlType::Paginated(inner) => format!("Paginated<{}>", self.cidl_type(inner, idl)),
            CidlType::R2Object => match self.kind {
                TypeScriptMapperKind::BackendTypes => "R2ObjectBody".to_string(),
                TypeScriptMapperKind::ClientApi => "R2Object".to_string(),
            },
            CidlType::UnresolvedReference { name } => {
                unreachable!("references should have been resolved by this point: {name}")
            }
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }

    fn interpolate_format(&self, format: &str, parameters: &[Field]) -> String {
        let result = parameters.iter().fold(format.to_string(), |acc, field| {
            acc.replace(
                &format!("{{{}}}", field.name),
                &format!("${{{}}}", field.name),
            )
        });
        format!("`{result}`")
    }

    fn include_tree(&self, tree: &IncludeTree) -> String {
        serde_json::to_string(&tree).unwrap()
    }
}
