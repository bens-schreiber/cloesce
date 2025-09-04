use common::{CidlSpec, D1Database, WranglerSpec};

use anyhow::{Result, anyhow};
use sea_query::{Alias, ColumnDef, SqliteQueryBuilder, Table};

pub struct D1Generator {
    cidl: CidlSpec,
    wrangler: WranglerSpec,
}

impl D1Generator {
    pub fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self {
        Self { cidl, wrangler }
    }

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

    pub fn sqlite(&self) -> Result<String> {
        let mut res = Vec::<String>::default();

        for model in self.cidl.models.iter() {
            /*
                Note: SeaQuery will make sure that table or column names are quoted, so even
                reserved SQL keywords are fine.
            */
            let mut table = Table::create();
            table.table(Alias::new(model.name.clone()));

            let mut pk_name: Option<String> = None;
            for attribute in model.attributes.iter() {
                if let Some(pk_name) = &pk_name
                    && attribute.primary_key
                {
                    return Err(anyhow!(
                        "Duplicate primary keys {} {}",
                        pk_name,
                        attribute.value.name
                    ));
                }

                if attribute.primary_key && attribute.value.nullable {
                    return Err(anyhow!("A primary key cannot be nullable."));
                }

                let mut column = ColumnDef::new(Alias::new(attribute.value.name.clone()));

                if attribute.primary_key {
                    column.primary_key();
                    pk_name = Some(attribute.value.name.clone());
                } else if !attribute.value.nullable {
                    column.not_null();
                }

                match attribute.value.cidl_type {
                    common::CidlType::Integer => column.integer(),
                    common::CidlType::Real => column.decimal(),
                    common::CidlType::Text => column.text(),
                    common::CidlType::Blob => column.blob(),
                    // TODO: default => return Err(anyhow!("Invalid SQLite type {:?}", default)),
                };

                table.col(column);
            }

            res.push(table.to_string(SqliteQueryBuilder));
        }

        Ok(res.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use common::{Attribute, CidlSpec, InputLanguage, Model, TypedValue, WranglerSpec};

    use crate::D1Generator;

    fn create_cidl(model: Model) -> CidlSpec {
        CidlSpec {
            version: "1.0".to_string(),
            project_name: "test".to_string(),
            language: InputLanguage::TypeScript,
            models: vec![model],
        }
    }

    fn create_wrangler() -> WranglerSpec {
        WranglerSpec {
            d1_databases: vec![],
        }
    }

    #[test]
    fn test_primary_key_and_value_yields_sqlite() {
        // Arrange
        let spec = create_cidl(Model {
            name: String::from("User"),
            attributes: vec![
                Attribute {
                    value: TypedValue {
                        name: String::from("id"),
                        cidl_type: common::CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("name"),
                        cidl_type: common::CidlType::Text,
                        nullable: true,
                    },
                    primary_key: false,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("age"),
                        cidl_type: common::CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: false,
                },
            ],
            methods: vec![],
        });

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
        let spec = create_cidl(Model {
            name: String::from("User"),
            attributes: vec![
                Attribute {
                    value: TypedValue {
                        name: String::from("id"),
                        cidl_type: common::CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                },
                Attribute {
                    value: TypedValue {
                        name: String::from("user_id"),
                        cidl_type: common::CidlType::Integer,
                        nullable: false,
                    },
                    primary_key: true,
                },
            ],
            methods: vec![],
        });

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        assert!(err.to_string().contains("Duplicate primary keys"));
    }

    #[test]
    fn test_nullable_primary_key_error() {
        // Arrange
        let spec = create_cidl(Model {
            name: String::from("User"),
            attributes: vec![Attribute {
                value: TypedValue {
                    name: String::from("id"),
                    cidl_type: common::CidlType::Integer,
                    nullable: true,
                },
                primary_key: true,
            }],
            methods: vec![],
        });

        let d1gen = D1Generator::new(spec, create_wrangler());

        // Act
        let err = d1gen.sqlite().unwrap_err();

        // Assert
        assert!(
            err.to_string()
                .contains("A primary key cannot be nullable.")
        );
    }
}
