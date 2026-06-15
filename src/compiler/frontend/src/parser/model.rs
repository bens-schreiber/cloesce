use chumsky::prelude::*;

use crate::{
    AstBlockKind, ForeignBlock, KvFieldBlock, ModelBlock, ModelBlockKind, NavAdj, NavigationBlock,
    R2FieldBlock, Spd, SqlBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, symbol, tagged_typed_symbol, tags},
};

/// `foreign(AdjModel::field1, ...) [optional] { localField ... }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    let adj_ref = symbol()
        .then_ignore(just(Token::DoubleColon))
        .then(symbol());

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
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((adj, optional), fields)| ForeignBlock {
            adj,
            is_optional: optional.is_some(),
            fields,
        })
}

/// `kv Binding::field(arg1, arg2, ...) { localField }`
/// or `kv Binding::field { localField }`
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
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not(),
        )
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|(((binding, binding_field), args), field)| KvFieldBlock {
            binding,
            binding_template: binding_field,
            args: args.unwrap_or_default(),
            field,
        })
}

/// `r2 Binding::field(arg1, arg2, ...) { localField }`
/// or `r2 Binding::field { localField }`
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
            binding_template: binding_field,
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

    // `route { ([tag]* ident: cidl_type)* }`
    let route_block = kw!(Route).ignore_then(
        tagged_typed_symbol()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(ModelBlockKind::Route),
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

    // `nav AdjModel::field { ident }`            (1:M single)
    // `nav (Adj::f1, Adj::f2) { ident }`         (1:M composite)
    // `nav AdjModel::field(localKey) { ident }`  (1:1 single)
    // `nav (Adj::f1(l1), Adj::f2(l2)) { ident }` (1:1 composite)
    let nav_block = {
        let nav_adj = || {
            symbol()
                .then_ignore(just(Token::DoubleColon))
                .then(symbol())
                .then(
                    symbol()
                        .delimited_by(just(Token::LParen), just(Token::RParen))
                        .or_not(),
                )
                .map(|((model, field), local_key)| NavAdj {
                    model,
                    field,
                    local_key,
                })
        };

        let composite = nav_adj()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen));
        let single = nav_adj().map(|a| vec![a]);

        kw!(Nav)
            .ignore_then(composite.or(single))
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
        route_block,
        unique_block,
    ))
    .boxed();

    // `for Binding`
    // or `for Binding(shard1, shard2, ...)`
    let backing = kw!(For).ignore_then(symbol()).then(
        symbol()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .or_not(),
    );

    let model_body = tags()
        .then_ignore(kw!(Model))
        .then(symbol())
        .then(backing.or_not())
        .then(
            sub_blocks
                .map_spanned(|k| k)
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(((tags, symbol), backing), blocks)| {
            let (database_binding, shard_args) = match backing {
                Some((binding, shard_args)) => (Some(binding), shard_args),
                None => (None, None),
            };
            ModelBlock {
                symbol: Symbol { tags, ..symbol },
                database_binding,
                shard_args,
                blocks,
            }
        });

    model_body.map_spanned(AstBlockKind::Model).boxed()
}
