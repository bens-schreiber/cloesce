//! Parses for Cloudflare Environment bindings: D1, KV, R2, Durable Objects, and Variables

use chumsky::prelude::*;

use crate::{
    AstBlockKind, D1BindingBlock, DurableBindingBlock, DurableShardBlock, KvBindingBlock,
    KvBindingTemplate, R2BindingBlock, R2BindingTemplate, Spd, Symbol, VarsBlock,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, kw, symbol, tagged_typed_symbol, tags},
};

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
        .boxed()
}

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
        .boxed()
}

/// Parses a single storage template of the form `[tag]* name(params) -> type { "format" }`.
fn kv_template<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<KvBindingTemplate<'src>>, Extra<'tokens, 'src>>
{
    tags()
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
        .map_spanned(
            |((((value_tags, sym), params), return_type), key_format)| KvBindingTemplate {
                symbol: Symbol {
                    cidl_type: return_type,
                    tags: value_tags,
                    ..sym
                },
                params,
                key_format,
            },
        )
        .boxed()
}

/// ```cloesce
/// kv UserMetadata {
///     // template for fetching a single metadata object by id
///     meta(id: int) -> json {
///         "metadata/{id}"
///     }
///
///     // template for fetching another metadata object by name
///     named(name: string) -> json {
///         "metadata/{name}"
///     }
/// }
/// ```
pub fn kv_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(Kv)
        .ignore_then(symbol())
        .then(
            kv_template()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, templates)| AstBlockKind::KvBinding(KvBindingBlock { symbol, templates }))
        .boxed()
}

/// ```cloesce
/// r2 UserAvatars {
///     // template for fetching a single avatar by id
///     avatar(id: int) {
///         "key/{id}"
///     }
/// }
/// ```
///
/// R2 binding templates do not specify a return type.
pub fn r2_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // `name(params) { "format" }`
    let template = symbol()
        .then(
            tagged_typed_symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(
            select! { Token::StringLit(value) => value }
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|((sym, params), key_format)| R2BindingTemplate {
            symbol: sym,
            params,
            key_format,
        });

    kw!(R2)
        .ignore_then(symbol())
        .then(
            template
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, templates)| AstBlockKind::R2Binding(R2BindingBlock { symbol, templates }))
        .boxed()
}

/// ```cloesce
/// durable MyDurableObject {
///     shard {
///         shardField1: cidl_type
///         shardField2: cidl_type
///    }
///
///    storageTemplate1(params) -> returnType {
///         "keyFormat"
///    }
/// }
/// ```
///
/// Shard block is optional.
pub fn durable_binding_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    let shard_block = kw!(Shard)
        .ignore_then(
            tagged_typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|fields| DurableShardBlock { fields });

    let body = shard_block
        .repeated()
        .collect::<Vec<_>>()
        .then(kv_template().repeated().collect::<Vec<_>>())
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    kw!(Durable)
        .ignore_then(symbol())
        .then(body)
        .map(|(symbol, (shard_blocks, templates))| {
            AstBlockKind::DurableBinding(DurableBindingBlock {
                symbol,
                shard_blocks,
                templates,
            })
        })
        .boxed()
}
