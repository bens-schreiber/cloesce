use ariadne::{Color, Label, Report, ReportKind};
use ast::CrudKind;
use frontend::{FileTable, Span, Spd, Tag, err::DisplayError};

use crate::Symbol;

pub type BatchResult<'src, 'p, T> = std::result::Result<T, Vec<SemanticError<'src, 'p>>>;

#[derive(Debug, Clone)]
pub enum SemanticError<'src, 'p> {
    /// A symbol was defined more than once in the same scope.
    DuplicateSymbol {
        first: &'p Symbol<'src>,
        second: &'p Symbol<'src>,
    },

    /// A symbol was referenced but not defined in any visible scope.
    UnresolvedSymbol {
        symbol: &'p Symbol<'src>,
    },

    /// A model relies on a Wrangler environment block that is not defined within the project.
    MissingWranglerEnvBlock,

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    D1ModelMissingD1Binding {
        model: &'p Symbol<'src>,
    },

    /// A model that specifies a D1 binding that does not resolve to an actual Wrangler D1 binding.
    D1ModelInvalidD1Binding {
        model: &'p Symbol<'src>,
        binding: &'p Spd<&'src str>,
    },

    D1ModelMultipleD1Bindings {
        model: &'p Symbol<'src>,
        bindings: Vec<&'p Spd<&'src str>>,
    },

    /// A model that specifies a D1 binding but does not specify a primary key.
    D1ModelMissingPrimaryKey {
        model: &'p Symbol<'src>,
    },

    /// A column in a D1 model can only be a SQLite type
    InvalidColumnType {
        column: &'p Symbol<'src>,
    },

    /// A primary key column in a D1 model cannot be nullable
    NullablePrimaryKey {
        column: &'p Symbol<'src>,
    },

    /// A foreign key in a D1 model cannot reference it's own model
    ForeignKeyReferencesSelf {
        model: &'p Symbol<'src>,
        foreign_key: &'p Symbol<'src>,
    },

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        model: &'p Symbol<'src>,
        fk_model: &'p Symbol<'src>,
        fk_binding: Option<&'p Spd<&'src str>>,
    },

    // A foreign key references a field that is not a valid SQLite type
    ForeignKeyInvalidColumnType {
        field: &'p Symbol<'src>,
    },

    /// A foreign key has a different number of adj references than local fields
    ForeignKeyInconsistentFieldAdj {
        span: Span,
        adj_count: usize,
        field_count: usize,
    },

    /// A foreign key or navigation adj list references more than one model name
    InconsistentModelAdjacency {
        first_model: &'p Symbol<'src>,
        second_model: &'p Symbol<'src>,
    },

    NavigationReferencesDifferentDatabase {
        field: &'p Symbol<'src>,
    },

    /// A many-to-many navigation property requires exactly one reciprocal M2M nav on the adjacent model, but none was found.
    NavigationMissingReciprocalM2M {
        field: &'p Symbol<'src>,
    },

    /// A many-to-many navigation property found multiple reciprocal M2M navs on the adjacent model.
    NavigationAmbiguousM2M {
        field: &'p Symbol<'src>,
    },

    CyclicalRelationship {
        cycle: Vec<&'src str>,
    },

    KeyFieldInvalidType {
        field: &'p Symbol<'src>,
    },

    /// A KV tag references an env binding that is not a KV namespace
    KvInvalidBinding {
        binding: &'p Symbol<'src>,
    },

    /// An R2 tag references an env binding that is not an R2 bucket
    R2InvalidBinding {
        binding: &'p Symbol<'src>,
    },

    /// A KV/R2 key format string references a variable that is not a field or key param on the model
    KvR2UnknownKeyVariable {
        field: &'p Symbol<'src>,
        variable: &'src str,
    },

    /// A KV/R2 key format string has invalid syntax (e.g. nested or unclosed braces)
    KvR2InvalidKeyFormat {
        field: &'p Symbol<'src>,
        reason: String,
    },

    PlainOldObjectInvalidFieldType {
        field: &'p Symbol<'src>,
    },

    /// A data source references a model that does not exist or is not a model.
    DataSourceUnknownModelReference {
        source: &'p Symbol<'src>,
    },

    /// A data source include tree references a name that is not a navigation property, KV, or R2 on the model.
    DataSourceInvalidIncludeTreeReference {
        source: &'p Symbol<'src>,
        model: &'p Symbol<'src>,
        field: &'p Symbol<'src>,
    },

    /// A data source method parameter is not a valid SQLite type.
    DataSourceInvalidMethodParam {
        source: &'p Symbol<'src>,
        param: &'p Symbol<'src>,
    },

    /// A data source method SQL references a `$name` placeholder that does not match any parameter
    /// (and is not the reserved `$include` placeholder).
    DataSourceUnknownSqlParam {
        source: &'p Symbol<'src>,
        name: String,
    },

    /// A model has a CRUD operation that is not supported for its backing store.
    UnsupportedCrudOperation {
        model: &'p Symbol<'src>,
        crud: &'p Spd<CrudKind>,
    },

    /// An API block references a model that does not exist.
    ApiUnknownNamespaceReference {
        api: &'p Symbol<'src>,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: &'p Symbol<'src>,
        data_source: &'p Spd<&'src str>,
    },

    /// An API method has an invalid return type.
    ApiInvalidReturn {
        method: &'p Symbol<'src>,
    },

    /// An API method has an invalid parameter.
    ApiInvalidParam {
        method: &'p Symbol<'src>,
        param: &'p Symbol<'src>,
    },

    ValidatorInvalidForType {
        validator: &'p Spd<Tag<'src>>,
        symbol: &'p Symbol<'src>,
    },

    ValidatorInvalidArgument {
        validator: &'p Spd<Tag<'src>>,
        symbol: &'p Symbol<'src>,
        reason: String,
    },

    InstanceTagOnNonField {
        tag: &'p Spd<Tag<'src>>,
        source: &'p Symbol<'src>,
        param: &'p Symbol<'src>,
    },

    TagInvalidInContext {
        tag: &'p Spd<Tag<'src>>,
        symbol: &'p Symbol<'src>,
    },
}

#[derive(Debug, Default)]
pub struct ErrorSink<'src, 'p> {
    pub errors: Vec<SemanticError<'src, 'p>>,
}

impl<'src, 'p> ErrorSink<'src, 'p> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, kind: SemanticError<'src, 'p>) {
        self.errors.push(kind);
    }

    pub fn drain(&mut self) -> Vec<SemanticError<'src, 'p>> {
        std::mem::take(&mut self.errors)
    }

    pub fn extend(&mut self, other: Vec<SemanticError<'src, 'p>>) {
        self.errors.extend(other);
    }

    /// Returns Err if any errors were accumulated
    pub fn finish(self) -> std::result::Result<(), Vec<SemanticError<'src, 'p>>> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(self.errors)
        }
    }
}

/// If the condition is false, pushes an error into the sink but continues execution
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $sink:expr, $kind:expr) => {
        if !$cond {
            $sink.push($kind)
        }
    };
}

impl DisplayError for SemanticError<'_, '_> {
    fn display_error(&self, file_table: &FileTable) {
        let mut cache = file_table.cache();
        display(self, file_table, &mut cache);
    }
}

fn span_parts(span: &Span, file_table: &FileTable) -> (String, std::ops::Range<usize>) {
    let (_, path) = file_table.resolve(span.context);
    (path.display().to_string(), span.start..span.end)
}

/// Returns the textual name of a validator tag, e.g. "gt" / "len" / "regex".
fn validator_name(tag: &Tag<'_>) -> &'static str {
    match tag {
        Tag::Validator { name, .. } => name.as_str(),
        _ => "<non-validator>",
    }
}

fn display(
    error: &SemanticError<'_, '_>,
    file_table: &FileTable,
    cache: &mut impl ariadne::Cache<String>,
) {
    macro_rules! report {
        ($path:expr, $range:expr) => {
            Report::build(ReportKind::Error, ($path, $range))
        };
    }

    let report = match error {
        SemanticError::DuplicateSymbol { first, second } => {
            let (first_path, first_range) = span_parts(&first.span, file_table);
            let (second_path, second_range) = span_parts(&second.span, file_table);
            report!(second_path.clone(), second_range.clone())
                .with_message(format!("'{}' is defined more than once", second.name))
                .with_label(
                    Label::new((second_path, second_range))
                        .with_message("duplicate definition here")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((first_path, first_range))
                        .with_message("first defined here")
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::UnresolvedSymbol { symbol } => {
            let (path, range) = span_parts(&symbol.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!("unresolved symbol '{}'", symbol.name))
                .with_label(
                    Label::new((path, range))
                        .with_message("this name could not be resolved")
                        .with_color(Color::Red),
                )
        }
        SemanticError::MissingWranglerEnvBlock => {
            eprintln!("error: project has models but no `env` block is defined");
            return;
        }
        SemanticError::D1ModelMissingD1Binding { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' has columns but no `[use]` binding is specified",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `[use (\"<binding>\")]` tag to this model")
                        .with_color(Color::Red),
                )
        }
        SemanticError::D1ModelInvalidD1Binding { model, binding } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let (binding_path, binding_range) = span_parts(&binding.span, file_table);
            report!(binding_path.clone(), binding_range.clone())
                .with_message(format!(
                    "'{}' is not a valid D1 binding in the env block",
                    binding.inner
                ))
                .with_label(
                    Label::new((binding_path, binding_range))
                        .with_message("this binding is not defined in the env block")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((model_path, model_range))
                        .with_message(format!("required by model '{}'", model.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::D1ModelMultipleD1Bindings { model, bindings } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let mut report = report!(model_path.clone(), model_range.clone())
                .with_message(format!(
                    "model '{}' specifies multiple D1 bindings",
                    model.name,
                ))
                .with_label(
                    Label::new((model_path, model_range))
                        .with_message("a model may only have one `[use]` binding")
                        .with_color(Color::Yellow),
                );
            for binding in bindings {
                let (b_path, b_range) = span_parts(&binding.span, file_table);
                report = report.with_label(
                    Label::new((b_path, b_range))
                        .with_message(format!("binding '{}' specified here", binding.inner))
                        .with_color(Color::Red),
                );
            }
            report
        }
        SemanticError::D1ModelMissingPrimaryKey { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "D1 model '{}' does not declare a primary key",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `primary { (\"<field>\") }` block to this model")
                        .with_color(Color::Red),
                )
        }
        SemanticError::InvalidColumnType { column } => {
            let (path, range) = span_parts(&column.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "column '{}' has a type that is not a valid SQLite type",
                    column.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "only string, int, real, date, json, bool, and blob are allowed",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::NullablePrimaryKey { column } => {
            let (path, range) = span_parts(&column.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "primary key column '{}' cannot be nullable",
                    column.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("remove the `?` from this column's type")
                        .with_color(Color::Red),
                )
        }
        SemanticError::ForeignKeyReferencesSelf { model, foreign_key } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let (fk_path, fk_range) = span_parts(&foreign_key.span, file_table);
            report!(fk_path.clone(), fk_range.clone())
                .with_message(format!(
                    "foreign key on model '{}' references its own model",
                    model.name
                ))
                .with_label(
                    Label::new((fk_path, fk_range))
                        .with_message("a foreign key cannot reference the model it is defined on")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((model_path, model_range))
                        .with_message(format!("model '{}' defined here", model.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ForeignKeyReferencesDifferentDatabase {
            model,
            fk_model,
            fk_binding,
        } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let (fk_path, fk_range) = span_parts(&fk_model.span, file_table);
            report!(fk_path.clone(), fk_range.clone())
                .with_message(format!(
                    "foreign key on model '{}' references model '{}' in a different database",
                    model.name, fk_model.name
                ))
                .with_label(
                    Label::new((fk_path, fk_range))
                        .with_message(match fk_binding {
                            Some(spd) => {
                                format!(
                                    "model '{}' belongs to binding '{}'",
                                    fk_model.name, spd.inner
                                )
                            }
                            None => format!("model '{}' has no D1 binding", fk_model.name),
                        })
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((model_path, model_range))
                        .with_message(format!("model '{}' defined here", model.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ForeignKeyInvalidColumnType { field } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "foreign key references column '{}' which is not a valid SQLite type",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("foreign key columns must be a valid SQLite type")
                        .with_color(Color::Red),
                )
        }
        SemanticError::InconsistentModelAdjacency {
            first_model,
            second_model,
        } => {
            let (first_path, first_range) = span_parts(&first_model.span, file_table);
            let (second_path, second_range) = span_parts(&second_model.span, file_table);

            report!(second_path.clone(), second_range.clone())
                .with_message("adjacency list references multiple models")
                .with_label(
                    Label::new((second_path, second_range))
                        .with_message(format!("'{}' referenced here", second_model.name))
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((first_path, first_range))
                        .with_message(format!(
                            "'{}' referenced here — all entries must point to the same model",
                            first_model.name
                        ))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ForeignKeyInconsistentFieldAdj {
            span,
            adj_count,
            field_count,
        } => {
            let (path, range) = span_parts(span, file_table);
            report!(path.clone(), range.clone())
                .with_message("foreign key has mismatched adjacency and field counts")
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "{adj_count} adjacent field(s) listed but {field_count} local field(s) declared"
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::NavigationReferencesDifferentDatabase { field: nav } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "navigation property '{}' references a model in a different database",
                    nav.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "navigation properties must reference models in the same D1 database",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::NavigationMissingReciprocalM2M { field: nav } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message("many-to-many navigation property has no reciprocal `nav` on the adjacent model")
                .with_label(
                    Label::new((path, range))
                        .with_message("the adjacent model must have exactly one reciprocal many-to-many `nav`")
                        .with_color(Color::Red),
                )
        }
        SemanticError::NavigationAmbiguousM2M { field: nav } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message("many-to-many navigation property has multiple reciprocal `nav`s on the adjacent model")
                .with_label(
                    Label::new((path, range))
                        .with_message("there must be exactly one reciprocal many-to-many `nav`")
                        .with_color(Color::Red),
                )
        }
        SemanticError::CyclicalRelationship { cycle } => {
            eprintln!(
                "error: cyclical relationship detected among: {}",
                cycle.join(" -> ")
            );
            return;
        }
        SemanticError::KvInvalidBinding { binding } => {
            let (path, range) = span_parts(&binding.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "'{}' is not a valid KV namespace binding",
                    binding.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "this binding does not refer to a KV namespace in the env block",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::R2InvalidBinding { binding } => {
            let (path, range) = span_parts(&binding.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "'{}' is not a valid R2 bucket binding",
                    binding.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "this binding does not refer to an R2 bucket in the env block",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::KvR2UnknownKeyVariable { field, variable } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "key format references unknown variable '${variable}'"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this variable is not a field or key param on the model")
                        .with_color(Color::Red),
                )
        }
        SemanticError::KvR2InvalidKeyFormat { field, reason } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message("invalid key format string")
                .with_label(
                    Label::new((path, range))
                        .with_message(reason.as_str())
                        .with_color(Color::Red),
                )
        }
        SemanticError::PlainOldObjectInvalidFieldType { field } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "field '{}' has an invalid type for a plain object",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "`stream` and `void` are not valid field types in a `poo` block",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::DataSourceUnknownModelReference { source } => {
            let (path, range) = span_parts(&source.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "data source '{}' references an unknown or non-model type",
                    source.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this model does not exist")
                        .with_color(Color::Red),
                )
        }
        SemanticError::DataSourceInvalidIncludeTreeReference {
            source,
            model,
            field,
        } => {
            let (field_path, field_range) = span_parts(&field.span, file_table);
            let (source_path, source_range) = span_parts(&source.span, file_table);
            report!(field_path.clone(), field_range.clone())
                .with_message(format!(
                    "'{}' is not a valid include on model '{}'",
                    field.name, model.name
                ))
                .with_label(
                    Label::new((field_path, field_range))
                        .with_message(format!(
                            "not a navigation property, KV, or R2 on '{}'",
                            model.name
                        ))
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((source_path, source_range))
                        .with_message(format!("data source '{}' declared here", source.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::DataSourceInvalidMethodParam { source, param } => {
            let (param_path, param_range) = span_parts(&param.span, file_table);
            let (source_path, source_range) = span_parts(&source.span, file_table);
            report!(param_path.clone(), param_range.clone())
                .with_message(format!(
                    "parameter '{}' on data source '{}' is not a valid SQLite type",
                    param.name, source.name
                ))
                .with_label(
                    Label::new((param_path, param_range))
                        .with_message("only string, int, real, date, json, bool, and blob are allowed as method params")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((source_path, source_range))
                        .with_message(format!("data source '{}' declared here", source.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::DataSourceUnknownSqlParam { source, name } => {
            let (path, range) = span_parts(&source.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!("SQL references unknown placeholder '${name}'"))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "'${name}' does not match any parameter on data source '{}'",
                            source.name
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::UnsupportedCrudOperation { model, crud } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' has unsupported CRUD operation '{:?}'",
                    model.name, crud.inner
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "the backing store for this model does not support '{:?}'",
                            crud.inner
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::ApiUnknownNamespaceReference { api } => {
            let (path, range) = span_parts(&api.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "API block '{}' references an unknown model or service",
                    api.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this model or service does not exist")
                        .with_color(Color::Red),
                )
        }
        SemanticError::ApiUnknownDataSourceReference {
            method,
            data_source,
        } => {
            let (path, range) = span_parts(&method.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "API method '{}' references unknown data source '{}'",
                    method.name, data_source.inner
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "'{}' is not defined on the model",
                            data_source.inner
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::ApiInvalidReturn { method } => {
            let (path, range) = span_parts(&method.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "API method '{}' has an invalid return type",
                    method.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this return type is not valid for an API method")
                        .with_color(Color::Red),
                )
        }
        SemanticError::ApiInvalidParam { method, param } => {
            let (param_path, param_range) = span_parts(&param.span, file_table);
            let (method_path, method_range) = span_parts(&method.span, file_table);
            report!(param_path.clone(), param_range.clone())
                .with_message(format!(
                    "parameter '{}' on API method '{}' has an invalid type",
                    param.name, method.name
                ))
                .with_label(
                    Label::new((param_path, param_range))
                        .with_message("this parameter type is not valid for an API method")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((method_path, method_range))
                        .with_message(format!("method '{}' declared here", method.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ValidatorInvalidArgument {
            validator,
            symbol,
            reason,
        } => {
            let (path, range) = span_parts(&symbol.span, file_table);
            let (v_path, v_range) = span_parts(&validator.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "invalid argument for validator `{}`",
                    validator_name(&validator.inner)
                ))
                .with_label(
                    Label::new((v_path, v_range))
                        .with_message(reason.as_str())
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((path, range))
                        .with_message("applied to this field")
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ValidatorInvalidForType { validator, symbol } => {
            let (path, range) = span_parts(&symbol.span, file_table);
            let (v_path, v_range) = span_parts(&validator.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "validator `{}` is not valid for this type",
                    validator_name(&validator.inner)
                ))
                .with_label(
                    Label::new((v_path, v_range))
                        .with_message("this validator cannot be applied to this field type")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((path, range))
                        .with_message("applied to this field")
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::TagInvalidInContext { tag, symbol } => {
            let (path, range) = span_parts(&symbol.span, file_table);
            let (t_path, t_range) = span_parts(&tag.span, file_table);
            report!(path.clone(), range.clone())
                .with_message("tag is not valid in this context")
                .with_label(
                    Label::new((t_path, t_range))
                        .with_message("this tag cannot be applied here")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((path, range))
                        .with_message("applied to this symbol")
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::KeyFieldInvalidType { field } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "key field '{}' has a type that is not a valid SQLite type",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "key fields must be a valid SQLite type (string, int, real, date, json, bool, or blob)"
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::UnknownInjectSymbol { method, binding } => {
            let (b_path, b_range) = span_parts(&binding.span, file_table);
            let (m_path, m_range) = span_parts(&method.span, file_table);
            report!(b_path.clone(), b_range.clone())
                .with_message(format!("'{}' is not an injectable symbol", binding.inner))
                .with_label(
                    Label::new((b_path, b_range))
                        .with_message(
                            "must reference an `env` binding, env var, or `inject` block symbol",
                        )
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((m_path, m_range))
                        .with_message(format!("on method '{}'", method.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::InstanceTagOnNonField { source, param, tag } => {
            let (s_path, s_range) = span_parts(&source.span, file_table);
            let (p_path, p_range) = span_parts(&param.span, file_table);
            let (t_path, t_range) = span_parts(&tag.span, file_table);
            report!(s_path.clone(), s_range.clone())
                .with_message(format!(
                    "instance tag applied to non-field symbol '{}'",
                    source.name
                ))
                .with_label(
                    Label::new((s_path, s_range))
                        .with_message("instance tags can only be applied to fields")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((p_path, p_range))
                        .with_message(format!("this parameter '{}' is not a field", param.name))
                        .with_color(Color::Yellow),
                )
                .with_label(
                    Label::new((t_path, t_range))
                        .with_message("this tag is an instance tag")
                        .with_color(Color::Blue),
                )
        }
    };

    report.finish().write(cache, std::io::stderr()).ok();
}
