#[cfg(test)]
mod tests {
    use crate::{TsWorkersApiBuilder, WorkersApiBuilder, WorkersGenerator};

    use common::WranglerSpec;
    use common::{
        Attribute, CidlSpec, CidlType, D1Database, HttpVerb, InputLanguage, Method, Model,
        TypedValue,
    };

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
                        is_static: false,         // Instance method
                        http_verb: HttpVerb::GET, // Fixed: was GET, now Get
                        parameters: vec![TypedValue {
                            name: "message".to_string(),
                            cidl_type: CidlType::Text,
                            nullable: false,
                        }],
                    },
                    Method {
                        name: "getAverageAge".to_string(),
                        is_static: true,          // Static method
                        http_verb: HttpVerb::GET, // Fixed: was GET, now Get
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

        let mut builder = TsWorkersApiBuilder::new();
        let output = builder
            .header(&cidl.version, &cidl.project_name)
            .imports(&cidl.models)
            .build()
            .unwrap();

        // Check that imports are generated
        assert!(output.contains("import { Person } from './models'"));
    }

    #[test]
    fn test_generator_creates_valid_output() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let result = WorkersGenerator::generate(&cidl, &wrangler);

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

        let output = WorkersGenerator::generate(&cidl, &wrangler).unwrap();

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

        let output = WorkersGenerator::generate(&cidl, &wrangler).unwrap();

        // Check for validation code
        assert!(output.contains("Required parameter missing"));
        assert!(output.contains("must be a string"));
    }

    #[test]
    fn test_http_verb_validation() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let output = WorkersGenerator::generate(&cidl, &wrangler).unwrap();

        // Check for HTTP method validation
        assert!(output.contains("Method Not Allowed"));
        assert!(output.contains("request.method !== \"GET\""));
    }

    #[test]
    fn test_primary_key_field_used_in_query() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        let output = WorkersGenerator::generate(&cidl, &wrangler).unwrap();

        println!("{}", output);

        // Check that the primary key field is used in the SQL query
        assert!(output.contains("WHERE id = ?"));
    }

    #[test]
    fn test_fluent_builder_pattern() {
        let cidl = create_test_cidl();
        let wrangler = create_test_wrangler();

        // Test manual fluent API usage
        let all_methods: Vec<Method> = cidl
            .models
            .iter()
            .flat_map(|model| model.methods.clone())
            .collect();

        let all_verbs: Vec<HttpVerb> = all_methods
            .iter()
            .map(|method| method.http_verb.clone())
            .collect();

        let mut builder = TsWorkersApiBuilder::new();
        let output = builder
            .header(&cidl.version, &cidl.project_name)
            .imports(&cidl.models)
            .parameter_validation(&all_methods)
            .http_verb_validation(&all_verbs)
            .method_handlers(&cidl.models)
            .router_trie(&cidl.models)
            .route_matcher()
            .fetch_handler()
            .build()
            .unwrap();

        // Verify all sections are present
        assert!(output.contains("// Generated Cloudflare Workers API"));
        assert!(output.contains("import { Person }"));
        assert!(output.contains("PARAMETER VALIDATION FUNCTIONS"));
        assert!(output.contains("HTTP VERB VALIDATION FUNCTIONS"));
        assert!(output.contains("METHOD HANDLERS"));
        assert!(output.contains("ROUTER STRUCTURE"));
        assert!(output.contains("ROUTE MATCHING LOGIC"));
        assert!(output.contains("WORKER ENTRY POINT"));
    }

    #[test]
    fn test_partial_generation() {
        let cidl = create_test_cidl();

        let mut builder = TsWorkersApiBuilder::new();
        let output = builder
            .imports(&cidl.models)
            .router_trie(&cidl.models)
            .build()
            .unwrap();

        // Should contain imports and router but not other sections
        assert!(output.contains("import { Person }"));
        assert!(output.contains("const router"));
        assert!(!output.contains("PARAMETER VALIDATION"));
        assert!(!output.contains("export default"));
    }

    #[test]
    fn test_empty_models_handling() {
        let empty_cidl = CidlSpec {
            version: "1.0".to_string(),
            project_name: "empty_project".to_string(),
            language: InputLanguage::TypeScript,
            models: vec![],
        };

        let mut builder = TsWorkersApiBuilder::new();
        let output = builder
            .header(&empty_cidl.version, &empty_cidl.project_name)
            .imports(&empty_cidl.models)
            .router_trie(&empty_cidl.models)
            .build()
            .unwrap();

        // Should handle empty models gracefully
        assert!(output.contains("Generated Cloudflare Workers API"));
        assert!(output.contains("import {  } from './models'"));
        assert!(output.contains("api: {}"));
    }
}
