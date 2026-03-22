use std::collections::BTreeMap;

use chumsky::prelude::*;

use ast::{Service, ServiceAttribute};
use lexer::Token;

use chumsky::extra::SimpleState;

use crate::{Extra, SymbolTable};

/// Parses a service block of the form:
/// ```cloesce
/// service MyAppService {
///     api1: OpenApiService
///     api2: YouTubeApi
/// }
/// ```
/// Services are namespaces for API implementations and can compose injected values.
/// API methods are defined separately via `api` blocks, which are merged at the AST level.
pub fn service_block<'t>() -> impl Parser<'t, &'t [Token], Service, Extra<'t>> {
    let attribute = select! { Token::Ident(var_name) => var_name }
        .then_ignore(just(Token::Colon))
        .then(select! { Token::Ident(inject_ref) => inject_ref });

    just(Token::Service)
        .ignore_then(select! { Token::Ident(name) => name })
        .then(
            attribute
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(service_name, fields), e| {
            let symbol_table: &mut SimpleState<SymbolTable> = e.state();
            let service_symbol = symbol_table.intern_global(&service_name);

            let attributes = fields
                .into_iter()
                .map(|(var_name, inject_reference)| {
                    let attr_symbol = symbol_table.intern_scoped(&service_name, &var_name);
                    ServiceAttribute {
                        symbol: attr_symbol,
                        var_name,
                        inject_reference,
                    }
                })
                .collect();

            Service {
                symbol: service_symbol,
                name: service_name,
                attributes,
                initializer: None,
                methods: BTreeMap::default(),
                source_path: std::path::PathBuf::new(),
            }
        })
}
