use ariadne::{Color, Label, Report, ReportKind};
use frontend::{FileTable, Span, Spd, Tag, err::DisplayError};
use idl::CrudKind;

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

    /// Route fields and SQL blocks are mutually exclusive
    ModelMixesRoutesAndSql {
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

    /// A foreign key or navigation adj list references more than one model name
    InconsistentModelAdjacency {
        first_model: &'p Symbol<'src>,
        second_model: &'p Symbol<'src>,
    },

    /// A navigation property references a model with a different backing: a different D1
    /// database, or one side is a route model and the other is not.
    NavigationReferencesDifferentBacking {
        field: &'p Symbol<'src>,
    },

    /// A route model declares a navigation that is not a 1:1 to another route model.
    RouteNavigationInvalid {
        field: &'p Symbol<'src>,
        reason: &'static str,
    },

    /// A navigation property could not be resolved to either a 1:1 or a 1:M
    /// relationship because no matching foreign key exists.
    NavigationMissingForeignKey {
        field: &'p Symbol<'src>,
        model_reference: &'src str,
    },

    /// A navigation property mixes 1:1 entries (with a local key) and 1:M
    /// entries (without one) in the same adjacency list.
    NavigationMixedAdjacency {
        field: &'p Symbol<'src>,
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
        SemanticError::ModelMixesRoutesAndSql { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' mixes route fields with a SQLite table",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "a `route` block cannot coexist with SQL blocks or a D1 backing",
                        )
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
        SemanticError::NavigationReferencesDifferentBacking { field: nav } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "navigation property '{}' references a model with a different backing",
                    nav.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "navigation properties must reference models with the same backing \
                             (the same D1 database, or both route models)",
                        )
                        .with_color(Color::Red),
                )
        }
        SemanticError::RouteNavigationInvalid { field: nav, reason } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "navigation property '{}' is not a valid route navigation",
                    nav.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(*reason)
                        .with_color(Color::Red),
                )
        }
        SemanticError::NavigationMissingForeignKey {
            field: nav,
            model_reference,
        } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "navigation property '{}' has no matching foreign key",
                    nav.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "a 1:1 nav needs a foreign key on this model referencing '{model_reference}', \
                             and a 1:M nav needs a foreign key on '{model_reference}' referencing this model"
                        ))
                        .with_color(Color::Red),
                )
        }
        SemanticError::NavigationMixedAdjacency { field: nav } => {
            let (path, range) = span_parts(&nav.span, file_table);
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "navigation property '{}' mixes 1:1 and 1:M entries",
                    nav.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "all entries must either have a local key (1:1) or none (1:M)",
                        )
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
        SemanticError::UnsupportedCrudOperation { model, crud } => {
            let (path, range) = span_parts(&model.span, file_table);
            let op = match crud.inner {
                CrudKind::Get => "get",
                CrudKind::List => "list",
                CrudKind::Save => "save",
            };
            report!(path.clone(), range.clone())
                .with_message(format!(
                    "model '{}' does not support the `{}` CRUD operation",
                    model.name, op
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "`{op}` is not supported for this model's backing store"
                        ))
                        .with_color(Color::Red),
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
