use chumsky::prelude::*;

use ast::CidlType;
use lexer::Token;

use crate::Extra;
use crate::blocks::sqlite_column_types;
use crate::parse_ast::{SpannedName, SpannedTypedName, WranglerEnvBlock};

enum BindingKind {
    D1,
    R2,
    Kv,
}

enum EnvEntry {
    Binding(BindingKind),
    Var(CidlType),
}

/// Parses a block of the form:
///
/// ```cloesce
/// env {
///     ident1: d1
///     ident2: r2
///     ident3: cidl_type
/// }
/// ```
pub fn env_block<'t>() -> impl Parser<'t, &'t [Token], WranglerEnvBlock, Extra<'t>> {
    // Environment variables can only be SQLite column types or JSON
    let env_var = choice((
        sqlite_column_types(),
        just(Token::Json).map(|_| CidlType::Json),
    ));

    // ident: (d1 | r2 | kv | cidl_type)
    let env_entry = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(choice((
            just(Token::D1).map(|_| EnvEntry::Binding(BindingKind::D1)),
            just(Token::R2).map(|_| EnvEntry::Binding(BindingKind::R2)),
            just(Token::Kv).map(|_| EnvEntry::Binding(BindingKind::Kv)),
            env_var.map(EnvEntry::Var),
        )));

    // env { ... }
    just(Token::Env)
        .ignore_then(
            env_entry
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|entries, e| {
            let mut block = WranglerEnvBlock {
                span: e.span(),
                d1_bindings: Vec::new(),
                kv_bindings: Vec::new(),
                r2_bindings: Vec::new(),
                vars: Vec::new(),
            };
            for ((name, span), entry) in entries {
                match entry {
                    EnvEntry::Binding(BindingKind::D1) => {
                        block.d1_bindings.push(SpannedName { span, name });
                    }
                    EnvEntry::Binding(BindingKind::R2) => {
                        block.r2_bindings.push(SpannedName { span, name });
                    }
                    EnvEntry::Binding(BindingKind::Kv) => {
                        block.kv_bindings.push(SpannedName { span, name });
                    }
                    EnvEntry::Var(ty) => {
                        block.vars.push(SpannedTypedName { span, name, ty });
                    }
                }
            }
            block
        })
}
