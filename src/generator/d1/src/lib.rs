use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use common::{CidlForeignKeyKind, CidlSpec, CidlType, D1Database, Model, TypedValue, WranglerSpec};

use anyhow::{Result, anyhow};

use sea_query::{Alias, ColumnDef, ForeignKey, Index, SqliteQueryBuilder, Table};

/// Topological sort via Kahns algorithm. Places models in SQL table
/// insertion order, such that there are no FK errors.
///
/// Returns an error if there is:
/// - a cycle
/// - duplicate models
/// - unknown FK model
/// - unknown attribute reference
fn sql_topo_sort(models: &[Model]) -> Result<Vec<&Model>> {
    let mut model_lookup = HashMap::<&str, &Model>::new();
    let mut in_degree = BTreeMap::<&str, usize>::new();
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();

    // Detect dups, populate reverse lookup
    for model in models {
        if model_lookup.insert(&model.name, model).is_some() {
            return Err(anyhow!("Duplicate model name: {}", model.name));
        }

        graph.entry(&model.name).or_default();
        in_degree.entry(&model.name).or_insert(0);
    }

    // Increment in-degree
    for model in models {
        let mut attr_to_fk = HashMap::new();

        // Handle attribute FK's (One To One)
        for attr in &model.attributes {
            let Some(fk_model_name) = &attr.foreign_key else {
                // No FK, No in degree
                continue;
            };

            if !model_lookup.contains_key(fk_model_name.as_str()) {
                return Err(anyhow!(
                    "Unknown Model for foreign key {}.{} => {}?",
                    model.name,
                    attr.value.name,
                    fk_model_name
                ));
            }

            if attr.value.nullable {
                // Nullable FK's do not constrain table creation order, and thus
                // can be left out of the topo sort
                continue;
            }

            // One To One: Person(Dog), but Dog must appear before Person in sql thus
            // the dependency is Dog -> Person; increase in degree of Person
            graph.entry(fk_model_name).or_default().push(&model.name);
            in_degree.entry(&model.name).and_modify(|d| *d += 1);

            attr_to_fk.insert(&attr.value.name, fk_model_name);
        }

        for nav in &model.navigation_properties {
            match &nav.foreign_key {
                CidlForeignKeyKind::OneToOne(fk_attr) => {
                    let Some(&fk_model_name) = attr_to_fk.get(fk_attr) else {
                        return Err(anyhow!(
                            "Unknown OneToOne attribute {}.{}",
                            model.name,
                            fk_attr
                        ));
                    };

                    if let CidlType::Model(ref model_name) = nav.value.cidl_type {
                        if model_name != fk_model_name {
                            return Err(anyhow!(
                                "Mismatched OneToOne types {}.{}",
                                model.name,
                                fk_attr
                            ));
                        }
                    } else {
                        return Err(anyhow!(
                            "Expected model type for OneToOne {}.{}",
                            model.name,
                            fk_attr
                        ));
                    }

                    // TODO: Revisit this. Should a user be able to decorate a One To One
                    // navigation property, but have no foreign key for it?
                    // ( ie, make the enum OneToOne(Option<String>) )
                }
                CidlForeignKeyKind::OneToMany => {
                    let CidlType::Model(fk_model_name) = nav.value.cidl_type.unwrap_array() else {
                        return Err(anyhow!("Invalid OneToMany type on {}", model.name));
                    };

                    if nav.value.nullable {
                        return Err(anyhow!(
                            "OneToMany cannot be nullable {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    }

                    // One to Many, Person( [Dog] ) => Dog -> Person,
                    // however in SQL this means Person must appear before Dog;
                    // increase in degree of Dog
                    graph.entry(&model.name).or_default().push(fk_model_name);
                    *in_degree.entry(fk_model_name).or_insert(0) += 1;
                }
                CidlForeignKeyKind::ManyToMany => {
                    // Ignore Many To Many relationships. We will inject these as
                    // junction tables after all tables are created, ensuring topo order.
                    continue;
                }
            }
        }
    }

    // Kahn's algorithm
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &v)| (v == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut ordered = Vec::with_capacity(models.len());
    while let Some(model_name) = queue.pop_front() {
        // Unwrap: safe because graph population covers the entire set
        ordered.push(*model_lookup.get(model_name).unwrap());

        if let Some(adjs) = graph.get(model_name) {
            for adj in adjs {
                // Unwrap: safe because `name_to_model` halts on unknown FK's
                let deg = in_degree.get_mut(adj).unwrap();
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if ordered.len() != models.len() {
        let cyclic: Vec<&str> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();
        return Err(anyhow!("Cycle detected: {}", cyclic.join(", ")));
    }

    Ok(ordered)
}

fn type_column(column: &mut ColumnDef, ty: &CidlType) -> Result<()> {
    match &ty {
        CidlType::Integer => column.integer(),
        CidlType::Real => column.decimal(),
        CidlType::Text => column.text(),
        CidlType::Blob => column.blob(),
        other => return Err(anyhow!("Invalid SQLite type {:?}", other)),
    };

    Ok(())
}

struct JunctionModel<'a> {
    model_name: &'a str,
    model_pk_name: &'a str,
    model_pk_type: CidlType,
}

#[derive(Default)]
struct JunctionTableBuilder<'a> {
    models: [Option<JunctionModel<'a>>; 2],
}

impl<'a> JunctionTableBuilder<'a> {
    fn key(model_name_a: &str, model_name_b: &str) -> String {
        let mut names = [model_name_a, model_name_b];
        names.sort();
        format!("fk_{}", names.join("_"))
    }

    fn model(&mut self, jm: JunctionModel<'a>) {
        if self.models[0].is_none() {
            self.models[0] = Some(jm);
        } else if self.models[1].is_none() {
            self.models[1] = Some(jm);
        } else {
            panic!("Too many models added");
        }
    }

    fn build(self) -> Result<String> {
        // Unwrap: assume program flow will never encounter an unfilled first model
        let [Some(a), Some(b)] = self.models else {
            return Err(anyhow!("Both models must be set for a junction table"));
        };

        let mut table = Table::create();

        // TODO: case jnct table better
        let mut col_a = ColumnDef::new(Alias::new(format!("a_{}", a.model_pk_name)));
        type_column(&mut col_a, &a.model_pk_type)?;

        let mut col_b = ColumnDef::new(Alias::new(format!("b_{}", b.model_pk_name)));
        type_column(&mut col_b, &b.model_pk_type)?;

        let key = Self::key(a.model_name, b.model_name);
        table
            .table(Alias::new(&key))
            .col(col_a.not_null())
            .col(col_b.not_null())
            .primary_key(
                Index::create()
                    .col(Alias::new(format!("a_{}", a.model_pk_name)))
                    .col(Alias::new(format!("b_{}", b.model_pk_name))),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(
                        Alias::new(&key),
                        Alias::new(format!("a_{}", a.model_pk_name)),
                    )
                    .to(Alias::new(a.model_name), Alias::new(a.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(
                        Alias::new(&key),
                        Alias::new(format!("b_{}", b.model_pk_name)),
                    )
                    .to(Alias::new(b.model_name), Alias::new(b.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            );

        Ok(format!("{};", table.to_string(SqliteQueryBuilder)))
    }
}

pub struct D1Generator {
    cidl: CidlSpec,
    wrangler: WranglerSpec,
}

impl D1Generator {
    pub fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self {
        Self { cidl, wrangler }
    }

    /// Validates and updates the Wrangler spec so that D1 can be used during
    /// code generation.
    pub fn wrangler(&self) -> WranglerSpec {
        // Validate existing database configs, filling in missing values with a default
        let mut res = self.wrangler.clone();
        for (i, d1) in res.d1_databases.iter_mut().enumerate() {
            if d1.binding.is_none() {
                d1.binding = Some(format!("D1_DB_{i}"));
            }

            if d1.database_name.is_none() {
                d1.database_name = Some(format!("{}_d1_{i}", self.cidl.project_name));
            }

            if d1.database_id.is_none() {
                eprintln!(
                    "Warning: Database \"{}\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/",
                    d1.database_name.as_ref().unwrap()
                )
            }
        }

        // Ensure a database exists (if there are even models), provide a default if not
        if !self.cidl.models.is_empty() && res.d1_databases.is_empty() {
            res.d1_databases.push(D1Database {
                binding: Some(String::from("D1_DB")),
                database_name: Some(String::from("default")),
                database_id: None,
            });

            eprintln!(
                "Database \"default\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/"
            );
        }

        res
    }

    /// Creates a Sqlite database schema from the CIDL's Model AST
    // NOTE: Model names, attributes do not need to be santized as SeaQuery
    // wraps them in quote literals.
    pub fn sqlite(&self) -> Result<String> {
        let mut res = Vec::new();
        let models = sql_topo_sort(&self.cidl.models)?;
        let mut pk_lookup = HashMap::<&String, &TypedValue>::new();
        let mut many_to_one_lookup = HashMap::<&String, Vec<&String>>::new();
        let mut junction_tables = HashMap::<String, JunctionTableBuilder>::new();

        for &model in &models {
            let mut table = Table::create();
            let mut column_names = HashSet::new();

            // Table will always just be the name of the model, in it's original case
            table.table(Alias::new(model.name.clone()));

            for attr in model.attributes.iter() {
                // Validate column name
                if !column_names.insert(attr.value.name.as_str()) {
                    return Err(anyhow!(
                        "Duplicate column names {}.{}",
                        model.name,
                        attr.value.name
                    ));
                }

                // Columns will always just be the name of the attribute in it's original case
                let mut column = ColumnDef::new(Alias::new(attr.value.name.clone()));

                // Set Sqlite type
                type_column(&mut column, &attr.value.cidl_type)?;

                // Set primary key
                if attr.primary_key {
                    if pk_lookup.contains_key(&model.name) {
                        return Err(anyhow!("Duplicate primary keys on model {}", model.name));
                    }

                    if attr.value.nullable {
                        return Err(anyhow!("A primary key cannot be nullable."));
                    }

                    if attr.foreign_key.is_some() {
                        // TODO: Revisit this, should this design be allowed?
                        return Err(anyhow!("A primary key cannot be a foreign key"));
                    }

                    column.primary_key();
                    pk_lookup.insert(&model.name, &attr.value);
                }
                // Set nullability
                else if !attr.value.nullable {
                    column.not_null();
                }

                // Set attribute foreign key
                if let Some(fk_model_name) = &attr.foreign_key {
                    // Unwrap: safe because of topo order
                    let pk_name = &pk_lookup.get(&fk_model_name).unwrap().name;
                    table.foreign_key(
                        ForeignKey::create()
                            .from(
                                Alias::new(model.name.clone()),
                                Alias::new(attr.value.name.as_str()),
                            )
                            .to(Alias::new(fk_model_name.clone()), Alias::new(pk_name))
                            .on_update(sea_query::ForeignKeyAction::Cascade)
                            .on_delete(sea_query::ForeignKeyAction::Restrict),
                    );
                }

                table.col(column);
            }

            if !pk_lookup.contains_key(&model.name) {
                return Err(anyhow!("Missing primary key on model {}", model.name));
            }

            let many_to_one = many_to_one_lookup.entry(&model.name).or_default();
            for model_name in many_to_one {
                // TODO: Hardcoding PascalCase
                let fk_id_col_name = format!("{}Id", *model_name);
                let mut fk_id_col = ColumnDef::new(Alias::new(&fk_id_col_name));

                // Unwrap: safe because `sql_topo_sort` guaruantees dependencies before dependents
                let pk = pk_lookup.get(*model_name).unwrap();
                type_column(&mut fk_id_col, &pk.cidl_type)?;
                table.col(fk_id_col);

                table.foreign_key(
                    ForeignKey::create()
                        .from(Alias::new(&model.name), Alias::new(&fk_id_col_name))
                        .to(Alias::new(*model_name), Alias::new(&pk.name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );
            }

            for nav_prop in &model.navigation_properties {
                let fk = &nav_prop.foreign_key;
                match fk {
                    CidlForeignKeyKind::OneToOne(_) => {
                        // Already validated in `topo_sort`, and created in attribute loop
                    }
                    CidlForeignKeyKind::OneToMany => {
                        let fk_model_name = match nav_prop.value.cidl_type.unwrap_array() {
                            CidlType::Model(model_name) => model_name,
                            _ => unreachable!("Expected topo sort type verificiation"),
                        };

                        many_to_one_lookup
                            .entry(fk_model_name)
                            .or_default()
                            .push(&model.name);
                    }
                    CidlForeignKeyKind::ManyToMany => {
                        let fk_model_name = match nav_prop.value.cidl_type.unwrap_array() {
                            CidlType::Model(model_name) => model_name,
                            _ => unreachable!("Expected topo sort type verificiation"),
                        };

                        let pk = pk_lookup.get(&model.name).unwrap();
                        junction_tables
                            .entry(JunctionTableBuilder::key(&model.name, fk_model_name))
                            .or_default()
                            .model(JunctionModel {
                                model_name: &model.name,
                                model_pk_name: &pk.name,
                                model_pk_type: pk.cidl_type.clone(),
                            });
                    }
                }
            }
            res.push(format!("{};", table.build(SqliteQueryBuilder)));
        }

        // Add junction tables to the SQL statement
        for (_, builder) in junction_tables {
            res.push(builder.build()?);
        }

        Ok(res.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use crate::{D1Generator, sql_topo_sort};
    use common::{
        CidlForeignKeyKind, CidlType, Model,
        builder::{ModelBuilder, create_cidl, create_wrangler},
    };

    macro_rules! expected_str {
        ($got:expr, $expected:expr) => {{
            let got_val = &$got;
            let expected_val = &$expected;
            assert!(
                got_val.to_string().contains(&expected_val.to_string()),
                "Expected `{}`, got:\n{:?}",
                expected_val,
                got_val
            );
        }};
    }

    fn is_topo_ordered(sorted: &[&Model]) -> bool {
        use std::collections::HashSet;

        let mut visited = HashSet::<String>::new();

        for &model in sorted {
            for attribute in &model.attributes {
                if let Some(fk_model_name) = &attribute.foreign_key {
                    if !visited.contains(fk_model_name) && !attribute.value.nullable {
                        return false;
                    }
                }
            }

            for nav_prop in &model.navigation_properties {
                match nav_prop.foreign_key {
                    CidlForeignKeyKind::OneToOne(_) => {
                        // Handled by attribute
                    }
                    CidlForeignKeyKind::OneToMany => {
                        let fk_model_name = match nav_prop.value.cidl_type.unwrap_array() {
                            CidlType::Model(model_name) => model_name,
                            _ => unreachable!("Assume types are sanitized"),
                        };

                        if visited.contains(fk_model_name) && !nav_prop.value.nullable {
                            return false;
                        }
                    }
                    CidlForeignKeyKind::ManyToMany => {
                        // Handeled by junction tables
                    }
                }
            }

            visited.insert(model.name.clone());
        }

        true
    }

    #[test]
    fn test_sqlite_output() {
        // Empty
        {
            // Arrange
            let cidl = create_cidl(vec![]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("Empty models should succeed");

            // Assert
            assert!(
                sql.is_empty(),
                "Expected empty SQL output for empty CIDL, got: {}",
                sql
            );
        }

        // Primary key, Basic attributes
        {
            // Arrange
            let cidl = create_cidl(vec![
                ModelBuilder::new("User")
                    .id() // adds a primary key
                    .attribute("name", CidlType::Text, true, None)
                    .attribute("age", CidlType::Integer, false, None)
                    .build(),
            ]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("gen_sqlite to work");

            // Assert
            expected_str!(sql, "CREATE TABLE");
            expected_str!(sql, "\"id\" integer PRIMARY KEY");
            expected_str!(sql, "\"name\" text");
            expected_str!(sql, "\"age\" integer NOT NULL");
        }

        // One to One FK's
        {
            // Arrange
            let cidl = create_cidl(vec![
                ModelBuilder::new("User")
                    .id()
                    .attribute("dogId", CidlType::Integer, false, Some("Dog".to_string()))
                    .build(),
                ModelBuilder::new("Dog").id().build(),
            ]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("gen_sqlite to work");

            // Assert
            expected_str!(
                sql,
                r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
            );
        }

        // One to One FK's with Nav Prop
        {
            // Arrange
            let cidl = create_cidl(vec![
                ModelBuilder::new("User")
                    .id()
                    .attribute("dogId", CidlType::Integer, false, Some("Dog".into()))
                    .nav_p(
                        "dog",
                        CidlType::Model("Dog".into()),
                        false,
                        CidlForeignKeyKind::OneToOne("dogId".into()),
                    )
                    .build(),
                ModelBuilder::new("Dog").id().build(),
            ]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("gen_sqlite to work");

            // Assert
            expected_str!(
                sql,
                r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
            );
        }

        // One to Many
        {
            // Arrange
            let cidl = create_cidl(vec![
                ModelBuilder::new("Dog").id().build(),
                ModelBuilder::new("Cat").id().build(),
                ModelBuilder::new("Person")
                    .id()
                    .nav_p(
                        "dogs",
                        CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                        false,
                        CidlForeignKeyKind::OneToMany,
                    )
                    .nav_p(
                        "cats",
                        CidlType::Array(Box::new(CidlType::Model("Cat".to_string()))),
                        false,
                        CidlForeignKeyKind::OneToMany,
                    )
                    .build(),
                ModelBuilder::new("Boss")
                    .id()
                    .nav_p(
                        "persons",
                        CidlType::Array(Box::new(CidlType::Model("Person".to_string()))),
                        false,
                        CidlForeignKeyKind::OneToMany,
                    )
                    .build(),
            ]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("gen_sqlite to work");

            // Assert: Person table has FK to Boss
            expected_str!(sql, r#"CREATE TABLE "Person""#);
            expected_str!(sql, r#""BossId" integer"#);
            expected_str!(
                sql,
                r#"FOREIGN KEY ("BossId") REFERENCES "Boss" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
            );

            // Assert: Dog table has FK to Person
            expected_str!(sql, r#"CREATE TABLE "Dog""#);
            expected_str!(sql, r#""PersonId" integer"#);
            expected_str!(
                sql,
                r#"FOREIGN KEY ("PersonId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
            );

            // Assert: Cat table has FK to Person
            expected_str!(sql, r#"CREATE TABLE "Cat""#);
            expected_str!(sql, r#""PersonId" integer"#);
            expected_str!(
                sql,
                r#"FOREIGN KEY ("PersonId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
            );
        }

        // Many to Many
        {
            // Arrange
            let cidl = create_cidl(vec![
                ModelBuilder::new("Student")
                    .id()
                    .nav_p(
                        "courses",
                        CidlType::Array(Box::new(CidlType::Model("Course".to_string()))),
                        false,
                        CidlForeignKeyKind::ManyToMany,
                    )
                    .build(),
                ModelBuilder::new("Course")
                    .id()
                    .nav_p(
                        "students",
                        CidlType::Array(Box::new(CidlType::Model("Student".to_string()))),
                        false,
                        CidlForeignKeyKind::ManyToMany,
                    )
                    .build(),
            ]);
            let d1gen = D1Generator::new(cidl, create_wrangler());

            // Act
            let sql = d1gen.sqlite().expect("gen_sqlite to work");

            // Assert: Junction table exists
            expected_str!(sql, r#"CREATE TABLE "fk_Course_Student""#);

            // Assert: Junction table has StudentId + CourseId composite PK
            expected_str!(sql, r#""a_id" integer NOT NULL"#);
            expected_str!(sql, r#""b_id" integer NOT NULL"#);
            expected_str!(sql, r#"PRIMARY KEY ("a_id", "b_id")"#);

            // Assert: FKs to Student and Course
            expected_str!(
                sql,
                r#"FOREIGN KEY ("b_id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
            );
            expected_str!(
                sql,
                r#"FOREIGN KEY ("a_id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
            );
        }
    }

    #[test]
    fn test_duplicate_column_error() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("foo", CidlType::Integer, false, None)
                .attribute("foo", CidlType::Real, true, None)
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "Duplicate column names");
    }

    #[test]
    fn test_duplicate_primary_key_error() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .pk("id", CidlType::Integer)
                .pk("user_id", CidlType::Integer)
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "Duplicate primary keys");
    }

    #[test]
    fn test_nullable_primary_key_error() {
        // Arrange
        let mut model = ModelBuilder::new("User").id().build();
        model.attributes[0].value.nullable = true;

        let spec = create_cidl(vec![model]);
        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "A primary key cannot be nullable.");
    }

    #[test]
    fn test_missing_primary_key_error() {
        // Arrange
        let spec = create_cidl(vec![ModelBuilder::new("User").build()]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "Missing primary key on model");
    }

    #[test]
    fn test_one_to_one_topo_sort_yields_correct_order() {
        // Arrange
        let creators: Vec<Box<dyn Fn() -> Model>> = vec![
            Box::new(|| ModelBuilder::new("Treat").id().build()),
            Box::new(|| ModelBuilder::new("Food").id().build()),
            Box::new(|| {
                ModelBuilder::new("Dog")
                    .id()
                    .attribute(
                        "treatId",
                        CidlType::Integer,
                        false,
                        Some("Treat".to_string()),
                    )
                    .attribute("foodId", CidlType::Integer, false, Some("Food".to_string()))
                    .build()
            }),
            Box::new(|| ModelBuilder::new("Independent").id().build()),
            Box::new(|| {
                ModelBuilder::new("Person")
                    .id()
                    .attribute("dogId", CidlType::Integer, false, Some("Dog".to_string()))
                    .build()
            }),
        ];

        // 5 items, 120 permutations
        for permutation in creators.iter().permutations(creators.len()) {
            // Act
            let perm_slice: Vec<Model> = permutation.into_iter().map(|f| f()).collect();

            let sorted = sql_topo_sort(&perm_slice).expect("topo_sort failed");

            // Assert
            assert!(is_topo_ordered(&sorted));
        }
    }

    #[test]
    fn test_duplicate_model_error() {
        // Arrange
        let models = vec![
            ModelBuilder::new("User").id().build(),
            ModelBuilder::new("User").id().build(),
        ];

        // Act
        let err = sql_topo_sort(&models).unwrap_err();

        // Assert
        expected_str!(err, "Duplicate model name");
    }

    #[test]
    fn test_unknown_foreign_key_error() {
        // Arrange
        let models = vec![
            ModelBuilder::new("User")
                .id()
                .attribute(
                    "nonExistentId",
                    CidlType::Integer,
                    false,
                    Some("NonExistent".to_string()),
                )
                .build(),
        ];

        // Act
        let err = sql_topo_sort(&models).unwrap_err();

        // Assert
        expected_str!(
            err,
            "Unknown Model for foreign key User.nonExistentId => NonExistent?"
        );
    }

    #[test]
    fn test_cycle_detection_error() {
        // Arrange
        // A -> B -> C -> A
        let models = vec![
            ModelBuilder::new("A")
                .id()
                .attribute("bId", CidlType::Integer, false, Some("B".to_string()))
                .build(),
            ModelBuilder::new("B")
                .id()
                .attribute("cId", CidlType::Integer, false, Some("C".to_string()))
                .build(),
            ModelBuilder::new("C")
                .id()
                .attribute("aId", CidlType::Integer, false, Some("A".to_string()))
                .build(),
        ];

        // Act
        let err = sql_topo_sort(&models).unwrap_err();

        // Assert
        expected_str!(err, "Cycle detected");
    }

    #[test]
    fn test_nullability_prevents_cycle_error() {
        // Arrange
        // A -> B -> C -> Nullable<A>
        let models = vec![
            ModelBuilder::new("A")
                .id()
                .attribute("bId", CidlType::Integer, false, Some("B".to_string()))
                .build(),
            ModelBuilder::new("B")
                .id()
                .attribute("cId", CidlType::Integer, false, Some("C".to_string()))
                .build(),
            ModelBuilder::new("C")
                .id()
                .attribute("aId", CidlType::Integer, true, Some("A".to_string()))
                .build(),
        ];

        // Act
        let sorted = sql_topo_sort(&models);

        // Assert
        assert!(is_topo_ordered(&sorted.unwrap()));
    }

    #[test]
    fn test_one_to_many_topo_sort_yields_correct_order() {
        // Arrange
        let models = [
            ModelBuilder::new("Dog").id().build(),
            ModelBuilder::new("Cat").id().build(),
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .nav_p(
                    "cats",
                    CidlType::Array(Box::new(CidlType::Model("Cat".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
            ModelBuilder::new("Boss")
                .id()
                .nav_p(
                    "persons",
                    CidlType::Array(Box::new(CidlType::Model("Person".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
        ];

        // Act
        let sorted = sql_topo_sort(&models).expect("topo_sort failed");

        // Assert
        assert!(is_topo_ordered(&sorted));
    }

    #[test]
    fn test_invalid_sqlite_type_error() {
        // Arrange: Attribute with unsupported type (Model instead of primitive)
        let spec = create_cidl(vec![
            ModelBuilder::new("BadType")
                .id()
                .attribute("attr", CidlType::Model("User".into()), false, None)
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "Invalid SQLite type");
    }

    #[test]
    fn test_one_to_one_nav_property_unknown_attribute_reference_error() {
        // Arrange: OneToOne nav property references a non-existent attribute
        let spec = create_cidl(vec![
            ModelBuilder::new("Dog").id().build(),
            ModelBuilder::new("User")
                .id()
                .nav_p(
                    "dog",
                    CidlType::Model("Dog".into()),
                    false,
                    CidlForeignKeyKind::OneToOne("dogId".to_string()),
                )
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        expected_str!(err, "Unknown OneToOne attribute User.dogId");
    }
}
