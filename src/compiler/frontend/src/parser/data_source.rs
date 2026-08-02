use chumsky::prelude::*;
use indexmap::IndexMap;

use crate::{
    AstBlockKind, DataSourceBlock, DataSourceBlockMethod, DataSourceIncludeBlock, Keyword,
    ParsedIncludeTree, Spd, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, method_body, symbol, tags},
};

enum DataSourceMember<'src> {
    Include(Spd<DataSourceIncludeBlock<'src>>),
    Method(Spd<DataSourceBlockMethod<'src>>),
}

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
    let include_tree = kw!(Include)
        .map_with(|_, e| e.span())
        .then(
            include_entry
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|(keyword_span, entries)| DataSourceIncludeBlock {
            keyword: Symbol {
                name: "include",
                span: keyword_span,
                ..Default::default()
            },
            tree: ParsedIncludeTree(entries.into_iter().collect::<IndexMap<_, _>>()),
        });

    // [tags]* name { param* inject* }
    let stub = |name: &'static str, token: Token<'src>| {
        tags()
            .then(just(token).map_with(|_, e| e.span()))
            .then(method_body())
            .map_spanned(move |((leading_tags, name_span), (parameters, injects))| {
                DataSourceBlockMethod {
                    method: Symbol {
                        name,
                        span: name_span,
                        tags: leading_tags,
                        ..Default::default()
                    },
                    parameters,
                    injects,
                }
            })
            .boxed()
    };

    let member = choice((
        include_tree.map(DataSourceMember::Include),
        stub("get", Keyword::Get.into()).map(DataSourceMember::Method),
        stub("list", Keyword::List.into()).map(DataSourceMember::Method),
        stub("save", Keyword::Save.into()).map(DataSourceMember::Method),
    ));

    // [tags]* source SourceName for ModelName { (include | get | list | save)* }
    let source_block = tags()
        .then_ignore(kw!(Source))
        .then(symbol())
        .then_ignore(kw!(For))
        .then(symbol())
        .then(
            member
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(((tags, symbol), model), members)| {
            let (includes, methods) = members.into_iter().fold(
                (Vec::new(), Vec::new()),
                |(mut includes, mut methods), member| {
                    match member {
                        DataSourceMember::Include(include) => includes.push(include),
                        DataSourceMember::Method(method) => methods.push(method),
                    }
                    (includes, methods)
                },
            );

            DataSourceBlock {
                symbol: Symbol { tags, ..symbol },
                model,
                includes,
                methods,
            }
        });

    source_block.map_spanned(AstBlockKind::DataSource).boxed()
}
