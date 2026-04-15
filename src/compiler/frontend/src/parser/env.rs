use chumsky::prelude::*;

use crate::{
    AstBlockKind, EnvBlock, EnvBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
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
    let ident = select! { Token::Ident(name) => name }.map_with(|name, e| Symbol {
        span: e.span(),
        name,
        ..Default::default()
    });

    // d1 { ident* }
    let d1 = just(Token::Ident("d1"))
        .ignore_then(
            ident
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| EnvBlockKind::D1 { symbols });

    // r2 { ident* }
    let r2 = just(Token::Ident("r2"))
        .ignore_then(
            ident
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| EnvBlockKind::R2 { symbols });

    // kv { ident* }
    let kv = just(Token::Ident("kv"))
        .ignore_then(
            ident
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| EnvBlockKind::Kv { symbols });

    // vars { ident: cidl_type* }
    let var = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), ty)| Symbol {
            span,
            name,
            cidl_type: ty,
        });

    let vars = just(Token::Ident("vars"))
        .ignore_then(
            var.repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| EnvBlockKind::Var { symbols });

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
            AstBlockKind::Env(EnvBlock {
                symbol: Symbol {
                    span,
                    ..Default::default()
                },
                blocks,
            })
        })
}
