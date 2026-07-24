use compiler_test::src_to_idl;

#[test]
fn default_data_source_tree_includes_all_relationships() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv kv_namespace {
            userCache -> json {
                id: int
                "{id}"
            }
        }

        r2 r2_namespace {
            userDocuments {
                id: int
                "{id}"
            }
        }

        model Profile for db {
            primary {
                id: int
            }
        }

        model Order for db {
            primary {
                id: int
            }

            foreign User::id {
                userId
            }
        }

        model User for db {
            primary {
                id: int
            }

            foreign Profile::id {
                profileId
            }

            one Profile::id(profileId) {
                profile
            }

            many Order::userId(id) {
                orders
            }

            kv kv_namespace::userCache(id) {
                userCache
            }

            r2 r2_namespace::userDocuments(id) {
                userDocuments
            }
        }
    "#,
    );

    let user = idl.models.get("User").unwrap();
    let default_ds = user
        .default_data_source()
        .expect("User should have default data source");
    let tree = &default_ds.tree;

    for key in ["profile", "orders", "userCache", "userDocuments"] {
        assert!(
            tree.0.contains_key(key),
            "Default data source should include '{key}'"
        );
    }

    assert!(!default_ds.is_internal);
    assert_eq!(default_ds.name, "Default");

    assert!(!default_ds.get.is_stub);
    assert!(!default_ds.list.is_stub);
    assert!(!default_ds.save.is_stub);
}

#[test]
fn default_data_source_get_list_plans_are_precompiled() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Profile for db {
            primary {
                id: int
            }
        }

        model User for db {
            primary {
                id: int
            }

            foreign Profile::id {
                profileId
            }

            one Profile::id(profileId) {
                profile
            }
        }
    "#,
    );

    let user = idl.models.get("User").unwrap();
    let default_ds = user.default_data_source().unwrap();

    let get_plan = default_ds
        .get_plan
        .as_ref()
        .expect("get_plan should be precompiled");
    let list_plan = default_ds
        .list_plan
        .as_ref()
        .expect("list_plan should be precompiled");

    // `SelectPlan` only implements `Serialize` (it borrows from the IDL), so assert on the
    // serialized JSON's structural shape rather than round-tripping through `Deserialize`.
    let get_stages = get_plan
        .get("stages")
        .and_then(|s| s.as_array())
        .expect("get_plan should be a SelectPlan with a `stages` array");
    let list_stages = list_plan
        .get("stages")
        .and_then(|s| s.as_array())
        .expect("list_plan should be a SelectPlan with a `stages` array");

    assert!(!get_stages.is_empty());
    assert!(!list_stages.is_empty());

    assert!(
        default_ds.get_explain.contains("GET"),
        "get_explain should render an EXPLAIN header: {}",
        default_ds.get_explain
    );
    assert!(
        default_ds.list_explain.contains("LIST"),
        "list_explain should render an EXPLAIN header: {}",
        default_ds.list_explain
    );
}

#[test]
fn omitted_include_uses_default_tree() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Profile for db {
            primary {
                id: int
            }
        }

        model User for db {
            primary {
                id: int
            }

            foreign Profile::id {
                profileId
            }

            one Profile::id(profileId) {
                profile
            }
        }

        source WithDefault for User {}

        source Empty for User {
            include {}
        }
        "#,
    );

    let user = idl.models.get("User").unwrap();

    // An omitted include block falls back to the default include tree.
    let with_default = user.data_sources.get("WithDefault").unwrap();
    let default = user.default_data_source().unwrap();
    assert_eq!(
        with_default.tree.0.keys().collect::<Vec<_>>(),
        default.tree.0.keys().collect::<Vec<_>>()
    );
    assert!(with_default.tree.0.contains_key("profile"));

    // An explicit `include {}` stays empty.
    let empty = user.data_sources.get("Empty").unwrap();
    assert!(empty.tree.0.is_empty());
}

#[test]
fn default_data_source_present_on_every_d1_model() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv my_kv {
            cached -> json {
                tag: string
                "{tag}"
            }
        }

        [crud get, list]
        model Item for db {
            primary {
                id: int
            }

            column {
                tag: string
            }

            kv my_kv::cached(tag) {
                cached
            }
        }

        source WithKv for Item {
            include { cached }
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();
    let with_kv = item
        .data_sources
        .get("WithKv")
        .expect("WithKv data source should exist");
    assert!(!with_kv.get.is_stub);
    assert!(!with_kv.list.is_stub);
    assert!(!with_kv.save.is_stub);

    let default_ds = item.default_data_source().expect("Should have default ds");
    assert!(!default_ds.get.is_stub);
    assert!(!default_ds.list.is_stub);
    assert!(!default_ds.save.is_stub);
}

#[test]
fn default_data_source_skips_nested_manys() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Grade for db {
            primary {
                id: int
            }

            foreign Student::id {
                studentId
            }
        }

                model Teacher for db {
            primary {
                id: int
            }

            many Student::teacherId(id) {
                students
            }
        }

        model Student for db {
            primary {
                id: int
            }

            foreign Teacher::id {
                teacherId
            }

            many Grade::studentId(id) {
                grades
            }
        }
    "#,
    );

    let teacher = idl.models.get("Teacher").unwrap();
    let default_ds = teacher
        .default_data_source()
        .expect("Teacher should have default data source");
    let tree = &default_ds.tree;

    assert!(tree.0.contains_key("students"));
    let students_node = tree.0.get("students").unwrap();
    assert!(
        !students_node.0.contains_key("grades"),
        "Default data source should NOT recurse past 1:N"
    );
}

#[test]
fn default_data_source_includes_multiple_one_to_ones() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Toy for db {
            primary {
                id: int
            }
            column {
                color: string
            }
        }

        model Dog for db {
            primary {
                id: int
            }
            column {
                breed: string
            }

            foreign Toy::id {
                toyId
            }

            one Toy::id(toyId) { toy }
        }

        model Owner for db {
            primary {
                id: int
            }
            column {
                name: string
            }

            foreign Dog::id {
                dogId
            }

            one Dog::id(dogId) { dog }
        }
    "#,
    );

    let owner = idl.models.get("Owner").unwrap();
    let default_ds = owner.default_data_source().unwrap();
    let tree = &default_ds.tree;

    let dog_node = tree.0.get("dog").expect("includes 'dog'");
    let toy_node = dog_node.0.get("toy").expect("includes 'dog.toy'");
    assert!(
        toy_node.0.is_empty(),
        "Default include should not recurse past leaf 1:1"
    );
}

#[test]
fn default_data_source_diamond_does_not_duplicate_traversal() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Team for db {
            primary {
                id: int
            }
            column {
                name: string
            }
        }

        model Department for db {
            primary {
                id: int
            }

            foreign Team::id {
                teamId
            }

            one Team::id(teamId) { team }
        }

        model Company for db {
            primary {
                id: int
            }

            foreign Department::id {
                departmentId
            }

            one Department::id(departmentId) { department }

            foreign Team::id {
                directTeamId
            }

            one Team::id(directTeamId) { team }
        }
    "#,
    );

    let company = idl.models.get("Company").unwrap();
    let default_ds = company.default_data_source().unwrap();
    let tree = &default_ds.tree;

    assert!(tree.0.contains_key("department"));
    assert!(tree.0.contains_key("team"));
    let department_node = tree.0.get("department").unwrap();
    assert!(department_node.0.contains_key("team"));
}

#[test]
fn default_data_source_composite_pk() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model OrderItem for db {
            primary {
                orderId: int
                productId: int
            }
            column {
                qty: int
            }
        }
    "#,
    );

    let ds = idl
        .models
        .get("OrderItem")
        .unwrap()
        .default_data_source()
        .unwrap();

    // GET takes every composite PK column.
    let get_params: Vec<&str> = ds
        .get
        .parameters
        .iter()
        .map(|p| p.parameter.name.as_ref())
        .collect();
    assert_eq!(get_params, vec!["orderId", "productId"]);

    // LIST with seek pagination
    let list_params: Vec<&str> = ds.list.parameters.iter().map(|p| p.name.as_ref()).collect();
    assert_eq!(
        list_params,
        vec!["lastSeen_orderId", "lastSeen_productId", "limit"]
    );
}

#[test]
fn custom_data_source_captures_stub_params_and_tags() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
            column {
                price: int
            }
        }

        source ById for Item {
            include {}

            get {
                [instance]
                id: int
            }
        }

        source PaginatedSince for Item {
            include {}

            list {
                lastId: int
                limit: int
            }
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();

    let by_id = item.data_sources.get("ById").unwrap();
    assert!(by_id.get.is_stub, "ById's get should be a user stub");
    assert_eq!(by_id.get.parameters.len(), 1);
    assert_eq!(by_id.get.parameters[0].parameter.name, "id");
    assert!(by_id.get.parameters[0].instance_field);

    let paginated = item.data_sources.get("PaginatedSince").unwrap();
    assert!(
        paginated.list.is_stub,
        "PaginatedSince's list should be a user stub"
    );
    assert_eq!(paginated.list.parameters.len(), 2);
    assert_eq!(paginated.list.parameters[0].name, "lastId");
    assert_eq!(paginated.list.parameters[1].name, "limit");
}

#[test]
fn custom_data_source_save_stub() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source Audited for Item {
            include {}

            save {
                item: partial<Item>
            }
        }
    "#,
    );

    let audited = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("Audited")
        .unwrap();
    assert!(audited.save.is_stub, "Audited's save should be a user stub");
    assert_eq!(audited.save.parameters.len(), 1);
    assert_eq!(audited.save.parameters[0].name, "item");
}

#[test]
fn custom_data_source_inject_tag_is_captured() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source WithDb for Item {
            include {}

            get {
                id: int
                inject { db }
            }
        }
    "#,
    );

    let with_db = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("WithDb")
        .unwrap();
    assert!(with_db.get.is_stub, "WithDb's get should be a user stub");
    assert_eq!(with_db.get.injected, vec!["db"]);
}

#[test]
fn api_method_defaults_to_default_data_source() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Item for db {
            primary {
                id: int
            }
        }

        source Custom for Item {
            include {}
        }

        api Item {
            self get fetch -> Item {}

            self(Custom) post fetchCustom -> Item {}

            post create -> Item {}
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();

    let fetch = item.apis.iter().find(|m| m.name == "fetch").unwrap();
    assert_eq!(fetch.data_source, Some("Default"));

    let fetch_custom = item.apis.iter().find(|m| m.name == "fetchCustom").unwrap();
    assert_eq!(fetch_custom.data_source, Some("Custom"));

    let create = item.apis.iter().find(|m| m.name == "create").unwrap();
    assert_eq!(create.data_source, None);
}

#[test]
fn default_data_source_durable_sqlite() {
    let idl = src_to_idl(
        r#"
        durable LeaderboardDo {
            shard {
                tenantId: int
            }

            topCache -> json {
                "top"
            }
        }

        [crud get, list, save]
        model LeaderboardEntry for LeaderboardDo(tenantId) {
            primary {
                id: int
            }

            column {
                playerName: string
                score: int
            }

            kv LeaderboardDo::{ topCache, tenantId(tenantId) } {
                top
            }
        }
    "#,
    );

    let entry = idl.models.get("LeaderboardEntry").unwrap();
    let ds = entry.default_data_source().expect("default data source");

    // Every method takes the shard fields first to locate the DO instance.
    let get_params: Vec<&str> = ds
        .get
        .parameters
        .iter()
        .map(|p| p.parameter.name.as_ref())
        .collect();
    assert_eq!(get_params, vec!["tenantId", "id"]);

    let list_params: Vec<&str> = ds.list.parameters.iter().map(|p| p.name.as_ref()).collect();
    assert_eq!(list_params, vec!["tenantId", "lastSeen_id", "limit"]);

    let save_params: Vec<&str> = ds.save.parameters.iter().map(|p| p.name.as_ref()).collect();
    assert_eq!(save_params, vec!["tenantId", "model"]);

    // All methods run inside the DO.
    for target in [
        &ds.get.durable_target,
        &ds.list.durable_target,
        &ds.save.durable_target,
    ] {
        let target = target.as_ref().expect("durable target");
        assert_eq!(target.binding, "LeaderboardDo");
    }

    // CRUD routes carry the durable target for Worker-to-DO forwarding.
    let get_api = entry.apis.iter().find(|a| a.name == "$get").unwrap();
    let target = get_api.durable_target.as_ref().expect("durable target");
    assert_eq!(target.binding, "LeaderboardDo");
    assert_eq!(target.shard_args, vec!["tenantId"]);
}
