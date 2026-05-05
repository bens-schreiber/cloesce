use chumsky::prelude::*;
use indexmap::IndexMap;

use crate::{
    AstBlockKind, DataSourceBlock, DataSourceBlockMethod, ParsedIncludeTree, Spd,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, symbol, tagged_typed_symbol, tags},
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
                        .collect::<IndexMap<_, _>>(),
                );
                (symbol, subtree)
            })
            .boxed()
    });

    // include { ... }
    let include_tree = kw!(Include).ignore_then(
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
        tagged_typed_symbol()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // sql get(...) { ... }
    let get_method = kw!(Sql)
        .then_ignore(kw!(Get))
        .ignore_then(method_params())
        .then(sql_block.clone())
        .map_spanned(|(parameters, raw_sql)| DataSourceBlockMethod {
            parameters,
            raw_sql,
        });

    // sql list(...) { ... }
    let list_method = kw!(Sql)
        .then_ignore(kw!(List))
        .ignore_then(method_params())
        .then(sql_block)
        .map_spanned(|(parameters, raw_sql)| DataSourceBlockMethod {
            parameters,
            raw_sql,
        });

    // [tag]* source SourceName for ModelName { ... }
    let source_block = tags()
        .then_ignore(kw!(Source))
        .then(symbol())
        .then_ignore(kw!(For))
        .then(symbol())
        .then(
            include_tree
                .then(get_method.or_not())
                .then(list_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(
            |(((leading_tags, mut symbol), model), ((include_entries, get), list))| {
                let mut all_tags = leading_tags;
                all_tags.append(&mut symbol.tags);
                symbol.tags = all_tags;
                let tree =
                    ParsedIncludeTree(include_entries.into_iter().collect::<IndexMap<_, _>>());
                DataSourceBlock {
                    symbol,
                    model,
                    tree,
                    get,
                    list,
                }
            },
        );

    source_block.map_spanned(|i| AstBlockKind::DataSource(i))
}
