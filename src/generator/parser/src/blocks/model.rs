use chumsky::prelude::*;

use ast::CidlType;
use lexer::Token;

use crate::Extra;
use crate::blocks::sqlite_column_types;
use crate::parse_ast::{
    D1NavigationProperty, D1NavigationPropertyKind, ForeignKey, KvR2Field, ModelBlock, SpannedName,
    SpannedTypedName, UnresolvedName,
};

struct PendingForeignKey {
    adj_model_name: String,
    columns: Vec<String>,
}

struct PendingNavigation {
    name: String,
    adj_model: String,
    kind: PendingNavigationKind,
}

enum PendingNavigationKind {
    OneOrManyByFieldType { key_columns: Vec<String> },
    ManyToMany,
}

struct PendingField {
    name: String,
    name_span: chumsky::span::SimpleSpan,
    cidl_type: CidlType,
    key_param: bool,
    kv_navigation: Option<PendingKvNavigation>,
    r2_navigation: Option<PendingR2Navigation>,
}

struct PendingKvNavigation {
    namespace_binding: String,
    format: String,
}

struct PendingR2Navigation {
    bucket_binding: String,
    format: String,
}

enum ModelEntry {
    Primary(Vec<String>),
    Unique(Vec<String>),
    Foreign(PendingForeignKey),
    Nav(PendingNavigation),
    Field(PendingField),
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
            select! { Token::Ident(name) => name }
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(ModelEntry::Primary);

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
        .map(ModelEntry::Unique);

    // [foreign ident1, ident2 -> TargetModel::ident3, ident4, ...]
    let foreign_tag =
        {
            let foreign_target_column_ref = select! { Token::Ident(model_name) => model_name }
                .then_ignore(just(Token::DoubleColon))
                .then(select! { Token::Ident(column_name) => column_name });

            let foreign_source_column_ref = select! { Token::Ident(name) => name };

            let foreign_source_ref_list = foreign_source_column_ref
                .clone()
                .map(|col| vec![col])
                .or(foreign_source_column_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)));

            let foreign_target_ref_list = foreign_target_column_ref
                .clone()
                .map(|col| vec![col])
                .or(foreign_target_column_ref
                    .separated_by(just(Token::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LParen), just(Token::RParen)));

            just(Token::LBracket)
                .ignore_then(just(Token::Ident("foreign".into())))
                .ignore_then(foreign_source_ref_list)
                .then_ignore(just(Token::Arrow))
                .then(foreign_target_ref_list)
                .then_ignore(just(Token::RBracket))
                .map(|(from, to)| {
                    let to_model_name = to[0].0.clone();
                    ModelEntry::Foreign(PendingForeignKey {
                        adj_model_name: to_model_name,
                        columns: from,
                    })
                })
        };

    // [nav RelationName -> TargetModel::ident7, ident8, ...]
    let nav_tag = {
        let nav_key_ref = select! { Token::Ident(model_name) => model_name }
            .then_ignore(just(Token::DoubleColon))
            .then(select! { Token::Ident(column_name) => column_name });

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
            .map(|(name, to)| {
                let to_model = to[0].0.clone();
                let key_columns = to
                    .into_iter()
                    .map(|(_, col_name)| col_name)
                    .collect::<Vec<_>>();
                ModelEntry::Nav(PendingNavigation {
                    name,
                    adj_model: to_model,
                    kind: PendingNavigationKind::OneOrManyByFieldType { key_columns },
                })
            });

        // [nav ident1 <> TargetModel::ident2]
        let nav_many_to_many = just(Token::LBracket)
            .ignore_then(just(Token::Ident("nav".into())))
            .ignore_then(select! { Token::Ident(name) => name })
            .then_ignore(just(Token::LAngle))
            .then_ignore(just(Token::RAngle))
            .then(nav_key_ref)
            .then_ignore(just(Token::RBracket))
            .map(|(name, (to_model, _))| {
                ModelEntry::Nav(PendingNavigation {
                    name,
                    adj_model: to_model,
                    kind: PendingNavigationKind::ManyToMany,
                })
            });

        nav_arrow.or(nav_many_to_many)
    };

    let object_name = select! { Token::Ident(name) => name };
    let object_array_generic = just(Token::Ident("Array".into()))
        .ignore_then(just(Token::LAngle))
        .ignore_then(object_name.clone())
        .then_ignore(just(Token::RAngle))
        .map(|object_name| CidlType::array(CidlType::Object(object_name)));

    let object_type = object_array_generic.or(object_name.map(CidlType::Object));

    let model_field_type = choice((
        sqlite_column_types(),
        just(Token::R2Object).map(|_| CidlType::R2Object),
        object_type,
    ));

    // @kv(namespaceBinding, "formatString")
    let kv_tag = just(Token::At)
        .ignore_then(just(Token::Kv))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen))
        .map(|(namespace_binding, format)| PendingKvNavigation {
            namespace_binding,
            format,
        });

    // @r2(bucketBinding, "formatString")
    let r2_tag = just(Token::At)
        .ignore_then(just(Token::R2))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::Comma))
        .then(select! { Token::StringLit(value) => unquote_string_literal(value) })
        .then_ignore(just(Token::RParen))
        .map(|(bucket_binding, format)| PendingR2Navigation {
            bucket_binding,
            format,
        });

    // @keyparam
    let key_param_tag = just(Token::At)
        .ignore_then(just(Token::Ident("keyparam".into())))
        .to(());

    // @kv(...) | @r2(...) | @keyparam
    let anchored_field_tags = choice((
        key_param_tag.map(|_| (true, None, None)),
        kv_tag.map(|kv| (false, Some(kv), None)),
        r2_tag.map(|r2| (false, None, Some(r2))),
    ))
    .or_not()
    .map(|tags| tags.unwrap_or((false, None, None)));

    let field = anchored_field_tags
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then_ignore(just(Token::Colon))
        .then(model_field_type)
        .map(
            |(((key_param, kv_navigation, r2_navigation), (name, name_span)), cidl_type)| {
                ModelEntry::Field(PendingField {
                    name,
                    name_span,
                    cidl_type,
                    key_param,
                    kv_navigation,
                    r2_navigation,
                })
            },
        );

    d1_binding
        .then_ignore(just(Token::Model))
        .then(
            select! { Token::Ident(name) => name }
                .map_with(|name, e| SpannedName { name, span: e.span() }),
        )
        .then(
            choice((primary_tag, unique_tag, foreign_tag, nav_tag, field))
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((d1_binding, span_name), items)| map_model(span_name, d1_binding, items))
}

fn map_model(
    span_name: SpannedName,
    d1_binding: Option<String>,
    items: Vec<ModelEntry>,
) -> ModelBlock {
    let mut columns: Vec<SpannedTypedName> = Vec::new();
    let mut foreign_keys: Vec<ForeignKey> = Vec::new();
    let mut pending_navs: Vec<PendingNavigation> = Vec::new();
    let mut primary_key_names: Vec<String> = Vec::new();
    let mut unique_constraint_names: Vec<Vec<String>> = Vec::new();
    let mut key_field_names: Vec<String> = Vec::new();
    let mut kv_fields: Vec<KvR2Field> = Vec::new();
    let mut r2_fields: Vec<KvR2Field> = Vec::new();

    for item in items {
        match item {
            ModelEntry::Primary(cols) => primary_key_names.extend(cols),
            ModelEntry::Unique(cols) => unique_constraint_names.push(cols),
            ModelEntry::Foreign(fk) => foreign_keys.push(ForeignKey {
                adj_model_name: UnresolvedName(fk.adj_model_name),
                column_names: fk.columns.into_iter().map(UnresolvedName).collect(),
            }),
            ModelEntry::Nav(nav) => pending_navs.push(nav),
            ModelEntry::Field(field) => {
                let typed_name = SpannedTypedName {
                    span: field.name_span,
                    name: field.name.clone(),
                    ty: field.cidl_type.clone(),
                };

                if field.key_param {
                    key_field_names.push(field.name.clone());
                }

                if let Some(kv) = field.kv_navigation {
                    kv_fields.push(KvR2Field {
                        typed_name: typed_name.clone(),
                        format: kv.format,
                        env_binding: UnresolvedName(kv.namespace_binding),
                    });
                }

                if let Some(r2) = field.r2_navigation {
                    r2_fields.push(KvR2Field {
                        typed_name: typed_name.clone(),
                        format: r2.format,
                        env_binding: UnresolvedName(r2.bucket_binding),
                    });
                }

                columns.push(typed_name);
            }
        }
    }

    let primary_key_columns = primary_key_names
        .iter()
        .filter_map(|name| columns.iter().find(|c| &c.name == name).cloned())
        .collect();

    let key_fields = key_field_names
        .iter()
        .filter_map(|name| columns.iter().find(|c| &c.name == name).cloned())
        .collect();

    let unique_constraints = unique_constraint_names
        .into_iter()
        .map(|names| names.into_iter().map(UnresolvedName).collect())
        .collect();

    let navigation_properties = pending_navs
        .into_iter()
        .map(|nav| {
            let kind = match nav.kind {
                PendingNavigationKind::ManyToMany => D1NavigationPropertyKind::ManyToMany {
                    column: UnresolvedName(nav.name.clone()),
                },
                PendingNavigationKind::OneOrManyByFieldType { key_columns } => {
                    let is_array = columns
                        .iter()
                        .find(|c| c.name == nav.name)
                        .map(|c| matches!(&c.ty, CidlType::Array(_)))
                        .unwrap_or(false);

                    let cols = key_columns.into_iter().map(UnresolvedName).collect();
                    if is_array {
                        D1NavigationPropertyKind::OneToMany { columns: cols }
                    } else {
                        D1NavigationPropertyKind::OneToOne { columns: cols }
                    }
                }
            };

            D1NavigationProperty {
                field_name: UnresolvedName(nav.name),
                adj_model_name: UnresolvedName(nav.adj_model),
                kind,
            }
        })
        .collect();

    ModelBlock {
        span_name,
        d1_binding: d1_binding.map(UnresolvedName),
        columns,
        primary_key_columns,
        key_fields,
        kv_fields,
        r2_fields,
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
