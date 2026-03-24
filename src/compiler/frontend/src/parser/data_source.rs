use std::collections::BTreeMap;
use std::path::PathBuf;

use chumsky::prelude::*;

use ast::CidlType;

use crate::{
    DataSourceBlock, DataSourceBlockMethod, IncludeTree, SpannedTypedName,
    lexer::Token,
    parser::{Extra, It, IdScope, cidl_type},
};

enum PendingSqlParam {
    Field {
        name: String,
        span: SimpleSpan,
        cidl_type: CidlType,
    },
}

/// Parses a block of the form:
///
/// ```cloesce
/// source SourceName for ModelName {
///     include { ... }
///     sql get(ident: cidl_type, ...) { "..." }
///     sql list(ident: cidl_type, ...) { "..." }
/// }
/// ```
pub fn data_source_block<'t>(it: It) -> impl Parser<'t, &'t [Token], DataSourceBlock, Extra<'t>> {
    // ident | ident { ... }
    let include_entry = recursive(|entry| {
        select! { Token::Ident(name) => name }
            .then(
                entry
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace))
                    .or_not(),
            )
            .map(|(name, children)| {
                let subtree = IncludeTree(
                    children
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<BTreeMap<_, _>>(),
                );
                (name, subtree)
            })
            .boxed()
    });

    // include { ... }
    let include_tree = just(Token::Include).ignore_then(
        include_entry
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    let st_get = it.clone();
    let st_list = it.clone();
    let st_map = it.clone();

    // ident: cidl_type
    let named_parameter = move || {
        let st_p = st_get.clone();
        select! { Token::Ident(name) => name }
            .map_with(|name, e| (name, e.span()))
            .then_ignore(just(Token::Colon))
            .then(cidl_type(st_p))
            .map(|((name, name_span), cidl_type)| PendingSqlParam::Field {
                name,
                span: name_span,
                cidl_type,
            })
    };

    // { "..." }
    let sql_block = select! { Token::StringLit(sql) => sql }
        .delimited_by(just(Token::LBrace), just(Token::RBrace));

    // sql get(ident: cidl_type, ...) { "..." }
    let method_params = move || {
        named_parameter()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
    };

    // sql get(...) { ... }
    let get_method = just(Token::Sql)
        .then_ignore(just(Token::Get))
        .ignore_then(method_params())
        .then(sql_block.clone());

    let named_parameter_list = move || {
        let st_p = st_list.clone();
        select! { Token::Ident(name) => name }
            .map_with(|name, e| (name, e.span()))
            .then_ignore(just(Token::Colon))
            .then(cidl_type(st_p))
            .map(|((name, name_span), cidl_type)| PendingSqlParam::Field {
                name,
                span: name_span,
                cidl_type,
            })
    };

    // sql list(...) { ... }
    let list_method = just(Token::Sql)
        .then_ignore(just(Token::Ident("list".into())))
        .ignore_then(
            named_parameter_list()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(sql_block);

    // source SourceName for ModelName { ... }
    just(Token::Source)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::For))
        .then(select! { Token::Ident(model) => model })
        .then(
            include_tree
                .then(get_method.or_not())
                .then(list_method.or_not())
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(
            move |((name, model), ((include_entries, get_method), list_method)), e| {
                map_data_source(
                    &st_map,
                    name,
                    model,
                    include_entries,
                    get_method,
                    list_method,
                    e.span(),
                )
            },
        )
}

fn map_data_source(
    it: &It,
    name: String,
    model: String,
    include_entries: Vec<(String, IncludeTree)>,
    get_method: Option<(Vec<PendingSqlParam>, String)>,
    list_method: Option<(Vec<PendingSqlParam>, String)>,
    span: SimpleSpan,
) -> DataSourceBlock {
    let id = it.borrow_mut().intern(name.clone(), IdScope::Global);
    let model_ref = it.borrow_mut().intern(model, IdScope::Global);
    let tree = IncludeTree(include_entries.into_iter().collect());
    let ds_scope = IdScope::DataSource(name.clone());

    let map_params = |params: Vec<PendingSqlParam>| {
        params
            .into_iter()
            .map(|p| match p {
                PendingSqlParam::Field {
                    name,
                    span: name_span,
                    cidl_type,
                } => {
                    let field_id = it.borrow_mut().intern(name.clone(), ds_scope.clone());
                    SpannedTypedName {
                        id: field_id,
                        span: name_span,
                        name,
                        cidl_type,
                    }
                }
            })
            .collect::<Vec<_>>()
    };

    let get = get_method.map(|(params, raw_sql)| DataSourceBlockMethod {
        span,
        parameters: map_params(params),
        raw_sql,
    });

    let list = list_method.map(|(params, raw_sql)| DataSourceBlockMethod {
        span,
        parameters: map_params(params),
        raw_sql,
    });

    DataSourceBlock {
        id,
        span,
        name,
        file: PathBuf::new(),
        model: model_ref,
        tree,
        get,
        list,
    }
}
