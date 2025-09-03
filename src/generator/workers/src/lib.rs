mod builders;
use anyhow::{Result};
use common::{CidlSpec, InputLanguage, WranglerSpec};

use crate::builders::typescript::TsWorkersApiBuilder;

pub trait WorkersApiBuilder {
    fn build(&self) -> Result<String>;
}


pub struct WorkersGenerator {
    cidl: CidlSpec,
    wrangler: WranglerSpec,
}

impl WorkersGenerator {
    pub fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self {
        Self { cidl, wrangler }
    }

    pub fn generate(&self) -> Result<String> {
        match self.cidl.language {
            InputLanguage::TypeScript => {

                let builder = TsWorkersApiBuilder::new(self.cidl.clone(), self.wrangler.clone());
                builder.build()
            } 

        }
    }
}


#[cfg(test)]
mod tests {
    use crate::{TsWorkersApiBuilder, WorkersApiBuilder, WorkersGenerator};

    use common::{Attribute, CidlSpec, CidlType, D1Database, HttpVerb, InputLanguage, Method, Model, TypedValue};
    use common::WranglerSpec;

    /// Helper to create test CIDL spec with a Person model
    fn create_test_cidl() -> CidlSpec {
        CidlSpec {
            version: "1.0".to_string(),
            project_name: "test_project".to_string(),
            language: InputLanguage::TypeScript,
            models: vec![Model {
                name: "Person".to_string(),
                attributes: vec![
                    Attribute {
                        value: TypedValue {
                            name: "id".to_string(),
                            cidl_type: CidlType::Integer,
                            nullable: false,
                        },
                        primary_key: true,
                    },
                    Attribute {
                        value: TypedValue {
                            name: "name".to_string(),
                            cidl_type: CidlType::Text,
                            nullable: false,
                        },
                        primary_key: false,
                    },
                    Attribute {
                        value: TypedValue {
                            name: "age".to_string(),
                            cidl_type: CidlType::Integer,
                            nullable: true,
                        },
                        primary_key: false,
                    },
                ],
                methods: vec![
                    Method {
                        name: "speak".to_string(),
                        is_static: false, // Instance method
                        http_verb: HttpVerb::Get,
                        parameters: vec![TypedValue {
                            name: "message".to_string(),
                            cidl_type: CidlType::Text,
                            nullable: false,
                        }],
                    },
                    Method {
                        name: "getAverageAge".to_string(),
                        is_static: true, // Static method
                        http_verb: HttpVerb::Get,
                        parameters: vec![],
                    },
                ],
            }],
        }
    }

    /// Helper to create test Wrangler spec with D1 database
    fn create_test_wrangler() -> WranglerSpec {
        WranglerSpec {
            d1_databases: vec![D1Database {
                binding: Some("D1_DB".to_string()),
                database_name: Some("test_db".to_string()),
                database_id: Some("test-id-123".to_string()),
            }],
        }
    }

    #[test]
    fn test_imports_are_generated() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let builder = TsWorkersApiBuilder::new(cidl, wrangler);
        let output = builder.build().unwrap();

        // Check that imports are generated
        assert!(output.contains("import { Person } from './models'"));
    }

    #[test]
    fn test_generator_creates_valid_output() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let generator = WorkersGenerator::new(cidl, wrangler);
        let result = generator.generate();

   
        assert!(result.is_ok());
        let output = result.unwrap();
        println!("{}", output);

        // Check for key components
        assert!(output.contains("import"));
        assert!(output.contains("const router"));
        assert!(output.contains("function match"));
        assert!(output.contains("export default"));
    }

    #[test]
    fn test_static_vs_instance_methods() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let builder = TsWorkersApiBuilder::new(cidl, wrangler);
        let output = builder.build().unwrap();

        // Static method should call directly on the class
        assert!(output.contains("Person.getAverageAge"));

        // Instance method should instantiate first
        assert!(output.contains("new Person(record)"));
        assert!(output.contains("instance.speak"));
        assert!(output.contains("<id>"));
    }

    #[test]
    fn test_validation_generation() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let builder = TsWorkersApiBuilder::new(cidl, wrangler);
        let output = builder.build().unwrap();

        // Check for validation code
        assert!(output.contains("Required parameter missing"));
        assert!(output.contains("must be a string"));
    }

    #[test]
    fn test_http_verb_validation() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let builder = TsWorkersApiBuilder::new(cidl, wrangler);
        let output = builder.build().unwrap();

        // Check for HTTP method validation
        assert!(output.contains("Method Not Allowed"));
        assert!(output.contains("request.method !== \"GET\""));
    }

    #[test]
    fn test_primary_key_field_used_in_query() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let builder = TsWorkersApiBuilder::new(cidl, wrangler);
        let output = builder.build().unwrap();

        print!("{}", output);

        // Check that the primary key field is used in the SQL query
        assert!(output.contains("WHERE id = ?"));
    }
}
