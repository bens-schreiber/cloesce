use chumsky::prelude::*;

use crate::{
    AstBlockKind, D1BindingBlock, KvBindingBlock, KvBindingField, R2BindingBlock, R2BindingField,
    Symbol, VarsBlock,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, kw, symbol, tagged_typed_symbol},
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
///     meta(id: int) -> json {
///         "metadata/{id}"
///     }
///
///     metas() -> paginated<json> {
///         "metadata/"
///     }
/// }
/// ```
pub fn kv_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // `name(params) -> type { "format" }`
    let field = symbol()
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
        .map_spanned(
            |(((sym, params), return_type), key_format)| KvBindingField {
                symbol: Symbol {
                    cidl_type: return_type,
                    ..sym
                },
                params,
                key_format,
            },
        );

    kw!(Kv)
        .ignore_then(symbol())
        .then(
            field
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, fields)| AstBlockKind::KvBinding(KvBindingBlock { symbol, fields }))
}

/// Parses a top-level R2 binding block of the form:
///
/// ```cloesce
/// r2 UserAvatars {
///     avatar(id: int) {
///         "key/{id}"
///     }
/// }
/// ```
///
/// R2 binding fields do not specify a return type, but may be marked with the
/// `paginated` infix keyword to indicate the field returns a `Paginated<R2Object>`
/// rather than a single `R2Object`.
pub fn r2_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // `name(params) [paginated] { "format" }`
    let field = symbol()
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
        .map_spanned(|(((sym, params), paginated), key_format)| R2BindingField {
            symbol: sym,
            params,
            key_format,
            is_paginated: paginated.is_some(),
        });

    kw!(R2)
        .ignore_then(symbol())
        .then(
            field
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, fields)| AstBlockKind::R2Binding(R2BindingBlock { symbol, fields }))
}
