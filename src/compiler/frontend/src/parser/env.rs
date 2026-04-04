use ast::CidlType;
use chumsky::prelude::*;

use crate::{
    Span, Symbol, SymbolKind, WranglerEnvBindingKind, WranglerEnvBlock,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
};

enum BindingBlock<'src> {
    D1(Vec<(&'src str, Span)>),
    R2(Vec<(&'src str, Span)>),
    Kv(Vec<(&'src str, Span)>),
    Vars(Vec<((&'src str, Span), CidlType<'src>)>),
}

/// Parses a block of the form:
///
/// ```cloesce
/// env {
///     d1 { db, db2 }
///     r2 { bucket }
///     kv { store }
///
///     vars {
///         var1: string
///         var2: int
///     }
/// }
/// ```
pub fn env_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, WranglerEnvBlock<'src>, Extra<'tokens, 'src>> {
    // ident (with span)
    let ident = select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span()));

    // d1 { ident* }
    let d1_sub = just(Token::D1).ignore_then(
        ident
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // r2 { ident* }
    let r2_sub = just(Token::R2).ignore_then(
        ident
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // kv { ident* }
    let kv_sub = just(Token::Kv).ignore_then(
        ident
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // vars { ident: cidl_type* }
    let var_entry = ident.then_ignore(just(Token::Colon)).then(cidl_type());

    let vars_sub = just(Token::Vars).ignore_then(
        var_entry
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    let sub_block = choice((
        d1_sub.map(BindingBlock::D1),
        r2_sub.map(BindingBlock::R2),
        kv_sub.map(BindingBlock::Kv),
        vars_sub.map(BindingBlock::Vars),
    ));

    // env { sub_block* }
    just(Token::Env)
        .ignore_then(
            sub_block
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|sub_blocks, e| {
            let mut block = WranglerEnvBlock {
                symbol: Symbol {
                    span: e.span(),
                    kind: SymbolKind::WranglerEnvDecl,
                    ..Default::default()
                },
                d1_bindings: Vec::new(),
                kv_bindings: Vec::new(),
                r2_bindings: Vec::new(),
                vars: Vec::new(),
            };
            for sub in sub_blocks {
                match sub {
                    BindingBlock::D1(names) => {
                        for (name, span) in names {
                            block.d1_bindings.push(Symbol {
                                span,
                                name,
                                kind: SymbolKind::WranglerEnvBinding {
                                    kind: WranglerEnvBindingKind::D1,
                                },
                                ..Default::default()
                            });
                        }
                    }
                    BindingBlock::R2(names) => {
                        for (name, span) in names {
                            block.r2_bindings.push(Symbol {
                                span,
                                name,
                                kind: SymbolKind::WranglerEnvBinding {
                                    kind: WranglerEnvBindingKind::R2,
                                },
                                ..Default::default()
                            });
                        }
                    }
                    BindingBlock::Kv(names) => {
                        for (name, span) in names {
                            block.kv_bindings.push(Symbol {
                                span,
                                name,
                                kind: SymbolKind::WranglerEnvBinding {
                                    kind: WranglerEnvBindingKind::Kv,
                                },
                                ..Default::default()
                            });
                        }
                    }
                    BindingBlock::Vars(entries) => {
                        for ((name, span), cidl_type) in entries {
                            block.vars.push(Symbol {
                                span,
                                name,
                                cidl_type,
                                kind: SymbolKind::WranglerEnvVar,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            block
        })
}
