use chumsky::prelude::*;

use crate::{Extra, SymbolTable, cidl_type, sqlite_column_types};
use ast::{
    DataSource,
};
use lexer::Token;


enum PendingSqlParam {
    Field { name: String, cidl_type: CidlType },
}

/// Parses a data source block of the form
/// ```cloesce
/// source SourceName for ModelName {
///     include { field1, field2, field3 { field4, ...} }
///     
///     sql get(id: int) {
///         select * from ....
///     }
/// 
///     sql list(id: int, offset: int, limit: int) {
///         select * from ....
///     }
/// }
/// ```
pub fn data_source_block<'t>() -> impl Parser<'t, &'t [Token], DataSource, Extra<'t>> {
    let include_tree = just(Token::Include)
        .then_ignore(just(Token::LBrace))
        .then(
            recursive(|include_tree| {
                select! { Token::Ident(name) => name }
                    .then(
                        just(Token::LBrace)
                            .then(include_tree.clone())
                            .then_ignore(just(Token::RBrace))
                            .or_not(),
                    )
                    .map(|(name, children)| (name, children.unwrap_or_default()))
            })
            .separated_by(just(Token::Comma))
            .allow_trailing(),
        )
        .then_ignore(just(Token::RBrace));


    let named_parameter = select! { Token::Ident(name) => name }
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|(name, cidl_type)| PendingSqlParam::Field { name, cidl_type });

    // Actual parsing of SQL is unneccessary, we can just capture it as a string.
    let sql_block = just(Token::LBrace)
        .ignore_then(select! { Token::Ident(sql) => sql })
        .then_ignore(just(Token::RBrace));
    
    // can have N parameters (no self param)
    let get_method = just(Token::Sql)
        .ignore_then(Token::Ident("get".into()))
        .then_ignore(just(Token::LParen))
        .then(named_parameter.clone().separated_by(just(Token::Comma).allow_trailing()))
        .then_ignore(just(Token::RParen))
        .then(sql_block());

    let list_method = just(Token::Sql)
        .ignore_then(just(Token::Ident("list".into())))
        .then_ignore(just(Token::LParen))
        .then(named_parameter.clone().separated_by(just(Token::Comma).allow_trailing()))
        .then_ignore(just(Token::RParen))
        .then(sql_block());

    just(Token::Source)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::For))
        .then(select! { Token::Ident(model) => model })
        .then(include_tree)
        .then(get_method.or_not())
        .then(list_method.or_not())
        .map_with(|((((name, model), include_tree), get_method), list_method), e| {
            let symbol_table = e.state();
            let symbol = symbol_table.intern_global(&name);
            map_source(name, model, include_tree, get_method, list_method, symbol_table)
        })

}

fn map_source(name: String, model: String, include_tree: Vec<(String, Vec<()>)>, get_method: Option<(Vec<PendingSqlParam>, String)>, list_method: Option<(Vec<PendingSqlParam>, String)>,  symbol: SymbolTable) -> DataSource {

}