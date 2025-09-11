use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use common::{CidlForeignKeyKind, CidlSpec, CidlType, D1Database, Model, WranglerSpec};

use anyhow::{Result, anyhow};
use sea_query::{Alias, ColumnDef, ForeignKey, SqliteQueryBuilder, Table};

/// Topological sort via Kahns algorithm, placing foreign keys before their dependencies.
///
/// Returns an error if there is:
/// - a cycle
/// - duplicate models
/// - unknown FK name
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

        for attribute in &model.attributes {
            if let Some(fk) = &attribute.foreign_key {
                if !name_to_model.contains_key(&fk.model_name) {
                    return Err(anyhow!(
                        "Unknown foreign key on model {}: {}",
                        model.name,
                        fk.model_name
                    ));
                }

                if attribute.value.nullable {
                    // Important distinction: nullable FK's do not constrain table creation order,
                    // and thus can be left out of our topo sort
                    continue;
                }

                graph.entry(&fk.model_name).or_default().push(&model.name);
                *name_to_in_degree.entry(&model.name).or_insert(0) += 1;
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

    /// Creates a Sqlite database schema from the CIDL Model AST
    // Note: Model names, attributes do not need to be santized as SeaQuery
    // wraps them in quote literals.
    pub fn sqlite(&self) -> Result<String> {
        let models = topo_sort(&self.cidl.models)?;

        let mut res = Vec::<String>::default();
        let mut pk_name_lookup = HashMap::<&String, String>::new();

        for model in models {
            let mut table = Table::create();
            let mut pk_name: Option<String> = None;
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

                // Set primary key
                if attribute.primary_key {
                    if let Some(pk_name) = &pk_name {
                        return Err(anyhow!(
                            "Duplicate primary keys {} {}",
                            pk_name,
                            attribute.value.name
                        ));
                    }

                    if attribute.value.nullable {
                        return Err(anyhow!("A primary key cannot be nullable."));
                    }

                    if attribute.foreign_key.is_some() {
                        // TODO: Revisit this, should this design be allowed?
                        return Err(anyhow!("A primary key cannot be a foreign key"));
                    }

                    column.primary_key();
                    pk_name = Some(attribute.value.name.clone());
                }
                // Set nullability
                else if !attribute.value.nullable {
                    column.not_null();
                }

                // Set Sqlite type
                match &attribute.value.cidl_type {
                    CidlType::Integer => column.integer(),
                    CidlType::Real => column.decimal(),
                    CidlType::Text => column.text(),
                    CidlType::Blob => column.blob(),
                    other => return Err(anyhow!("Invalid SQLite type {:?}", other)),
                };

                // Set foreign key
                if let Some(fk) = &attribute.foreign_key {
                    // Unwrap: safe because `topo_sort` validates all FK's
                    let pk_name = pk_name_lookup.get(&fk.model_name).unwrap();

                    match fk.kind {
                        CidlForeignKeyKind::OneToOne => {
                            table.foreign_key(
                                ForeignKey::create()
                                    .name(format!("fk_{}_{}", model.name, fk.model_name))
                                    .from(
                                        Alias::new(model.name.clone()),
                                        Alias::new(attribute.value.name.as_str()),
                                    )
                                    .to(Alias::new(fk.model_name.clone()), Alias::new(pk_name))
                                    .on_update(sea_query::ForeignKeyAction::Cascade)
                                    .on_delete(sea_query::ForeignKeyAction::Restrict),
                            );
                        }
                        CidlForeignKeyKind::ManyToMany => todo!(),
                        CidlForeignKeyKind::OneToMany => todo!(),
                    };
                }

                table.col(column);
            }

            // Verify a primary key exists
            match pk_name {
                Some(pk_name) => pk_name_lookup.insert(&model.name, pk_name.clone()),
                None => return Err(anyhow!("Model {} is missing a primary key.", model.name)),
            };

            res.push(format!("{};", table.to_string(SqliteQueryBuilder)));
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
                if let Some(fk) = &attribute.foreign_key {
                    if !visited.contains(&fk.model_name) && !attribute.value.nullable {
                        return false;
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
                .attribute("name", CidlType::Text, true)
                .attribute("age", CidlType::Integer, false)
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
        let spec = create_cidl(vec![ModelBuilder::new("User").id().id().build()]);

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
        expected_str!(err, "User is missing a primary key.");
    }

    #[test]
    fn test_topo_sort_yields_correct_order() {
        // Arrange
        // note: this test is kind of ridiculous
        let creators: Vec<Box<dyn Fn() -> Model>> = vec![
            Box::new(|| ModelBuilder::new("Treat").id().build()),
            Box::new(|| ModelBuilder::new("Food").id().build()),
            Box::new(|| {
                ModelBuilder::new("Dog")
                    .id()
                    .fk(
                        "TreatId",
                        CidlType::Integer,
                        CidlForeignKeyKind::OneToOne,
                        "Treat",
                        false,
                    )
                    .fk(
                        "FoodId",
                        CidlType::Integer,
                        CidlForeignKeyKind::OneToOne,
                        "Food",
                        false,
                    )
                    .build()
            }),
            Box::new(|| ModelBuilder::new("Independent").id().build()),
            Box::new(|| {
                ModelBuilder::new("Person")
                    .id()
                    .fk(
                        "DogId",
                        CidlType::Integer,
                        CidlForeignKeyKind::OneToOne,
                        "Dog",
                        false,
                    )
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
                .fk(
                    "NonExistentId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "NonExistent",
                    false,
                )
                .build(),
        ];

        // Act
        let err = topo_sort(&models).unwrap_err();

        // Assert
        expected_str!(err, "Unknown foreign key");
    }

    #[test]
    fn test_cycle_detection_error() {
        // Arrange
        // A -> B -> C -> A
        let models = vec![
            ModelBuilder::new("A")
                .id()
                .fk(
                    "BId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "B",
                    false,
                )
                .build(),
            ModelBuilder::new("B")
                .id()
                .fk(
                    "CId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "C",
                    false,
                )
                .build(),
            ModelBuilder::new("C")
                .id()
                .fk(
                    "AId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "A",
                    false,
                )
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
                .fk(
                    "BId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "B",
                    false,
                )
                .build(),
            ModelBuilder::new("B")
                .id()
                .fk(
                    "CId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "C",
                    false,
                )
                .build(),
            ModelBuilder::new("C")
                .id()
                .fk(
                    "AId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "A",
                    true, // nullable
                )
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
                .fk(
                    "dogId",
                    CidlType::Integer,
                    CidlForeignKeyKind::OneToOne,
                    "Dog",
                    false,
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
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }
}
