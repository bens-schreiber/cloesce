use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use common::{CidlForeignKeyKind, CidlSpec, CidlType, D1Database, Model, TypedValue, WranglerSpec};

use anyhow::{Result, anyhow};

use sea_query::{
    Alias, ColumnDef, ForeignKey, Index, SqliteQueryBuilder, Table, TableCreateStatement,
};

/// Topological sort via Kahns algorithm, placing attribute foreign keys
/// before their dependencies.
///
/// Returns an error if there is:
/// - a cycle
/// - duplicate models
/// - unknown FK model
/// - unknown attribute reference
fn topo_sort<'a>(models: &'a [Model]) -> Result<Vec<&'a Model>> {
    let mut name_to_model = HashMap::<&String, &'a Model>::new();
    let mut name_to_in_degree = BTreeMap::<&String, usize>::new();
    let mut graph = BTreeMap::<&String, Vec<&String>>::new();

    // Detect dups, populate reverse lookup
    for model in models {
        match name_to_model.entry(&model.name) {
            std::collections::hash_map::Entry::Vacant(vacant_entry) => vacant_entry.insert(model),
            std::collections::hash_map::Entry::Occupied(_) => {
                return Err(anyhow!("Duplicate model name: {}", model.name));
            }
        };
    }

    // Increment in-degree
    for model in models {
        graph.entry(&model.name).or_default();
        name_to_in_degree.entry(&model.name).or_insert(0);

        // Handle attribute FK's (One To One)
        let mut fk_attribute_names = HashSet::new();
        for attribute in &model.attributes {
            let fk_model_name = match &attribute.foreign_key {
                Some(fk_model_name) => fk_model_name,

                // No In-Degree
                _ => continue,
            };

            if !name_to_model.contains_key(&fk_model_name) {
                return Err(anyhow!(
                    "Unknown Model for foreign key {}.{} => {}?",
                    model.name,
                    attribute.value.name,
                    fk_model_name
                ));
            }

            if attribute.value.nullable {
                // Nullable FK's do not constrain table creation order, and thus
                // can be left out of the topo sort
                continue;
            }

            // One To One, ex: Person depends on Dog, so the topo order would be Dog -> Person,
            // increasing the in degree of person
            graph.entry(fk_model_name).or_default().push(&model.name);
            *name_to_in_degree.entry(&model.name).or_insert(0) += 1;
            fk_attribute_names.insert(&attribute.value.name);
        }

        for nav_prop in &model.navigation_properties {
            match &nav_prop.foreign_key {
                CidlForeignKeyKind::OneToOne(fk_attribute_name) => {
                    if !fk_attribute_names.contains(&fk_attribute_name) {
                        return Err(anyhow!(
                            "Unknown One to One attribute name on model {}: {}",
                            model.name,
                            fk_attribute_name
                        ));
                    }

                    // TODO: Revisit this. Should a user be able to decorate a One To One
                    // navigation property, but have no foreign key for it?
                    // ( ie, make the enum OneToOne(Option<String>) )
                    continue;
                }
                CidlForeignKeyKind::OneToMany => {
                    // One to Many should always be an array of Models
                    let fk_model_name = match &nav_prop.value.cidl_type.unwrap_array() {
                        CidlType::Model(fk_model_name) => fk_model_name,
                        _ => return Err(anyhow!("Invalid OneToMany type on model {}", model.name)),
                    };

                    if nav_prop.value.nullable {
                        return Err(anyhow!(
                            "A OneToMany collection cannot be nullable {}.{}",
                            model.name,
                            nav_prop.value.name
                        ));
                    }

                    /*
                        In the relationship Person([Dog]), where Person owns many dogs, you may think the correct
                        topological order would be Person -> Dog, because the ID for Person should exist on Dog, which is
                        true in pure SQL terms.

                        However, in our AST, Person “owns” the reference to Dog (navigation property), and it is the Person’s
                        responsibility to then go and place that dependency on dog, making the topo order Dog -> Person.

                        Thus, the in degree of Person should be increased.
                    */
                    graph.entry(fk_model_name).or_default().push(&model.name);
                    *name_to_in_degree.entry(&model.name).or_insert(0) += 1;
                }
                CidlForeignKeyKind::ManyToMany => {
                    // Ignore Many To Many relationships. We will inject these as
                    // junction tables after all tables are created, ensuring topo order.
                    continue;
                }
            }
        }
    }

    let mut queue = name_to_in_degree
        .iter()
        .filter_map(|(&name, &v)| (v == 0).then_some(name))
        .collect::<VecDeque<&String>>();

    let mut ordered = vec![];
    while let Some(model) = queue.pop_front() {
        // Unwrap: safe because graph population covers the entire set
        ordered.push(*name_to_model.get(model).unwrap());

        if let Some(adjs) = graph.get(model) {
            for adj in adjs {
                // Unwrap: safe because `name_to_model` halts on unknown FK's
                let in_degree = name_to_in_degree.get_mut(adj).unwrap();
                *in_degree -= 1;

                if *in_degree == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if ordered.len() != models.len() {
        let cyclic_models: Vec<String> = name_to_in_degree
            .iter()
            .filter_map(|(&name, &deg)| (deg > 0).then_some(name.clone()))
            .collect();
        return Err(anyhow!(
            "Cycle detected involving the following models: {}",
            cyclic_models.join(", ")
        ));
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
    a: Option<JunctionModel<'a>>,
    b: Option<JunctionModel<'a>>,
}

impl<'a> JunctionTableBuilder<'a> {
    fn key(model_name_a: &str, model_name_b: &str) -> String {
        let mut alpha_sorted = vec![model_name_a, model_name_b];
        alpha_sorted.sort();

        format!("fk_{}", alpha_sorted.join("_"))
    }

    fn model(&mut self, jm: JunctionModel<'a>) {
        match (&self.a, &self.b) {
            (None, None) => {
                self.a = Some(jm);
            }
            (Some(_), None) => {
                self.b = Some(jm);
            }
            _ => panic!("Bad state or extraneous models"),
        }
    }

    fn build(self) -> Result<String> {
        // Unwrap: assume program flow will never encounter an unfilled first model
        let a = self.a.unwrap();
        let b = self
            .b
            .ok_or_else(|| anyhow!("Missing linked ManyToMany relationship on {}", a.model_name))?;

        let mut table = Table::create();

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
                    .to(Alias::new(a.model_name), Alias::new(a.model_pk_name)),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(
                        Alias::new(&key),
                        Alias::new(format!("b_{}", b.model_pk_name)),
                    )
                    .to(Alias::new(b.model_name), Alias::new(b.model_pk_name)),
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
        let models = topo_sort(&self.cidl.models)?;
        let mut pk_lookup = HashMap::<&String, &TypedValue>::new();
        let mut table_lookup = HashMap::<&String, TableCreateStatement>::new();
        let mut junction_tables = HashMap::<String, JunctionTableBuilder>::new();

        for &model in &models {
            let mut table = Table::create();
            let mut column_names = HashSet::new();

            // Table will always just be the name of the model, in it's original case
            table.table(Alias::new(model.name.clone()));

            for attribute in model.attributes.iter() {
                // Validate column name
                if !column_names.insert(attribute.value.name.as_str()) {
                    return Err(anyhow!(
                        "Duplicate column names {}.{}",
                        model.name,
                        attribute.value.name
                    ));
                }

                // Columns will always just be the name of the attribute in it's original case
                let mut column = ColumnDef::new(Alias::new(attribute.value.name.clone()));

                // Set Sqlite type
                match &attribute.value.cidl_type {
                    CidlType::Integer => column.integer(),
                    CidlType::Real => column.decimal(),
                    CidlType::Text => column.text(),
                    CidlType::Blob => column.blob(),
                    other => return Err(anyhow!("Invalid SQLite type {:?}", other)),
                };

                // Set primary key
                if attribute.primary_key {
                    if pk_lookup.contains_key(&model.name) {
                        return Err(anyhow!("Duplicate primary keys on model {}", model.name));
                    }

                    if attribute.value.nullable {
                        return Err(anyhow!("A primary key cannot be nullable."));
                    }

                    if attribute.foreign_key.is_some() {
                        // TODO: Revisit this, should this design be allowed?
                        return Err(anyhow!("A primary key cannot be a foreign key"));
                    }

                    column.primary_key();
                    pk_lookup.insert(&model.name, &attribute.value);
                }
                // Set nullability
                else if !attribute.value.nullable {
                    column.not_null();
                }

                // Set attribute foreign key
                if let Some(fk_model_name) = &attribute.foreign_key {
                    // Unwrap: safe because of topo order
                    let pk_name = &pk_lookup.get(&fk_model_name).unwrap().name;

                    table.foreign_key(
                        ForeignKey::create()
                            .from(
                                Alias::new(model.name.clone()),
                                Alias::new(attribute.value.name.as_str()),
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

                        // Unwrap: safe because `topo_sort` ensures the fk model dependency has
                        // already been inserted
                        let fk_table = table_lookup.get_mut(&fk_model_name).unwrap();

                        // Unwrap: safe because we explicitly set it
                        let pk_name = &pk_lookup.get(&model.name).unwrap().name;

                        fk_table.foreign_key(
                            ForeignKey::create()
                                .from(
                                    Alias::new(fk_model_name.clone()),
                                    // TODO: We're hardcoding PascalCase here (or camelCase)
                                    Alias::new(format!("{}Id", model.name)),
                                )
                                .to(Alias::new(model.name.clone()), Alias::new(pk_name))
                                .on_update(sea_query::ForeignKeyAction::Cascade)
                                .on_delete(sea_query::ForeignKeyAction::Restrict),
                        );
                    }
                    CidlForeignKeyKind::ManyToMany => {
                        let fk_model_name = match nav_prop.value.cidl_type.unwrap_array() {
                            CidlType::Model(model_name) => model_name,
                            _ => unreachable!("Expected topo sort type verificiation"),
                        };

                        let pk = pk_lookup.get(&model.name).unwrap();

                        junction_tables
                            .entry(JunctionTableBuilder::key(&model.name, &fk_model_name))
                            .or_default()
                            .model(JunctionModel {
                                model_name: &model.name,
                                model_pk_name: &pk.name,
                                model_pk_type: pk.cidl_type.clone(),
                            });
                    }
                }
            }

            table_lookup.insert(&model.name, table);
        }

        // Loop in topo order and build the SQL statement
        let mut res = Vec::new();
        for model in models {
            // Unwrap: safe because the lookup was populated from `models`
            let table = table_lookup.get(&model.name).unwrap();
            res.push(format!("{};", table.to_string(SqliteQueryBuilder)));
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

    use crate::{D1Generator, topo_sort};
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

                        if !visited.contains(fk_model_name) && nav_prop.value.nullable {
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
    fn test_empty_cidl_models_yields_empty_sqlite() {
        // Arrange: Empty CIDL
        let spec = create_cidl(vec![]);
        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("Empty models should succeed");

        // Assert
        assert!(
            sql.is_empty(),
            "Expected empty SQL output for empty CIDL, got: {}",
            sql
        );
    }

    #[test]
    fn test_primary_key_and_value_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("name", CidlType::Text, true, None)
                .attribute("age", CidlType::Integer, false, None)
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        expected_str!(sql, "CREATE TABLE");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
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

            let sorted = topo_sort(&perm_slice).expect("topo_sort failed");

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
        let err = topo_sort(&models).unwrap_err();

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
        let err = topo_sort(&models).unwrap_err();

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
        let err = topo_sort(&models).unwrap_err();

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
        let sorted = topo_sort(&models);

        // Assert
        assert!(is_topo_ordered(&sorted.unwrap()));
    }

    #[test]
    fn test_one_to_one_fk_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("dogId", CidlType::Integer, false, Some("Dog".to_string()))
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    #[test]
    fn test_one_to_many_topo_sort_yields_correct_order() {
        // Arrange
        let models: Vec<Model> = vec![
            ModelBuilder::new("Treat").id().build(),
            ModelBuilder::new("Dog")
                .id()
                .attribute("treat", CidlType::Integer, false, Some("Treat".to_string()))
                .build(),
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
        ];

        // Act
        let sorted = topo_sort(&models).expect("topo_sort failed");

        // Assert
        assert!(is_topo_ordered(&sorted));
    }

    #[test]
    fn test_one_to_many_fk_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany,
                )
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("UserId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    #[test]
    fn test_many_to_many_fk_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![
            ModelBuilder::new("User")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                    false,
                    CidlForeignKeyKind::ManyToMany,
                )
                .build(),
            ModelBuilder::new("Dog")
                .id()
                .nav_p(
                    "users",
                    CidlType::Array(Box::new(CidlType::Model("User".to_string()))),
                    false,
                    CidlForeignKeyKind::ManyToMany,
                )
                .build(),
        ]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        std::fs::write("out", sql);
        // expected_str!(
        //     sql,
        //     r#"FOREIGN KEY ("UserId") REFERENCES "User" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        // );
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
        expected_str!(
            err,
            "Unknown One to One attribute name on model User: dogId"
        );
    }
}
