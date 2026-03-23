use chumsky::prelude::*;

use ast::CidlType;
use lexer::Token;

use crate::Extra;
use crate::parse_ast::{ServiceBlock, SpannedName, SpannedTypedName};

/// Parses a block of the form:
///
/// ```cloesce
/// service MyAppService {
///     ident1: InjectedService1
///     ident2: InjectedService2
/// }
/// ```
pub fn service_block<'t>() -> impl Parser<'t, &'t [Token], ServiceBlock, Extra<'t>> {
    // ident: InjectedService
    let attribute = select! { Token::Ident(var_name) => var_name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(select! { Token::Ident(inject_ref) => inject_ref });

    // service ServiceName { ... }
    just(Token::Service)
        .ignore_then(
            select! { Token::Ident(name) => name }.map_with(|name, e| SpannedName {
                name,
                span: e.span(),
            }),
        )
        .then(
            attribute
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(span_name, fields)| {
            let fields = fields
                .into_iter()
                .map(|((name, name_span), inject_reference)| SpannedTypedName {
                    span: name_span,
                    name,
                    ty: CidlType::Object(inject_reference),
                })
                .collect();

            ServiceBlock { span_name, fields }
        })
}
