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
