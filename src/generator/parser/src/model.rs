use chumsky::prelude::*;

use crate::{Extra, SymbolTable, sqlite_column_types};
use ast::{
    Binding, CidlType, Field, ForeignKey, Model, NavigationProperty, NavigationPropertyKind, Symbol,
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
}

enum ModelEntry {
    Primary(Vec<String>),
    Foreign(PendingForeignKey),
    Nav(PendingNavigation),
    Field(PendingField),
}

/// Parses a model block of the form:
/// ```cloesce
/// [optional_d1_binding]
/// model ModelName {
///     // floating primary key tag
///     [primary col1, col2, ...]
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
    // Binding tag on the model declaration
    let model_level_binding = just(Token::LBracket)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RBracket))
        .or_not();

    // [primary col1, col2, ...]
    let primary_decorator = just(Token::LBracket)
        .ignore_then(just(Token::Ident("primary".into())))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelEntry::Primary);

    // [foreign col -> Other::col]
    // [foreign (col1, col2) -> (Other::col1, Other::col2)]
    let foreign_decorator = {
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
    let nav_decorator = {
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

    // sqlite types
    let field = select! { Token::Ident(name) => name }
        .then_ignore(just(Token::Colon))
        .then(model_field_type)
        .map(|(name, cidl_type)| ModelEntry::Field(PendingField { name, cidl_type }));

    model_level_binding
        .then_ignore(just(Token::Model))
        .then(select! { Token::Ident(name) => name })
        .then(
            choice((primary_decorator, foreign_decorator, nav_decorator, field))
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
    let mut navigation_properties: Vec<NavigationProperty> = Vec::new();
    let mut pending_navigation_properties: Vec<PendingNavigation> = Vec::new();
    let mut primary_key_columns: Vec<Symbol> = Vec::new();

    for item in items {
        match item {
            ModelEntry::Primary(cols) => {
                let symbols = cols
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, &col_name));

                primary_key_columns.extend(symbols);
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
                pending_navigation_properties.push(nav);
            }
            ModelEntry::Field(field) => {
                let field = Field {
                    symbol: symbol_table.intern_scoped(&model_name, &field.name),
                    name: field.name,
                    cidl_type: field.cidl_type,
                };
                columns.push(field);
            }
        }
    }

    for nav in pending_navigation_properties {
        let nav_kind = match nav.kind {
            PendingNavigationKind::ManyToMany => NavigationPropertyKind::ManyToMany {
                column: symbol_table.intern_scoped(&model_name, &nav.name),
            },
            PendingNavigationKind::OneOrManyByFieldType { key_columns } => {
                let key_columns = key_columns
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, col_name))
                    .collect::<Vec<_>>();

                match columns.iter().find(|field| field.name == nav.name) {
                    Some(field) if matches!(&field.cidl_type, CidlType::Array(_)) => {
                        NavigationPropertyKind::OneToMany {
                            columns: key_columns,
                        }
                    }
                    _ => NavigationPropertyKind::OneToOne {
                        columns: key_columns,
                    },
                }
            }
        };

        let nav_prop = NavigationProperty {
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
        unique_constraints: Vec::new(),
        key_params: Vec::<Symbol>::new(),
        kv_objects: Vec::new(),
        r2_objects: Vec::new(),
        cruds: Vec::new(),
    }
}
