use std::path::PathBuf;

use chumsky::prelude::*;

use ast::CidlType;

use crate::{
    D1Tag, ForeignKeyTag, KeyFieldTag, KvR2Tag, ModelBlock, NavigationTag, PrimaryKeyTag,
    SpannedTypedName, UniqueTag,
    lexer::Token,
    parser::{Extra, IdScope, It, cidl_type},
};

struct PendingForeignKeyTag {
    span: SimpleSpan,
    adj_model: String,

    /// (source_field_name, target_field_name)
    references: Vec<(String, String)>,
}

struct PendingNavTag {
    span: SimpleSpan,
    field: String,
    adj_model: String,
    fields: Vec<String>,
    is_many_to_many: bool,
}

struct PendingKvR2Tag {
    field: String,
    span: SimpleSpan,
    cidl_type: CidlType,
    format: String,
    env_binding: String,
}

enum ModelField {
    Primary(SimpleSpan, Vec<String>),
    Unique(SimpleSpan, Vec<String>),
    Foreign(PendingForeignKeyTag),
    Nav(PendingNavTag),
    Field(SpannedTypedName),
    KeyField(SpannedTypedName),
    KvField(PendingKvR2Tag),
    R2Field(PendingKvR2Tag),
}

/// Parses a block of the form:
///
///```cloesce
/// @d1(binding)
/// model ModelName {
///   ident1: sqlite_column_type
///
///   @kv(namespaceBinding, "formatString") | @r2(bucketBinding, "formatString") | @keyparam
///   ident2: cidl_type
///
///   [primary ident3, ident4, ...]
///   [unique ident5, ident6, ...]
///   [foreign ident5 -> TargetModel::ident6]
///   [nav RelationName -> TargetModel::ident7, ident8, ...]
/// }
/// ```
pub fn model_block<'t>(it: It) -> impl Parser<'t, &'t [Token], ModelBlock, Extra<'t>> {
    // @d1(binding)
    let d1_binding = just(Token::At)
        .ignore_then(just(Token::D1))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen))
        .map_with(|name, e| (name, e.span()))
        .or_not();

    // [primary ident1, ident2, ...]
    let primary_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("primary".into())))
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
        .ignore_then(just(Token::Ident("unique".into())))
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

        let source_field_list = source_field_ref
            .clone()
            .map(|f| vec![f])
            .or(source_field_ref
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
            .ignore_then(just(Token::Ident("foreign".into())))
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
                ModelField::Foreign(PendingForeignKeyTag {
                    adj_model,
                    references,
                    span: e.span(),
                })
            })
    };

    // [nav RelationName -> TargetModel::field1, field2, ...]
    // [nav RelationName <> TargetModel::field]
    let nav_tag = {
        let nav_key_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => field_name });

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
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::Arrow))
            .then(nav_key_ref_list)
            .then_ignore(just(Token::RBracket))
            .map_with(|(field, to), e| {
                let adj_model = to[0].0.clone();
                let fields = to.into_iter().map(|(_, f)| f).collect::<Vec<_>>();
                ModelField::Nav(PendingNavTag {
                    span: e.span(),
                    field,
                    adj_model,
                    fields,
                    is_many_to_many: false,
                })
            });

        let nav_many_to_many = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::LAngle))
            .then_ignore(just(Token::RAngle))
            .then(nav_key_ref)
            .then_ignore(just(Token::RBracket))
            .map_with(|(field, (adj_model, f)), e| {
                ModelField::Nav(PendingNavTag {
                    span: e.span(),
                    field,
                    adj_model,
                    fields: vec![f],
                    is_many_to_many: true,
                })
            });

        nav_arrow.or(nav_many_to_many)
    };

    // @kv(namespaceBinding, "formatString") -> (env_binding, format)
    let kv_tag = just(Token::At)
        .ignore_then(just(Token::Kv))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen));

    // @r2(bucketBinding, "formatString") -> (env_binding, format)
    let r2_tag = just(Token::At)
        .ignore_then(just(Token::R2))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen));

    // @keyparam
    let key_param_tag = just(Token::At).ignore_then(just(Token::Ident("keyparam".into())));

    let st_field = it.clone();
    type Binding = (String, String);
    let field_tag = choice((
        key_param_tag.map(|_| (true, None::<Binding>, None::<Binding>)),
        kv_tag.map(|kv| (false, Some(kv), None)),
        r2_tag.map(|r2| (false, None, Some(r2))),
    ))
    .or_not()
    .map(|opt| opt.unwrap_or((false, None, None)));

    let field = field_tag
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then_ignore(just(Token::Colon))
        .then(cidl_type(st_field))
        .map(|(((key_param, kv, r2), (name, span)), cidl_type)| {
            if let Some((env_binding, format)) = kv {
                ModelField::KvField(PendingKvR2Tag {
                    field: name,
                    span,
                    cidl_type,
                    format,
                    env_binding,
                })
            } else if let Some((env_binding, format)) = r2 {
                ModelField::R2Field(PendingKvR2Tag {
                    field: name,
                    span,
                    cidl_type,
                    format,
                    env_binding,
                })
            } else if key_param {
                ModelField::KeyField(SpannedTypedName {
                    id: 0, // resolved in map_model
                    span,
                    name,
                    cidl_type,
                })
            } else {
                ModelField::Field(SpannedTypedName {
                    id: 0, // resolved in map_model
                    span,
                    name,
                    cidl_type,
                })
            }
        });

    d1_binding
        .then_ignore(just(Token::Model))
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            choice((primary_tag, unique_tag, foreign_tag, nav_tag, field))
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(move |((d1_binding, (name, span)), items)| {
            map_model(&it, name, span, d1_binding, items)
        })
}

fn map_model(
    it: &It,
    name: String,
    span: SimpleSpan,
    d1_binding: Option<(String, SimpleSpan)>,
    items: Vec<ModelField>,
) -> ModelBlock {
    let model_scope = IdScope::Model(name.clone());
    let id = it.borrow_mut().intern(name.clone(), IdScope::Global);
    let d1_binding = d1_binding.map(|(b, d1_span)| {
        let id = it.borrow_mut().new_id();
        let env_binding = it.borrow_mut().intern(b, IdScope::Env);
        D1Tag {
            id,
            span: d1_span,
            env_binding,
        }
    });

    let mut fields: Vec<SpannedTypedName> = Vec::new();
    let mut foreign_keys: Vec<ForeignKeyTag> = Vec::new();
    let mut navigation_properties: Vec<NavigationTag> = Vec::new();
    let mut primary_keys: Vec<PrimaryKeyTag> = Vec::new();
    let mut unique_constraints: Vec<UniqueTag> = Vec::new();
    let mut key_fields: Vec<KeyFieldTag> = Vec::new();
    let mut kvs: Vec<KvR2Tag> = Vec::new();
    let mut r2s: Vec<KvR2Tag> = Vec::new();

    for item in items {
        match item {
            ModelField::Primary(pk_span, cols) => {
                for col in cols {
                    let field = it.borrow_mut().intern(col, model_scope.clone());
                    primary_keys.push(PrimaryKeyTag {
                        id: it.borrow_mut().new_id(),
                        span: pk_span,
                        field,
                    });
                }
            }
            ModelField::Unique(span, cols) => {
                let fields = cols
                    .into_iter()
                    .map(|c| it.borrow_mut().intern(c, model_scope.clone()))
                    .collect();
                unique_constraints.push(UniqueTag {
                    id: it.borrow_mut().new_id(),
                    span,
                    fields,
                });
            }
            ModelField::Foreign(fk) => {
                let adj_model = it
                    .borrow_mut()
                    .intern(fk.adj_model.clone(), IdScope::Global);
                let adj_scope = IdScope::Model(fk.adj_model);
                let references = fk
                    .references
                    .into_iter()
                    .map(|(src, tgt)| {
                        let src_ref = it.borrow_mut().intern(src, model_scope.clone());
                        let tgt_ref = it.borrow_mut().intern(tgt, adj_scope.clone());
                        (src_ref, tgt_ref)
                    })
                    .collect();
                foreign_keys.push(ForeignKeyTag {
                    id: it.borrow_mut().new_id(),
                    span: fk.span,
                    adj_model,
                    references,
                });
            }
            ModelField::Nav(nav) => {
                let field = it.borrow_mut().intern(nav.field, model_scope.clone());
                let adj_model = it
                    .borrow_mut()
                    .intern(nav.adj_model.clone(), IdScope::Global);
                let adj_scope = IdScope::Model(nav.adj_model);
                let fields_refs = nav
                    .fields
                    .into_iter()
                    .map(|f| it.borrow_mut().intern(f, adj_scope.clone()))
                    .collect();
                navigation_properties.push(NavigationTag {
                    id: it.borrow_mut().new_id(),
                    span: nav.span,
                    field,
                    adj_model,
                    fields: fields_refs,
                    is_many_to_many: nav.is_many_to_many,
                });
            }
            ModelField::Field(mut f) => {
                f.id = it.borrow_mut().intern(f.name.clone(), model_scope.clone());
                fields.push(f);
            }
            ModelField::KeyField(mut f) => {
                f.id = it.borrow_mut().intern(f.name.clone(), model_scope.clone());
                key_fields.push(KeyFieldTag {
                    id: it.borrow_mut().new_id(),
                    span: f.span,
                    field: f.id,
                });
                fields.push(f);
            }
            ModelField::KvField(kv) => {
                let field = it
                    .borrow_mut()
                    .intern(kv.field.clone(), model_scope.clone());
                let env_binding = it.borrow_mut().intern(kv.env_binding, IdScope::Env);
                fields.push(SpannedTypedName {
                    id: field,
                    span: kv.span,
                    name: kv.field,
                    cidl_type: kv.cidl_type,
                });
                kvs.push(KvR2Tag {
                    id: it.borrow_mut().new_id(),
                    field,
                    span: kv.span,
                    format: kv.format,
                    env_binding,
                });
            }
            ModelField::R2Field(r2) => {
                let field = it
                    .borrow_mut()
                    .intern(r2.field.clone(), model_scope.clone());
                let env_binding = it.borrow_mut().intern(r2.env_binding, IdScope::Env);
                fields.push(SpannedTypedName {
                    id: field,
                    span: r2.span,
                    name: r2.field,
                    cidl_type: r2.cidl_type,
                });
                r2s.push(KvR2Tag {
                    id: it.borrow_mut().new_id(),
                    field,
                    span: r2.span,
                    format: r2.format,
                    env_binding,
                });
            }
        }
    }

    ModelBlock {
        id,
        span,
        name,
        file: PathBuf::new(),
        d1_binding,
        fields,
        primary_keys,
        key_fields,
        kvs,
        r2s,
        navigation_properties,
        foreign_keys,
        unique_constraints,
    }
}

fn unquote_string_literal(literal: String) -> String {
    literal
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&literal)
        .to_owned()
}
