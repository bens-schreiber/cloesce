use chumsky::prelude::*;

use ast::CrudKind;

use crate::{
    ForeignBlock, ForeignQualifier, KvBlock, ModelBlock, ModelBlockKind, NavigationBlock,
    PaginatedBlockKind, R2Block, SqlBlockKind, Symbol, UseTag, UseTagParamKind,
    lexer::Token,
    parser::{Extra, TokenInput, symbol, typed_symbol},
};

/// `nav { navName }`
fn foreign_nav_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    just(Token::Ident("nav"))
        .ignore_then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
}

/// `foreign(AdjModel::field1, ...) [primary|optional|unique] { localField ... nav { navName } }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    let adj_ref = symbol()
        .then_ignore(just(Token::DoubleColon))
        .then(symbol());

    let qualifier = choice((
        just(Token::Ident("primary")).to(ForeignQualifier::Primary),
        just(Token::Ident("optional")).to(ForeignQualifier::Optional),
        just(Token::Ident("unique")).to(ForeignQualifier::Unique),
    ))
    .or_not();

    let field = just(Token::Ident("nav")).not().ignore_then(symbol());

    just(Token::Ident("foreign"))
        .ignore_then(
            adj_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(qualifier)
        .then(
            field
                .repeated()
                .collect::<Vec<_>>()
                .then(foreign_nav_block().or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((adj, qualifier), (fields, nav)), e| ForeignBlock {
            span: e.span(),
            adj,
            qualifier,
            fields,
            nav,
        })
}

/// `kv (binding, "key/format/{id}") paginated { ident: cidl_type }`
fn kv_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, KvBlock<'src>, Extra<'tokens, 'src>> {
    just(Token::Ident("kv"))
        .ignore_then(
            symbol()
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(typed_symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map_with(
            |(((env_binding, key_format), paginated), field), e| KvBlock {
                span: e.span(),
                env_binding,
                key_format,
                field,
                is_paginated: paginated.is_some(),
            },
        )
}

/// `r2(binding, "key/format/{id}") paginated { ident }`
fn r2_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, R2Block<'src>, Extra<'tokens, 'src>> {
    just(Token::Ident("r2"))
        .ignore_then(
            symbol()
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map_with(
            |(((env_binding, key_format), paginated), field), e| R2Block {
                span: e.span(),
                env_binding,
                key_format,
                field,
                is_paginated: paginated.is_some(),
            },
        )
}

fn use_item<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, UseTagParamKind<'src>, Extra<'tokens, 'src>> {
    select! {
        Token::Ident("get") => UseTagParamKind::Crud(CrudKind::Get),
        Token::Ident("save") => UseTagParamKind::Crud(CrudKind::Save),
        Token::Ident("list") => UseTagParamKind::Crud(CrudKind::List),
    }
    .or(symbol().map(UseTagParamKind::EnvBinding))
}

pub fn model_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ModelBlock<'src>, Extra<'tokens, 'src>> {
    // [use d1, get, save, list]
    let use_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("use")))
        .ignore_then(
            use_item()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map_with(|params, e| UseTag {
            span: e.span(),
            params,
        });

    let choice_sql = || {
        choice((
            foreign_block().map(SqlBlockKind::Foreign),
            typed_symbol().map(SqlBlockKind::Column),
        ))
    };

    // `primary { typed_symbols... foreign(...) { ... } }`
    let primary_block = just(Token::Ident("primary")).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|blocks, e| ModelBlockKind::Primary {
                span: e.span(),
                blocks,
            }),
    );

    // `optional { foreign(...) { ... } ... }` — all contained foreigners are nullable
    let optional_block = just(Token::Ident("optional")).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|blocks, e| ModelBlockKind::Optional {
                span: e.span(),
                blocks,
            }),
    );

    // `unique { foreign(...) { ... } | typed_symbol ... }`
    let unique_block = just(Token::Ident("unique")).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map_with(|blocks, e| ModelBlockKind::Unique {
                span: e.span(),
                blocks,
            }),
    );

    // `nav(AdjModel::field1, AdjModel::field2) { ident }`
    let nav_block = {
        let adj_ref = symbol()
            .then_ignore(just(Token::DoubleColon))
            .then(symbol());

        just(Token::Ident("nav"))
            .ignore_then(
                adj_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
            .map_with(|(adj, field), e| {
                ModelBlockKind::Navigation(NavigationBlock {
                    span: e.span(),
                    adj,
                    field,
                })
            })
    };

    // `keyfield { ident* }`
    let keyfield_block = just(Token::Ident("keyfield"))
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|fields, e| ModelBlockKind::KeyField {
            span: e.span(),
            fields,
        });

    // `paginated { r2(...) { ... } kv(...) { ... } }`
    let paginated_block = just(Token::Ident("paginated")).ignore_then(
        choice((
            kv_block().map(PaginatedBlockKind::Kv),
            r2_block().map(PaginatedBlockKind::R2),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map_with(|blocks, e| ModelBlockKind::Paginated {
            span: e.span(),
            blocks,
        }),
    );

    let kv = kv_block().map(ModelBlockKind::Kv);
    let r2 = r2_block().map(ModelBlockKind::R2);
    let foreign = foreign_block().map(ModelBlockKind::Foreign);
    let column = typed_symbol().map(ModelBlockKind::Column);

    let sub_blocks = choice((
        foreign,
        kv,
        r2,
        column,
        nav_block,
        keyfield_block,
        paginated_block,
        primary_block,
        optional_block,
        unique_block,
    ));

    let use_tags = use_tag.repeated().collect::<Vec<_>>();

    use_tags
        .then_ignore(just(Token::Model))
        .then(symbol())
        .then(
            sub_blocks
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((use_tags, symbol), blocks), e| ModelBlock {
            symbol,
            span: e.span(),
            use_tags,
            blocks,
        })
}
