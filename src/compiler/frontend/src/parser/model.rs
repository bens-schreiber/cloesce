use std::path::PathBuf;

use chumsky::prelude::*;

use ast::CidlType;

use crate::{
    D1NavigationProperty, ForeignKey, KvR2, ModelBlock, SpannedTypedName, UnresolvedName,
    lexer::Token,
    parser::{Extra, cidl_type},
};

enum ModelField {
    Primary(Vec<UnresolvedName>),
    Unique(Vec<UnresolvedName>),
    Foreign(ForeignKey),
    Nav(D1NavigationProperty),
    Field(SpannedTypedName),
    KeyField(UnresolvedName),
    KvField(KvR2),
    R2Field(KvR2),
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
pub fn model_block<'t>() -> impl Parser<'t, &'t [Token], ModelBlock, Extra<'t>> {
    // @d1(binding)
    let d1_binding = just(Token::At)
        .ignore_then(just(Token::D1))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen))
        .or_not();

    // [primary ident1, ident2, ...]
    let primary_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("primary".into())))
        .ignore_then(
            select! { Token::Ident(name) => UnresolvedName(name) }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelField::Primary);

    // [unique ident1, ident2, ...]
    let unique_tag = just(Token::LBracket)
        .ignore_then(just(Token::Ident("unique".into())))
        .ignore_then(
            select! { Token::Ident(name) => UnresolvedName(name) }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelField::Unique);

    // [foreign ident1, ident2 -> TargetModel::ident3, ident4, ...]
    let foreign_tag = {
        let target_field_ref = select! { Token::Ident(model_name) => UnresolvedName(model_name) }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => UnresolvedName(field_name) });

        let source_field_ref = select! { Token::Ident(name) => UnresolvedName(name) };

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
            .map(|(fields, target_refs)| {
                // target_refs: Vec<(UnresolvedName, UnresolvedName)>
                // fields: Vec<UnresolvedName>
                // We expect the number of source fields and target_refs to match
                let (adj_model, _) = target_refs.first().cloned().unwrap();
                let references = fields
                    .into_iter()
                    .zip(target_refs.into_iter().map(|(_, f)| f))
                    .collect();
                ModelField::Foreign(ForeignKey {
                    adj_model,
                    references,
                })
            })
    };

    // [nav RelationName -> TargetModel::field1, field2, ...]
    // [nav RelationName <> TargetModel::field]
    let nav_tag = {
        let nav_key_ref = select! { Token::Ident(model_name) => UnresolvedName(model_name) }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(field_name) => UnresolvedName(field_name) });

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
            .ignore_then(select! { Token::Ident(name) => UnresolvedName(name) })
            .then_ignore(just(Token::Arrow))
            .then(nav_key_ref_list)
            .then_ignore(just(Token::RBracket))
            .map(|(field, to)| {
                let adj_model = UnresolvedName(to[0].0.0.clone());
                let fields = to.into_iter().map(|(_, f)| f).collect::<Vec<_>>();
                ModelField::Nav(D1NavigationProperty {
                    field,
                    adj_model,
                    fields,
                    is_many_to_many: false,
                })
            });

        let nav_many_to_many = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => UnresolvedName(name) })
            .then_ignore(just(Token::LAngle))
            .then_ignore(just(Token::RAngle))
            .then(nav_key_ref)
            .then_ignore(just(Token::RBracket))
            .map(|(field, (adj_model, f))| {
                ModelField::Nav(D1NavigationProperty {
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
        .ignore_then(select! { Token::Ident(name) => UnresolvedName(name) })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen));

    // @r2(bucketBinding, "formatString") -> (env_binding, format)
    let r2_tag = just(Token::At)
        .ignore_then(just(Token::R2))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => UnresolvedName(name) })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen));

    // @keyparam
    let key_param_tag = just(Token::At).ignore_then(just(Token::Ident("keyparam".into())));

    type Binding = (UnresolvedName, String);
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
        .then(cidl_type())
        .map(|(((key_param, kv, r2), (name, span)), cidl_type)| {
            if let Some((env_binding, format)) = kv {
                ModelField::KvField(KvR2 {
                    field: UnresolvedName(name),
                    span,
                    cidl_type,
                    format,
                    env_binding,
                })
            } else if let Some((env_binding, format)) = r2 {
                ModelField::R2Field(KvR2 {
                    field: UnresolvedName(name),
                    span,
                    cidl_type,
                    format,
                    env_binding,
                })
            } else if key_param {
                ModelField::KeyField(UnresolvedName(name))
            } else {
                ModelField::Field(SpannedTypedName {
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
        .map(|((d1_binding, (name, span)), items)| map_model(name, span, d1_binding, items))
}

fn map_model(
    name: String,
    span: SimpleSpan,
    d1_binding: Option<String>,
    items: Vec<ModelField>,
) -> ModelBlock {
    let mut fields: Vec<SpannedTypedName> = Vec::new();
    let mut foreign_keys: Vec<ForeignKey> = Vec::new();
    let mut navigation_properties: Vec<D1NavigationProperty> = Vec::new();
    let mut primary_keys: Vec<UnresolvedName> = Vec::new();
    let mut unique_constraints: Vec<Vec<UnresolvedName>> = Vec::new();
    let mut key_fields: Vec<UnresolvedName> = Vec::new();
    let mut kvs: Vec<KvR2> = Vec::new();
    let mut r2s: Vec<KvR2> = Vec::new();

    for item in items {
        match item {
            ModelField::Primary(cols) => primary_keys.extend(cols),
            ModelField::Unique(cols) => unique_constraints.push(cols),
            ModelField::Foreign(fk) => foreign_keys.push(fk),
            ModelField::Nav(nav) => navigation_properties.push(nav),
            ModelField::Field(field) => fields.push(field),
            ModelField::KeyField(name) => key_fields.push(name),
            ModelField::KvField(kv) => kvs.push(kv),
            ModelField::R2Field(r2) => r2s.push(r2),
        }
    }

    ModelBlock {
        span,
        name,
        file: PathBuf::new(), // TODO
        d1_binding: d1_binding.map(UnresolvedName),
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
