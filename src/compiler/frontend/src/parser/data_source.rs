use std::{borrow::Cow, collections::BTreeMap};

use chumsky::prelude::*;

use ast::IncludeTree;

use crate::{
    AstBlockKind, DataSourceBlock, DataSourceBlockMethod, Symbol,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
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
pub fn data_source_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
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
                (Cow::Borrowed(name), subtree)
            })
            .boxed()
    });

    // include { ... }
    let include_tree = just(Token::Ident("include")).ignore_then(
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
                span,
                cidl_type,
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
        .then_ignore(just(Token::Ident("get")))
        .ignore_then(method_params())
        .then(sql_block.clone());

    // sql list(...) { ... }
    let list_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("list")))
        .ignore_then(
            named_parameter()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(sql_block);

    // [internal]
    let internal_decorator = just(Token::LBracket)
        .ignore_then(just(Token::Ident("internal")))
        .then(just(Token::RBracket))
        .ignored();

    // source SourceName for ModelName { ... }
    internal_decorator
        .or_not()
        .then(just(Token::Source).ignore_then(select! { Token::Ident(name) => name }))
        .then_ignore(just(Token::Ident("for")))
        .then(select! { Token::Ident(model) => model })
        .then(
            include_tree
                .then(get_method.or_not())
                .then(list_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(
            |(((is_internal, name), model), ((include_entries, get_method), list_method)), e| {
                let tree = IncludeTree(include_entries.into_iter().collect::<BTreeMap<_, _>>());

                let get = get_method.map(|(parameters, raw_sql)| DataSourceBlockMethod {
                    span: e.span(),
                    parameters,
                    raw_sql,
                });
                let list = list_method.map(|(parameters, raw_sql)| DataSourceBlockMethod {
                    span: e.span(),
                    parameters,
                    raw_sql,
                });

                AstBlockKind::DataSource(DataSourceBlock {
                    symbol: Symbol {
                        name,
                        span: e.span(),
                        ..Default::default()
                    },
                    model,
                    tree,
                    get,
                    list,
                    is_internal: is_internal.is_some(),
                })
            },
        )
}
