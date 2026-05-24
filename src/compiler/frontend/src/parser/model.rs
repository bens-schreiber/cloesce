use chumsky::prelude::*;

use crate::{
    AstBlockKind, ForeignBlock, ForeignBlockNav, KvFieldBlock, ModelBlock, ModelBlockKind,
    NavigationBlock, R2FieldBlock, Spd, SqlBlockKind, Symbol,
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

/// `foreign(AdjModel::field1, ...) [optional] { localField ... nav { navName } }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    let adj_ref = symbol()
        .then_ignore(just(Token::DoubleColon))
        .then(symbol());

    let field = kw!(Nav).not().ignore_then(symbol());

    kw!(Foreign)
        .ignore_then(
            adj_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(kw!(Optional).or_not())
        .then(
            field
                .repeated()
                .collect::<Vec<_>>()
                .then(foreign_nav_block().or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((adj, optional), (fields, nav))| ForeignBlock {
            adj,
            is_optional: optional.is_some(),
            fields,
            nav,
        })
}

/// `kv Binding::field(arg1, arg2, ...) { localField }`
fn kv_field_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, KvFieldBlock<'src>, Extra<'tokens, 'src>> {
    kw!(Kv)
        .ignore_then(symbol())
        .then_ignore(just(Token::DoubleColon))
        .then(symbol())
        .then(
            symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((binding, binding_field), args), field)| KvFieldBlock {
            binding,
            binding_field,
            args,
            field,
        })
}

/// `r2 Binding::field(arg1, arg2, ...) { localField }`
fn r2_field_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, R2FieldBlock<'src>, Extra<'tokens, 'src>> {
    kw!(R2)
        .ignore_then(symbol())
        .then_ignore(just(Token::DoubleColon))
        .then(symbol())
        .then(
            symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((binding, binding_field), args), field)| R2FieldBlock {
            binding,
            binding_field,
            args,
            field,
        })
}

pub fn model_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<AstBlockKind<'src>>, Extra<'tokens, 'src>> {
    // `column { ([tag]* ident: cidl_type)* }`
    let column_block = kw!(Column).ignore_then(
        tagged_typed_symbol()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(ModelBlockKind::Column),
    );

    // `primary { typed_symbols... foreign(...) { ... } }`
    let primary_block = kw!(Primary).ignore_then(
        choice((
            foreign_block().map(SqlBlockKind::Foreign),
            tagged_typed_symbol().map(SqlBlockKind::Column),
        ))
        .map_spanned(|k| k)
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace))
        .map(ModelBlockKind::Primary),
    );

    // `unique (field1, field2, ...)`
    let unique_block = kw!(Unique).ignore_then(
        symbol()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
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

    let sub_blocks = choice((
        foreign_block().map(ModelBlockKind::Foreign),
        kv_field_block().map(ModelBlockKind::Kv),
        r2_field_block().map(ModelBlockKind::R2),
        column_block,
        nav_block,
        primary_block,
        unique_block,
    ))
    .boxed();

    let model_body = tags()
        .then_ignore(kw!(Model))
        .then(symbol())
        .then(kw!(For).ignore_then(symbol()).or_not())
        .then(
            sub_blocks
                .map_spanned(|k| k)
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(((tags, symbol), backing_binding), blocks)| ModelBlock {
            symbol: Symbol { tags, ..symbol },
            backing_binding,
            blocks,
        });

    model_body.map_spanned(AstBlockKind::Model).boxed()
}
