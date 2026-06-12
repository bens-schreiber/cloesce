use askama::Template;

use crate::mappers::{LanguageTypeMapper, TypeScriptMapper};
#[derive(Template)]
#[template(path = "durable_migration.ts.jinja", escape = "none")]
struct DurableMigrationTemplate<'src> {
    name: &'src str,
    timestamp: u64,
    sql: String,
}

pub struct DurableMigrationGenerator;
impl DurableMigrationGenerator {
    pub fn generate(name: &str, timestamp: u64, sql: &str) -> String {
        let mapper = TypeScriptMapper::backend();
        let escaped = mapper.escape_string(sql);

        DurableMigrationTemplate {
            name,
            timestamp,
            sql: escaped,
        }
        .render()
        .expect("Failed to render durable migration template")
    }
}

#[cfg(test)]
mod tests {
    use super::DurableMigrationGenerator;

    #[test]
    fn durable_migration_generation_snapshot() {
        const NAME: &str = "create_users_table";
        const TIMESTAMP: u64 = 1_700_000_000;
        const SQL: &str = r#"
            CREATE TABLE User (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE
            );
        "#;

        let migration_code = DurableMigrationGenerator::generate(NAME, TIMESTAMP, SQL);
        insta::assert_snapshot!(migration_code);
    }
}
