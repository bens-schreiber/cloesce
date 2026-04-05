use ariadne::{Color, Label, Report};
use frontend::{FileTable, Span, fmt::DisplayError};

use crate::err::SemanticError;

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

fn display(
    error: &SemanticError<'_, '_>,
    file_table: &FileTable,
    cache: &mut impl ariadne::Cache<String>,
) {
    match error {
        SemanticError::DuplicateSymbol { first, second } => {
            let (first_path, first_range) = span_parts(&first.span, file_table);
            let (second_path, second_range) = span_parts(&second.span, file_table);
            Report::build(
                ariadne::ReportKind::Error,
                (second_path.clone(), second_range.clone()),
            )
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
            .finish()
            .write(cache, std::io::stderr())
            .ok();
        }

        SemanticError::UnresolvedSymbol { span, name } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("unresolved symbol '{}'", name))
                .with_label(
                    Label::new((path, range))
                        .with_message("this name could not be resolved")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::MissingWranglerEnvBlock => {
            eprintln!("error: project has models but no `env` block is defined");
        }

        SemanticError::D1ModelMissingD1Binding { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "model '{}' has columns but no `[use]` binding is specified",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `[use (\"<binding>\")]` tag to this model")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::D1ModelInvalidD1Binding { model, binding } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            Report::build(
                ariadne::ReportKind::Error,
                (model_path.clone(), model_range.clone()),
            )
            .with_message(format!(
                "'{binding}' is not a valid D1 binding in the env block",
            ))
            .with_label(
                Label::new((model_path, model_range))
                    .with_message(format!("used by model '{}'", model.name))
                    .with_color(Color::Yellow),
            )
            .finish()
            .write(cache, std::io::stderr())
            .ok();
        }

        SemanticError::D1ModelMultipleD1Bindings { model, bindings } => {
            let (path, range) = span_parts(&model.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "D1 model '{}' specifies multiple D1 bindings: {}",
                    model.name,
                    bindings.join(", ")
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("a model may only have one `[use]` binding")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::D1ModelMissingPrimaryKey { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "D1 model '{}' does not declare a primary key",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("add a `primary { (\"<field>\") }` block to this model")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::InvalidColumnType { column } => {
            let (path, range) = span_parts(&column.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "column '{}' has a type that is not a valid SQLite type",
                    column.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "only string, int, double, date, json, bool, and blob are allowed",
                        )
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::NullablePrimaryKey { column } => {
            let (path, range) = span_parts(&column.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "primary key column '{}' cannot be nullable",
                    column.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("remove the `?` from this column's type")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ForeignKeyReferencesSelf { model, foreign_key } => {
            let (model_path, model_range) = span_parts(&model.span, file_table);
            let (fk_path, fk_range) = span_parts(foreign_key, file_table);
            Report::build(
                ariadne::ReportKind::Error,
                (fk_path.clone(), fk_range.clone()),
            )
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
            .finish()
            .write(cache, std::io::stderr())
            .ok();
        }

        SemanticError::ForeignKeyReferencesDifferentDatabase { span, binding } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "foreign key references a model in a different database (binding '{binding}')"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("all models in a foreign key must share the same D1 binding")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ForeignKeyInvalidColumnType { span, field } => {
            let (path, range) = span_parts(span, file_table);
            let (field_path, field_range) = span_parts(&field.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "foreign key references column '{}' which is not a valid SQLite type",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("foreign key columns must be a valid SQLite type")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((field_path, field_range))
                        .with_message(format!("'{}' declared here", field.name))
                        .with_color(Color::Yellow),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::InconsistentModelAdjacency {
            span,
            first_model,
            second_model,
        } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message("adjacency list references multiple models")
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "references both '{first_model}' and '{second_model}' — all entries must point to the same model"
                        ))
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ForeignKeyInconsistentFieldAdj {
            span,
            adj_count,
            field_count,
        } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message("foreign key has mismatched adjacency and field counts")
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "{adj_count} adjacent field(s) listed but {field_count} local field(s) declared"
                        ))
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::NavigationReferencesDifferentDatabase { span, binding } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "navigation property references a model in a different database (binding '{binding}')"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("navigation properties must reference models in the same D1 database")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::NavigationMissingReciprocalM2M { span } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message("many-to-many navigation property has no reciprocal `nav` on the adjacent model")
                .with_label(
                    Label::new((path, range))
                        .with_message("the adjacent model must have exactly one reciprocal many-to-many `nav`")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::NavigationAmbiguousM2M { span } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message("many-to-many navigation property has multiple reciprocal `nav`s on the adjacent model")
                .with_label(
                    Label::new((path, range))
                        .with_message("there must be exactly one reciprocal many-to-many `nav`")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::CyclicalRelationship { cycle } => {
            eprintln!(
                "error: cyclical relationship detected among: {}",
                cycle.join(" -> ")
            );
        }

        SemanticError::KvInvalidBinding { span, binding } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("'{binding}' is not a valid KV namespace binding"))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "this binding does not refer to a KV namespace in the env block",
                        )
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::R2InvalidBinding { span, binding } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("'{binding}' is not a valid R2 bucket binding"))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "this binding does not refer to an R2 bucket in the env block",
                        )
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::KvR2UnknownKeyVariable { span, variable } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "key format references unknown variable '${variable}'"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this variable is not a field or key param on the model")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::KvR2InvalidKeyFormat { span, reason } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message("invalid key format string")
                .with_label(
                    Label::new((path, range))
                        .with_message(reason.as_str())
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::KvR2InvalidField { span, field } => {
            let (path, range) = span_parts(span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("'{field}' is not a valid field on this model"))
                .with_label(
                    Label::new((path, range))
                        .with_message("this field does not exist")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::KvR2InvalidKeyParam { span, field } => {
            let (path, range) = span_parts(span, file_table);
            let (field_path, field_range) = span_parts(&field.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "key param '{}' must be of type `string`",
                    field.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("key params used in format strings must be `string`")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((field_path, field_range))
                        .with_message(format!("'{}' declared here", field.name))
                        .with_color(Color::Yellow),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::PlainOldObjectInvalidFieldType { field } => {
            let (path, range) = span_parts(&field.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
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
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::DataSourceUnknownModelReference { source } => {
            let (path, range) = span_parts(&source.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "data source '{}' references an unknown or non-model type",
                    source.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this model does not exist")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::DataSourceInvalidIncludeTreeReference {
            source,
            model,
            name,
        } => {
            let (path, range) = span_parts(&source.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "include tree references unknown name '{name}' on model '{model}'"
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "'{name}' is not a navigation property, KV, or R2 on '{model}'"
                        ))
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::DataSourceInvalidMethodParam { source, param } => {
            let (param_path, param_range) = span_parts(&param.span, file_table);
            let (source_path, source_range) = span_parts(&source.span, file_table);
            Report::build(ariadne::ReportKind::Error, (param_path.clone(), param_range.clone()))
                .with_message(format!(
                    "parameter '{}' on data source '{}' is not a valid SQLite type",
                    param.name, source.name
                ))
                .with_label(
                    Label::new((param_path, param_range))
                        .with_message("only string, int, double, date, bool, and blob are allowed as method params")
                        .with_color(Color::Red),
                )
                .with_label(
                    Label::new((source_path, source_range))
                        .with_message(format!("data source '{}' declared here", source.name))
                        .with_color(Color::Yellow),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::DataSourceUnknownSqlParam { source, name } => {
            let (path, range) = span_parts(&source.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("SQL references unknown placeholder '${name}'"))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!(
                            "'${name}' does not match any parameter on data source '{}'",
                            source.name
                        ))
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::UnsupportedCrudOperation { model } => {
            let (path, range) = span_parts(&model.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "model '{}' has a CRUD operation that is not supported for its backing store",
                    model.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this CRUD operation is not available for this model type")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ApiUnknownNamespaceReference { api } => {
            let (path, range) = span_parts(&api.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "API block '{}' references an unknown model or service",
                    api.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this model or service does not exist")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ApiStaticMethodWithDataSource { method } => {
            let (path, range) = span_parts(&method.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "API method '{}' is static but references a data source",
                    method.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("static methods cannot have a data source")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ApiUnknownDataSourceReference {
            method,
            data_source,
        } => {
            let (path, range) = span_parts(&method.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "API method '{}' references unknown data source '{data_source}'",
                    method.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message(format!("'{data_source}' is not defined on the model"))
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ApiInvalidReturn { method } => {
            let (path, range) = span_parts(&method.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!(
                    "API method '{}' has an invalid return type",
                    method.name
                ))
                .with_label(
                    Label::new((path, range))
                        .with_message("this return type is not valid for an API method")
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }

        SemanticError::ApiInvalidParam { method, param } => {
            let (param_path, param_range) = span_parts(&param.span, file_table);
            let (method_path, method_range) = span_parts(&method.span, file_table);
            Report::build(
                ariadne::ReportKind::Error,
                (param_path.clone(), param_range.clone()),
            )
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
            .finish()
            .write(cache, std::io::stderr())
            .ok();
        }

        SemanticError::ApiReservedMethod { method } => {
            let (path, range) = span_parts(&method.span, file_table);
            Report::build(ariadne::ReportKind::Error, (path.clone(), range.clone()))
                .with_message(format!("API method '{}' uses a reserved name", method.name))
                .with_label(
                    Label::new((path, range))
                        .with_message(
                            "names like `$get`, `$list`, and `$save` are reserved by the compiler",
                        )
                        .with_color(Color::Red),
                )
                .finish()
                .write(cache, std::io::stderr())
                .ok();
        }
    }
}
