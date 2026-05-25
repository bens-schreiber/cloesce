use compiler_test::lex_and_ast;
use frontend::{
    ArgumentLiteral, Ast, AstBlockKind, ForeignBlock, Keyword, ModelBlock, ModelBlockKind, Spd,
    SqlBlockKind, Symbol, Tag,
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
fn top_level_bindings() {
    // Act
    let ast = lex_and_ast(
        r#"
        d1 {
            db
            db2
        }

        r2 Assets {
            asset(id: int) {
                "assets/{id}"
            }
        }

        kv Cache {
            entry(id: string) -> json {
                "cache/{id}"
            }

            page() -> paginated<json> {
                "cache/"
            }
        }

        vars {
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
    assert_eq!(r2.fields.len(), 1);
    let asset = &r2.fields[0].inner;
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
    assert_eq!(kv.fields.len(), 2);

    let entry = &kv.fields[0].inner;
    assert_eq!(entry.symbol.name, "entry");
    assert_eq!(entry.symbol.cidl_type, CidlType::Json);
    assert_eq!(entry.key_format, "cache/{id}");
    assert_eq!(entry.params.len(), 1);

    let page = &kv.fields[1].inner;
    assert_eq!(page.symbol.name, "page");
    assert_eq!(page.symbol.cidl_type, CidlType::paginated(CidlType::Json));
    assert_eq!(page.key_format, "cache/");
    assert!(page.params.is_empty());

    // vars
    let vars = ast
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Vars(v) => Some(
                v.vars
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
fn api_block() {
    // Act
    let ast = lex_and_ast(
        r#"
        model MyAppService {}
        model AnotherService {}

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
                foreign(Company::id) { companyId }
                foreign(Parent::orgId, Parent::userId) { orgId userId }
            }

            foreign(Tag::id) { tagId }
            foreign(Org::id) { orgId2 }
            foreign(Author::id) optional { authorId }
            foreign(Dept::id) { deptId }
            foreign(Draft::id) optional { draftId }

            unique (a, b)
            unique (orgId2)
            unique (deptId, role)
        }
        "#,
    );

    let m = find_model(&ast, "M");

    assert_eq!(m.backing_binding.as_ref().map(|s| s.name), Some("d1_db"));

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
    assert!(!tag_fb.is_optional);

    let author_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Author", "id")]) => Some(fb),
            _ => None,
        })
        .unwrap();
    assert!(author_fb.is_optional);

    let draft_fb = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::Foreign(fb) if adj_matches(&fb.adj, &[("Draft", "id")]) => Some(fb),
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
fn kv_r2_bindings_fields() {
    let ast = lex_and_ast(
        r#"
        kv NsA {
            value(id: int) -> json {
                "data/{id}"
            }
        }

        kv NsB {
            page(cursor: string) -> paginated<json> {
                "list/{cursor}"
            }
        }

        r2 BucketA {
            photo(id: int) {
                "photos/{id}.jpg"
            }
        }

        r2 BucketB {
            thumb(cursor: string) {
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
    assert_eq!(kv_value.binding_field.name, "value");
    assert_eq!(
        kv_value.args.iter().map(|s| s.name).collect::<Vec<_>>(),
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
    assert_eq!(kv_page.binding_field.name, "page");

    let r2_photo = m
        .blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            ModelBlockKind::R2(r2) if r2.field.name == "photo" => Some(r2),
            _ => None,
        })
        .unwrap();
    assert_eq!(r2_photo.binding.name, "BucketA");
    assert_eq!(r2_photo.binding_field.name, "photo");
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
    assert_eq!(r2_thumb.binding_field.name, "thumb");
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

fn find_model<'a>(ast: &'a Ast<'a>, name: &str) -> &'a ModelBlock<'a> {
    ast.blocks
        .iter()
        .find_map(|spd| match &spd.inner {
            AstBlockKind::Model(m) if m.symbol.name == name => Some(m),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} model to be present"))
}
