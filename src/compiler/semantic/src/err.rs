use ariadne::{Color, Config, IndexType, Label, Report, ReportKind};
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

    /// A model with any columns or navigation properties requires a specific D1 binding to be specified.
    ModelMissingDatabaseBinding {
        model: &'p Symbol<'src>,
    },

    /// A model specifies a binding that does not resolve to a D1 or DO binding.
    ModelInvalidBinding {
        model: &'p Symbol<'src>,
        binding: &'p Symbol<'src>,
    },

    /// A model that specifies a database binding but does not specify a primary key.
    ModelMissingPrimaryKey {
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

    /// A foreign key references a model in a different database (i.e. one with a different D1 binding)
    ForeignKeyReferencesDifferentDatabase {
        model: &'p Symbol<'src>,
        fk_model: &'p Symbol<'src>,
        fk_binding: Option<&'p Symbol<'src>>,
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

    /// A navigation is missing one of its target's route fields, or a DO-KV reference
    /// is missing a shard discriminator.
    RelationMissingDiscriminator {
        field: &'p Symbol<'src>,
        missing: &'src str,
    },

    /// A navigation discriminator key omits the local field that supplies it, i.e.
    /// `Target::key` rather than `Target::key(local)`.
    RelationMissingLocalKey {
        target: &'p Symbol<'src>,
    },

    CyclicalRelationship {
        cycle: Vec<&'src str>,
    },

    /// A model's KV/R2 reference supplies a different number of args than the binding field's params.
    ArgCountMismatch {
        field: &'p Symbol<'src>,
        expected: usize,
        got: usize,
    },

    /// A model's KV/R2 reference supplies an arg whose type does not match the binding param's type.
    ArgTypeMismatch {
        field: &'p Symbol<'src>,
        arg: &'p Symbol<'src>,
    },

    /// A KV reference must name exactly one of its binding's storage templates;
    /// it named `count` (zero, or more than one).
    KvTemplateCount {
        field: &'p Symbol<'src>,
        count: usize,
    },

    /// A template format string references a variable that is not in the
    /// parameter list
    TemplateUnknownVariable {
        field: &'p Symbol<'src>,
        variable: &'src str,
    },

    /// A template format string has invalid syntax (e.g. nested or unclosed braces)
    TemplateInvalidFormat {
        field: &'p Symbol<'src>,
        reason: String,
    },

    /// Two key formats in the same namespace have overlapping prefixes, so a
    /// prefix `list` could not unambiguously distinguish them.
    KeyFormatOverlap {
        first: &'p Symbol<'src>,
        second: &'p Symbol<'src>,
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

    /// An API block references a model that does not exist.
    ApiUnknownNamespaceReference {
        api: &'p Symbol<'src>,
    },

    /// An API method references a data source that does not exist on the model.
    ApiUnknownDataSourceReference {
        method: &'p Symbol<'src>,
        data_source: &'p Symbol<'src>,
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

    ApiInjectsDurableWhenSourceInjectsDurable {
        method: &'p Symbol<'src>,
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

/// A sink for accumulating semantic errors during analysis,
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

/// Displays a [SemanticError] to stderr using [ariadne]
fn display(
    error: &SemanticError<'_, '_>,
    file_table: &FileTable,
    cache: &mut impl ariadne::Cache<String>,
) {
    macro_rules! report {
        ($path:expr, $range:expr) => {
            Report::build(ReportKind::Error, ($path, $range))
                .with_config(Config::new().with_index_type(IndexType::Byte))
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
        SemanticError::ModelMissingDatabaseBinding { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' has SQL blocks but no backing binding is specified",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `[use \"<binding>\"]` tag to this model")
                        .with_color(Color::Red),
                )
        }
        SemanticError::ModelInvalidBinding { model, binding } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let (binding_path, binding_range) = span_parts(&binding.span, file_table);
            report!(binding_path.clone(), binding_range.clone())
                .with_message(format!(
                    "'{}' is not a valid D1 or DO binding",
                    binding.name
                ))
                .with_label(
                    Label::new((binding_path, binding_range))
                        .with_message("must be declared as a top-level `d1` or `do` binding")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((model_path, model_range))
                        .with_message(format!("required by model '{}'", model.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ModelMissingPrimaryKey { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' does not declare a primary key",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `primary { ... }` block to this model")
                        .with_color(Color::Red),
                )
        }
        SemanticError::InvalidColumnType { column } => {
            let (path, range) = span_parts(&column.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!("'{}' is not a valid SQLite type", column.name))
                .with_label(
                    Label::new((path, range))
                        .with_message("allowed types: string, int, real, date, json, bool, blob")
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
                        .with_message("remove the `option` from this column's type")
                        .with_color(Color::Red),
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
                            Some(sym) => {
                                format!(
                                    "model '{}' belongs to binding '{}'",
                                    fk_model.name, sym.name
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
        SemanticError::RelationMissingDiscriminator { field, missing } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "relation '{}' is missing the discriminator '{missing}'",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "the target's '{missing}' must be supplied to construct its state"
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::RelationMissingLocalKey { target } => {
            let (path, range) = span_parts(&target.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "relation discriminator '{}' is missing a local field",
                    target.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "supply the local field that resolves it, e.g. `{}(localField)`",
                            target.name
                        ))
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
        SemanticError::TemplateUnknownVariable { field, variable } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "template format references unknown variable '${variable}'"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "this variable is not declared as a parameter in the template",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::TemplateInvalidFormat { field, reason } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message("invalid template format string")
                .with_label(
                    Label::new((path, range))
                        .with_message(reason.as_str())
                        .with_color(Color::Red),
                )
        }
        SemanticError::KeyFormatOverlap { first, second } => {
            let (first_path, first_range) = span_parts(&first.span, file_table);
            let (second_path, second_range) = span_parts(&second.span, file_table);
            report!(second_path.clone(), second_range.clone())
                .with_message(format!(
                    "key format for '{}' overlaps with '{}'",
                    second.name, first.name
                ))
                .with_label(
                    Label::new((second_path, second_range))
                        .with_message("overlapping prefix here")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((first_path, first_range))
                        .with_message("first key format here")
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ArgCountMismatch {
            field,
            expected,
            got,
        } => {
            let (path, range) = span_parts(&field.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "'{}' expects {expected} argument(s), got {got}",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!("expected {expected}, got {got}"))
                        .with_color(Color::Red),
                )
        }
        SemanticError::ArgTypeMismatch { field, arg } => {
            let (path, range) = span_parts(&field.span, file_table);
            let (arg_path, arg_range) = span_parts(&arg.span, file_table);
            report!(arg_path.clone(), arg_range.clone())
                .with_message(format!(
                    "argument '{}' has the wrong type for '{}'",
                    arg.name, field.name
                ))
                .with_label(
                    Label::new((arg_path, arg_range))
                        .with_message("type does not match the expected parameter type")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((path, range))
                        .with_message(format!("'{}' declared here", field.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::KvTemplateCount { field, count } => {
            let (path, range) = span_parts(&field.span, file_table);
            let detail = if *count == 0 {
                "no storage template is referenced".to_string()
            } else {
                format!("{count} storage templates are referenced")
            };
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "'{}' must reference exactly one storage template, but {detail}",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("a kv field needs exactly one `template(args)` reference")
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
        SemanticError::ApiUnknownNamespaceReference { api } => {
            let (path, range) = span_parts(&api.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "API block '{}' references an unknown model",
                    api.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this model does not exist")
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
                    method.name, data_source.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "'{}' is not defined on the model",
                            data_source.name
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
                        .with_message("`stream` must be the top-level return type, not wrapped")
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
                        .with_message("object, r2object, and stream parameters are not allowed on GET methods; stream must be the only non-injected parameter")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((method_path, method_range))
                        .with_message(format!("method '{}' declared here", method.name))
                        .with_color(Color::Yellow),
                )
        }
        SemanticError::ApiInjectsDurableWhenSourceInjectsDurable { method } => {
            let (method_path, method_range) = span_parts(&method.span, file_table);
            report!(method_path.clone(), method_range.clone())
                .with_message(format!(
                    "API method '{}' injects a Durable Object context but already inherits one from its data source",
                    method.name
                ))
                .with_label(
                    Label::new((method_path, method_range))
                        .with_message("an instantiated method runs inside its data source's Durable Object; remove the explicit context injection")
                        .with_color(Color::Red),
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
