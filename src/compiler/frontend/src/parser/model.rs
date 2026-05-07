use chumsky::prelude::*;

use crate::{
    AstBlockKind, ForeignBlock, ForeignBlockNav, ForeignQualifier, KvBlock, ModelBlock,
    ModelBlockKind, NavigationBlock, PaginatedBlockKind, R2Block, Spd, SqlBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, symbol, tagged_typed_symbol, tags},
};

/// `nav { navName }`
fn foreign_nav_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<ForeignBlockNav<'src>>, Extra<'tokens, 'src>>
{
    kw!(Nav)
        .ignore_then(
            symbol()
                .map(|nav| ForeignBlockNav { symbol: nav })
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|s| s)
}

/// `foreign(AdjModel::field1, ...) [primary|optional|unique] { localField ... nav { navName } }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    let adj_ref = symbol()
        .then_ignore(just(Token::DoubleColon))
        .then(symbol());

    let qualifier = choice((
        kw!(Primary).to(ForeignQualifier::Primary),
        kw!(Optional).to(ForeignQualifier::Optional),
        kw!(Unique).to(ForeignQualifier::Unique),
    ))
    .or_not();

    let field = kw!(Nav).not().ignore_then(symbol());

    kw!(Foreign)
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
        .map(|((adj, qualifier), (fields, nav))| ForeignBlock {
            adj,
            qualifier,
            fields,
            nav,
        })
}

/// `kv (binding, "key/format/{id}") paginated { ident: cidl_type }`
fn kv_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, KvBlock<'src>, Extra<'tokens, 'src>> {
    kw!(Kv)
        .ignore_then(
            symbol()
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(kw!(Paginated).or_not())
        .then(tagged_typed_symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((env_binding, key_format), paginated), field)| KvBlock {
            env_binding,
            key_format,
            field,
            is_paginated: paginated.is_some(),
        })
}

/// `r2(binding, "key/format/{id}") paginated { ident }`
fn r2_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, R2Block<'src>, Extra<'tokens, 'src>> {
    kw!(R2)
        .ignore_then(
            symbol()
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(kw!(Paginated).or_not())
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((env_binding, key_format), paginated), field)| R2Block {
            env_binding,
            key_format,
            field,
            is_paginated: paginated.is_some(),
        })
}

pub fn model_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<AstBlockKind<'src>>, Extra<'tokens, 'src>> {
    let choice_sql = || {
        choice((
            foreign_block().map(SqlBlockKind::Foreign),
            tagged_typed_symbol().map(SqlBlockKind::Column),
        ))
        .map_spanned(|k| k)
    };

    // `primary { typed_symbols... foreign(...) { ... } }`
    let primary_block = kw!(Primary).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(ModelBlockKind::Primary),
    );

    // `optional { foreign(...) { ... } ... }` — all contained foreigners are nullable
    let optional_block = kw!(Optional).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(ModelBlockKind::Optional),
    );

    // `unique { foreign(...) { ... } | typed_symbol ... }`
    let unique_block = kw!(Unique).ignore_then(
        choice_sql()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(ModelBlockKind::Unique),
    );

    // `nav(AdjModel::field1, AdjModel::field2) { ident }`
    let nav_block = {
        let adj_ref = symbol()
            .then_ignore(just(Token::DoubleColon))
            .then(symbol());

        kw!(Nav)
            .ignore_then(
                adj_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(
                symbol()
                    .map_spanned(|s| s)
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(|(adj, nav)| ModelBlockKind::Navigation(NavigationBlock { adj, nav }))
    };

    // `keyfield { ([tag]* ident: cidl_type)* }`
    let keyfield_block = {
        kw!(KeyField)
            .ignore_then(
                tagged_typed_symbol()
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(ModelBlockKind::KeyField)
    };

    // `paginated { r2(...) { ... } kv(...) { ... } }`
    let paginated_block = kw!(Paginated).ignore_then(
        choice((
            kv_block().map(PaginatedBlockKind::Kv),
            r2_block().map(PaginatedBlockKind::R2),
        ))
        .map_spanned(|k| k)
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map(ModelBlockKind::Paginated),
    );

    let kv = kv_block().map(ModelBlockKind::Kv);
    let r2 = r2_block().map(ModelBlockKind::R2);
    let foreign = foreign_block().map(ModelBlockKind::Foreign);
    let column = tagged_typed_symbol().map(ModelBlockKind::Column);

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
    ))
    .boxed();

    let model_body = tags()
        .then_ignore(kw!(Model))
        .then(symbol())
        .then(
            sub_blocks
                .map_spanned(|k| k)
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((tags, symbol), blocks)| ModelBlock {
            symbol: Symbol { tags, ..symbol },
            blocks,
        });

    model_body.map_spanned(AstBlockKind::Model).boxed()
}
