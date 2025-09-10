use std::collections::{BTreeMap, HashMap, VecDeque};

use common::{CidlForeignKey, CidlSpec, CidlType, D1Database, Model, WranglerSpec};

use anyhow::{Result, anyhow};
use sea_query::{Alias, ColumnDef, ForeignKey, SqliteQueryBuilder, Table};

/// Topological sort via Kahns algorithm
/// Returns an error if there is a cycle, duplicate models, or an unknown FK.
fn topo_sort<'a>(models: &'a [Model]) -> Result<Vec<&'a Model>> {
    let mut name_to_model = HashMap::<&'a str, &'a Model>::new();
    let mut name_to_in_degree = BTreeMap::<&'a str, usize>::new();
    let mut graph = BTreeMap::<&'a str, Vec<&'a str>>::new();

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
                if !name_to_model.contains_key(fk.as_str()) {
                    return Err(anyhow!(
                        "Unknown foreign key on model {}: {}",
                        model.name,
                        fk.as_str()
                    ));
                }

                if attribute.value.nullable {
                    // Important distinction: nullable FK's do not constrain table creation order,
                    // and thus can be left out of our topo sort
                    continue;
                }

                graph.entry(fk.as_str()).or_default().push(&model.name);
                *name_to_in_degree.entry(&model.name).or_insert(0) += 1;
            }
        }
    }

    let mut queue = name_to_in_degree
        .iter()
        .filter_map(|(&name, &v)| (v == 0).then_some(name))
        .collect::<VecDeque<&str>>();

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
        let cyclic_models: Vec<&str> = name_to_in_degree
            .iter()
            .filter_map(|(&name, &deg)| (deg > 0).then_some(name))
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
        let mut res = Vec::<String>::default();
        let mut pk_name_lookup = HashMap::<&str, String>::new();

        let models = topo_sort(&self.cidl.models)?;

        for model in models {
            let mut table = Table::create();
            table.table(Alias::new(model.name.clone()));

            let mut pk_name: Option<String> = None;
            for attribute in model.attributes.iter() {
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
                }

                if let Some(_) = &attribute.foreign_key {
                    match attribute.value.cidl_type {
                        CidlType::Integer | CidlType::Model(_) => {
                            // Allowed types for a foreign key
                        }
                        _ => {
                            return Err(anyhow!(
                                "A foreign key must be either a Model or Integer type ({}.{})",
                                model.name,
                                attribute.value.name
                            ));
                        }
                    }
                }

                let mut column = ColumnDef::new(Alias::new(attribute.value.name.clone()));

                if attribute.primary_key {
                    column.primary_key();
                    pk_name = Some(attribute.value.name.clone());
                } else if !attribute.value.nullable {
                    column.not_null();
                }

                if let Some(fk) = &attribute.foreign_key {
                    match fk {
                        CidlForeignKey::OneToOne(fk_model_name) => {
                            table.foreign_key(
                                ForeignKey::create()
                                    .name(format!("fk_{}_{}", model.name, fk_model_name))
                                    .from(
                                        Alias::new(model.name.clone()),
                                        Alias::new(format!("{}Id", fk_model_name)),
                                    )
                                    .to(
                                        Alias::new(fk_model_name.clone()),
                                        Alias::new(
                                            pk_name_lookup.get(fk_model_name.as_str()).unwrap(),
                                        ), // Dog.id
                                    )
                                    .on_update(sea_query::ForeignKeyAction::Cascade)
                                    .on_delete(sea_query::ForeignKeyAction::Restrict),
                            );
                        }
                        _ => todo!(),
                    }
                }

                match &attribute.value.cidl_type {
                    CidlType::Integer => column.integer(),
                    CidlType::Real => column.decimal(),
                    CidlType::Text => column.text(),
                    CidlType::Blob => column.blob(),
                    other => return Err(anyhow!("Invalid SQLite type {:?}", other)),
                };

                table.col(column);
            }

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
        Attribute, CidlForeignKey, CidlSpec, CidlType, InputLanguage, Model, TypedValue,
        WranglerSpec,
    };

    fn create_cidl(models: Vec<Model>) -> CidlSpec {
        CidlSpec {
            version: "1.0".to_string(),
            project_name: "test".to_string(),
            language: InputLanguage::TypeScript,
            models,
        }
    }

    fn create_wrangler() -> WranglerSpec {
        WranglerSpec {
            d1_databases: vec![],
        }
    }

    fn create_model(name: &str, fk: Vec<&str>) -> Model {
        let mut attributes = fk
            .iter()
            .map(|fk| Attribute {
                value: TypedValue {
                    name: format!("{}Id", fk),
                    cidl_type: CidlType::Integer,
                    nullable: false,
                },
                foreign_key: Some(CidlForeignKey::OneToOne(fk.to_string())),
                primary_key: false,
            })
            .collect::<Vec<Attribute>>();

        attributes.push(Attribute {
            value: TypedValue {
                name: "id".to_string(),
                cidl_type: CidlType::Integer,
                nullable: false,
            },
            foreign_key: None,
            primary_key: true,
        });

        Model {
            name: name.to_string(),
            attributes,
            methods: vec![],
            data_sources: vec![],
            source_path: "".into(),
        }
    }

    fn is_topo_ordered(sorted: &[&Model]) -> bool {
        use std::collections::HashSet;

        let mut visited = HashSet::<String>::new();

        for &model in sorted {
            for attribute in &model.attributes {
                if let Some(fk) = &attribute.foreign_key {
                    if !visited.contains(fk.as_str()) && !attribute.value.nullable {
                        return false;
                    }
                }
            }

            visited.insert(model.name.clone());
        }

        true
    }

    #[test]
    fn test_primary_key_and_value_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![Model {
            source_path: "./models/user.cloesce.ts".into(),
            name: String::from("User"),
            attributes: vec![
                Attribute {
                    value: TypedValue {
                        name: String::from("id"),
                        cidl_type: CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                    foreign_key: None,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("name"),
                        cidl_type: CidlType::Text,
                        nullable: true,
                    },
                    primary_key: false,
                    foreign_key: None,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("age"),
                        cidl_type: CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: false,
                    foreign_key: None,
                },
            ],
            methods: vec![],
            data_sources: vec![],
        }]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        assert!(sql.contains("CREATE TABLE"));
        assert!(sql.contains("\"id\" integer PRIMARY KEY"));
        assert!(sql.contains("\"name\" text"));
        assert!(sql.contains("\"age\" integer NOT NULL"));
    }

    #[test]
    fn test_duplicate_primary_key_error() {
        // Arrange
        let spec = create_cidl(vec![Model {
            source_path: "./models/user.cloesce.ts".into(),
            name: String::from("User"),
            attributes: vec![
                Attribute {
                    value: TypedValue {
                        name: String::from("id"),
                        cidl_type: CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                    foreign_key: None,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("user_id"),
                        cidl_type: CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                    foreign_key: None,
                },
            ],
            methods: vec![],
            data_sources: vec![],
        }]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        assert!(err.to_string().contains("Duplicate primary keys"));
    }

    #[test]
    fn test_nullable_primary_key_error() {
        // Arrange
        let spec = create_cidl(vec![Model {
            source_path: "./models/user.cloesce.ts".into(),
            name: String::from("User"),
            attributes: vec![Attribute {
                value: TypedValue {
                    name: String::from("id"),
                    cidl_type: CidlType::Integer,
                    nullable: true,
                },
                primary_key: true,
                foreign_key: None,
            }],
            methods: vec![],
            data_sources: vec![],
        }]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        assert!(
            err.to_string()
                .contains("A primary key cannot be nullable.")
        );
    }

    #[test]
    fn test_missing_primary_key_error() {
        // Arrange
        let spec = create_cidl(vec![Model {
            source_path: "./models/user.cloesce.ts".into(),
            name: String::from("User"),
            attributes: vec![Attribute {
                value: TypedValue {
                    name: String::from("id"),
                    cidl_type: CidlType::Integer,
                    nullable: true,
                },
                primary_key: false,
                foreign_key: None,
            }],
            methods: vec![],
            data_sources: vec![],
        }]);

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        assert!(err.to_string().contains("User is missing a primary key."));
    }

    #[test]
    fn test_topo_sort_yields_correct_order() {
        // Arrange
        // note: this test is kind of ridiculous
        let creators: Vec<Box<dyn Fn() -> Model>> = vec![
            Box::new(|| create_model("Treat", vec![])),
            Box::new(|| create_model("Food", vec![])),
            Box::new(|| create_model("Dog", vec!["Treat", "Food"])),
            Box::new(|| create_model("Independent", vec![])),
            Box::new(|| create_model("Person", vec!["Dog"])),
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
        let models = vec![create_model("User", vec![]), create_model("User", vec![])];

        // Act
        let err = topo_sort(&models).unwrap_err();

        // Assert
        assert!(err.to_string().contains("Duplicate model name"));
    }

    #[test]
    fn test_unknown_foreign_key_error() {
        // Arrange
        let models = vec![create_model("User", vec!["NonExistent"])];

        // Act
        let err = topo_sort(&models).unwrap_err();

        // Assert
        assert!(err.to_string().contains("Unknown foreign key"));
    }

    #[test]
    fn test_cycle_detection_error() {
        // Arrange
        // A -> B -> C -> A
        let models = vec![
            create_model("A", vec!["B"]),
            create_model("B", vec!["C"]),
            create_model("C", vec!["A"]),
        ];

        // Act
        let err = topo_sort(&models).unwrap_err();

        // Assert
        assert!(err.to_string().contains("Cycle detected"));
    }

    #[test]
    fn test_nullability_prevents_cycle_error() {
        // Arrange
        // A -> B -> C -> Nullable<A>
        let mut models = vec![
            create_model("A", vec!["B"]),
            create_model("B", vec!["C"]),
            create_model("C", vec!["A"]),
        ];
        models[2].attributes[0].value.nullable = true;

        // Act
        let sorted = topo_sort(&models);

        // Assert
        assert!(is_topo_ordered(&sorted.unwrap()));
    }

    #[test]
    fn test_one_to_one_fk_yields_sqlite() {
        // Arrange
        let spec = create_cidl(vec![
            create_model("User", vec!["Dog"]),
            create_model("Dog", vec![]),
        ]);
        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let sql = d1gen.sqlite().expect("gen_sqlite to work");

        // Assert
        assert!(sql.contains(
            r#"FOREIGN KEY ("DogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        ));
    }
}
