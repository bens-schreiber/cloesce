use compiler_test::src_to_idl;

#[test]
fn default_data_source_tree_includes_all_relationships() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
            kv { kv_namespace }
            r2 { r2_namespace }
        }

        [use db]
        model Profile {
            primary {
                id: int
            }
        }

        [use db]
        model Role {
            primary {
                id: int
            }

            nav(User::id) {
                users
            }
        }

        [use db]
        model Order {
            primary {
                id: int
            }

            foreign(User::id) {
                userId
            }
        }

        [use db]
        model User {
            primary {
                id: int
            }

            foreign(Profile::id) {
                profileId
                nav { profile }
            }

            nav(Order::userId) {
                orders
            }

            nav(Role::id) {
                roles
            }

            kv(kv_namespace, "{id}") {
                userCache: json
            }

            r2(r2_namespace, "{id}") {
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

    for key in ["profile", "orders", "roles", "userCache", "userDocuments"] {
        assert!(
            tree.0.contains_key(key),
            "Default data source should include '{key}'"
        );
    }

    assert!(!default_ds.is_internal);
    assert_eq!(default_ds.name, "Default");

    assert!(default_ds.get.is_none());
    assert!(default_ds.list.is_none());
    assert!(default_ds.save.is_none());
    assert!(default_ds.include_query.to_uppercase().contains("SELECT"));
}

#[test]
fn default_data_source_present_on_every_d1_model() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
            kv { my_kv }
        }

        [use db]
        [crud get, list]
        model Item {
            primary {
                id: int
            }

            keyfield {
                tag: string
            }

            kv(my_kv, "{tag}") {
                cached: json
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
    assert!(with_kv.get.is_none());
    assert!(with_kv.list.is_none());
    assert!(with_kv.save.is_none());

    let default_ds = item.default_data_source().expect("Should have default ds");
    assert!(default_ds.get.is_none());
    assert!(default_ds.list.is_none());
    assert!(default_ds.save.is_none());
}

#[test]
fn default_data_source_skips_nested_manys() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model Grade {
            primary {
                id: int
            }

            foreign(Student::id) {
                studentId
            }
        }

        [use db]
        model Teacher {
            primary {
                id: int
            }

            nav(Student::teacherId) {
                students
            }
        }

        [use db]
        model Student {
            primary {
                id: int
            }

            foreign(Teacher::id) {
                teacherId
            }

            nav(Grade::studentId) {
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
        env {
            d1 { db }
        }

        [use db]
        model Toy {
            primary {
                id: int
            }
            column {
                color: string
            }
        }

        [use db]
        model Dog {
            primary {
                id: int
            }
            column {
                breed: string
            }

            foreign(Toy::id) {
                toyId
                nav { toy }
            }
        }

        [use db]
        model Owner {
            primary {
                id: int
            }
            column {
                name: string
            }

            foreign(Dog::id) {
                dogId
                nav { dog }
            }
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
        env {
            d1 { db }
        }

        [use db]
        model Team {
            primary {
                id: int
            }
            column {
                name: string
            }
        }

        [use db]
        model Department {
            primary {
                id: int
            }

            foreign(Team::id) {
                teamId
                nav { team }
            }
        }

        [use db]
        model Company {
            primary {
                id: int
            }

            foreign(Department::id) {
                departmentId
                nav { department }
            }

            foreign(Team::id) {
                directTeamId
                nav { team }
            }
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
fn custom_data_source_captures_stub_params_and_tags() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model Item {
            primary {
                id: int
            }
            column {
                price: int
            }
        }

        source ById for Item {
            include {}

            get([instance] id: int)
        }

        source PaginatedSince for Item {
            include {}

            list(lastId: int, limit: int)
        }
    "#,
    );

    let item = idl.models.get("Item").unwrap();

    let by_id = item.data_sources.get("ById").unwrap();
    let get = by_id.get.as_ref().expect("ById should have a get stub");
    assert_eq!(get.parameters.len(), 1);
    assert_eq!(get.parameters[0].parameter.name, "id");
    assert!(get.parameters[0].instance_field);

    let paginated = item.data_sources.get("PaginatedSince").unwrap();
    let list = paginated
        .list
        .as_ref()
        .expect("PaginatedSince should have a list stub");
    assert_eq!(list.parameters.len(), 2);
    assert_eq!(list.parameters[0].name, "lastId");
    assert_eq!(list.parameters[1].name, "limit");
}

#[test]
fn custom_data_source_save_stub() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model Item {
            primary {
                id: int
            }
        }

        source Audited for Item {
            include {}

            save(item: partial<Item>)
        }
    "#,
    );

    let save = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("Audited")
        .unwrap()
        .save
        .as_ref()
        .expect("Audited should have a save stub");
    assert_eq!(save.parameters.len(), 1);
    assert_eq!(save.parameters[0].name, "item");
}

#[test]
fn custom_data_source_inject_tag_is_captured() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model Item {
            primary {
                id: int
            }
        }

        source WithDb for Item {
            include {}

            [inject db]
            get(id: int)
        }
    "#,
    );

    let get = idl
        .models
        .get("Item")
        .unwrap()
        .data_sources
        .get("WithDb")
        .unwrap()
        .get
        .as_ref()
        .expect("WithDb should have a get stub");
    assert_eq!(get.injected, vec!["db"]);
}

#[test]
fn api_method_defaults_to_default_data_source() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model Item {
            primary {
                id: int
            }
        }

        source Custom for Item {
            include {}
        }

        api Item {
            get fetch(self) -> Item
            post fetchCustom([source Custom] self) -> Item
            post create() -> Item
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
