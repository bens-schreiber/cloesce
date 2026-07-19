//! Render a [SavePlan] / [SelectPlan] as a human-readable set of stages and steps
//! in a SQL `EXPLAIN`-style tree.

use idl::IncludeTree;

use crate::query::save::plan::SavePlan;
use crate::query::select::plan::SelectPlan;
use crate::query::select::planner::SelectOperation;

/// Render a [SavePlan] as an `EXPLAIN`-style tree.
pub fn explain_save(model: &str, tree: &IncludeTree, plan: &SavePlan) -> String {
    render::explain(
        format!("SAVE PLAN `{model}`"),
        tree,
        plan.stages
            .iter()
            .map(|stage| stage.steps.iter().map(save::step).collect())
            .collect(),
    )
}

/// Render a [SelectPlan] as an `EXPLAIN`-style tree.
pub fn explain_select(
    op: SelectOperation,
    model: &str,
    tree: &IncludeTree,
    plan: &SelectPlan,
) -> String {
    let kind = match op {
        SelectOperation::Get => "GET",
        SelectOperation::List => "LIST",
    };
    render::explain(
        format!("SELECT PLAN ({kind}) `{model}`"),
        tree,
        plan.stages
            .iter()
            .map(|stage| {
                stage
                    .steps
                    .iter()
                    .map(|step| select::step(step, &plan.tables))
                    .collect()
            })
            .collect(),
    )
}

mod render {
    use std::fmt::Write;

    use idl::IncludeTree;

    pub struct Node {
        pub text: String,
        pub children: Vec<Node>,
    }

    impl Node {
        pub fn leaf(text: String) -> Self {
            Self {
                text,
                children: vec![],
            }
        }
    }

    pub fn explain(header: String, tree: &IncludeTree, stages: Vec<Vec<Node>>) -> String {
        fn count(n: usize, noun: &str) -> String {
            format!("{n} {noun}{}", if n == 1 { "" } else { "s" })
        }

        let steps: usize = stages.iter().map(Vec::len).sum();
        let mut out = format!(
            "{header} · {} · {}\n",
            count(stages.len(), "stage"),
            count(steps, "step")
        );

        if !tree.0.is_empty() {
            out.push_str("INCLUDE\n");
            render(&mut out, &include_nodes(tree), "");
        }
        for (i, steps) in stages.iter().enumerate() {
            let _ = writeln!(out, "\nSTAGE {i}");
            render(&mut out, steps, "");
        }
        out
    }

    /// Renders a tree of [Node] into `out`, with each child indented and prefixed
    /// with a branch glyph.
    fn render(out: &mut String, nodes: &[Node], prefix: &str) {
        let last = nodes.len().saturating_sub(1);
        for (i, node) in nodes.iter().enumerate() {
            let (glyph, cont) = if i == last {
                ("└─ ", "   ")
            } else {
                ("├─ ", "│  ")
            };

            let mut lines = node.text.lines();
            let _ = writeln!(out, "{prefix}{glyph}{}", lines.next().unwrap_or(""));
            for line in lines {
                let _ = writeln!(out, "{prefix}{cont}   {line}");
            }
            render(out, &node.children, &format!("{prefix}{cont}"));
        }
    }

    /// Converts an [IncludeTree] into a tree of [Node]
    fn include_nodes(tree: &IncludeTree) -> Vec<Node> {
        tree.0
            .iter()
            .map(|(field, sub)| Node {
                text: format!("`{field}`"),
                children: include_nodes(sub),
            })
            .collect()
    }
}

mod save {
    use std::fmt::Write;

    use super::render::Node;
    use super::{fmt, sql};
    use crate::query::save::plan::{
        PathSegment, SaveArg, SaveQuery, SaveStep, SqlStatement, TMP_TABLE,
    };
    use crate::query::select::plan::MapCardinality;

    /// Converts a [SaveStep] into a [Node]
    pub fn step(step: &SaveStep) -> Node {
        match &step.query {
            SaveQuery::SqlBatch {
                database: db,
                statements,
                shard,
            } => {
                // tmp DELETE and tmp-capture INSERTs are filtered out
                let children = statements
                    .iter()
                    .filter(
                        |stmt| !matches!(stmt, SqlStatement::Write { sql, .. } if writes_tmp(sql)),
                    )
                    .map(|stmt| {
                        Node::leaf(match stmt {
                            SqlStatement::Write { sql, arguments } => {
                                summarize_write(sql, arguments)
                            }
                            SqlStatement::Hydrate { sql, result, .. } => {
                                format!(
                                    "READBACK `{}` INTO `{}`",
                                    sql::select_model(sql),
                                    path(result)
                                )
                            }
                        })
                    })
                    .collect();

                Node {
                    text: format!(
                        "BATCH ON {}{}",
                        fmt::database(db),
                        fmt::shard_clause(shard, arg)
                    ),

                    children,
                }
            }
            SaveQuery::KeyWrite {
                database: db,
                segments,
                value,
                shard,
                ..
            } => Node {
                text: format!(
                    "WRITE {} KEY {} INTO `{}`{}",
                    fmt::database(db),
                    fmt::key_template(segments, |a| match a {
                        SaveArg::Payload(v) => fmt::truncate(&v.to_string()),
                        SaveArg::Result(p) => format!("saved.{}", path(p)),
                    }),
                    path(&step.result),
                    fmt::shard_clause(shard, arg)
                ),
                children: vec![Node::leaf(format!(
                    "VALUE {}",
                    fmt::truncate(&value.to_string())
                ))],
            },
            SaveQuery::Synthesize {
                fields,
                create,
                cardinality,
            } => Node {
                text: format!(
                    "SYNTHESIZE {} INTO `{}`{}",
                    if *create { "CREATE" } else { "MERGE" },
                    path(&step.result),
                    match cardinality {
                        MapCardinality::One => " ONE",
                        MapCardinality::Many => " MANY",
                    }
                ),
                children: fmt::synth_fields(fields, arg),
            },
        }
    }

    fn arg(arg: &SaveArg) -> String {
        match arg {
            SaveArg::Payload(v) => fmt::truncate(&v.to_string()),
            SaveArg::Result(p) => format!("`saved.{}`", path(p)),
        }
    }

    /// A save body path as a dot-ref, e.g. `dogs[0].id`
    ///
    /// `result` when empty.
    fn path<'src>(segments: &[PathSegment<'src>]) -> String {
        match segments {
            [] => "result".into(),
            [PathSegment::Field(f)] => f.to_string(),
            _ => {
                let mut out = String::new();
                for seg in segments {
                    match seg {
                        PathSegment::Field(f) => {
                            if !out.is_empty() {
                                out.push('.');
                            }
                            out.push_str(f);
                        }
                        PathSegment::Index(i) => {
                            let _ = write!(out, "[{i}]");
                        }
                    }
                }
                out
            }
        }
    }

    fn writes_tmp(sql: &str) -> bool {
        let sql = sql.trim_start();
        sql.starts_with(&format!("INSERT OR REPLACE INTO \"{TMP_TABLE}\""))
            || sql.starts_with(&format!("DELETE FROM \"{TMP_TABLE}\""))
    }

    fn summarize_write(sql: &str, arguments: &[SaveArg]) -> String {
        let trimmed = sql.trim_start();
        match trimmed.strip_prefix("INSERT INTO ") {
            Some(rest) => format!(
                "INSERT `{}`{}",
                sql::leading_ident(rest),
                insert_pairs(rest, arguments)
            ),
            None => fmt::truncate(trimmed),
        }
    }

    fn insert_pairs(rest: &str, arguments: &[SaveArg]) -> String {
        if rest.contains("DEFAULT VALUES") {
            // short-circuit since its clean enough
            return " DEFAULT VALUES".to_string();
        }
        let Some(cols) = sql::paren_body(rest) else {
            return String::new();
        };

        let after = &rest[rest.find(") VALUES").map(|i| i + 2).unwrap_or(0)..];
        let vals = sql::paren_body(after)
            .map(sql::split_top)
            .unwrap_or_default();

        let rendered = cols
            .split(',')
            .map(|col| col.trim().trim_matches('"'))
            .zip(vals)
            .map(|(col, val)| format!("`{col}` = {}", resolve_value(val, arguments)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(" ({rendered})")
    }

    /// Resolve a ?N placeholder to a dot-ref
    fn resolve_value(val: &str, arguments: &[SaveArg]) -> String {
        if let Some(n) = val.strip_prefix('?').and_then(|d| d.parse::<usize>().ok())
            && let Some(a) = arguments.get(n - 1)
        {
            return arg(a);
        }
        if let Some(reference) = tmp_lookup_ref(val) {
            return reference;
        }
        fmt::truncate(val)
    }

    fn tmp_lookup_ref(val: &str) -> Option<String> {
        if !val.contains(TMP_TABLE) {
            return None;
        }
        let col = val.split("'$.").nth(1)?.split('\'').next()?;
        let raw_path = val.rsplit("\"path\" = '").next()?.split('\'').next()?;

        // The tmp path is dotted (`dogs.0`); the display dot-ref indexes arrays (`dogs[0]`).
        let dotted = raw_path
            .split('.')
            .map(|seg| match seg.parse::<usize>() {
                Ok(i) => format!("[{i}]"),
                Err(_) => format!(".{seg}"),
            })
            .collect::<String>();
        let dotted = dotted.strip_prefix('.').unwrap_or(&dotted);
        if dotted.is_empty() {
            Some(format!("`saved.{col}`"))
        } else {
            Some(format!("`saved.{dotted}.{col}`"))
        }
    }
}

mod select {
    use std::fmt::Write;

    use super::render::Node;
    use super::{fmt, sql};
    use crate::query::select::plan::{
        JoinKeys, MapCardinality, Select, SelectArg, SelectStep, SqlSegment, TableDef,
    };

    pub fn step(step: &SelectStep, tables: &[TableDef]) -> Node {
        let arg = |a: &SelectArg| arg_str(a, tables);
        let path = table_path(tables, step.table);
        match &step.query {
            Select::Sql {
                database: db,
                sql,
                mapping,
                shard,
                route_fields,
                ..
            } => {
                let query = render_sql(sql);

                // SEARCH when there is a predicate, SCAN for an unfiltered read
                let verb = if query.contains(" WHERE ") {
                    "SEARCH"
                } else {
                    "SCAN"
                };

                let mut head = format!(
                    "{verb} `{}` ON {}",
                    sql::select_model(&query),
                    fmt::database(db)
                );
                if !path.is_empty() {
                    let _ = write!(head, " INTO `{}`", str_path(&path));
                }
                if query.contains(" LIMIT ") {
                    head.push_str(" LIMIT `$limit`");
                }
                head.push_str(cardinality(mapping.cardinality));

                if !mapping.join.is_empty() {
                    let _ = write!(head, "\n{}", join_clause(&mapping.join));
                }
                if !shard.is_empty() {
                    let _ = write!(head, "\n{}", fmt::shard_clause(shard, &arg).trim_start());
                }
                if !route_fields.is_empty() {
                    let _ = write!(head, "\n{}", attach_clause(route_fields, &arg));
                }
                Node::leaf(head)
            }
            Select::Key {
                database: db,
                segments: key,
                shard,
            } => Node::leaf(format!(
                "READ {} KEY {} INTO `{}`{}",
                fmt::database(db),
                fmt::key_template(key, &arg),
                str_path(&path),
                fmt::shard_clause(shard, &arg)
            )),
            Select::Synthesize { fields, .. } => Node {
                text: format!("SYNTHESIZE INTO `{}`", str_path(&path)),
                children: fmt::synth_fields(fields, &arg),
            },
        }
    }

    fn arg_str(arg: &SelectArg, tables: &[TableDef]) -> String {
        match arg {
            SelectArg::Param(p) => format!("`${p}`"),
            SelectArg::Field { table, field } => {
                let mut path = table_path(tables, *table);
                path.push(field);
                path.join(".")
            }
        }
    }

    /// Render a step's SQL segments to a display string, with each [SqlSegment::Bind]
    /// shown as its 1-based `?N` placeholder.
    fn render_sql(segments: &[SqlSegment]) -> String {
        segments
            .iter()
            .map(|seg| match seg {
                SqlSegment::Literal(text) => text.clone(),
                SqlSegment::Bind(i) => format!("?{}", i + 1),
            })
            .collect()
    }

    /// The dotted result path of a table, climbing parent links from the root.
    fn table_path<'a>(tables: &[TableDef<'a>], id: usize) -> Vec<&'a str> {
        match &tables[id].parent {
            None => vec![],
            Some(parent) => {
                let mut path = table_path(tables, parent.table);
                path.push(parent.field);
                path
            }
        }
    }

    fn str_path(segments: &[&str]) -> String {
        match segments {
            [] => "result".into(),
            [only] => only.to_string(),
            _ => segments.join("."),
        }
    }

    fn cardinality(c: MapCardinality) -> &'static str {
        match c {
            MapCardinality::One => " ONE",
            MapCardinality::Many => " MANY",
        }
    }

    /// `ATTACH `<field>` = <arg>, ...` for each route field riding onto the rows.
    fn attach_clause(fields: &[(&str, SelectArg)], arg: impl Fn(&SelectArg) -> String) -> String {
        let body = fields
            .iter()
            .map(|(f, a)| format!("`{f}` = {}", arg(a)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("ATTACH {body}")
    }

    fn join_clause(keys: &[JoinKeys]) -> String {
        let body = keys
            .iter()
            .map(|k| format!("`parent.{}` = `row.{}`", k.parent_key, k.child_key))
            .collect::<Vec<_>>()
            .join(" AND ");
        format!("JOIN {body}")
    }
}

mod fmt {
    use std::fmt::Write;

    use super::render::Node;
    use crate::query::{Database, DatabaseKind, TemplateSegment};

    const MAX_LITERAL: usize = 40;

    pub fn database(db: &Database) -> String {
        let kind = match db.kind {
            DatabaseKind::D1 => "d1",
            DatabaseKind::DurableObject => "durable",
            DatabaseKind::Kv => "kv",
            DatabaseKind::R2 => "r2",
        };
        format!("{kind} `{}`", db.name)
    }

    /// Render a KV/R2 key template as a quoted literal whose interpolations
    /// hold the dot-refs, e.g. `"users/{user_id}"` -> `"users/{parent.userId}"`.
    pub fn key_template<V>(key: &[TemplateSegment<V>], value: impl Fn(&V) -> String) -> String {
        let mut out = String::from("\"");
        for seg in key {
            match seg {
                TemplateSegment::Literal(s) => out.push_str(s),
                TemplateSegment::Value(v) => {
                    let _ = write!(out, "{{{}}}", value(v));
                }
            }
        }
        out.push('"');
        out
    }

    pub fn synth_fields<A>(fields: &[(&str, A)], arg: impl Fn(&A) -> String) -> Vec<Node> {
        fields
            .iter()
            .map(|(field, a)| Node::leaf(format!("`{field}` = {}", arg(a))))
            .collect()
    }

    /// ` SHARD `<field>` = <arg>` for each shard pair.
    pub fn shard_clause<A>(shard: &[(&str, A)], arg: impl Fn(&A) -> String) -> String {
        shard
            .iter()
            .map(|(field, a)| format!(" SHARD `{field}` = {}", arg(a)))
            .collect()
    }

    /// Collapse a literal past [MAX_LITERAL]
    pub fn truncate(s: &str) -> String {
        if s.chars().count() > MAX_LITERAL {
            let head: String = s.chars().take(MAX_LITERAL).collect();
            format!("{head}...")
        } else {
            s.into()
        }
    }
}

mod sql {
    /// The model name from a `SELECT ... FROM "Model" ...` statement.
    pub fn select_model(sql: &str) -> &str {
        sql.split(" FROM ").nth(1).map(leading_ident).unwrap_or("")
    }

    /// The identifier immediately after the keyword, stripped of its quotes.
    pub fn leading_ident(rest: &str) -> &str {
        rest.trim_start()
            .trim_start_matches('"')
            .split('"')
            .next()
            .unwrap_or("")
    }

    /// The comma-separated contents of the first top-level `(...)` group in `s`.
    pub fn paren_body(s: &str) -> Option<&str> {
        let open = s.find('(')?;
        let mut depth = 0;
        for (i, c) in s[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&s[open + 1..open + i]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Split a comma list at the top nesting level, ignoring commas inside `(...)`.
    pub fn split_top(body: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut depth = 0;
        let mut start = 0;
        for (i, c) in body.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push(body[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        let tail = body[start..].trim();
        if !tail.is_empty() {
            out.push(tail);
        }
        out
    }
}
