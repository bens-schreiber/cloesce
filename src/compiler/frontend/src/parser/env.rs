use chumsky::prelude::*;

use crate::{
    AstBlockKind, D1BindingBlock, KvBindingBlock, KvBindingTemplate, R2BindingBlock,
    R2BindingTemplate, Symbol, VarsBlock,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, kw, symbol, tagged_typed_symbol, tags},
};

/// Parses a top-level D1 bindings block of the form:
///
/// ```cloesce
/// d1 {
///     db
///     db2
/// }
/// ```
pub fn d1_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(D1)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|bindings| AstBlockKind::D1Binding(D1BindingBlock { bindings }))
}

/// Parses a top-level vars block of the form:
///
/// ```cloesce
/// vars {
///     api_url: string
///     max_retries: int
/// }
/// ```
pub fn vars_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(Vars)
        .ignore_then(
            tagged_typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|vars| AstBlockKind::Vars(VarsBlock { vars }))
}

/// Parses a top-level KV binding block of the form:
///
/// ```cloesce
/// kv UserMetadata {
///     // template for fetching a single metadata object by id
///     meta(id: int) -> json {
///         "metadata/{id}"
///     }
///     
///     // template for fetching all metadata objects with a common prefix
///     metas() -> paginated<json> {
///         "metadata/"
///     }
/// }
/// ```
pub fn kv_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // `[tag]* name(params) -> type { "format" }`
    let template = tags()
        .then(symbol())
        .then(
            tagged_typed_symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Arrow).ignore_then(cidl_type()))
        .then(
            select! { Token::StringLit(value) => value }
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|((((value_tags, sym), params), return_type), key_format)| {
            KvBindingTemplate {
                symbol: Symbol {
                    cidl_type: return_type,
                    tags: value_tags,
                    ..sym
                },
                params,
                key_format,
            }
        });

    kw!(Kv)
        .ignore_then(symbol())
        .then(
            template
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, templates)| AstBlockKind::KvBinding(KvBindingBlock { symbol, templates }))
}

/// Parses a top-level R2 binding block of the form:
///
/// ```cloesce
/// r2 UserAvatars {
///     // template for fetching a single avatar by id
///     avatar(id: int) {
///         "key/{id}"
///     }
/// }
/// ```
///
/// R2 binding templates do not specify a return type, but may be marked with the
/// `paginated` infix keyword to indicate the field returns a `Paginated<R2Object>`
/// rather than a single `R2Object`.
pub fn r2_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // `name(params) [paginated] { "format" }`
    let template = symbol()
        .then(
            tagged_typed_symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(kw!(GPaginated).or_not())
        .then(
            select! { Token::StringLit(value) => value }
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(
            |(((sym, params), paginated), key_format)| R2BindingTemplate {
                symbol: sym,
                params,
                key_format,
                is_paginated: paginated.is_some(),
            },
        );

    kw!(R2)
        .ignore_then(symbol())
        .then(
            template
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, templates)| AstBlockKind::R2Binding(R2BindingBlock { symbol, templates }))
}
