use std::ops::Range;

use chumsky::extra;
use chumsky::prelude::*;

use crate::parse_ast::{
    ApiBlock, DataSourceBlock, ModelBlock, ParseAst, PlainOldObjectBlock, ServiceBlock,
    UnresolvedName, WranglerEnvBlock,
};
use lexer::Token;

mod blocks;
pub mod parse_ast;

pub(crate) type Extra<'t> = extra::Err<Rich<'t, Token>>;

#[derive(Default)]
pub struct CloesceParser;

impl CloesceParser {
    pub fn parse(
        &self,
        tokens: Vec<(Token, Range<usize>)>,
    ) -> Result<ParseAst, Vec<Rich<'static, Token>>> {
        let tokens = tokens.into_iter().map(|(t, _)| t).collect::<Vec<_>>();

        self._parse()
            .parse(&tokens)
            .into_result()
            .map_err(|errs| errs.into_iter().map(|e| e.into_owned()).collect())
    }

    fn _parse<'t>(&self) -> impl chumsky::Parser<'t, &'t [Token], ParseAst, Extra<'t>> {
        choice((
            blocks::env_block().map(Global::Env),
            blocks::model_block().map(Global::Model),
            blocks::api_block().map(Global::Api),
            blocks::service_block().map(Global::Service),
            blocks::poo_block().map(Global::Poo),
            blocks::data_source_block().map(Global::DataSource),
            blocks::inject_block().map(Global::Inject),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| {
            let mut ast = ParseAst::default();
            for item in items {
                match item {
                    Global::Env(env) => ast.wrangler_env.push(env),
                    Global::Model(model) => ast.models.push(model),
                    Global::Api(api) => ast.apis.push(api),
                    Global::Service(service) => ast.services.push(service),
                    Global::Poo(poo) => ast.poos.push(poo),
                    Global::DataSource(ds) => ast.sources.push(ds),
                    Global::Inject(symbols) => ast.injectables.extend(symbols),
                }
            }
            ast
        })
    }
}

enum Global {
    Env(WranglerEnvBlock),
    Model(ModelBlock),
    Api(ApiBlock),
    Service(ServiceBlock),
    Poo(PlainOldObjectBlock),
    DataSource(DataSourceBlock),
    Inject(Vec<UnresolvedName>),
}
