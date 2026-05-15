use compiler_test::lex_and_ast;
use frontend::{
    ArgumentLiteral, Ast, AstBlockKind, EnvBindingBlockKind, ForeignBlock, Keyword, ModelBlock,
    ModelBlockKind, PaginatedBlockKind, Spd, SqlBlockKind, Symbol, Tag,
};
use idl::{CidlType, CrudKind, HttpVerb};

fn adj_matches(adj: &[(Symbol, Symbol)], expected: &[(&str, &str)]) -> bool {
    adj.len() == expected.len()
        && adj
            .iter()
            .zip(expected)
            .all(|((m, f), (em, ef))| m.name == *em && f.name == *ef)
}

#[test]
fn env_block() {
    // Act
    let ast = lex_and_ast(
        r#"

        env {
            d1 { db db2 }
            r2 { assets }
            kv { cache }

            vars {
                api_url: string
                max_retries: int
                threshold: real
                created_at: date
                payload: json
                enabled: bool
            }
        }
        "#,
    );

    // Assert
    let env_blocks = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Env(blocks) => Some(blocks),
            _ => None,
        })
        .expect("env block to be present");

    let d1_bindings = env_blocks
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner.kind {
            EnvBindingBlockKind::D1 => {
                Some(spd.inner.symbols.iter().map(|s| s.name).collect::<Vec<_>>())
            }
            _ => None,
        })
        .expect("d1 block to be present");
    assert_eq!(d1_bindings, vec!["db", "db2"]);

    let r2_bindings = env_blocks
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner.kind {
            EnvBindingBlockKind::R2 => {
                Some(spd.inner.symbols.iter().map(|s| s.name).collect::<Vec<_>>())
            }
            _ => None,
        })
        .expect("r2 block to be present");
    assert_eq!(r2_bindings, vec!["assets"]);

    let kv_bindings = env_blocks
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner.kind {
            EnvBindingBlockKind::Kv => {
                Some(spd.inner.symbols.iter().map(|s| s.name).collect::<Vec<_>>())
            }
            _ => None,
        })
        .expect("kv block to be present");
    assert_eq!(kv_bindings, vec!["cache"]);

    let vars = env_blocks
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner.kind {
            EnvBindingBlockKind::Var => Some(
                spd.inner
                    .symbols
                    .iter()
                    .map(|s| (s.name, &s.cidl_type))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("vars block to be present");

    assert_eq!(
        vars,
        vec![
            ("api_url", &CidlType::String),
            ("max_retries", &CidlType::Int),
            ("threshold", &CidlType::Real),
            ("created_at", &CidlType::DateIso),
            ("payload", &CidlType::Json),
            ("enabled", &CidlType::Boolean)
        ]
    )
}

#[test]
fn poo_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        poo Address {
            street: string
            city: string
            zipcode: option<string>
        }

        poo User {
            id: int
            name: string
            email: string
            age: option<int>
            active: bool
            balance: real
            created: date
            address: Address
            tags: array<string>
            metadata: option<json>
            optional_items: option<array<Item>>
            nullable_arrays: array<option<string>>
        }

        poo Container {
            items: array<Item>
            nested: array<array<int>>
        }
        "#,
    );

    // Assert
    let address = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::PlainOldObject(p) if p.symbol.name == "Address" => Some(p),
            _ => None,
        })
        .expect("Address poo to be present");
    assert_eq!(address.fields.len(), 3);

    let zipcode = address
        .fields
        .iter()
        .find(|f| f.name == "zipcode")
        .expect("zipcode field to be present");
    assert_eq!(zipcode.cidl_type, CidlType::nullable(CidlType::String));

    let user = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::PlainOldObject(p) if p.symbol.name == "User" => Some(p),
            _ => None,
        })
        .expect("User poo to be present");
    assert_eq!(user.fields.len(), 12);

    assert_eq!(
        user.fields
            .iter()
            .map(|f| (f.name, f.cidl_type.clone()))
            .collect::<Vec<_>>(),
        vec![
            ("id", CidlType::Int),
            ("name", CidlType::String),
            ("email", CidlType::String),
            ("age", CidlType::nullable(CidlType::Int)),
            ("active", CidlType::Boolean),
            ("balance", CidlType::Real),
            ("created", CidlType::DateIso),
            ("address", CidlType::UnresolvedReference { name: "Address" }),
            ("tags", CidlType::array(CidlType::String)),
            ("metadata", CidlType::nullable(CidlType::Json)),
            (
                "optional_items",
                CidlType::nullable(CidlType::array(CidlType::UnresolvedReference {
                    name: "Item",
                }))
            ),
            (
                "nullable_arrays",
                CidlType::array(CidlType::nullable(CidlType::String))
            )
        ]
    );

    let container = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::PlainOldObject(p) if p.symbol.name == "Container" => Some(p),
            _ => None,
        })
        .expect("Container poo to be present");
    assert_eq!(container.fields.len(), 2);
    assert_eq!(
        container
            .fields
            .iter()
            .find(|f| f.name == "nested")
            .unwrap()
            .cidl_type,
        CidlType::array(CidlType::array(CidlType::Int))
    );
}

#[test]
fn inject_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        inject {
            OpenApiService
        }

        inject {
            YouTubeApi
            SlackApi
        }
        "#,
    );

    // Assert
    let all_injected: Vec<&str> = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::Inject(i) => Some(i),
            _ => None,
        })
        .flat_map(|i| i.symbols.iter())
        .map(|s| s.name)
        .collect();

    assert_eq!(
        all_injected,
        vec!["OpenApiService", "YouTubeApi", "SlackApi"]
    );
}

#[test]
fn service_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        service {
            MyAppService
            AnotherService
        }
        api MyAppService {
            post createItem(
                name: string,
                count: int
            ) -> string
        }

        api MyAppService {
            get listItems() -> array<string>
        }
        "#,
    );

    // Assert: the single `service { ... }` block contains two service symbols
    let services: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::Service(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(services.len(), 1);
    let names: Vec<_> = services[0].symbols.iter().map(|s| s.name).collect();
    assert_eq!(names, vec!["MyAppService", "AnotherService"]);

    let api_blocks: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::Api(a) => Some(a),
            _ => None,
        })
        .filter(|a| a.symbol.name == "MyAppService")
        .collect();
    assert_eq!(
        api_blocks.len(),
        2,
        "should have two separate api blocks for MyAppService"
    );

    let create_block = api_blocks
        .iter()
        .find(|a| {
            a.methods
                .iter()
                .any(|m| m.inner.symbol.name == "createItem")
        })
        .expect("block with createItem");
    let create = create_block
        .methods
        .iter()
        .find(|m| m.inner.symbol.name == "createItem")
        .unwrap();
    assert!(matches!(create.inner.http_verb, HttpVerb::Post));
    assert_eq!(create.inner.parameters.len(), 2);
    assert_eq!(create.inner.return_type, CidlType::String);

    let list_block = api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.inner.symbol.name == "listItems"))
        .expect("block with listItems");
    let list = list_block
        .methods
        .iter()
        .find(|m| m.inner.symbol.name == "listItems")
        .unwrap();
    assert!(matches!(list.inner.http_verb, HttpVerb::Get));
}

#[test]
fn model_primary_unique_optional_foreign() {
    let ast = lex_and_ast(
        r#"
        [use d1_db]
        [use d2_db]
        [crud get, save, list]
        model M {
            score: real

            primary {
                id: int
                foreign(Company::id) { companyId }
                foreign(Parent::orgId, Parent::userId) { orgId userId }
            }

            foreign(Tag::id) { tagId }
            foreign(Person::id) primary { personId }
            foreign(Org::id) unique { orgId }
            foreign(Author::id) optional { authorId }

            optional {
                foreign(Draft::id) { draftId }
            }

            unique {
                a: int
                b: int
            }

            unique {
                foreign(Dept::id) unique { deptId }
                role: string
            }
        }
        "#,
    );

    let m = find_model(&ast, "M");

    let env_bindings = m
        .symbol
        .tags
        .iter()
        .filter_map(|t| match &t.inner {
            Tag::Use { binding } => Some(binding.inner),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(env_bindings, vec!["d1_db", "d2_db"]);

    let cruds: Vec<CrudKind> = m
        .symbol
        .tags
        .iter()
        .flat_map(|t| match &t.inner {
            Tag::Crud { kinds } => kinds.iter().map(|k| k.inner.clone()).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    assert!(
        cruds.iter().any(|c| matches!(c, CrudKind::Get))
            && cruds.iter().any(|c| matches!(c, CrudKind::Save))
            && cruds.iter().any(|c| matches!(c, CrudKind::List))
    );

    let col = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Column(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!((col.name, &col.cidl_type), ("score", &CidlType::Real));

    let primary = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Primary(blocks) => Some(blocks),
            _ => None,
        })
        .unwrap();
    assert_eq!(sql_columns(primary), vec!["id"]);
    assert_eq!(
        sql_foreigns(primary)
            .iter()
            .find(|fb| adj_matches(&fb.adj, &[("Company", "id")]))
            .unwrap()
            .fields[0]
            .name,
        "companyId"
    );
    assert_eq!(
        sql_foreigns(primary)
            .iter()
            .find(|fb| adj_matches(&fb.adj, &[("Parent", "orgId"), ("Parent", "userId")]))
            .unwrap()
            .fields
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        vec!["orgId", "userId"]
    );

    let tag_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Tag", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(tag_fb.fields[0].name, "tagId");
    assert!(tag_fb.qualifier.is_none());

    let person_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Person", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        person_fb.qualifier,
        Some(frontend::ForeignQualifier::Primary)
    ));

    let org_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Org", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(matches!(
        org_fb.qualifier,
        Some(frontend::ForeignQualifier::Unique)
    ));

    let author_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Author", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(author_fb.is_optional());

    let opt = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Optional(blocks) => Some(blocks),
            _ => None,
        })
        .unwrap();
    assert_eq!(sql_foreigns(opt)[0].fields[0].name, "draftId");

    let uniques: Vec<&Vec<Spd<SqlBlockKind>>> = m
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            ModelBlockKind::Unique(blocks) => Some(blocks),
            _ => None,
        })
        .collect();
    assert!(uniques.iter().any(|u| sql_columns(u) == vec!["a", "b"]));

    let dept_unique = uniques
        .iter()
        .find(|u| {
            u.iter()
                .any(|b| matches!(&b.inner, SqlBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Dept", "id")])))
        })
        .unwrap();
    assert_eq!(sql_columns(dept_unique), vec!["role"]);
    assert!(matches!(
        sql_foreigns(dept_unique)[0].qualifier,
        Some(frontend::ForeignQualifier::Unique)
    ));
}

#[test]
fn model_navigation() {
    let ast = lex_and_ast(
        r#"
        model M {
            foreign(Location::id) {
                locationId
                nav { location }
            }
            foreign(Tag::id) { tagId }
            nav(Weather::reportId) { weathers }
            nav(Alert::regionId, Alert::zoneId) { alerts }
        }
        "#,
    );

    let m = find_model(&ast, "M");

    let loc_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Location", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(loc_fb.fields[0].name, "locationId");
    assert_eq!(loc_fb.nav.as_ref().unwrap().inner.symbol.name, "location");

    let tag_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Tag", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(tag_fb.nav.is_none());

    let weathers_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.nav.inner.name == "weathers" => Some(n),
            _ => None,
        })
        .unwrap();
    assert!(adj_matches(&weathers_nav.adj, &[("Weather", "reportId")]));

    let alerts_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.nav.inner.name == "alerts" => Some(n),
            _ => None,
        })
        .unwrap();
    assert!(adj_matches(
        &alerts_nav.adj,
        &[("Alert", "regionId"), ("Alert", "zoneId")]
    ));
}

#[test]
fn model_kv_r2_paginated() {
    let ast = lex_and_ast(
        r#"
        [crud get, save, list]
        model Cache {
            kv(ns_a, "data/{id}") { value: json }
            kv(ns_b, "list/{cursor}") paginated { page: json }
            r2(bucket_a, "photos/{id}.jpg") { photo }
            r2(bucket_b, "thumbs/{cursor}") paginated { thumb }
            paginated {
                kv(ns_c, "cache/{id}") { cached_val: string }
                r2(bucket_c, "archive/{id}") { archive }
                kv(ns_d, "feed/{cursor}") paginated { feed_item: json }
                r2(bucket_d, "feed/{cursor}.mp4") paginated { feed_video }
            }
        }

        [crud get, save, list]
        model PureKv {
            keyfield { 
                key: string
                secondary: int 
            }
            paginated {
                kv(kv_ns, "entry/{key}/{secondary}") { entry: json }
            }
        }
        "#,
    );

    let m = find_model(&ast, "Cache");

    let kv_a = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) if kv.env_binding.name == "ns_a" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (kv_a.key_format, kv_a.field.name, kv_a.is_paginated),
        ("data/{id}", "value", false)
    );

    let kv_b = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) if kv.env_binding.name == "ns_b" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (kv_b.key_format, kv_b.field.name, kv_b.is_paginated),
        ("list/{cursor}", "page", true)
    );

    let r2_a = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::R2(r2) if r2.env_binding.name == "bucket_a" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (r2_a.key_format, r2_a.field.name, r2_a.is_paginated),
        ("photos/{id}.jpg", "photo", false)
    );

    let r2_b = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::R2(r2) if r2.env_binding.name == "bucket_b" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (r2_b.key_format, r2_b.field.name, r2_b.is_paginated),
        ("thumbs/{cursor}", "thumb", true)
    );

    let pblocks: Vec<_> = m
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            ModelBlockKind::Paginated(blocks) => Some(blocks),
            _ => None,
        })
        .flat_map(|bs| bs.iter())
        .collect();

    let kv_c = pblocks
        .iter()
        .find_map(|b| match &b.inner {
            PaginatedBlockKind::Kv(kv) if kv.env_binding.name == "ns_c" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (
            kv_c.key_format,
            kv_c.field.name,
            kv_c.field.cidl_type.clone(),
            kv_c.is_paginated
        ),
        ("cache/{id}", "cached_val", CidlType::String, false)
    );

    let r2_c = pblocks
        .iter()
        .find_map(|b| match &b.inner {
            PaginatedBlockKind::R2(r2) if r2.env_binding.name == "bucket_c" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (r2_c.key_format, r2_c.field.name, r2_c.is_paginated),
        ("archive/{id}", "archive", false)
    );

    let kv_d = pblocks
        .iter()
        .find_map(|b| match &b.inner {
            PaginatedBlockKind::Kv(kv) if kv.env_binding.name == "ns_d" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (kv_d.key_format, kv_d.field.name, kv_d.is_paginated),
        ("feed/{cursor}", "feed_item", true)
    );

    let r2_d = pblocks
        .iter()
        .find_map(|b| match &b.inner {
            PaginatedBlockKind::R2(r2) if r2.env_binding.name == "bucket_d" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (r2_d.key_format, r2_d.field.name, r2_d.is_paginated),
        ("feed/{cursor}.mp4", "feed_video", true)
    );

    let kv_model = find_model(&ast, "PureKv");

    let kv_env_bindings = kv_model
        .symbol
        .tags
        .iter()
        .filter_map(|t| match &t.inner {
            Tag::Use { binding } => Some(binding),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(kv_env_bindings.is_empty());

    let kv_cruds: Vec<CrudKind> = kv_model
        .symbol
        .tags
        .iter()
        .flat_map(|t| match &t.inner {
            Tag::Crud { kinds } => kinds.iter().map(|k| k.inner.clone()).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    assert!(
        kv_cruds.iter().any(|c| matches!(c, CrudKind::Get))
            && kv_cruds.iter().any(|c| matches!(c, CrudKind::Save))
            && kv_cruds.iter().any(|c| matches!(c, CrudKind::List))
    );

    let keyfields: Vec<&str> = kv_model
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::KeyField(fields) => Some(fields.iter().map(|s| s.name).collect()),
            _ => None,
        })
        .unwrap();
    assert_eq!(keyfields, vec!["key", "secondary"]);

    let kv_entry = kv_model
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Paginated(blocks) => blocks.iter().find_map(|b| match &b.inner {
                PaginatedBlockKind::Kv(kv) if kv.env_binding.name == "kv_ns" => Some(kv),
                _ => None,
            }),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        (
            kv_entry.key_format,
            kv_entry.field.name,
            kv_entry.field.cidl_type.clone()
        ),
        ("entry/{key}/{secondary}", "entry", CidlType::Json)
    );
}

#[test]
fn validator_tags() {
    let ast = lex_and_ast(
        r#"
        model M {
            [regex /[a-z]+/]
            [minlen 1]
            [gt 42]
            [lte 100.5]
            email: string
        }
        "#,
    );

    let m = find_model(&ast, "M");
    let col = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Column(s) => Some(s),
            _ => None,
        })
        .unwrap();

    assert_eq!(col.name, "email");
    assert_eq!(col.cidl_type, CidlType::String);
    assert_eq!(col.tags.len(), 4);

    let validators: Vec<(&Keyword, &ArgumentLiteral)> = col
        .tags
        .iter()
        .filter_map(|t| match &t.inner {
            Tag::Validator { name, argument } => Some((name, argument)),
            _ => None,
        })
        .collect();
    assert_eq!(validators.len(), 4);

    assert!(matches!(validators[0].0, Keyword::Regex));
    assert_eq!(validators[0].1, &ArgumentLiteral::Regex("[a-z]+"));

    assert!(matches!(validators[1].0, Keyword::MinLen));
    assert_eq!(validators[1].1, &ArgumentLiteral::Int("1"));

    assert!(matches!(validators[2].0, Keyword::GreaterThan));
    assert_eq!(validators[2].1, &ArgumentLiteral::Int("42"));

    assert!(matches!(validators[3].0, Keyword::LessThanOrEqual));
    assert_eq!(validators[3].1, &ArgumentLiteral::Real("100.5"));
}

fn sql_columns<'a>(blocks: &'a [Spd<SqlBlockKind<'a>>]) -> Vec<&'a str> {
    blocks
        .iter()
        .filter_map(|b| match &b.inner {
            SqlBlockKind::Column(s) => Some(s.name),
            _ => None,
        })
        .collect()
}

fn sql_foreigns<'a>(blocks: &'a [Spd<SqlBlockKind<'a>>]) -> Vec<&'a ForeignBlock<'a>> {
    blocks
        .iter()
        .filter_map(|b| match &b.inner {
            SqlBlockKind::Foreign(fb) => Some(fb),
            _ => None,
        })
        .collect()
}

fn find_model<'a>(ast: &'a Ast<'a>, name: &str) -> &'a ModelBlock<'a> {
    ast.blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Model(m) if m.symbol.name == name => Some(m),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} model to be present"))
}

#[test]
fn service_block_supports_multiple_symbols() {
    let ast = lex_and_ast(
        r#"
        service {
            FooService
            BarService
            BazService
        }
        "#,
    );

    let services: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::Service(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(services.len(), 1);
    let names: Vec<_> = services[0].symbols.iter().map(|s| s.name).collect();
    assert_eq!(names, vec!["FooService", "BarService", "BazService"]);
}
