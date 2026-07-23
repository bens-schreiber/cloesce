use idl::{CidlType, MediaType, TemplateSegment};

pub trait LanguageTypeMapper {
    /// Maps a [CidlType] to a type in the target language
    fn cidl_type(&self, ty: &CidlType) -> String;

    /// Maps a [MediaType] to a type in the target language
    fn media_type(&self, ty: &MediaType) -> String;

    /// Renders key [TemplateSegment]s as a string in the target language's
    /// interpolation syntax, with each `Value` placeholder interpolated by name.
    fn interpolate_segments(&self, segments: &[TemplateSegment<&str>]) -> String;

    /// Renders `text` as the body lines of a doc comment, each prefixed with `indent`
    /// and safe against terminating the comment early.
    fn doc_block(&self, text: &str, indent: &str) -> String;

    fn escape_string(&self, s: &str) -> String;
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
}
impl LanguageTypeMapper for TypeScriptMapper {
    fn cidl_type(&self, ty: &CidlType) -> String {
        match ty {
            CidlType::Json => "unknown".to_string(),
            CidlType::Int | CidlType::Real => "number".to_string(),
            CidlType::String => "string".to_string(),
            CidlType::Boolean => "boolean".to_string(),
            CidlType::DateIso => "Date".to_string(),
            CidlType::Blob => "Uint8Array".to_string(),
            CidlType::Object { name, .. } => name.to_string(),
            CidlType::Nullable(inner) => format!("{} | null", self.cidl_type(inner)),
            CidlType::Array(inner) => format!("{}[]", self.cidl_type(inner)),
            CidlType::Void => "void".to_string(),
            CidlType::Partial { object_name, .. } => {
                format!("DeepPartial<{}>", object_name)
            }
            CidlType::Stream => match self.kind {
                TypeScriptMapperKind::BackendTypes => "ReadableStream".to_string(),
                TypeScriptMapperKind::ClientApi => "Uint8Array".to_string(),
            },
            CidlType::KvObject(inner) => format!("KValue<{}>", self.cidl_type(inner)),
            CidlType::R2Object => match self.kind {
                TypeScriptMapperKind::BackendTypes => "R2ObjectBody".to_string(),
                TypeScriptMapperKind::ClientApi => "R2Object".to_string(),
            },
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }

    fn interpolate_segments(&self, segments: &[TemplateSegment<&str>]) -> String {
        let body = segments
            .iter()
            .map(|segment| match segment {
                TemplateSegment::Literal(text) => self.escape_string(text.as_ref()),
                TemplateSegment::Value(name) => format!("${{{name}}}"),
            })
            .collect::<String>();
        format!("`{body}`")
    }

    fn doc_block(&self, text: &str, indent: &str) -> String {
        text.replace("*/", "*\\/")
            .lines()
            .map(|line| format!("{indent}* {line}").trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn escape_string(&self, s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace("${", "\\${")
    }
}
