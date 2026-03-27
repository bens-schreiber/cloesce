use std::collections::BTreeMap;

use chumsky::prelude::*;

use ast::IncludeTree;

use crate::{
    DataSourceBlock, DataSourceBlockMethod, FileSpan, Symbol, SymbolKind,
    lexer::Token,
    parser::{Extra, cidl_type},
};

/// Parses a block of the form:
///
/// ```cloesce
/// source SourceName for ModelName {
///     include { ... }
///     sql get(ident: cidl_type, ...) { "..." }
///     sql list(ident: cidl_type, ...) { "..." }
/// }
/// ```
pub fn data_source_block<'t>() -> impl Parser<'t, &'t [Token], DataSourceBlock, Extra<'t>> {
    // ident | ident { ... }
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

    // include { ... }
    let include_tree = just(Token::Include).ignore_then(
        include_entry
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // ident: cidl_type
    let named_parameter = || {
        select! { Token::Ident(name) => name }
            .map_with(|name, e| (name, e.span()))
            .then_ignore(just(Token::Colon))
            .then(cidl_type())
            .map(|((name, span), cidl_type)| Symbol {
                name,
                span: FileSpan::from_simple_span(span),
                cidl_type,
                kind: SymbolKind::DataSourceMethodParam,
                ..Default::default()
            })
    };

    // { "..." }
    let sql_block = select! { Token::StringLit(sql) => sql }
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    // sql get(ident: cidl_type, ...) { "..." }
    let method_params = || {
        named_parameter()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // sql get(...) { ... }
    let get_method = just(Token::Sql)
        .then_ignore(just(Token::Get))
        .ignore_then(method_params())
        .then(sql_block.clone());

    // sql list(...) { ... }
    let list_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("list".into())))
        .ignore_then(
            named_parameter()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(sql_block);

    // source SourceName for ModelName { ... }
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
                let tree = IncludeTree(include_entries.into_iter().collect());
                let set_parent = |mut params: Vec<Symbol>, method: &str| -> Vec<Symbol> {
                    let parent = format!("{model}::{name}::{method}");
                    for p in &mut params {
                        p.parent_name = parent.clone();
                    }
                    params
                };
                let get = get_method.map(|(params, raw_sql)| DataSourceBlockMethod {
                    span: e.span(),
                    parameters: set_parent(params, "get"),
                    raw_sql,
                });
                let list = list_method.map(|(params, raw_sql)| DataSourceBlockMethod {
                    span: e.span(),
                    parameters: set_parent(params, "list"),
                    raw_sql,
                });

                DataSourceBlock {
                    symbol: Symbol {
                        name,
                        span: FileSpan::from_simple_span(e.span()),
                        kind: SymbolKind::DataSourceDecl,
                        parent_name: model.clone(),
                        ..Default::default()
                    },
                    model,
                    tree,
                    get,
                    list,
                }
            },
        )
}
