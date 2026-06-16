use idl::{ApiMethod, CidlType, CloesceIdl, IncludeTree, MediaType};

pub trait LanguageTypeMapper {
    /// Maps a [CidlType] to a type in the target language
    fn cidl_type(&self, ty: &CidlType, idl: &CloesceIdl) -> String;

    /// Maps a [MediaType] to a type in the target language
    fn media_type(&self, ty: &MediaType) -> String;

    /// The type an injected `name` resolves to in a method's `env`.
    fn inject_type(&self, idl: &CloesceIdl, name: &str) -> String;

    /// Like [Self::inject_type], but resolves [idl::CONTEXT_INJECT_KEY] to the
    /// method's Durable Object instance type.
    fn api_injected_type(&self, idl: &CloesceIdl, api: &ApiMethod, name: &str) -> String;

    /// Converts a format string to the target languages string interpolation syntax,
    /// using the provided parameter names to identify placeholders.
    fn interpolate_format<'src>(
        &self,
        format: &str,
        param_names: impl Iterator<Item = &'src str>,
    ) -> String;

    /// Converts an [IncludeTree] to a string representation in the target language
    fn include_tree(&self, tree: &IncludeTree) -> String;

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

    fn namespace(&self, idl: &CloesceIdl, name: &str) -> String {
        if matches!(self.kind, TypeScriptMapperKind::ClientApi) {
            return name.to_string();
        }

        if idl.models.contains_key(name) {
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
        }
    }

    fn media_type(&self, ty: &MediaType) -> String {
        match ty {
            MediaType::Json => "MediaType.Json".to_string(),
            MediaType::Octet => "MediaType.Octet".to_string(),
        }
    }

    fn inject_type(&self, idl: &CloesceIdl, name: &str) -> String {
        if idl.injects.contains(&name) {
            name.to_string()
        } else {
            format!("Env[\"{name}\"]")
        }
    }

    fn api_injected_type(&self, idl: &CloesceIdl, api: &ApiMethod, name: &str) -> String {
        if name == idl::CONTEXT_INJECT_KEY {
            return api
                .durable_target
                .as_ref()
                .map(|t| t.binding.to_string())
                .unwrap();
        }
        self.inject_type(idl, name)
    }

    fn interpolate_format<'src>(
        &self,
        format: &str,
        param_names: impl Iterator<Item = &'src str>,
    ) -> String {
        let result = param_names.fold(format.to_string(), |acc, name| {
            acc.replace(&format!("{{{name}}}"), &format!("${{{name}}}"))
        });
        format!("`{result}`")
    }

    fn include_tree(&self, tree: &IncludeTree) -> String {
        serde_json::to_string(&tree).unwrap()
    }

    fn escape_string(&self, s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('`', "\\`")
            .replace("${", "\\${")
    }
}
