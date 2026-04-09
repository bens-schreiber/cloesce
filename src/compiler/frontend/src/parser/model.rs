use chumsky::prelude::*;

use ast::CrudKind;

use crate::{
    ForeignBlock, ForeignQualifier, KvBlock, ModelBlock, NavigationBlock, R2Block, Symbol,
    SymbolKind, UniqueConstraint, UseTag,
    lexer::Token,
    parser::{Extra, Span, TokenInput, cidl_type},
};

struct ParsedForeign<'src> {
    block: ForeignBlock<'src>,
    nav: Option<Symbol<'src>>,
}

enum UniqueItem<'src> {
    Foreign(ParsedForeign<'src>),
    Field(Symbol<'src>),
}

enum PrimaryItem<'src> {
    Foreign(ParsedForeign<'src>),
    Field(Symbol<'src>),
}

enum PaginatedItem<'src> {
    Kv(KvBlock<'src>),
    R2(R2Block<'src>),
}

enum ModelItem<'src> {
    Primary(Span, Vec<PrimaryItem<'src>>),
    Optional(Vec<ParsedForeign<'src>>),
    Unique(Span, Vec<UniqueItem<'src>>),
    Foreign(ParsedForeign<'src>),
    Field(Symbol<'src>),
    Kv(KvBlock<'src>),
    R2(R2Block<'src>),
    Paginated(Vec<PaginatedItem<'src>>),
    Nav(NavigationBlock<'src>),
    KeyField(Vec<Symbol<'src>>),
}

/// `ident: cidl_type`
fn typed_field<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), cidl_type)| Symbol {
            span,
            name,
            cidl_type,
            kind: SymbolKind::ModelField,
            ..Default::default()
        })
}

/// `nav { navName }`
fn foreign_nav_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    just(Token::Ident("nav")).ignore_then(
        select! { Token::Ident(name) => name }
            .map_with(|name, e| Symbol {
                span: e.span(),
                name,
                kind: SymbolKind::ModelField,
                ..Default::default()
            })
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    )
}

/// `foreign(AdjModel::field1, ...) [primary|optional|unique] { localField ... nav { navName } }`
fn foreign_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ParsedForeign<'src>, Extra<'tokens, 'src>> {
    let adj_ref = select! { Token::Ident(model_name) => model_name }
        .then_ignore(just(Token::DoubleColon))
        .then(select! { Token::Ident(field_name) => field_name });

    let qualifier = choice((
        just(Token::Ident("primary")).to(ForeignQualifier::Primary),
        just(Token::Ident("optional")).to(ForeignQualifier::Optional),
        just(Token::Ident("unique")).to(ForeignQualifier::Unique),
    ))
    .or_not();

    just(Token::Ident("foreign"))
        .ignore_then(
            adj_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(qualifier)
        .then(
            choice((
                foreign_nav_block().map(|nav| (None::<Symbol<'src>>, Some(nav))),
                select! { Token::Ident(name) => name }
                    .map_with(|name, e| Symbol {
                        span: e.span(),
                        name,
                        kind: SymbolKind::ModelField,
                        ..Default::default()
                    })
                    .map(|sym| (Some(sym), None)),
            ))
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((adj_refs, qualifier), body_items), e| {
            let mut fields: Vec<Symbol<'src>> = Vec::new();
            let mut nav: Option<Symbol<'src>> = None;
            for (field_opt, nav_opt) in body_items {
                if let Some(f) = field_opt {
                    fields.push(f);
                }
                if let Some(n) = nav_opt {
                    nav = Some(n);
                }
            }

            ParsedForeign {
                block: ForeignBlock {
                    span: e.span(),
                    adj: adj_refs,
                    fields,
                    qualifier,
                },
                nav,
            }
        })
}

/// `kv(binding, "key/format/{id}") paginated { ident: cidl_type }`
fn kv_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, KvBlock<'src>, Extra<'tokens, 'src>> {
    just(Token::Kv)
        .ignore_then(
            select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(typed_field().delimited_by(just(Token::LBrace), just(Token::RBrace)))
        .map_with(
            |(((env_binding, key_format), paginated), field), e| KvBlock {
                span: e.span(),
                env_binding,
                key_format,
                field,
                is_paginated: paginated.is_some(),
            },
        )
}

/// `r2(binding, "key/format/{id}") paginated{ ident }`
fn r2_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, R2Block<'src>, Extra<'tokens, 'src>> {
    just(Token::R2)
        .ignore_then(
            select! { Token::Ident(name) => name }
                .then_ignore(just(Token::Comma))
                .then(select! { Token::StringLit(value) => value })
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then(just(Token::Ident("paginated")).or_not())
        .then(
            select! { Token::Ident(name) => name }
                .map_with(|name, e| Symbol {
                    span: e.span(),
                    name,
                    kind: SymbolKind::ModelField,
                    ..Default::default()
                })
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(
            |(((env_binding, key_format), paginated), field), e| R2Block {
                span: e.span(),
                env_binding,
                key_format,
                field,
                is_paginated: paginated.is_some(),
            },
        )
}

enum UseItem<'src> {
    Crud(CrudKind),
    Binding(&'src str),
}

fn use_item<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, UseItem<'src>, Extra<'tokens, 'src>> {
    select! {
        Token::Ident("get") => UseItem::Crud(CrudKind::Get),
        Token::Ident("save") => UseItem::Crud(CrudKind::Save),
        Token::Ident("list") => UseItem::Crud(CrudKind::List),
        Token::Ident(name) => UseItem::Binding(name),
    }
}

pub fn model_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ModelBlock<'src>, Extra<'tokens, 'src>> {
    // [use d1, get, save, list]
    let use_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("use")))
        .ignore_then(
            use_item()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map_with(|items, e| (items, e.span()));

    // `primary { typed_fields... foreign(...) { ... } }`
    let primary_block = just(Token::Ident("primary")).ignore_then(
        choice((
            foreign_block().map(PrimaryItem::Foreign),
            typed_field().map(PrimaryItem::Field),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // `optional { foreign(...) { ... } ... }` — all contained foreigners are nullable
    let optional_block = just(Token::Ident("optional")).ignore_then(
        foreign_block()
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    // `unique { foreign(...) { ... } | typed_field ... }`
    let unique_block = just(Token::Ident("unique"))
        .ignore_then(
            choice((
                foreign_block().map(UniqueItem::Foreign),
                typed_field().map(UniqueItem::Field),
            ))
            .repeated()
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|items, e| ModelItem::Unique(e.span(), items));

    // `nav(AdjModel::field1, AdjModel::field2) { ident }`
    let nav_block = {
        let adj_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => field_name });

        just(Token::Ident("nav"))
            .ignore_then(
                adj_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(
                select! { Token::Ident(name) => name }
                    .map_with(|name, e| Symbol {
                        span: e.span(),
                        name,
                        kind: SymbolKind::ModelField,
                        ..Default::default()
                    })
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(adj, field), e| {
                ModelItem::Nav(NavigationBlock {
                    span: e.span(),
                    adj,
                    field,
                    is_one_to_one: false,
                })
            })
    };

    // `keyfield { ident* }`
    let keyfield_block = just(Token::Ident("keyfield"))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .map_with(|name, e| Symbol {
                    span: e.span(),
                    name,
                    kind: SymbolKind::ModelField,
                    ..Default::default()
                })
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(ModelItem::KeyField);

    // `paginated { r2(...) { ... } kv(...) { ... } }`
    let paginated_block = just(Token::Ident("paginated")).ignore_then(
        choice((
            kv_block().map(PaginatedItem::Kv),
            r2_block().map(PaginatedItem::R2),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .delimited_by(just(Token::LBrace), just(Token::RBrace)),
    );

    let model_item = choice((
        primary_block.map_with(|items, e| ModelItem::Primary(e.span(), items)),
        optional_block.map(ModelItem::Optional),
        unique_block,
        foreign_block().map(ModelItem::Foreign),
        paginated_block.map(ModelItem::Paginated),
        nav_block,
        kv_block().map(ModelItem::Kv),
        r2_block().map(ModelItem::R2),
        keyfield_block,
        typed_field().map(ModelItem::Field),
    ));

    let use_tags = use_tag.repeated().collect::<Vec<_>>();

    use_tags
        .then_ignore(just(Token::Model))
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            model_item
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((tag_lists, (model_name, model_span)), items)| {
            map_model(model_name, model_span, tag_lists, items)
        })
}

fn map_model<'src>(
    model_name: &'src str,
    model_span: Span,
    tag_lists: Vec<(Vec<UseItem<'src>>, Span)>,
    items: Vec<ModelItem<'src>>,
) -> ModelBlock<'src> {
    let mut cruds: Vec<CrudKind> = Vec::new();
    let mut env_bindings: Vec<&str> = Vec::new();
    let use_tag = {
        let mut use_span: Option<Span> = None;
        for (items_list, tag_span) in tag_lists {
            for item in items_list {
                match item {
                    UseItem::Crud(c) => {
                        if !cruds.contains(&c) {
                            cruds.push(c);
                        }
                    }
                    UseItem::Binding(b) => {
                        env_bindings.push(b);
                    }
                }
            }

            use_span = Some(match use_span {
                Some(existing) => Span {
                    start: existing.start,
                    end: tag_span.end,
                    context: existing.context,
                },
                None => tag_span,
            });
        }

        use_span.map(|span| UseTag {
            span,
            cruds,
            env_bindings,
        })
    };

    let mut fields: Vec<Symbol<'src>> = Vec::new();
    let mut primary_fields: Vec<&'src str> = Vec::new();
    let mut key_fields: Vec<Symbol<'src>> = Vec::new();
    let mut unique_constraints: Vec<UniqueConstraint<'src>> = Vec::new();
    let mut kvs: Vec<KvBlock<'src>> = Vec::new();
    let mut r2s: Vec<R2Block<'src>> = Vec::new();
    let mut foreign_blocks: Vec<ForeignBlock<'src>> = Vec::new();
    let mut navigation_blocks: Vec<NavigationBlock<'src>> = Vec::new();

    for item in items {
        match item {
            ModelItem::Primary(_span, primary_items) => {
                for pi in primary_items {
                    match pi {
                        PrimaryItem::Field(mut sym) => {
                            sym.parent_name = model_name.into();
                            primary_fields.push(sym.name);
                            fields.push(sym);
                        }
                        PrimaryItem::Foreign(pf) => {
                            foreign_blocks.push(drain_foreign(
                                model_name,
                                pf,
                                &mut navigation_blocks,
                                &mut primary_fields,
                                &mut unique_constraints,
                            ));
                        }
                    }
                }
            }
            ModelItem::Optional(pfs) => {
                for pf in pfs {
                    let mut drained = drain_foreign(
                        model_name,
                        pf,
                        &mut navigation_blocks,
                        &mut primary_fields,
                        &mut unique_constraints,
                    );

                    drained.qualifier = Some(ForeignQualifier::Optional);
                    foreign_blocks.push(drained);
                }
            }
            ModelItem::Unique(span, unique_items) => {
                let mut constraint_names: Vec<&'src str> = Vec::new();
                for ui in unique_items {
                    match ui {
                        UniqueItem::Foreign(pf) => {
                            for sym in &pf.block.fields {
                                constraint_names.push(sym.name);
                            }
                            foreign_blocks.push(drain_foreign(
                                model_name,
                                pf,
                                &mut navigation_blocks,
                                &mut primary_fields,
                                &mut unique_constraints,
                            ));
                        }
                        UniqueItem::Field(mut sym) => {
                            sym.parent_name = model_name.into();
                            constraint_names.push(sym.name);
                            fields.push(sym);
                        }
                    }
                }
                unique_constraints.push(UniqueConstraint {
                    span,
                    fields: constraint_names,
                });
            }
            ModelItem::Foreign(pf) => {
                foreign_blocks.push(drain_foreign(
                    model_name,
                    pf,
                    &mut navigation_blocks,
                    &mut primary_fields,
                    &mut unique_constraints,
                ));
            }
            ModelItem::Field(mut sym) => {
                sym.parent_name = model_name.into();
                fields.push(sym);
            }
            ModelItem::Nav(mut nb) => {
                nb.field.parent_name = model_name.into();
                navigation_blocks.push(nb);
            }
            ModelItem::Kv(block) => kvs.push(block),
            ModelItem::R2(block) => r2s.push(block),
            ModelItem::Paginated(inner_items) => {
                for inner in inner_items {
                    match inner {
                        PaginatedItem::Kv(mut block) => {
                            block.is_paginated = true;
                            kvs.push(block);
                        }
                        PaginatedItem::R2(mut block) => {
                            block.is_paginated = true;
                            r2s.push(block);
                        }
                    }
                }
            }
            ModelItem::KeyField(mut syms) => {
                for sym in &mut syms {
                    sym.parent_name = model_name.into()
                }
                key_fields.extend(syms);
            }
        }
    }

    ModelBlock {
        symbol: Symbol {
            span: model_span,
            name: model_name,
            kind: SymbolKind::ModelDecl,
            ..Default::default()
        },
        use_tag,
        typed_idents: fields,
        primary_fields,
        key_fields,
        unique_constraints,
        kvs,
        r2s,
        navigation_blocks,
        foreign_blocks,
    }
}

fn drain_foreign<'src>(
    parent_name: &'src str,
    pf: ParsedForeign<'src>,
    navigation_blocks: &mut Vec<NavigationBlock<'src>>,
    primary_fields: &mut Vec<&'src str>,
    unique_constraints: &mut Vec<UniqueConstraint<'src>>,
) -> ForeignBlock<'src> {
    let mut block = pf.block;
    let mut unique_constraint = UniqueConstraint {
        span: block.span,
        fields: Vec::new(),
    };
    for sym in &mut block.fields {
        sym.parent_name = parent_name.into();

        match block.qualifier {
            Some(ForeignQualifier::Primary) => {
                primary_fields.push(sym.name);
            }
            Some(ForeignQualifier::Unique) => {
                unique_constraint.fields.push(sym.name);
            }
            _ => {}
        }
    }

    if !unique_constraint.fields.is_empty() {
        unique_constraints.push(unique_constraint);
    }

    if let Some(mut nav) = pf.nav {
        nav.parent_name = parent_name.into();
        navigation_blocks.push(NavigationBlock {
            span: nav.span,
            adj: block.adj.clone(),
            field: nav,
            is_one_to_one: true,
        });
    }
    block
}
