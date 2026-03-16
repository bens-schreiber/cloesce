use chumsky::prelude::*;

use crate::{Extra, SymbolTable, sqlite_column_types};
use ast::{
    Binding, CidlType, D1NavigationProperty, D1NavigationPropertyKind, Field, ForeignKey,
    KvNavigationProperty, Model, R2NavigationProperty, Symbol,
};
use lexer::Token;

struct PendingForeignKey {
    to_model_name: String,
    columns: Vec<String>,
}

struct PendingNavigation {
    name: String,
    to_model: String,
    kind: PendingNavigationKind,
}

enum PendingNavigationKind {
    OneOrManyByFieldType { key_columns: Vec<String> },
    ManyToMany,
}

struct PendingField {
    name: String,
    cidl_type: CidlType,
    kv_navigation: Option<PendingKvNavigation>,
    r2_navigation: Option<PendingR2Navigation>,
}

struct PendingKvNavigation {
    namespace_binding: String,
    format: String,
}

struct PendingR2Navigation {
    bucket_binding: String,
    format: String,
}

enum ModelEntry {
    Primary(Vec<String>),
    Unique(Vec<String>),
    Foreign(PendingForeignKey),
    Nav(PendingNavigation),
    Field(PendingField),
}

/// Parses a model block of the form:
/// ```cloesce
/// @d1(optional_d1_binding)
/// model ModelName {
///     // floating primary key tag
///     [primary col1, col2, ...]
///
///     // floating unique constraint tag
///     [unique col1, col2, ...]
///
///     col1: sqlite_type
///     col2: sqlite_type
///
///     otherModel: OtherModel
///
///     // floating foreign key tag
///     [foreign col1 -> OtherModel::col1]
///
///      // floating composite foreign key tag
///     [foreign (col1, col2) -> (OtherModel::col1, OtherModel::col2)]
///     
///     // navigation tag
///     [nav otherModel -> OtherModel::col1]
///     
///     // composite navigation tag
///     [nav otherModel -> (OtherModel::col1, OtherModel::col2)]
///     
///     ...
/// }
/// ```
pub fn model_block<'t>() -> impl Parser<'t, &'t [Token], Model, Extra<'t>> {
    // Anchored D1 binding tag on the model declaration: @d1(binding_name)
    let model_level_binding = just(Token::At)
        .ignore_then(just(Token::D1))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen))
        .or_not();

    // [primary col1, col2, ...]
    let primary_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("primary".into())))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelEntry::Primary);

    // [unique col1, col2, ...]
    let unique_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("unique".into())))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelEntry::Unique);

    // [foreign col -> Other::col]
    // [foreign (col1, col2) -> (Other::col1, Other::col2)]
    let foreign_tag = {
        // OtherModel::col
        let foreign_target_column_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(column_name) => column_name })
            .map(|(target_model_name, target_column_name)| (target_model_name, target_column_name));

        // col1
        let foreign_source_column_ref = select! { Token::Ident(name) => name };

        let foreign_source_ref_list = foreign_source_column_ref
            .clone()
            .map(|column_ref| vec![column_ref])
            .or(foreign_source_column_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        let foreign_target_ref_list = foreign_target_column_ref
            .clone()
            .map(|column_ref| vec![column_ref])
            .or(foreign_target_column_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        just(Token::LBracket)
            .ignore_then(just(Token::Ident("foreign".into())))
            .ignore_then(foreign_source_ref_list)
            .then_ignore(just(Token::Arrow))
            .then(foreign_target_ref_list)
            .then_ignore(just(Token::RBracket))
            .map(|(from, to)| {
                let to_model_name = to[0].0.clone();
                ModelEntry::Foreign(PendingForeignKey {
                    to_model_name,
                    columns: from,
                })
            })
    };

    // [nav col -> Other::col]
    // [nav col -> (Other::col1, Other::col2)]
    // [nav col <> Other::otherNav]
    let nav_tag = {
        // ModelName::col
        let nav_key_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(column_name) => column_name });

        let nav_key_ref_list = nav_key_ref
            .clone()
            .map(|key_ref| vec![key_ref])
            .or(nav_key_ref
                .clone()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        let nav_arrow = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::Arrow))
            .then(nav_key_ref_list)
            .then_ignore(just(Token::RBracket))
            .map(|(name, to)| {
                let to_model = to[0].0.clone();
                let key_columns = to
                    .into_iter()
                    .map(|(model_name, column_name)| {
                        debug_assert_eq!(model_name, to_model);
                        column_name
                    })
                    .collect::<Vec<_>>();

                ModelEntry::Nav(PendingNavigation {
                    name,
                    to_model,
                    kind: PendingNavigationKind::OneOrManyByFieldType { key_columns },
                })
            });

        let nav_many_to_many = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::LAngle))
            .then_ignore(just(Token::RAngle))
            .then(nav_key_ref)
            .then_ignore(just(Token::RBracket))
            .map(|(name, (to_model, _to_navigation_name))| {
                ModelEntry::Nav(PendingNavigation {
                    name,
                    to_model,
                    kind: PendingNavigationKind::ManyToMany,
                })
            });

        nav_arrow.or(nav_many_to_many)
    };

    // otherModel: OtherModel
    // otherModels: Array<OtherModel>
    let object_name = select! { Token::Ident(name) => name };
    let object_array_generic = just(Token::Ident("Array".into()))
        .ignore_then(just(Token::LAngle))
        .ignore_then(object_name.clone())
        .then_ignore(just(Token::RAngle))
        .map(|object_name| CidlType::array(CidlType::Object(object_name)));

    let object_type = object_array_generic.or(object_name.map(CidlType::Object));

    let model_field_type = choice((sqlite_column_types(), object_type));

    // @kv(binding_name, "prefix/{id}")
    let kv_tag = just(Token::At)
        .ignore_then(just(Token::Kv))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen))
        .map(|(namespace_binding, format)| PendingKvNavigation {
            namespace_binding,
            format,
        });

    // @r2(binding_name, "prefix/{id}")
    let r2_tag = just(Token::At)
        .ignore_then(just(Token::R2))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen))
        .map(|(bucket_binding, format)| PendingR2Navigation {
            bucket_binding,
            format,
        });

    // At most one @kv and one @r2 per field, in either order.
    let anchored_field_tags = choice((
        kv_tag
            .clone()
            .then(r2_tag.clone().or_not())
            .map(|(kv, r2)| (Some(kv), r2)),
        r2_tag
            .clone()
            .then(kv_tag.clone().or_not())
            .map(|(r2, kv)| (kv, Some(r2))),
    ))
    .or_not()
    .map(|tags| tags.unwrap_or((None, None)));

    // sqlite types
    let field = anchored_field_tags
        .then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Colon))
        .then(model_field_type)
        .map(|(((kv_navigation, r2_navigation), name), cidl_type)| {
            ModelEntry::Field(PendingField {
                name,
                cidl_type,
                kv_navigation,
                r2_navigation,
            })
        });

    model_level_binding
        .then_ignore(just(Token::Model))
        .then(select! { Token::Ident(name) => name })
        .then(
            choice((primary_tag, unique_tag, foreign_tag, nav_tag, field))
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((d1_binding, name), items), e| {
            let symbol_table = e.state();
            map_model(name, d1_binding, items, symbol_table)
        })
}

fn map_model(
    model_name: String,
    d1_binding: Option<String>,
    items: Vec<ModelEntry>,
    symbol_table: &mut SymbolTable,
) -> Model {
    let model_symbol = symbol_table.intern_global(&model_name);
    let mut columns: Vec<Field> = Vec::new();
    let mut foreign_keys: Vec<ForeignKey> = Vec::new();
    let mut navigation_properties: Vec<D1NavigationProperty> = Vec::new();
    let mut kv_navigation_properties: Vec<KvNavigationProperty> = Vec::new();
    let mut r2_navigation_properties: Vec<R2NavigationProperty> = Vec::new();
    let mut pending_d1_navigation_properties: Vec<PendingNavigation> = Vec::new();
    let mut primary_key_columns: Vec<Symbol> = Vec::new();
    let mut unique_constraints: Vec<Vec<Symbol>> = Vec::new();

    for item in items {
        match item {
            ModelEntry::Primary(cols) => {
                let symbols = cols
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, &col_name));

                primary_key_columns.extend(symbols);
            }
            ModelEntry::Unique(cols) => {
                let symbols = cols
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, col_name))
                    .collect::<Vec<_>>();

                unique_constraints.push(symbols);
            }
            ModelEntry::Foreign(foreign_key) => {
                let columns = foreign_key
                    .columns
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, col_name))
                    .collect();
                let foreign_key = ForeignKey {
                    hash: 0,
                    to_model: symbol_table.intern_global(&foreign_key.to_model_name),
                    columns,
                };
                foreign_keys.push(foreign_key);
            }
            ModelEntry::Nav(nav) => {
                pending_d1_navigation_properties.push(nav);
            }
            ModelEntry::Field(field) => {
                let field_symbol = symbol_table.intern_scoped(&model_name, &field.name);
                let field_value = Field {
                    symbol: field_symbol.clone(),
                    name: field.name,
                    cidl_type: field.cidl_type,
                };

                if let Some(kv_navigation) = field.kv_navigation {
                    kv_navigation_properties.push(KvNavigationProperty {
                        namespace_binding: symbol_table
                            .intern_scoped(&model_name, &kv_navigation.namespace_binding),
                        field: Field {
                            symbol: field_symbol.clone(),
                            name: field_value.name.clone(),
                            cidl_type: field_value.cidl_type.clone(),
                        },
                        format: kv_navigation.format,
                        list_prefix: false,
                    });
                }

                if let Some(r2_navigation) = field.r2_navigation {
                    r2_navigation_properties.push(R2NavigationProperty {
                        name: field_value.name.clone(),
                        symbol: field_symbol.clone(),
                        format: r2_navigation.format,
                        bucket_binding: symbol_table
                            .intern_scoped(&model_name, &r2_navigation.bucket_binding),
                        list_prefix: false,
                    });
                }

                columns.push(field_value);
            }
        }
    }

    for nav in pending_d1_navigation_properties {
        let nav_kind = match nav.kind {
            PendingNavigationKind::ManyToMany => D1NavigationPropertyKind::ManyToMany {
                column: symbol_table.intern_scoped(&model_name, &nav.name),
            },
            PendingNavigationKind::OneOrManyByFieldType { key_columns } => {
                let key_columns = key_columns
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, col_name))
                    .collect::<Vec<_>>();

                match columns.iter().find(|field| field.name == nav.name) {
                    Some(field) if matches!(&field.cidl_type, CidlType::Array(_)) => {
                        D1NavigationPropertyKind::OneToMany {
                            columns: key_columns,
                        }
                    }
                    _ => D1NavigationPropertyKind::OneToOne {
                        columns: key_columns,
                    },
                }
            }
        };

        let nav_prop = D1NavigationProperty {
            hash: 0,
            field: symbol_table.intern_scoped(&model_name, &nav.name),
            to_model: symbol_table.intern_global(&nav.to_model),
            kind: nav_kind,
        };
        navigation_properties.push(nav_prop);
    }

    Model {
        hash: 0,
        symbol: model_symbol,
        name: model_name,
        d1_binding: d1_binding.map(|binding| Binding {
            symbol: symbol_table.intern_scoped("d1", &binding),
            name: binding,
        }),
        primary_key_columns,
        columns,
        navigation_properties,
        foreign_keys,
        unique_constraints,
        key_params: Vec::<Symbol>::new(),
        kv_navigation_properties,
        r2_navigation_properties,
    }
}

fn unquote_string_literal(literal: String) -> String {
    literal
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&literal)
        .to_owned()
}
