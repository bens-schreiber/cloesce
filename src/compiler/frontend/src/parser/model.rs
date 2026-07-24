use chumsky::prelude::*;

use crate::{
    AstBlockKind, Cardinality, ForeignBlock, KvFieldArgument, KvFieldBlock, ModelBlock,
    ModelBlockKind, NavigationBlock, NavigationKey, R2FieldBlock, Spd, SqlBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, symbol, tagged_typed_symbol, tags},
};

/// `foreign AdjModel::field [optional] { localField ... }`
/// or `foreign AdjModel::{ field1, field2 } [option] { localField ... }`.
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    // `::field` (single) or `::{ field1, field2 }` (spider)
    let targets = just(Token::DoubleColon).ignore_then(choice((
        symbol()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        symbol().map(|t| vec![t]),
    )));

    kw!(Foreign)
        .ignore_then(symbol())
        .then(targets)
        .then(kw!(GOption).or_not())
        .then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(((model, targets), optional), fields)| ForeignBlock {
            model,
            targets,
            is_optional: optional.is_some(),
            fields,
        })
        .boxed()
}

/// `kv Binding::target(local1, ...) { localField }`
/// | `kv Binding::target { localField }`
/// | `kv Binding::{ template(args), shardField(local), ... } { localField }`.
fn kv_field_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, KvFieldBlock<'src>, Extra<'tokens, 'src>> {
    // `target(local1, local2, ...)` | `target`
    let arg = || {
        symbol()
            .then(
                symbol()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen))
                    .or_not(),
            )
            .map(|(target, local)| KvFieldArgument {
                target,
                local: local.unwrap_or_default(),
            })
    };

    // `::{ target(args), shardField(local), ... }`
    // | `::target(args)`
    let args = just(Token::DoubleColon).ignore_then(choice((
        arg()
            .separated_by(just(Token::Comma))
            .at_least(1)
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        arg().map(|a| vec![a]),
    )));

    kw!(Kv)
        .ignore_then(symbol())
        .then(args)
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|((binding, args), field)| KvFieldBlock {
            binding,
            args,
            field,
        })
        .boxed()
}

/// `r2 Binding::field(arg1, arg2, ...) { localField }`
/// | `r2 Binding::field { localField }`
fn r2_field_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, R2FieldBlock<'src>, Extra<'tokens, 'src>> {
    let binding_call = symbol().then(
        symbol()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .or_not(),
    );

    let binding_ref = symbol()
        .then_ignore(just(Token::DoubleColon))
        .then(binding_call)
        .map(|(binding, (template, args))| (binding, template, args.unwrap_or_default()));

    kw!(R2)
        .ignore_then(binding_ref)
        .then(symbol().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map(|((binding, binding_template, args), field)| R2FieldBlock {
            binding,
            binding_template,
            args,
            field,
        })
        .boxed()
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

    // `one|many Model { ident }`                          (discriminator-less)
    // `one|many Model::target(local) { ident }`           (single direct)
    // `one|many Model::target { ident }`                  (shard-only shorthand)
    // `one|many Model::{ t1(l1), t2(l2) } { ident }`      (spider)
    let navigation_block = {
        // `target(local)` | `target`
        let key = || {
            symbol()
                .then(
                    symbol()
                        .delimited_by(just(Token::LParen), just(Token::RParen))
                        .or_not(),
                )
                .map(|(target, local)| NavigationKey { target, local })
        };

        // `::target(local)` / `::target` (single) or `::{ t1(l1), t2(l2) }` (spider)
        let keys = just(Token::DoubleColon)
            .ignore_then(choice((
                key()
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
                key().map(|k| vec![k]),
            )))
            .or_not()
            .map(Option::unwrap_or_default);

        let cardinality = choice((
            kw!(One).to(Cardinality::One),
            kw!(Many).to(Cardinality::Many),
        ));

        cardinality
            .then(symbol())
            .then(keys)
            .then(
                symbol()
                    .map_spanned(|s| s)
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(|(((cardinality, model), keys), field)| {
                ModelBlockKind::Navigation(NavigationBlock {
                    cardinality,
                    model,
                    keys,
                    field,
                })
            })
    };

    let sub_blocks = choice((
        foreign_block().map(ModelBlockKind::Foreign),
        kv_field_block().map(ModelBlockKind::Kv),
        r2_field_block().map(ModelBlockKind::R2),
        column_block,
        navigation_block,
        primary_block,
        route_block,
        unique_block,
    ))
    .boxed();

    // `for Binding`
    // | `for Binding(shard1, shard2, ...)`
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
