use chumsky::prelude::*;
use indexmap::IndexMap;

use crate::{
    AstBlockKind, DataSourceBlock, DataSourceBlockMethod, Keyword, ParsedIncludeTree, Spd, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, method_body, symbol, tags},
};

/// ```cloesce
/// source SourceName for ModelName {
///     include { ... }
///
///     get {
///         [tag]* ident: cidl_type
///         inject { Db }
///     }
///
///     list {
///         ident: cidl_type
///     }
///
///     save {
///         user: partial<User>
///     }
/// }
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

    // include { include_entry* }
    let include_tree = kw!(Include).ignore_then(
        include_entry
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // [tags]* name { param* source* inject* }
    let stub = |name: &'static str, token: Token<'src>| {
        tags()
            .then(just(token).map_with(|_, e| e.span()))
            .then(method_body(false))
            .map_spanned(
                move |((leading_tags, name_span), (parameters, injects, sources))| {
                    DataSourceBlockMethod {
                        method: Symbol {
                            name,
                            span: name_span,
                            tags: leading_tags,
                            ..Default::default()
                        },
                        parameters,
                        injects,
                        sources,
                    }
                },
            )
            .boxed()
    };

    let get_method = stub("get", Keyword::Get.into());
    let list_method = stub("list", Keyword::List.into());
    let save_method = stub("save", Keyword::Save.into());

    // [tags]* source SourceName for ModelName { include { ... } get? list? save? }
    let source_block = tags()
        .then_ignore(kw!(Source))
        .then(symbol())
        .then_ignore(kw!(For))
        .then(symbol())
        .then(
            include_tree
                .or_not()
                .then(get_method.or_not())
                .then(list_method.or_not())
                .then(save_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(
            |(((tags, symbol), model), (((include_entries, get), list), save))| {
                let tree = include_entries.map(|entries| {
                    ParsedIncludeTree(entries.into_iter().collect::<IndexMap<_, _>>())
                });
                DataSourceBlock {
                    symbol: Symbol { tags, ..symbol },
                    model,
                    tree,
                    get,
                    list,
                    save,
                }
            },
        );

    source_block.map_spanned(AstBlockKind::DataSource).boxed()
}
