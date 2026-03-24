use std::path::PathBuf;

use ast::CidlType;
use chumsky::prelude::*;

use crate::{
    SpannedName, SpannedTypedName, WranglerEnvBlock,
    lexer::Token,
    parser::{Extra, IdScope, It, cidl_type},
};

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
pub fn env_block<'t>(it: It) -> impl Parser<'t, &'t [Token], WranglerEnvBlock, Extra<'t>> {
    let st_vars = it.clone();
    let st_id = it.clone();

    // ident: (d1 | r2 | kv | cidl_type)
    let env_entry = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(choice((
            just(Token::D1).map(|_| EnvEntry::Binding(BindingKind::D1)),
            just(Token::R2).map(|_| EnvEntry::Binding(BindingKind::R2)),
            just(Token::Kv).map(|_| EnvEntry::Binding(BindingKind::Kv)),
            cidl_type(st_vars).map(EnvEntry::Var),
        )));

    // env { ... }
    just(Token::Env)
        .ignore_then(
            env_entry
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(move |entries, e| {
            let id = st_id.borrow_mut().new_id();
            let mut block = WranglerEnvBlock {
                id,
                span: e.span(),
                file: PathBuf::new(),
                d1_bindings: Vec::new(),
                kv_bindings: Vec::new(),
                r2_bindings: Vec::new(),
                vars: Vec::new(),
            };
            for ((name, span), entry) in entries {
                match entry {
                    EnvEntry::Binding(BindingKind::D1) => {
                        let id = st_id.borrow_mut().intern(name.clone(), IdScope::Env);
                        block.d1_bindings.push(SpannedName { id, span, name });
                    }
                    EnvEntry::Binding(BindingKind::R2) => {
                        let id = st_id.borrow_mut().intern(name.clone(), IdScope::Env);
                        block.r2_bindings.push(SpannedName { id, span, name });
                    }
                    EnvEntry::Binding(BindingKind::Kv) => {
                        let id = st_id.borrow_mut().intern(name.clone(), IdScope::Env);
                        block.kv_bindings.push(SpannedName { id, span, name });
                    }
                    EnvEntry::Var(ty) => {
                        let id = st_id.borrow_mut().intern(name.clone(), IdScope::Env);
                        block.vars.push(SpannedTypedName {
                            id,
                            span,
                            name,
                            cidl_type: ty,
                        });
                    }
                }
            }
            block
        })
}
