use std::collections::BTreeMap;

use chumsky::prelude::*;

use crate::{
    AstBlockKind, CidlType, DataSourceBlock, DataSourceBlockMethod, ParsedIncludeTree, Spd, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, symbol, typed_symbol},
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
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<AstBlockKind<'src>>, Extra<'tokens, 'src>> {
    // ident | ident { ... }
    let include_entry = recursive(|entry| {
        symbol()
            .then(
                entry
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace))
                    .or_not(),
            )
            .map(|(symbol, children)| {
                let subtree = ParsedIncludeTree(
                    children
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<BTreeMap<_, _>>(),
                );
                (symbol, subtree)
            })
            .boxed()
    });

    // include { ... }
    let include_tree = just(Token::Ident("include")).ignore_then(
        include_entry
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // { "..." }
    let sql_block = select! { Token::StringLit(sql) => sql }
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    // sql get(ident: cidl_type, ...) { "..." }
    let method_params = || {
        typed_symbol()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // sql get(...) { ... }
    let get_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("get")))
        .ignore_then(method_params())
        .then(sql_block.clone())
        .map_spanned(|(parameters, raw_sql)| DataSourceBlockMethod {
            parameters,
            raw_sql,
        });

    // sql list(...) { ... }
    let list_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("list")))
        .ignore_then(method_params())
        .then(sql_block)
        .map_spanned(|(parameters, raw_sql)| DataSourceBlockMethod {
            parameters,
            raw_sql,
        });

    // [internal]
    let internal_decorator = just(Token::LBracket)
        .ignore_then(just(Token::Ident("internal")))
        .then_ignore(just(Token::RBracket))
        .map_spanned(|_| ())
        .map(|spd: Spd<()>| Symbol {
            name: "internal",
            cidl_type: CidlType::default(),
            span: spd.span,
        });

    // source SourceName for ModelName { ... }
    let source_block = just(Token::Source)
        .ignore_then(symbol())
        .then_ignore(just(Token::Ident("for")))
        .then(symbol())
        .then(
            include_tree
                .then(get_method.or_not())
                .then(list_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|((symbol, model), ((include_entries, get), list))| {
            let tree = ParsedIncludeTree(include_entries.into_iter().collect::<BTreeMap<_, _>>());
            DataSourceBlock {
                symbol,
                model,
                tree,
                get,
                list,
                internal: None,
            }
        });

    internal_decorator.or_not().then(source_block).map(
        |(internal, mut spd): (Option<Symbol>, Spd<DataSourceBlock>)| {
            spd.block.internal = internal;
            Spd {
                block: AstBlockKind::DataSource(spd.block),
                span: spd.span,
            }
        },
    )
}
