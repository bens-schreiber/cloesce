use chumsky::prelude::*;

use crate::{
    AstBlockKind, EnvBindingBlock, EnvBindingBlockKind, EnvBlock,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, kw, symbol, typed_symbol},
};

/// Parses a block of the form:
///
/// ```cloesce
/// env {
///     d1 {
///         db
///         db2
///     }
///
///     r2 { bucket }
///     kv { store }
///
///     vars {
///         var1: cidl_type
///         var2: cidl_type
///     }
/// }
/// ```
pub fn env_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // d1 { ident* }
    let d1 = kw!(D1)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|symbols| EnvBindingBlock {
            symbols,
            kind: EnvBindingBlockKind::D1,
        });

    // r2 { ident* }
    let r2 = kw!(R2)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|symbols| EnvBindingBlock {
            symbols,
            kind: EnvBindingBlockKind::R2,
        });

    // kv { ident* }
    let kv = kw!(Kv)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|symbols| EnvBindingBlock {
            symbols,
            kind: EnvBindingBlockKind::Kv,
        });

    let vars = kw!(Vars)
        .ignore_then(
            typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_spanned(|symbols| EnvBindingBlock {
            symbols,
            kind: EnvBindingBlockKind::Var,
        });

    let sub_block = choice((d1, r2, kv, vars));

    // env { sub_block* }
    kw!(Env)
        .ignore_then(
            sub_block
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|blocks| AstBlockKind::Env(EnvBlock { blocks }))
}
