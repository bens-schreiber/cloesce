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
    to_model: String,
    key_columns: Vec<String>,
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
///     col3: OtherModel
///
///     // floating foreign key tag
///     [foreign col -> OtherModel::col]
///
///      // floating composite foreign key tag
///     [foreign (col1, col2) -> (OtherModel::col1, OtherModel::col2)]
///     
///     // floating navigation tag
///     [nav col3 -> OtherModel::col1]
///     
///     // floating composite navigation tag
///     [nav col3 -> (OtherModel::col1, OtherModel::col2)]
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

    // [nav myNav -> localKey]
    // [nav myNav -> (Model::key1, key2)]
    let nav_decorator = {
        // ModelName::col (namespace required)
        let nav_key_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(column_name) => column_name });

        let nav_key_ref_list = nav_key_ref
            .clone()
            .map(|key_ref| vec![key_ref])
            .or(nav_key_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::Arrow))
            .then(nav_key_ref_list)
            .then_ignore(just(Token::RBracket))
            .map(|(_, to)| {
                let to_model = to[0].0.clone();
                let key_columns = to
                    .into_iter()
                    .map(|(model_name, column_name)| {
                        debug_assert_eq!(model_name, to_model);
                        column_name
                    })
                    .collect::<Vec<_>>();

                ModelEntry::Nav(PendingNavigation {
                    to_model,
                    key_columns,
                })
            })
    };

    let object_type = select! { Token::Ident(name) => name }
        .map(CidlType::Object)
        .then(
            just(Token::LBracket)
                .ignore_then(just(Token::RBracket))
                .or_not(),
        )
        .map(|(cidl_type, is_array)| {
            if is_array.is_some() {
                CidlType::array(cidl_type)
            } else {
                cidl_type
            }
        });

    let model_field_type = choice((sqlite_column_types(), object_type));

    // col1: sqlite_type
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
                let columns = nav
                    .key_columns
                    .iter()
                    .map(|col_name| symbol_table.intern_scoped(&model_name, col_name))
                    .collect();
                let nav_prop = NavigationProperty {
                    hash: 0,
                    to_model: symbol_table.intern_global(&nav.to_model),
                    // Assuming 1:1 for now
                    kind: NavigationPropertyKind::OneToOne { columns },
                };
                navigation_properties.push(nav_prop);
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
