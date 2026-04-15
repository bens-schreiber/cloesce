use chumsky::prelude::*;

use crate::{
    AstBlockKind, EnvBlock, EnvBlockKind,
    lexer::Token,
    parser::{Extra, TokenInput, symbol, typed_symbol},
};

/// Parses a block of the form:
///
/// ```cloesce
/// env {
///     d1 { db, db2 }
///     r2 { bucket }
///     kv { store }
///
///     vars {
///         var1
///         var2
///     }
/// }
/// ```
pub fn env_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // d1 { ident* }
    let d1 = just(Token::Ident("d1"))
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|symbols, e| EnvBlockKind::D1 {
            span: e.span(),
            symbols,
        });

    // r2 { ident* }
    let r2 = just(Token::Ident("r2"))
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|symbols, e| EnvBlockKind::R2 {
            span: e.span(),
            symbols,
        });

    // kv { ident* }
    let kv = just(Token::Ident("kv"))
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|symbols, e| EnvBlockKind::Kv {
            span: e.span(),
            symbols,
        });

    let vars = just(Token::Ident("vars"))
        .ignore_then(
            typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|symbols, e| EnvBlockKind::Var {
            span: e.span(),
            symbols,
        });

    let sub_block = choice((d1, r2, kv, vars));

    // env { sub_block* }
    just(Token::Env)
        .ignore_then(
            sub_block
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|blocks, e| {
            let span = e.span();
            AstBlockKind::Env(EnvBlock { span, blocks })
        })
}
