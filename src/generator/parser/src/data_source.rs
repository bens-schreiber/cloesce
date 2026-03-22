use std::collections::BTreeMap;

use chumsky::prelude::*;

use crate::{Extra, SymbolTable, cidl_type};
use ast::{CidlType, DataSource, DataSourceMethod, Field, IncludeTree};
use lexer::Token;

enum PendingSqlParam {
    Field { name: String, cidl_type: CidlType },
}

/// Parses a data source block of the form
/// ```cloesce
/// source SourceName for ModelName {
///     include { field1, field2, field3 { field4, ...} }
///
///     sql get(id: int) {
///         "SELECT * FROM ..."
///     }
///
///     sql list(id: int, offset: int, limit: int) {
///         "SELECT * FROM ..."
///     }
/// }
/// ```
pub fn data_source_block<'t>() -> impl Parser<'t, &'t [Token], DataSource, Extra<'t>> {
    let include_entry = recursive(|entry| {
        select! { Token::Ident(name) => name }
            .then(
                entry
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace))
                    .or_not(),
            )
            .map(|(name, children)| {
                let subtree = IncludeTree(
                    children
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<BTreeMap<_, _>>(),
                );
                (name, subtree)
            })
            .boxed()
    });

    let include_tree = just(Token::Include).ignore_then(
        include_entry
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    let named_parameter = || {
        select! { Token::Ident(name) => name }
            .then_ignore(just(Token::Colon))
            .then(cidl_type())
            .map(|(name, cidl_type)| PendingSqlParam::Field { name, cidl_type })
    };

    // SQL body is a quoted string literal between braces.
    let sql_block = select! { Token::StringLit(sql) => sql }
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    let method_params = || {
        named_parameter()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // can have N parameters (no self param)
    let get_method = just(Token::Sql)
        .then_ignore(just(Token::Get))
        .ignore_then(method_params())
        .then(sql_block.clone());

    let list_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("list".into())))
        .ignore_then(method_params())
        .then(sql_block);

    just(Token::Source)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::For))
        .then(select! { Token::Ident(model) => model })
        .then(
            include_tree
                .then(get_method.or_not())
                .then(list_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(
            |((name, model), ((include_entries, get_method), list_method)), e| {
                let symbol_table = e.state();
                map_source(
                    name,
                    model,
                    include_entries,
                    get_method,
                    list_method,
                    symbol_table,
                )
            },
        )
}

fn map_source(
    name: String,
    model: String,
    include_entries: Vec<(String, IncludeTree)>,
    get_method: Option<(Vec<PendingSqlParam>, String)>,
    list_method: Option<(Vec<PendingSqlParam>, String)>,
    symbol_table: &mut SymbolTable,
) -> DataSource {
    let symbol = symbol_table.intern_global(&name);
    let model_symbol = symbol_table.intern_global(&model);
    let tree = IncludeTree(include_entries.into_iter().collect());

    let get = get_method.map(|(params, raw_sql)| DataSourceMethod {
        parameters: params
            .into_iter()
            .map(|p| match p {
                PendingSqlParam::Field {
                    name: field_name,
                    cidl_type,
                } => Field {
                    symbol: symbol_table.intern_scoped(&name, &field_name),
                    name: field_name,
                    cidl_type,
                },
            })
            .collect(),
        raw_sql,
    });

    let list = list_method.map(|(params, raw_sql)| DataSourceMethod {
        parameters: params
            .into_iter()
            .map(|p| match p {
                PendingSqlParam::Field {
                    name: field_name,
                    cidl_type,
                } => Field {
                    symbol: symbol_table.intern_scoped(&name, &field_name),
                    name: field_name,
                    cidl_type,
                },
            })
            .collect(),
        raw_sql,
    });

    DataSource {
        symbol,
        model_symbol,
        name,
        tree,
        is_private: false,
        get,
        list,
    }
}
