// mod common;

// use compiler_test::{expected_str, src_to_idl};
// use idl::IncludeTree;
// use serde_json::json;
// use sqlx::{Row, SqlitePool};

// use orm::select::SelectModel;

// use common::test_sql;

// fn include(val: serde_json::Value) -> Option<IncludeTree<'static>> {
//     let s = serde_json::to_string(&val).unwrap();

//     // leak the string so IncludeTree can have a 'static lifetime
//     let tree: IncludeTree<'static> = serde_json::from_str(Box::leak(s.into_boxed_str())).unwrap();
//     Some(tree)
// }

// #[sqlx::test]
// async fn scalar_model(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Person for db {
//                 primary {
//                     id: int
//                 }

//                 column {
//                     name: string
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
//         "#
//     .to_string();

//     // Act
//     let select_stmt = SelectModel::query("Person", None, None, &idl);

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM "Person""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
// }

// #[sqlx::test]
// async fn one_to_one(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Person for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Dog::id) {
//                     dogId
//                 }

//                 nav Dog::id(dogId) {
//                     dog
//                 }
//             }

//             model Dog for db {
//                 primary {
//                     id: int
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Dog (id) VALUES (1), (2);
//             INSERT INTO Person (id, dogId) VALUES (1, 1), (2, 2);
//         "#
//     .to_string();

//     // Act
//     let select_stmt =
//         SelectModel::query("Person", None, include(json!({"dog": {}})).as_ref(), &idl);

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Person"."id" AS "id", "Person"."dogId" AS "dogId", "Dog_1"."id" AS "dog.id" FROM "Person" LEFT JOIN "Dog" AS "Dog_1" ON "Person"."dogId" = "Dog_1"."id""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("dogId").unwrap(), 1);
// }

// #[sqlx::test]
// async fn one_to_one_worker_model_is_not_joined(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Person for db {
//                 primary {
//                     id: int
//                 }

//                 column {
//                     name: string
//                 }

//                 nav Profile::ownerId(id) {
//                     profile
//                 }
//             }

//             model Profile {
//                 route {
//                     ownerId: int
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Person (id, name) VALUES (1, 'Alice');
//         "#
//     .to_string();

//     // Act
//     let select_stmt = SelectModel::query(
//         "Person",
//         None,
//         include(json!({"profile": {}})).as_ref(),
//         &idl,
//     );

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM "Person""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
// }

// #[sqlx::test]
// async fn one_to_many(db: SqlitePool) {
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Dog for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Person::id) {
//                     personId
//                 }
//             }

//             model Cat for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Person::id) {
//                     personId
//                 }
//             }

//             model Person for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Boss::id) {
//                     bossId
//                 }

//                 nav(Dog::personId) {
//                     dogs
//                 }

//                 nav(Cat::personId) {
//                     cats
//                 }
//             }

//             model Boss for db {
//                 primary {
//                     id: int
//                 }

//                 nav(Person::bossId) {
//                     persons
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Boss (id) VALUES (1);
//             INSERT INTO Person (id, bossId) VALUES (1, 1), (2, 1);
//             INSERT INTO Dog (id, personId) VALUES (1, 1), (2, 2);
//             INSERT INTO Cat (id, personId) VALUES (1, 1), (2, 2);
//         "#
//     .to_string();

//     // Act
//     let sql = SelectModel::query(
//         "Boss",
//         None,
//         include(json!({
//             "persons": {
//                 "dogs": {},
//                 "cats": {}
//             }
//         }))
//         .as_ref(),
//         &idl,
//     );

//     // Assert
//     expected_str!(
//         sql,
//         r#"
//             SELECT
//             "Boss"."id" AS "id",
//             "Person_1"."id" AS "persons.id",
//             "Person_1"."bossId" AS "persons.bossId",
//             "Dog_2"."id" AS "persons.dogs.id",
//             "Dog_2"."personId" AS "persons.dogs.personId",
//             "Cat_3"."id" AS "persons.cats.id",
//             "Cat_3"."personId" AS "persons.cats.personId"
//         FROM "Boss"
//         LEFT JOIN "Person" AS "Person_1" ON "Boss"."id" = "Person_1"."bossId"
//         LEFT JOIN "Dog" AS "Dog_2" ON "Person_1"."id" = "Dog_2"."personId"
//         LEFT JOIN "Cat" AS "Cat_3" ON "Person_1"."id" = "Cat_3"."personId"
//         "#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (sql, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.bossId").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.dogs.id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.dogs.personId").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.cats.id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("persons.cats.personId").unwrap(), 1);
// }

// #[sqlx::test]
// async fn composite_one_to_one(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Student for db {
//                 primary {
//                     school_id: int
//                     student_number: int
//                 }

//                 column {
//                     name: string
//                 }
//             }

//             model Enrollment for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Student::school_id, Student::student_number) {
//                     school_id
//                     student_number
//                 }

//                 nav (Student::school_id(school_id), Student::student_number(student_number)) {
//                     student
//                 }

//                 column {
//                     course: string
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Student (school_id, student_number, name) VALUES (10, 5001, 'Alice'), (10, 5002, 'Bob');
//             INSERT INTO Enrollment (id, school_id, student_number, course) VALUES (1, 10, 5001, 'Math 101'), (2, 10, 5002, 'Physics 101');
//         "#
//         .to_string();

//     // Act
//     let select_stmt = SelectModel::query(
//         "Enrollment",
//         None,
//         include(json!({"student": {}})).as_ref(),
//         &idl,
//     );

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Enrollment"."id" AS "id", "Enrollment"."school_id" AS "school_id", "Enrollment"."student_number" AS "student_number", "Enrollment"."course" AS "course", "Student_1"."school_id" AS "student.school_id", "Student_1"."student_number" AS "student.student_number", "Student_1"."name" AS "student.name" FROM "Enrollment" LEFT JOIN "Student" AS "Student_1" ON "Enrollment"."school_id" = "Student_1"."school_id" AND "Enrollment"."student_number" = "Student_1"."student_number""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<u32, _>("school_id").unwrap(), 10);
//     assert_eq!(value.try_get::<u32, _>("student_number").unwrap(), 5001);
//     assert_eq!(value.try_get::<String, _>("course").unwrap(), "Math 101");
//     assert_eq!(value.try_get::<u32, _>("student.school_id").unwrap(), 10);
//     assert_eq!(
//         value.try_get::<u32, _>("student.student_number").unwrap(),
//         5001
//     );
//     assert_eq!(value.try_get::<String, _>("student.name").unwrap(), "Alice");
// }

// #[sqlx::test]
// async fn composite_one_to_many(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Order for db {
//                 primary {
//                     region_id: int
//                     order_number: int
//                 }

//                 column {
//                     customer: string
//                 }

//                 nav(OrderItem::region_id, OrderItem::order_number) {
//                     items
//                 }
//             }

//             model OrderItem for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Order::region_id, Order::order_number) {
//                     region_id
//                     order_number
//                 }

//                 column {
//                     product: string
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO "Order" (region_id, order_number, customer) VALUES (1, 100, 'Bob');
//             INSERT INTO OrderItem (id, region_id, order_number, product) VALUES (1, 1, 100, 'Widget'), (2, 1, 100, 'Gadget');
//         "#
//         .to_string();

//     // Act
//     let select_stmt =
//         SelectModel::query("Order", None, include(json!({"items": {}})).as_ref(), &idl);

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Order"."region_id" AS "region_id", "Order"."order_number" AS "order_number", "Order"."customer" AS "customer", "OrderItem_1"."id" AS "items.id", "OrderItem_1"."region_id" AS "items.region_id", "OrderItem_1"."order_number" AS "items.order_number", "OrderItem_1"."product" AS "items.product" FROM "Order" LEFT JOIN "OrderItem" AS "OrderItem_1" ON "Order"."region_id" = "OrderItem_1"."region_id" AND "Order"."order_number" = "OrderItem_1"."order_number""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value1 = &results[1][0];
//     assert_eq!(value1.try_get::<u32, _>("region_id").unwrap(), 1);
//     assert_eq!(value1.try_get::<u32, _>("order_number").unwrap(), 100);
//     assert_eq!(value1.try_get::<String, _>("customer").unwrap(), "Bob");
//     assert_eq!(value1.try_get::<u32, _>("items.id").unwrap(), 2);
//     assert_eq!(value1.try_get::<u32, _>("items.region_id").unwrap(), 1);
//     assert_eq!(value1.try_get::<u32, _>("items.order_number").unwrap(), 100);
//     assert_eq!(
//         value1.try_get::<String, _>("items.product").unwrap(),
//         "Gadget"
//     );

//     let value2 = &results[1][1];
//     assert_eq!(value2.try_get::<u32, _>("region_id").unwrap(), 1);
//     assert_eq!(value2.try_get::<u32, _>("order_number").unwrap(), 100);
//     assert_eq!(value2.try_get::<String, _>("customer").unwrap(), "Bob");
//     assert_eq!(value2.try_get::<u32, _>("items.id").unwrap(), 1);
//     assert_eq!(value2.try_get::<u32, _>("items.region_id").unwrap(), 1);
//     assert_eq!(value2.try_get::<u32, _>("items.order_number").unwrap(), 100);
//     assert_eq!(
//         value2.try_get::<String, _>("items.product").unwrap(),
//         "Widget"
//     );
// }

// #[sqlx::test]
// async fn gensym_stops_ambigious_table(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Horse for db {
//                 primary {
//                     id: int
//                 }

//                 column {
//                     name: string
//                     bio: option<string>
//                 }

//                 nav(Match::horseId1) {
//                     matches
//                 }
//             }

//             model Match for db {
//                 primary {
//                     id: int
//                 }

//                 foreign(Horse::id) {
//                     horseId1
//                 }

//                 foreign(Horse::id) {
//                     horseId2
//                 }

//                 nav Horse::id(horseId2) {
//                     horse2
//                 }
//             }
//         "#,
//     );

//     let include_tree = json!({
//         "matches": {
//             "horse2": {}
//         }
//     });

//     let insert_query = r#"
//             INSERT INTO Horse (id, name, bio) VALUES (1, 'Spirit', 'Wild and free'), (2, 'Thunder', 'Fast and strong');
//             INSERT INTO Match (id, horseId1, horseId2) VALUES (1, 1, 2);
//         "#.to_string();

//     // Act
//     let sql = SelectModel::query("Horse", None, include(include_tree).as_ref(), &idl);

//     // Assert
//     expected_str!(
//         sql,
//         r#"SELECT "Horse"."id" AS "id", "Horse"."name" AS "name", "Horse"."bio" AS "bio", "Match_1"."id" AS "matches.id", "Match_1"."horseId1" AS "matches.horseId1", "Match_1"."horseId2" AS "matches.horseId2", "Horse_2"."id" AS "matches.horse2.id", "Horse_2"."name" AS "matches.horse2.name", "Horse_2"."bio" AS "matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" AS "Match_1" ON "Horse"."id" = "Match_1"."horseId1" LEFT JOIN "Horse" AS "Horse_2" ON "Match_1"."horseId2" = "Horse_2"."id""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (sql, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<String, _>("name").unwrap(), "Spirit");
//     assert_eq!(value.try_get::<String, _>("bio").unwrap(), "Wild and free");
// }

// #[sqlx::test]
// async fn custom_from(db: SqlitePool) {
//     // Arrange
//     let idl = src_to_idl(
//         r#"
//             d1 { db }

//             model Person for db {
//                 primary {
//                     id: int
//                 }

//                 column {
//                     name: string
//                 }
//             }
//         "#,
//     );

//     let insert_query = r#"
//             INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
//         "#
//     .to_string();

//     // Act
//     let custom_from = "SELECT * FROM Person WHERE name = 'Alice'".to_string();
//     let select_stmt = SelectModel::query("Person", Some(custom_from), None, &idl);

//     // Assert
//     expected_str!(
//         select_stmt,
//         r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM (SELECT * FROM Person WHERE name = 'Alice') AS "Person""#
//     );

//     let results = test_sql(idl, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
//         .await
//         .expect("SQL to execute");

//     let value = &results[1][0];
//     assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
//     assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
// }
