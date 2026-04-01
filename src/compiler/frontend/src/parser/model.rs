use std::borrow::Cow;

use chumsky::prelude::*;

use ast::{CidlType, CrudKind};

use crate::{
    D1Tag, ForeignKeyTag, KeyFieldTag, KvR2Tag, ModelBlock, NavigationTag, PrimaryKeyTag, Symbol,
    SymbolKind, UniqueTag,
    lexer::Token,
    parser::{Extra, Span, TokenInput, cidl_type},
};

enum ModelTag<'src> {
    Crud(Vec<CrudKind>),
    D1(&'src str, Span),
}

enum ModelField<'src> {
    Primary(Span, Vec<&'src str>),
    Unique(Span, Vec<&'src str>),
    Foreign(ForeignKeyTag<'src>),
    Nav {
        span: Span,
        field: &'src str,

        /// (model_name or None for current model, field_name)
        fields: Vec<(Option<&'src str>, &'src str)>,

        is_many_to_many: bool,
    },
    Field(Symbol<'src>),
    KeyField(&'src str, Span, CidlType<'src>),
    KvField(Symbol<'src>, KvR2Tag<'src>),
    R2Field(Symbol<'src>, KvR2Tag<'src>),
}

/// Parses a block of the form:
///
///```cloesce
/// @d1(binding)
/// @crud(get | save | list, ...)
/// model ModelName {
///   ident1: sqlite_column_type
///
///   @kv(namespaceBinding, "formatString") | @r2(bucketBinding, "formatString") | @keyparam
///   ident2: cidl_type
///
///   [primary ident3, ident4, ...]
///   [unique ident5, ident6, ...]
///   [foreign ident5 -> TargetModel::ident6]
///   [nav RelationName -> ident7, TargetModel::ident8, ...]
/// }
/// ```
pub fn model_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ModelBlock<'src>, Extra<'tokens, 'src>> {
    // @crud(get | save | list, ...)
    let crud_tag = just(Token::At)
        .ignore_then(just(Token::Crud))
        .ignore_then(just(Token::LParen))
        .ignore_then(
            crud_kind()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RParen));

    // @d1(binding)
    let d1_binding = just(Token::At)
        .ignore_then(just(Token::D1))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen))
        .map_with(|name, e| (name, e.span()));

    // [primary ident1, ident2, ...]
    let primary_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("primary")))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map_with(|cols, e| ModelField::Primary(e.span(), cols));

    // [unique ident1, ident2, ...]
    let unique_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("unique")))
        .ignore_then(
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map_with(|cols, e| ModelField::Unique(e.span(), cols));

    // [foreign ident1, ident2 -> TargetModel::ident3, ident4, ...]
    let foreign_tag = {
        let target_field_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => field_name });

        let source_field_ref = select! { Token::Ident(name) => name };

        let source_field_list = source_field_ref.map(|f| vec![f]).or(source_field_ref
            .separated_by(just(Token::Comma))
            .at_least(1)
            .collect::<Vec<_>>()
            .delimited_by(just(Token::LParen), just(Token::RParen)));

        let target_field_list = target_field_ref
            .clone()
            .map(|f| vec![f])
            .or(target_field_ref
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        just(Token::LBracket)
            .ignore_then(just(Token::Ident("foreign")))
            .ignore_then(source_field_list)
            .then_ignore(just(Token::Arrow))
            .then(target_field_list)
            .then_ignore(just(Token::RBracket))
            .map_with(|(fields, target_refs), e| {
                let (adj_model, _) = target_refs.first().cloned().unwrap();
                let references = fields
                    .into_iter()
                    .zip(target_refs.into_iter().map(|(_, f)| f))
                    .collect();
                ModelField::Foreign(ForeignKeyTag {
                    adj_model,
                    references,
                    span: e.span(),
                })
            })
    };

    // [nav RelationName -> Model::field1, field2, ...]
    // [nav RelationName -> field1, field2, ...]  (assumes current model)
    // [nav RelationName <> Model::field]
    // [nav RelationName <> field]  (assumes current model)
    let nav_tag = {
        // A field ref is either Model::field or just field (None means current model)
        let nav_key_ref = select! { Token::Ident(name) => name }
            .then(
                just(Token::DoubleColon)
                    .ignore_then(select! { Token::Ident(field_name) => field_name })
                    .or_not(),
            )
            .map(|(first, second)| match second {
                Some(field) => (Some(first), field),
                None => (None, first),
            });

        let nav_key_ref_list = nav_key_ref
            .clone()
            .map(|key_ref| vec![key_ref])
            .or(nav_key_ref
                .clone()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)));

        let nav_arrow = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav")))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::Arrow))
            .then(nav_key_ref_list)
            .then_ignore(just(Token::RBracket))
            .map_with(|(field, fields), e| ModelField::Nav {
                span: e.span(),
                field,
                fields,
                is_many_to_many: false,
            });

        let nav_many_to_many = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav")))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::LAngle))
            .then_ignore(just(Token::RAngle))
            .then(nav_key_ref)
            .then_ignore(just(Token::RBracket))
            .map_with(|(field, key_ref), e| ModelField::Nav {
                span: e.span(),
                field,
                fields: vec![key_ref],
                is_many_to_many: true,
            });

        nav_arrow.or(nav_many_to_many)
    };

    // @kv(namespaceBinding, "formatString") -> (env_binding, format)
    let kv_tag = just(Token::At)
        .ignore_then(just(Token::Kv))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => value })
        .then_ignore(just(Token::RParen));

    // @r2(bucketBinding, "formatString") -> (env_binding, format)
    let r2_tag = just(Token::At)
        .ignore_then(just(Token::R2))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => value })
        .then_ignore(just(Token::RParen));

    // @keyparam
    let key_param_tag = just(Token::At).ignore_then(just(Token::Ident("keyparam")));

    let field_tag = choice((
        key_param_tag.map(|_| (true, None::<(&str, &str)>, None::<(&str, &str)>)),
        kv_tag.map(|kv| (false, Some(kv), None::<(&str, &str)>)),
        r2_tag.map(|r2| (false, None::<(&str, &str)>, Some(r2))),
    ))
    .or_not()
    .map(|opt| opt.unwrap_or((false, None::<(&str, &str)>, None::<(&str, &str)>)));

    let field = field_tag
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|(((key_param, kv, r2), (name, span)), cidl_type)| {
            let typed = Symbol {
                span,
                name,
                cidl_type,
                kind: SymbolKind::ModelField,
                ..Default::default()
            };
            match (key_param, kv, r2) {
                (_, Some((env_binding, format)), _) => ModelField::KvField(
                    typed,
                    KvR2Tag {
                        field: name,
                        span,
                        format,
                        env_binding,
                    },
                ),
                (_, _, Some((env_binding, format))) => ModelField::R2Field(
                    typed,
                    KvR2Tag {
                        field: name,
                        span,
                        format,
                        env_binding,
                    },
                ),
                (true, _, _) => ModelField::KeyField(name, span, typed.cidl_type),
                _ => ModelField::Field(typed),
            }
        });

    let model_tags = choice((
        crud_tag.map(ModelTag::Crud),
        d1_binding.map(|(name, span)| ModelTag::D1(name, span)),
    ))
    .repeated()
    .collect::<Vec<_>>();

    model_tags
        .then_ignore(just(Token::Model))
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            choice((primary_tag, unique_tag, foreign_tag, nav_tag, field))
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((tags, (model_name, model_span)), items)| {
            let mut cruds = Vec::new();
            let mut d1_tag = None;
            for tag in tags {
                match tag {
                    ModelTag::Crud(c) => cruds = c,
                    ModelTag::D1(b, span) => {
                        d1_tag = Some(D1Tag {
                            span,
                            env_binding: b,
                        })
                    }
                }
            }

            let mut fields: Vec<Symbol<'src>> = Vec::new();
            let mut foreign_keys: Vec<ForeignKeyTag<'src>> = Vec::new();
            let mut navigation_properties: Vec<NavigationTag<'src>> = Vec::new();
            let mut primary_keys: Vec<PrimaryKeyTag<'src>> = Vec::new();
            let mut unique_constraints: Vec<UniqueTag<'src>> = Vec::new();
            let mut key_fields: Vec<KeyFieldTag<'src>> = Vec::new();
            let mut kvs: Vec<KvR2Tag<'src>> = Vec::new();
            let mut r2s: Vec<KvR2Tag<'src>> = Vec::new();

            for item in items {
                match item {
                    ModelField::Primary(span, cols) => {
                        for field in cols {
                            primary_keys.push(PrimaryKeyTag { span, field });
                        }
                    }
                    ModelField::Unique(span, cols) => {
                        unique_constraints.push(UniqueTag { span, fields: cols });
                    }
                    ModelField::Foreign(fk) => foreign_keys.push(fk),
                    ModelField::Nav {
                        span,
                        field,
                        fields: nav_fields,
                        is_many_to_many,
                    } => {
                        let resolved = nav_fields
                            .into_iter()
                            .map(|(model, f)| (model.unwrap_or(model_name), f))
                            .collect();
                        navigation_properties.push(NavigationTag {
                            span,
                            field,
                            fields: resolved,
                            is_many_to_many,
                        });
                    }
                    ModelField::Field(mut f) => {
                        f.parent_name = Cow::Borrowed(model_name);
                        fields.push(f);
                    }
                    ModelField::KeyField(field, span, cidl_type) => {
                        key_fields.push(KeyFieldTag { span, field });
                        fields.push(Symbol {
                            span,
                            name: field,
                            cidl_type,
                            kind: SymbolKind::ModelField,
                            parent_name: Cow::Borrowed(model_name),
                        });
                    }
                    ModelField::KvField(mut f, tag) => {
                        f.parent_name = Cow::Borrowed(model_name);
                        fields.push(f);
                        kvs.push(tag);
                    }
                    ModelField::R2Field(mut f, tag) => {
                        f.parent_name = Cow::Borrowed(model_name);
                        fields.push(f);
                        r2s.push(tag);
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
                d1_binding: d1_tag,
                fields,
                primary_keys,
                key_fields,
                kvs,
                r2s,
                navigation_properties,
                foreign_keys,
                unique_constraints,
                cruds,
            }
        })
}

fn crud_kind<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, CrudKind, Extra<'tokens, 'src>> {
    choice((
        just(Token::Get).map(|_| CrudKind::Get),
        select! { Token::Ident(name) if name == "get" => CrudKind::Get },
        select! { Token::Ident(name) if name == "save" => CrudKind::Save },
        select! { Token::Ident(name) if name == "list" => CrudKind::List },
    ))
}
