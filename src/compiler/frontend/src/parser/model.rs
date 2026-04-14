use chumsky::prelude::*;

use ast::CrudKind;

use crate::{
    ForeignBlock, ForeignQualifier, KvBlock, ModelBlock, ModelBlockKind, NavigationBlock,
    PaginatedBlockKind, R2Block, SqlBlockKind, Symbol, UseTag, UseTagParamKind,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
};

/// `ident: cidl_type`
fn typed_field<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), cidl_type)| Symbol {
            span,
            name,
            cidl_type,
            ..Default::default()
        })
}

/// `nav { navName }`
fn foreign_nav_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    just(Token::Ident("nav")).ignore_then(
        select! { Token::Ident(name) => name }
            .map_with(|name, e| Symbol {
                span: e.span(),
                name,
                ..Default::default()
            })
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    )
}

/// `foreign(AdjModel::field1, ...) [primary|optional|unique] { localField ... nav { navName } }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ForeignBlock<'src>, Extra<'tokens, 'src>> {
    let adj_ref = select! { Token::Ident(model_name) => model_name }
        .then_ignore(just(Token::DoubleColon))
        .then(select! { Token::Ident(field_name) => field_name });

    let qualifier = choice((
        just(Token::Ident("primary")).to(ForeignQualifier::Primary),
        just(Token::Ident("optional")).to(ForeignQualifier::Optional),
        just(Token::Ident("unique")).to(ForeignQualifier::Unique),
    ))
    .or_not();

    let field = just(Token::Ident("nav"))
        .not()
        .ignore_then(select! { Token::Ident(name) => name })
        .map_with(|name, e| Symbol {
            span: e.span(),
            name,
            ..Default::default()
        });

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
            select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(typed_field().delimited_by(just(Token::LBrace), just(Token::RBrace)))
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
            select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(
            select! { Token::Ident(name) => name }
                .map_with(|name, e| Symbol {
                    span: e.span(),
                    name,
                    ..Default::default()
                })
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
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
        Token::Ident(name) => UseTagParamKind::EnvBinding(name),
    }
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
            typed_field().map(SqlBlockKind::Column),
        ))
    };

    // `primary { typed_fields... foreign(...) { ... } }`
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

    // `unique { foreign(...) { ... } | typed_field ... }`
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
        let adj_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => field_name });

        just(Token::Ident("nav"))
            .ignore_then(
                adj_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, e| Symbol {
                        span: e.span(),
                        name,
                        ..Default::default()
                    })
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
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
            select! { Token::Ident(name) => name }
                .map_with(|name, e| Symbol {
                    span: e.span(),
                    name,
                    ..Default::default()
                })
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
    let column = typed_field().map(ModelBlockKind::Column);

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
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            sub_blocks
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((use_tags, (name, span)), blocks)| ModelBlock {
            symbol: Symbol {
                name,
                span,
                ..Default::default()
            },
            use_tags,
            blocks,
        })
}
