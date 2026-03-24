use std::path::PathBuf;

use chumsky::prelude::*;

use crate::{ServiceBlock, SpannedTypedName, lexer::Token, parser::{Extra, It, IdScope, cidl_type}};

/// Parses a block of the form:
///
/// ```cloesce
/// service MyAppService {
///     ident1: InjectedService1
///     ident2: InjectedService2
/// }
/// ```
pub fn service_block<'t>(it: It) -> impl Parser<'t, &'t [Token], ServiceBlock, Extra<'t>> {
    let st_fields = it.clone();
    let st_id = it.clone();

    // ident: InjectedService
    let attribute = select! { Token::Ident(var_name) => var_name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type(st_fields));

    // service ServiceName { ... }
    just(Token::Service)
        .ignore_then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            attribute
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(move |((name, span), fields)| {
            let id = st_id.borrow_mut().intern(name.clone(), IdScope::Global);
            let service_scope = IdScope::Service(name.clone());
            let fields = fields
                .into_iter()
                .map(|((field_name, name_span), inject_type)| {
                    let field_id = st_id.borrow_mut().intern(field_name.clone(), service_scope.clone());
                    SpannedTypedName {
                        id: field_id,
                        span: name_span,
                        name: field_name,
                        cidl_type: inject_type,
                    }
                })
                .collect();

            ServiceBlock {
                id,
                span,
                name,
                file: PathBuf::new(),
                fields,
            }
        })
}
