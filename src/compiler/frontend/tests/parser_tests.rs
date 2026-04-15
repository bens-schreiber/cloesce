use ast::{CidlType, CrudKind, HttpVerb};
use compiler_test::lex_and_parse;
use frontend::{
    AstBlockKind, EnvBlockKind, ForeignBlock, ModelBlock, ModelBlockKind, PaginatedBlockKind,
    ParseAst, SqlBlockKind, Symbol, UseTagParamKind,
};

fn adj_matches(adj: &[(Symbol, Symbol)], expected: &[(&str, &str)]) -> bool {
    adj.len() == expected.len()
        && adj
            .iter()
            .zip(expected)
            .all(|((m, f), (em, ef))| m.name == *em && f.name == *ef)
}

macro_rules! expect_block {
    ($iter:expr, $pat:pat => $out:expr, $msg:literal) => {{
        $iter
            .iter()
            .find_map(|b| match b {
                $pat => Some($out),
                _ => None,
            })
            .expect($msg)
    }};
}

#[test]
fn env_block() {
    // Act
    let ast = lex_and_parse(
        r#"

        env {
            d1 { db db2 }
            r2 { assets }
            kv { cache }

            vars {
                api_url: string
                max_retries: int
                threshold: double
                created_at: date
                payload: json
                enabled: bool
            }
        }
        "#,
    );

    // Assert
    let env = expect_block!(
        ast.blocks,
        AstBlockKind::Env(e) => e,
        "env block to be present"
    );

    let d1_bindings = expect_block!(
        env.blocks,
        EnvBlockKind::D1 { symbols, .. } => symbols
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        "d1 block to be present"
    );
    assert_eq!(d1_bindings, vec!["db", "db2"]);

    let r2_bindings = expect_block!(
        env.blocks,
        EnvBlockKind::R2 { symbols, .. } => symbols
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        "r2 block to be present"
    );
    assert_eq!(r2_bindings, vec!["assets"]);

    let kv_bindings = expect_block!(
        env.blocks,
        EnvBlockKind::Kv { symbols, .. } => symbols
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        "kv block to be present"
    );
    assert_eq!(kv_bindings, vec!["cache"]);

    let vars = expect_block!(
        env.blocks,
        EnvBlockKind::Var { symbols, .. } => symbols
            .iter()
            .map(|s| (s.name, &s.cidl_type))
            .collect::<Vec<_>>(),
        "vars block to be present"
    );

    assert_eq!(
        vars,
        vec![
            ("api_url", &CidlType::String),
            ("max_retries", &CidlType::Integer),
            ("threshold", &CidlType::Double),
            ("created_at", &CidlType::DateIso),
            ("payload", &CidlType::Json),
            ("enabled", &CidlType::Boolean)
        ]
    )
}

#[test]
fn poo_block() {
    // Act
    let ast = lex_and_parse(
        r#"
        poo Address {
            street: string
            city: string
            zipcode: Option<string>
        }

        poo User {
            id: int
            name: string
            email: string
            age: Option<int>
            active: bool
            balance: double
            created: date
            address: Address
            tags: Array<string>
            metadata: Option<json>
            optional_items: Option<Array<Item>>
            nullable_arrays: Array<Option<string>>
        }

        poo Container {
            items: Array<Item>
            nested: Array<Array<int>>
        }
        "#,
    );

    // Assert
    let address = ast
        .blocks
        .iter()
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
            ("id", CidlType::Integer),
            ("name", CidlType::String),
            ("email", CidlType::String),
            ("age", CidlType::nullable(CidlType::Integer)),
            ("active", CidlType::Boolean),
            ("balance", CidlType::Double),
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
        .find_map(|b| match b {
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
        CidlType::array(CidlType::array(CidlType::Integer))
    );
}

#[test]
fn inject_block() {
    // Act
    let ast = lex_and_parse(
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
        .filter_map(|b| match b {
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
    let ast = lex_and_parse(
        r#"
        service MyAppService {
            api1: OpenApiService
            api2: YouTubeApi
        }

        service EmptyService {}

        api MyAppService {
            post createItem(
                name: string,
                count: int
            ) -> string
        }

        api MyAppService {
            get listItems(self) -> Array<string>
        }
        "#,
    );

    // Assert
    let services: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|b| match b {
            AstBlockKind::Service(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(services.len(), 2);

    let service = services
        .iter()
        .find(|s| s.symbol.name == "MyAppService")
        .expect("MyAppService service to be present");
    assert_eq!(service.fields.len(), 2);

    let api1 = service
        .fields
        .iter()
        .find(|f| f.name == "api1")
        .expect("api1 field");
    assert_eq!(
        api1.cidl_type,
        CidlType::UnresolvedReference {
            name: "OpenApiService",
        }
    );

    let api2 = service
        .fields
        .iter()
        .find(|f| f.name == "api2")
        .expect("api2 field");
    assert_eq!(
        api2.cidl_type,
        CidlType::UnresolvedReference { name: "YouTubeApi" }
    );
    assert_ne!(api1.span, api2.span, "fields should have distinct spans");

    let empty = services
        .iter()
        .find(|s| s.symbol.name == "EmptyService")
        .expect("EmptyService to be present");
    assert_eq!(empty.fields.len(), 0);
    assert_ne!(
        service.symbol.span, empty.symbol.span,
        "services should have distinct spans"
    );

    let api_blocks: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|b| match b {
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
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "createItem"))
        .expect("block with createItem");
    let create = create_block
        .methods
        .iter()
        .find(|m| m.symbol.name == "createItem")
        .unwrap();
    assert_eq!(create.http_verb, HttpVerb::Post);
    // no SelfParam means it's static
    assert!(
        create
            .parameters
            .iter()
            .all(|p| matches!(p, frontend::ApiBlockMethodParamKind::Field(_)))
    );
    assert_eq!(create.parameters.len(), 2);
    assert_eq!(create.return_type, CidlType::String);

    let list_block = api_blocks
        .iter()
        .find(|a| a.methods.iter().any(|m| m.symbol.name == "listItems"))
        .expect("block with listItems");
    let list = list_block
        .methods
        .iter()
        .find(|m| m.symbol.name == "listItems")
        .unwrap();
    assert_eq!(list.http_verb, HttpVerb::Get);
    // has a SelfParam means it's an instance method
    assert!(
        list.parameters
            .iter()
            .any(|p| matches!(p, frontend::ApiBlockMethodParamKind::SelfParam { .. }))
    );
    assert_eq!(list.return_type, CidlType::array(CidlType::String));
}

#[test]
fn model_primary_unique_optional_foreign() {
    let ast = lex_and_parse(
        r#"
        [use d1_db, list]
        [use get, save, d2_db]
        model M {
            score: double

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
        .use_tags
        .iter()
        .flat_map(|t| t.params.iter())
        .filter_map(|p| match p {
            UseTagParamKind::EnvBinding(n) => Some(n.name),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(env_bindings, vec!["d1_db", "d2_db"]);

    let cruds: Vec<CrudKind> = m
        .use_tags
        .iter()
        .flat_map(|t| t.params.iter())
        .filter_map(|p| match p {
            UseTagParamKind::Crud(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    assert!(
        cruds.contains(&CrudKind::List)
            && cruds.contains(&CrudKind::Get)
            && cruds.contains(&CrudKind::Save)
    );

    let col = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Column(s) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!((col.name, &col.cidl_type), ("score", &CidlType::Double));

    let primary = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Primary { blocks, .. } => Some(blocks),
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
        .find_map(|b| match b {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Tag", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(tag_fb.fields[0].name, "tagId");
    assert!(tag_fb.qualifier.is_none());

    let person_fb = m
        .blocks
        .iter()
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Author", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(author_fb.is_optional());

    let opt = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Optional { blocks, .. } => Some(blocks),
            _ => None,
        })
        .unwrap();
    assert_eq!(sql_foreigns(opt)[0].fields[0].name, "draftId");

    let uniques: Vec<&Vec<SqlBlockKind>> = m
        .blocks
        .iter()
        .filter_map(|b| match b {
            ModelBlockKind::Unique { blocks, .. } => Some(blocks),
            _ => None,
        })
        .collect();
    assert!(uniques.iter().any(|u| sql_columns(u) == vec!["a", "b"]));

    let dept_unique = uniques
        .iter()
        .find(|u| {
            u.iter()
                .any(|b| matches!(b, SqlBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Dept", "id")])))
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
    let ast = lex_and_parse(
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
        .find_map(|b| match b {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Location", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(loc_fb.fields[0].name, "locationId");
    assert_eq!(loc_fb.nav.as_ref().unwrap().name, "location");

    let tag_fb = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Tag", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(tag_fb.nav.is_none());

    let weathers_nav = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Navigation(n) if n.field.name == "weathers" => Some(n),
            _ => None,
        })
        .unwrap();
    assert!(adj_matches(&weathers_nav.adj, &[("Weather", "reportId")]));

    let alerts_nav = m
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Navigation(n) if n.field.name == "alerts" => Some(n),
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
    let ast = lex_and_parse(
        r#"
        [use get, save, list]
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

        [use get, save, list]
        model PureKv {
            keyfield { key secondary }
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .filter_map(|b| match b {
            ModelBlockKind::Paginated { blocks, .. } => Some(blocks),
            _ => None,
        })
        .flat_map(|bs| bs.iter())
        .collect();

    let kv_c = pblocks
        .iter()
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .find_map(|b| match b {
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
        .use_tags
        .iter()
        .flat_map(|t| t.params.iter())
        .filter_map(|p| match p {
            UseTagParamKind::EnvBinding(n) => Some(n),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(kv_env_bindings.is_empty());

    let kv_cruds: Vec<CrudKind> = kv_model
        .use_tags
        .iter()
        .flat_map(|t| t.params.iter())
        .filter_map(|p| match p {
            UseTagParamKind::Crud(c) => Some(c.clone()),
            _ => None,
        })
        .collect();
    assert!(
        kv_cruds.contains(&CrudKind::Get)
            && kv_cruds.contains(&CrudKind::Save)
            && kv_cruds.contains(&CrudKind::List)
    );

    let keyfields: Vec<&str> = kv_model
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::KeyField { fields, .. } => {
                Some(fields.iter().map(|s| s.name).collect())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(keyfields, vec!["key", "secondary"]);

    let kv_entry = kv_model
        .blocks
        .iter()
        .find_map(|b| match b {
            ModelBlockKind::Paginated { blocks, .. } => blocks.iter().find_map(|b| match b {
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

fn sql_columns<'a>(blocks: &'a [SqlBlockKind]) -> Vec<&'a str> {
    blocks
        .iter()
        .filter_map(|b| match b {
            SqlBlockKind::Column(s) => Some(s.name),
            _ => None,
        })
        .collect()
}

fn sql_foreigns<'a>(blocks: &'a [SqlBlockKind<'a>]) -> Vec<&'a ForeignBlock<'a>> {
    blocks
        .iter()
        .filter_map(|b| match b {
            SqlBlockKind::Foreign(fb) => Some(fb),
            _ => None,
        })
        .collect()
}

fn find_model<'a>(ast: &'a ParseAst<'a>, name: &str) -> &'a ModelBlock<'a> {
    ast.blocks
        .iter()
        .find_map(|b| match b {
            AstBlockKind::Model(m) if m.symbol.name == name => Some(m),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} model to be present"))
}
