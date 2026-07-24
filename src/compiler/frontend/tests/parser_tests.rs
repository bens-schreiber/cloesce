use compiler_test::lex_and_ast;
use frontend::{
    ArgumentLiteral, Ast, AstBlockKind, Cardinality, ForeignBlock, InjectEntry, Keyword,
    ModelBlock, ModelBlockKind, NavigationKey, Spd, SqlBlockKind, Tag,
};
use idl::{CidlType, CrudKind, HttpVerb};

/// Matches a foreign block against its referenced `model` and `targets`.
fn foreign_matches(fb: &ForeignBlock, model: &str, targets: &[&str]) -> bool {
    fb.model.name == model
        && fb.targets.len() == targets.len()
        && fb.targets.iter().zip(targets).all(|(t, et)| t.name == *et)
}

/// Matches a navigation block's key pairs against `(target, local)` tuples.
fn nav_keys_match(keys: &[NavigationKey], expected: &[(&str, Option<&str>)]) -> bool {
    keys.len() == expected.len()
        && keys
            .iter()
            .zip(expected)
            .all(|(k, (et, el))| k.target.name == *et && k.local.as_ref().map(|s| s.name) == *el)
}

#[test]
fn top_level_bindings() {
    // Act
    let ast = lex_and_ast(
        r#"
        d1 {
            db
            db2
        }

        r2 Assets {
            asset {
                id: int
                "assets/{id}"
            }
        }

        kv Cache {
            entry -> json {
                id: string
                "cache/{id}"
            }
        }

        var {
            api_url: string
            max_retries: int
            threshold: real
            created_at: date
            payload: json
            enabled: bool
        }
        "#,
    );

    // d1
    let d1_bindings = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::D1Binding(b) => {
                Some(b.bindings.iter().map(|s| s.name).collect::<Vec<_>>())
            }
            _ => None,
        })
        .expect("d1 binding block to be present");
    assert_eq!(d1_bindings, vec!["db", "db2"]);

    // r2
    let r2 = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::R2Binding(b) => Some(b),
            _ => None,
        })
        .expect("r2 binding block to be present");
    assert_eq!(r2.symbol.name, "Assets");
    assert_eq!(r2.templates.len(), 1);
    let asset = &r2.templates[0].inner;
    assert_eq!(asset.symbol.name, "asset");
    assert_eq!(asset.key_format, "assets/{id}");
    assert_eq!(asset.params.len(), 1);
    assert_eq!(asset.params[0].name, "id");
    assert_eq!(asset.params[0].cidl_type, CidlType::Int);

    // kv
    let kv = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::KvBinding(b) => Some(b),
            _ => None,
        })
        .expect("kv binding block to be present");
    assert_eq!(kv.symbol.name, "Cache");
    assert_eq!(kv.templates.len(), 1);

    let entry = &kv.templates[0].inner;
    assert_eq!(entry.symbol.name, "entry");
    assert_eq!(entry.symbol.cidl_type, CidlType::Json);
    assert_eq!(entry.key_format, "cache/{id}");
    assert_eq!(entry.params.len(), 1);

    // var
    let vars = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Var(v) => Some(
                v.vars
                    .iter()
                    .map(|s| (s.name, &s.cidl_type))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("var block to be present");

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
fn durable_binding_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        durable LeaderboardDo {
            shard {
                [gt 0]
                tenantId: int
                region: string
            }

            topEntryCache -> json {
                "top"
            }

            topEntryCacheWithDate -> json {
                date: string
                "top/{date}"
            }
        }

        durable GlobalDo {
            config -> json {
                "config"
            }
        }
        "#,
    );

    let durables = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::DurableBinding(b) => Some(b),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(durables.len(), 2);

    // Sharded DO
    let leaderboard = durables
        .iter()
        .find(|b| b.symbol.name == "LeaderboardDo")
        .expect("LeaderboardDo to be present");

    assert_eq!(leaderboard.shard_blocks.len(), 1);
    let shard_fields = &leaderboard.shard_blocks[0].inner.fields;
    assert_eq!(shard_fields.len(), 2);
    let tenant = &shard_fields[0];
    assert_eq!(tenant.name, "tenantId");
    assert_eq!(tenant.cidl_type, CidlType::Int);
    assert!(
        tenant.tags.iter().any(|t| matches!(
            &t.inner,
            Tag::Validator {
                name: Keyword::GreaterThan,
                argument: ArgumentLiteral::Int("0"),
            }
        )),
        "tenantId should carry a [gt 0] validator tag"
    );
    let region = &shard_fields[1];
    assert_eq!(region.name, "region");
    assert_eq!(region.cidl_type, CidlType::String);

    assert_eq!(leaderboard.templates.len(), 2);
    let cache = &leaderboard.templates[0].inner;
    assert_eq!(cache.symbol.name, "topEntryCache");
    assert_eq!(cache.symbol.cidl_type, CidlType::Json);
    assert_eq!(cache.key_format, "top");
    assert!(cache.params.is_empty());

    let cache_with_date = &leaderboard.templates[1].inner;
    assert_eq!(cache_with_date.symbol.name, "topEntryCacheWithDate");
    assert_eq!(cache_with_date.key_format, "top/{date}");
    assert_eq!(cache_with_date.params.len(), 1);
    assert_eq!(cache_with_date.params[0].name, "date");
    assert_eq!(cache_with_date.params[0].cidl_type, CidlType::String);

    // Global DO (no shard block)
    let global = durables
        .iter()
        .find(|b| b.symbol.name == "GlobalDo")
        .expect("GlobalDo to be present");
    assert!(global.shard_blocks.is_empty());
    assert_eq!(global.templates.len(), 1);
    assert_eq!(global.templates[0].inner.symbol.name, "config");
}

#[test]
fn model_durable_backing() {
    let ast = lex_and_ast(
        r#"
        model Leaderboard for LeaderboardDo(tenantId, region) {
            kv LeaderboardDo::topEntryCache {
                top
            }

            kv LeaderboardDo::topEntryCacheWithDate(date) {
                topWithDate
            }
        }

        model Global for GlobalDo {}
        "#,
    );

    let leaderboard = find_model(&ast, "Leaderboard");
    assert_eq!(
        leaderboard.database_binding.as_ref().map(|s| s.name),
        Some("LeaderboardDo")
    );
    let shard_args = leaderboard
        .shard_args
        .as_ref()
        .expect("Leaderboard to carry shard args");
    assert_eq!(
        shard_args.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["tenantId", "region"]
    );

    let top_entry_cache = leaderboard
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) => Some(kv),
            _ => None,
        })
        .expect("Leaderboard to have a kv field");
    assert_eq!(top_entry_cache.binding.name, "LeaderboardDo");
    assert_eq!(top_entry_cache.args[0].target.name, "topEntryCache");
    assert!(top_entry_cache.args[0].local.is_empty());
    assert_eq!(top_entry_cache.field.name, "top");

    let top_entry_cache_with_date = leaderboard
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv)
                if kv.args.first().map(|a| a.target.name) == Some("topEntryCacheWithDate") =>
            {
                Some(kv)
            }
            _ => None,
        })
        .expect("Leaderboard to have a topEntryCacheWithDate kv field");
    assert_eq!(top_entry_cache_with_date.binding.name, "LeaderboardDo");
    // `topEntryCacheWithDate(date)` — one template local, no shard discriminators.
    assert_eq!(top_entry_cache_with_date.args.len(), 1);
    assert_eq!(top_entry_cache_with_date.args[0].local.len(), 1);

    let global = find_model(&ast, "Global");
    assert_eq!(
        global.database_binding.as_ref().map(|s| s.name),
        Some("GlobalDo")
    );
    assert!(global.shard_args.is_none());
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
            ("address", CidlType::Object { name: "Address" }),
            ("tags", CidlType::array(CidlType::String)),
            ("metadata", CidlType::nullable(CidlType::Json)),
            (
                "optional_items",
                CidlType::nullable(CidlType::array(CidlType::Object { name: "Item" }))
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
fn api_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        model MyAppService {}
        model AnotherService {}

        api MyAppService {
            post createItem -> string {
                name: string
                count: int
            }
        }

        api MyAppService {
            get listItems -> array<string> { }
        }
        "#,
    );

    // Assert
    let dataless_models: Vec<_> = ast
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            AstBlockKind::Model(m) if m.blocks.is_empty() => Some(m),
            _ => None,
        })
        .collect();
    assert_eq!(dataless_models.len(), 2);
    let names: Vec<_> = dataless_models.iter().map(|m| m.symbol.name).collect();
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
    assert_eq!(create.inner.symbol.cidl_type, CidlType::String);

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
fn api_context_tag() {
    // Act
    let ast = lex_and_ast(
        r#"
        model Leaderboard {}

        api Leaderboard {
            get topScores -> array<string> {
                tenantId: int

                inject {
                    LeaderboardDo::tenantId(tenantId)
                }
            }

            get config -> json {
                inject { GlobalDo::{} }
            }
        }
        "#,
    );

    // Assert
    let api = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Api(a) if a.symbol.name == "Leaderboard" => Some(a),
            _ => None,
        })
        .expect("Leaderboard api block");

    let context_of = |method: &str| -> (String, Vec<String>) {
        let m = api
            .methods
            .iter()
            .find(|m| m.inner.symbol.name == method)
            .unwrap();
        m.inner
            .injects
            .iter()
            .flat_map(|blk| blk.inner.entries.iter())
            .find_map(|entry| match &entry.inner {
                InjectEntry::Context {
                    symbol,
                    initializers,
                } => Some((
                    symbol.name.to_string(),
                    initializers
                        .iter()
                        .flat_map(|i| i.arg.iter())
                        .map(|a| a.name.to_string())
                        .collect(),
                )),
                InjectEntry::Binding(_) => None,
            })
            .expect("context entry")
    };

    let (sharded_do, sharded_args) = context_of("topScores");
    assert_eq!(sharded_do, "LeaderboardDo");
    assert_eq!(sharded_args, vec!["tenantId"]);

    let (global_do, global_args) = context_of("config");
    assert_eq!(global_do, "GlobalDo");
    assert!(global_args.is_empty());
}

#[test]
fn api_self_receiver() {
    // Act
    let ast = lex_and_ast(
        r#"
        model Item {}

        api Item {
            self(Custom) post named -> Item {}

            self post defaulted -> Item {}

            post staticMethod -> Item {}
        }
        "#,
    );

    // Assert
    let api = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Api(a) if a.symbol.name == "Item" => Some(a),
            _ => None,
        })
        .expect("Item api block");

    let method = |name: &str| {
        &api.methods
            .iter()
            .find(|m| m.inner.symbol.name == name)
            .unwrap()
            .inner
    };

    let named = method("named");
    let source = named.source.as_ref().expect("named is an instance method");
    assert_eq!(source.inner.source.as_ref().map(|s| s.name), Some("Custom"));

    let defaulted = method("defaulted");
    let source = defaulted
        .source
        .as_ref()
        .expect("defaulted is an instance method");
    assert!(source.inner.source.is_none());

    // No `self` receiver: a static method.
    assert!(method("staticMethod").source.is_none());
}

#[test]
fn model_primary_unique_optional_foreign() {
    let ast = lex_and_ast(
        r#"
        [crud get, save, list]
        model M for d1_db {
            column {
                score: real
                a: int
                b: int
                role: string
            }

            primary {
                id: int
                foreign Company::id { companyId }
                foreign Parent::{ orgId, userId } { orgId userId }
            }

            foreign Tag::id { tagId }
            foreign Org::id { orgId2 }
            foreign Author::id option {authorId }
            foreign Dept::id { deptId }
            foreign Draft::id option {draftId }

            unique (a, b)
            unique (orgId2)
            unique (deptId, role)
        }
        "#,
    );

    let m = find_model(&ast, "M");

    assert_eq!(m.database_binding.as_ref().map(|s| s.name), Some("d1_db"));

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

    let columns: Vec<&str> = m
        .blocks
        .iter()
        .flat_map(|spd| match &spd.inner {
            ModelBlockKind::Column(syms) => syms.iter().map(|s| s.name).collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect();
    assert!(columns.contains(&"score"));

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
            .find(|fb| foreign_matches(fb, "Company", &["id"]))
            .unwrap()
            .fields[0]
            .name,
        "companyId"
    );
    assert_eq!(
        sql_foreigns(primary)
            .iter()
            .find(|fb| foreign_matches(fb, "Parent", &["orgId", "userId"]))
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
            ModelBlockKind::Foreign(fb) if foreign_matches(fb, "Tag", &["id"]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(tag_fb.fields[0].name, "tagId");
    assert!(!tag_fb.is_optional);

    let author_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if foreign_matches(fb, "Author", &["id"]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(author_fb.is_optional);

    let draft_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if foreign_matches(fb, "Draft", &["id"]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(draft_fb.is_optional);
    assert_eq!(draft_fb.fields[0].name, "draftId");

    let uniques: Vec<Vec<&str>> = m
        .blocks
        .iter()
        .filter_map(|spd| match &spd.inner {
            ModelBlockKind::Unique(fields) => {
                Some(fields.iter().map(|s| s.name).collect::<Vec<_>>())
            }
            _ => None,
        })
        .collect();
    assert!(uniques.iter().any(|u| u == &vec!["a", "b"]));
    assert!(uniques.iter().any(|u| u == &vec!["orgId2"]));
    assert!(uniques.iter().any(|u| u == &vec!["deptId", "role"]));
}

#[test]
fn model_navigation() {
    let ast = lex_and_ast(
        r#"
        model M {
            foreign Location::id {
                locationId
            }
            foreign Tag::id { tagId }
            one Location::id(locationId) { location }
            many Weather::reportId { weathers }
            many Alert::{ regionId(regionId), zoneId(zoneId) } { alerts }
        }
        "#,
    );

    let m = find_model(&ast, "M");

    let loc_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if foreign_matches(fb, "Location", &["id"]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert_eq!(loc_fb.fields[0].name, "locationId");

    let location_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.field.inner.name == "location" => Some(n),
            _ => None,
        })
        .unwrap();
    assert_eq!(location_nav.cardinality, Cardinality::One);
    assert_eq!(location_nav.model.name, "Location");
    assert!(nav_keys_match(
        &location_nav.keys,
        &[("id", Some("locationId"))]
    ));

    let weathers_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.field.inner.name == "weathers" => Some(n),
            _ => None,
        })
        .unwrap();
    assert_eq!(weathers_nav.cardinality, Cardinality::Many);
    assert_eq!(weathers_nav.model.name, "Weather");
    assert!(nav_keys_match(&weathers_nav.keys, &[("reportId", None)]));

    let alerts_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.field.inner.name == "alerts" => Some(n),
            _ => None,
        })
        .unwrap();
    assert_eq!(alerts_nav.cardinality, Cardinality::Many);
    assert_eq!(alerts_nav.model.name, "Alert");
    assert!(nav_keys_match(
        &alerts_nav.keys,
        &[("regionId", Some("regionId")), ("zoneId", Some("zoneId"))]
    ));
}

#[test]
fn model_keyless_singleton_nav() {
    let ast = lex_and_ast(
        r#"
        model M {
            one Singleton { config }
        }
        "#,
    );

    let m = find_model(&ast, "M");
    let config_nav = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Navigation(n) if n.field.inner.name == "config" => Some(n),
            _ => None,
        })
        .unwrap();

    assert_eq!(config_nav.cardinality, Cardinality::One);
    assert_eq!(config_nav.model.name, "Singleton");
    assert!(config_nav.keys.is_empty());
}

#[test]
fn kv_r2_bindings_fields() {
    let ast = lex_and_ast(
        r#"
        kv NsA {
            value -> json {
                id: int
                "data/{id}"
            }
        }

        kv NsB {
            page -> json {
                cursor: string
                "list/{cursor}"
            }
        }

        r2 BucketA {
            photo {
                id: int
                "photos/{id}.jpg"
            }
        }

        r2 BucketB {
            thumb {
                cursor: string
                "thumbs/{cursor}"
            }
        }

        [crud get, save, list]
        model Cache {
            primary {
                id: int
            }

            column {
                cursor: string
            }

            kv NsA::value(id) { value }
            kv NsB::page(cursor) { page }
            r2 BucketA::photo(id) { photo }
            r2 BucketB::thumb(cursor) { thumb }
        }
        "#,
    );

    let m = find_model(&ast, "Cache");

    let kv_value = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) if kv.field.name == "value" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(kv_value.binding.name, "NsA");
    assert_eq!(kv_value.args[0].target.name, "value");
    assert_eq!(
        kv_value.args[0]
            .local
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>(),
        vec!["id"]
    );

    let kv_page = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) if kv.field.name == "page" => Some(kv),
            _ => None,
        })
        .unwrap();
    assert_eq!(kv_page.binding.name, "NsB");
    assert_eq!(kv_page.args[0].target.name, "page");

    let r2_photo = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::R2(r2) if r2.field.name == "photo" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(r2_photo.binding.name, "BucketA");
    assert_eq!(r2_photo.binding_template.name, "photo");
    assert_eq!(
        r2_photo.args.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["id"]
    );

    let r2_thumb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::R2(r2) if r2.field.name == "thumb" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(r2_thumb.binding.name, "BucketB");
    assert_eq!(r2_thumb.binding_template.name, "thumb");
}

#[test]
fn validator_tags() {
    let ast = lex_and_ast(
        r#"
        model M {
            column {
                [regex /[a-z]+/]
                [minlen 1]
                [gt 42]
                [lte 100.5]
                email: string
            }
        }
        "#,
    );

    let m = find_model(&ast, "M");
    let col = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Column(syms) => syms.first(),
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

#[test]
fn kv_durable_spider_form() {
    let ast = lex_and_ast(
        r#"
        durable MyDurable {
            shard {
                doId: string
            }

            value -> string {
                key1: string
                key2: string
                "value/{key1}/{key2}"
            }
        }

        model Foo {
            route {
                key1: string
                key2: string
                doId: string
            }

            kv MyDurable::{ value(key1, key2), doId(doId) } { myValue }
        }
        "#,
    );

    let m = find_model(&ast, "Foo");
    let kv = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Kv(kv) if kv.field.name == "myValue" => Some(kv),
            _ => None,
        })
        .unwrap();

    assert_eq!(kv.binding.name, "MyDurable");

    assert_eq!(kv.args[0].target.name, "value");
    assert_eq!(
        kv.args[0].local.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["key1", "key2"]
    );

    assert_eq!(kv.args[1].target.name, "doId");
    assert_eq!(
        kv.args[1].local.iter().map(|s| s.name).collect::<Vec<_>>(),
        vec!["doId"]
    );
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
