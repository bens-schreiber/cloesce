use ast::{CidlType, HttpVerb};

use sqlformat::{FormatOptions, QueryParams};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, DataSourceBlock, EnvBlock,
    EnvBlockKind, ForeignBlock, ForeignQualifier, InjectBlock, KvBlock, ModelBlock, ModelBlockKind,
    NavigationBlock, PaginatedBlockKind, ParseAst, ParsedIncludeTree, PlainOldObjectBlock, R2Block,
    ServiceBlock, SqlBlockKind, Symbol, UseTag, UseTagParamKind, lexer::CommentMap,
};

/// Format a `ParseAst` back into a canonical Cloesce source string.
pub fn format(ast: &ParseAst<'_>, comment_map: &CommentMap<'_>) -> String {
    let mut f = Formatter::new(comment_map);
    f.format_ast(ast);
    f.finish()
}

struct Formatter<'a> {
    comment_map: &'a CommentMap<'a>,
    out: String,

    /// Byte offset of the end of the last AST node emitted
    cursor: usize,
}

impl<'a> Formatter<'a> {
    fn new(comment_map: &'a CommentMap<'a>) -> Self {
        Self {
            comment_map,
            out: String::new(),
            cursor: 0,
        }
    }

    fn finish(self) -> String {
        self.out
    }

    fn push(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn newline(&mut self) {
        self.out.push('\n');
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
    }

    /// Emit any comments that fall between `self.cursor` and `node_start`.
    fn emit_comments_before(&mut self, node_start: usize, indent_depth: usize) {
        let prev_end = self.cursor;
        let comments: Vec<(usize, &str)> = self
            .comment_map
            .between(prev_end, node_start)
            .iter()
            .copied()
            .collect();
        for (_, text) in comments {
            self.indent(indent_depth);
            self.push(text);
            self.newline();
        }
    }

    /// Advance the source cursor past a node that ends at `end`.
    fn advance(&mut self, end: usize) {
        if end > self.cursor {
            self.cursor = end;
        }
    }

    fn format_ast(&mut self, ast: &ParseAst<'_>) {
        let mut first = true;
        for block in &ast.blocks {
            let start = block_start(block);
            self.emit_comments_before(start, 0);
            self.advance(start);
            if !first {
                // blank line between top level blocks
                self.newline();
            }
            first = false;
            self.format_block(block);
            self.advance(block_end(block));
        }

        // trailing comments after the last block
        let eof = usize::MAX;
        self.emit_comments_before(eof, 0);
    }

    fn format_block(&mut self, block: &AstBlockKind<'_>) {
        match block {
            AstBlockKind::Model(b) => self.format_model(b),
            AstBlockKind::Api(b) => self.format_api(b),
            AstBlockKind::DataSource(b) => self.format_data_source(b),
            AstBlockKind::Service(b) => self.format_service(b),
            AstBlockKind::PlainOldObject(b) => self.format_poo(b),
            AstBlockKind::Env(b) => self.format_env(b),
            AstBlockKind::Inject(b) => self.format_inject(b),
        }
    }

    fn format_model(&mut self, b: &ModelBlock<'_>) {
        for tag in &b.use_tags {
            self.emit_comments_before(tag.span.start, 0);
            self.format_use_tag(tag);
            self.newline();
            self.advance(tag.span.end);
        }
        self.push("model ");
        self.push(b.symbol.name);

        if b.blocks.is_empty() {
            // Compact form for empty model blocks
            self.push(" {}");
            self.newline();
            return;
        }

        self.push(" {");
        self.newline();

        for item in &b.blocks {
            let start = model_block_kind_start(item);
            self.emit_comments_before(start, 1);
            self.indent(1);
            self.format_model_block_kind(item);
            self.newline();
            self.advance(model_block_kind_end(item));
        }

        self.push("}");
        self.newline();
    }

    fn format_use_tag(&mut self, tag: &UseTag<'_>) {
        self.push("[use ");
        let params: Vec<String> = tag
            .params
            .iter()
            .map(|p| match p {
                UseTagParamKind::Crud(k) => format_crud(k),
                UseTagParamKind::EnvBinding(b) => b.name.to_string(),
            })
            .collect();
        self.push(&params.join(", "));
        self.push("]");
    }

    fn format_model_block_kind(&mut self, item: &ModelBlockKind<'_>) {
        match item {
            ModelBlockKind::Column(sym) => self.format_typed_field(sym),
            ModelBlockKind::Foreign(fb) => self.format_foreign(fb),
            ModelBlockKind::Navigation(nb) => self.format_navigation(nb),
            ModelBlockKind::Kv(kv) => self.format_kv(kv),
            ModelBlockKind::R2(r2) => self.format_r2(r2),
            ModelBlockKind::Primary { blocks, .. } => {
                if blocks.is_empty() {
                    self.push("primary {}");
                } else {
                    self.push("primary {");
                    self.newline();
                    self.format_sql_blocks(blocks, 2);
                    self.indent(1);
                    self.push("}");
                }
            }
            ModelBlockKind::Unique { blocks, .. } => {
                if blocks.is_empty() {
                    self.push("unique {}");
                } else {
                    self.push("unique {");
                    self.newline();
                    self.format_sql_blocks(blocks, 2);
                    self.indent(1);
                    self.push("}");
                }
            }
            ModelBlockKind::Optional { blocks, .. } => {
                if blocks.is_empty() {
                    self.push("optional {}");
                } else {
                    self.push("optional {");
                    self.newline();
                    self.format_sql_blocks(blocks, 2);
                    self.indent(1);
                    self.push("}");
                }
            }
            ModelBlockKind::Paginated { blocks, .. } => {
                if blocks.is_empty() {
                    self.push("paginated {}");
                } else {
                    self.push("paginated {");
                    self.newline();
                    for pb in blocks {
                        self.indent(2);
                        match pb {
                            PaginatedBlockKind::R2(r2) => self.format_r2(r2),
                            PaginatedBlockKind::Kv(kv) => self.format_kv(kv),
                        }
                        self.newline();
                    }
                    self.indent(1);
                    self.push("}");
                }
            }
            ModelBlockKind::KeyField { fields, .. } => {
                if fields.is_empty() {
                    self.push("keyfield {}");
                } else {
                    self.push("keyfield {");
                    self.newline();
                    for f in fields {
                        self.indent(2);
                        self.push(f.name);
                        self.newline();
                    }
                    self.indent(1);
                    self.push("}");
                }
            }
        }
    }

    fn format_sql_blocks(&mut self, blocks: &[SqlBlockKind<'_>], depth: usize) {
        for b in blocks {
            self.indent(depth);
            match b {
                SqlBlockKind::Column(sym) => self.format_typed_field(sym),
                SqlBlockKind::Foreign(fb) => self.format_foreign(fb),
            }
            self.newline();
        }
    }

    fn format_foreign(&mut self, fb: &ForeignBlock<'_>) {
        self.push("foreign (");
        let adj: Vec<String> = fb
            .adj
            .iter()
            .map(|(m, f)| format!("{}::{}", m.name, f.name))
            .collect();
        self.push(&adj.join(", "));
        self.push(")");

        if let Some(q) = &fb.qualifier {
            self.push(" ");
            self.push(match q {
                ForeignQualifier::Primary => "primary",
                ForeignQualifier::Optional => "optional",
                ForeignQualifier::Unique => "unique",
            });
        }

        self.push(" {");
        self.newline();
        for field in &fb.fields {
            self.indent(2);
            self.push(field.name);
            self.newline();
        }
        if let Some(nav) = &fb.nav {
            self.indent(2);
            self.push("nav { ");
            self.push(nav.name);
            self.push(" }");
            self.newline();
        }
        self.indent(1);
        self.push("}");
    }

    fn format_navigation(&mut self, nb: &NavigationBlock<'_>) {
        self.push("nav (");
        let adj: Vec<String> = nb
            .adj
            .iter()
            .map(|(m, f)| format!("{}::{}", m.name, f.name))
            .collect();
        self.push(&adj.join(", "));
        self.push(") {");
        self.newline();
        self.indent(2);
        self.push(nb.field.name);
        self.newline();
        self.indent(1);
        self.push("}");
    }

    fn format_kv(&mut self, kv: &KvBlock<'_>) {
        self.push("kv (");
        self.push(kv.env_binding.name);
        self.push(", \"");
        self.push(kv.key_format);
        self.push("\"");
        self.push(")");
        if kv.is_paginated {
            self.push(" paginated");
        }
        self.push(" {");
        self.newline();
        self.indent(2);
        self.format_typed_field(&kv.field);
        self.newline();
        self.indent(1);
        self.push("}");
    }

    fn format_r2(&mut self, r2: &R2Block<'_>) {
        self.push("r2 (");
        self.push(r2.env_binding.name);
        self.push(", \"");
        self.push(r2.key_format);
        self.push("\"");
        self.push(")");
        if r2.is_paginated {
            self.push(" paginated");
        }
        self.push(" {");
        self.newline();
        self.indent(2);
        self.push(r2.field.name);
        self.newline();
        self.indent(1);
        self.push("}");
    }

    fn format_api(&mut self, b: &ApiBlock<'_>) {
        self.push("api ");
        self.push(b.symbol.name);

        if b.methods.is_empty() {
            // Compact form for empty api blocks
            self.push(" {}");
            self.newline();
            return;
        }

        self.push(" {");
        self.newline();

        for method in &b.methods {
            self.emit_comments_before(method.span.start, 1);
            self.advance(method.span.start);
            self.indent(1);
            self.format_api_method(method);
            self.newline();
            self.advance(method.span.end);
        }

        self.push("}");
        self.newline();
    }

    fn format_api_method(&mut self, m: &ApiBlockMethod<'_>) {
        self.push(format_http_verb(m.http_verb));
        self.push(" ");
        self.push(m.symbol.name);
        self.push("(");

        let params: Vec<String> = m
            .parameters
            .iter()
            .map(|p| match p {
                ApiBlockMethodParamKind::SelfParam {
                    symbol: _,
                    data_source,
                } => {
                    if let Some(ds) = data_source {
                        format!("[source {}] self", ds.name)
                    } else {
                        "self".to_string()
                    }
                }
                ApiBlockMethodParamKind::Field(sym) => {
                    format!("{}: {}", sym.name, format_cidl_type(&sym.cidl_type))
                }
            })
            .collect();

        self.push(&params.join(", "));
        self.push(") -> ");
        self.push(&format_cidl_type(&m.return_type));
    }

    /// Format one level of a parsed include tree at `depth`.
    /// Entries whose subtree is empty are leaf nodes — emitted inline (comma-separated).
    /// Entries with children are branch nodes — emitted with a braced block.
    fn format_include_tree(&mut self, tree: &ParsedIncludeTree<'_>, depth: usize) {
        let leaves: Vec<&str> = tree
            .0
            .iter()
            .filter(|(_, v)| v.0.is_empty())
            .map(|(k, _)| k.name)
            .collect();
        let branches: Vec<(&str, &ParsedIncludeTree<'_>)> = tree
            .0
            .iter()
            .filter(|(_, v)| !v.0.is_empty())
            .map(|(k, v)| (k.name, v))
            .collect();

        if !leaves.is_empty() {
            self.indent(depth);
            self.push(&leaves.join(", "));
            self.newline();
        }

        for (name, subtree) in branches {
            self.indent(depth);
            self.push(name);
            self.push(" {");
            self.newline();
            self.format_include_tree(subtree, depth + 1);
            self.indent(depth);
            self.push("}");
            self.newline();
        }
    }

    fn format_data_source(&mut self, b: &DataSourceBlock<'_>) {
        if b.is_internal {
            self.push("[internal]");
            self.newline();
        }

        self.push("source ");
        self.push(b.symbol.name);
        self.push(" for ");
        self.push(b.model.name);
        self.push(" {");
        self.newline();

        // Compact `include {}` when there are no entries; otherwise render
        // the full multi-line include tree.
        if b.tree.0.is_empty() {
            self.indent(1);
            self.push("include {}");
            self.newline();
        } else {
            self.indent(1);
            self.push("include {");
            self.newline();
            self.format_include_tree(&b.tree, 2);
            self.indent(1);
            self.push("}");
            self.newline();
        }

        if let Some(get) = &b.get {
            self.indent(1);
            self.push("sql get(");
            let params: Vec<String> = get
                .parameters
                .iter()
                .map(|p| format!("{}: {}", p.name, format_cidl_type(&p.cidl_type)))
                .collect();
            self.push(&params.join(", "));
            self.push(") {");
            self.newline();
            self.format_sql_string(get.raw_sql, 2);
            self.indent(1);
            self.push("}");
            self.newline();
        }

        if let Some(list) = &b.list {
            self.indent(1);
            self.push("sql list(");
            let params: Vec<String> = list
                .parameters
                .iter()
                .map(|p| format!("{}: {}", p.name, format_cidl_type(&p.cidl_type)))
                .collect();
            self.push(&params.join(", "));
            self.push(") {");
            self.newline();
            self.format_sql_string(list.raw_sql, 2);
            self.indent(1);
            self.push("}");
            self.newline();
        }

        self.push("}");
        self.newline();
    }

    fn format_service(&mut self, b: &ServiceBlock<'_>) {
        self.push("service ");
        self.push(b.symbol.name);

        if b.fields.is_empty() {
            // Compact form for empty service blocks
            self.push(" {}");
            self.newline();
            return;
        }

        self.push(" {");
        self.newline();

        for field in &b.fields {
            self.emit_comments_before(field.span.start, 1);
            self.indent(1);
            self.format_typed_field(field);
            self.newline();
            self.advance(field.span.end);
        }

        self.push("}");
        self.newline();
    }

    fn format_poo(&mut self, b: &PlainOldObjectBlock<'_>) {
        self.push("poo ");
        self.push(b.symbol.name);

        if b.fields.is_empty() {
            // Compact form for empty poo blocks
            self.push(" {}");
            self.newline();
            return;
        }

        self.push(" {");
        self.newline();

        for field in &b.fields {
            self.emit_comments_before(field.span.start, 1);
            self.indent(1);
            self.format_typed_field(field);
            self.newline();
            self.advance(field.span.end);
        }

        self.push("}");
        self.newline();
    }

    fn format_env(&mut self, b: &EnvBlock<'_>) {
        self.push("env");

        if b.blocks.is_empty() {
            // Compact form for completely empty env blocks
            self.push(" {}");
            self.newline();
            return;
        }

        self.push(" {");
        self.newline();

        for sub in &b.blocks {
            let (sub_start, sub_end) = match sub {
                EnvBlockKind::D1 { span, .. }
                | EnvBlockKind::R2 { span, .. }
                | EnvBlockKind::Kv { span, .. }
                | EnvBlockKind::Var { span, .. } => (span.start, span.end),
            };
            self.emit_comments_before(sub_start, 1);
            self.advance(sub_start);
            self.indent(1);
            match sub {
                EnvBlockKind::D1 { symbols, .. } => {
                    self.push("d1 { ");
                    self.push(
                        &symbols
                            .iter()
                            .map(|s| s.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    self.push(" }");
                }
                EnvBlockKind::R2 { symbols, .. } => {
                    self.push("r2 { ");
                    self.push(
                        &symbols
                            .iter()
                            .map(|s| s.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    self.push(" }");
                }
                EnvBlockKind::Kv { symbols, .. } => {
                    self.push("kv { ");
                    self.push(
                        &symbols
                            .iter()
                            .map(|s| s.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    self.push(" }");
                }
                EnvBlockKind::Var { symbols, .. } => {
                    self.push("vars {");
                    self.newline();
                    for sym in symbols {
                        self.emit_comments_before(sym.span.start, 2);
                        self.indent(2);
                        self.format_typed_field(sym);
                        self.newline();
                        self.advance(sym.span.end);
                    }
                    self.indent(1);
                    self.push("}");
                }
            }
            self.newline();
            self.advance(sub_end);
        }

        self.push("}");
        self.newline();
    }

    fn format_inject(&mut self, b: &InjectBlock<'_>) {
        if b.symbols.is_empty() {
            // Compact form for empty inject blocks
            self.push("inject {}");
            self.newline();
            return;
        }

        self.push("inject {");
        self.newline();

        for sym in &b.symbols {
            self.emit_comments_before(sym.span.start, 1);
            self.indent(1);
            self.push(sym.name);
            self.newline();
            self.advance(sym.span.end);
        }

        self.push("}");
        self.newline();
    }

    fn format_typed_field(&mut self, sym: &Symbol<'_>) {
        self.push(sym.name);
        self.push(": ");
        self.push(&format_cidl_type(&sym.cidl_type));
    }

    /// Format a raw SQL string as a quoted, potentially multi-line string
    /// with indentation aligned to the given depth.
    ///
    /// The opening and closing quotation marks are always placed on their
    /// own lines (for non-empty SQL), with the SQL content in between.
    fn format_sql_string(&mut self, raw_sql: &str, indent_depth: usize) {
        let formatted = format_sql(raw_sql);
        let mut lines = formatted.lines();

        match lines.next() {
            Some(first) => {
                // Opening quote on its own line
                self.indent(indent_depth);
                self.push("\"");
                self.newline();

                // First line of SQL content
                self.indent(indent_depth);
                self.push(first);
                self.newline();

                // Subsequent SQL lines, each on their own line
                for line in lines {
                    self.indent(indent_depth);
                    self.push(line);
                    self.newline();
                }

                // Closing quote on its own line
                self.indent(indent_depth);
                self.push("\"");
                self.newline();
            }
            None => {
                // Empty SQL string: keep as a simple "" literal
                self.indent(indent_depth);
                self.push("\"\"");
                self.newline();
            }
        }
    }
}

fn format_cidl_type(ty: &CidlType<'_>) -> String {
    match ty {
        CidlType::Void => "void".into(),
        CidlType::Integer => "int".into(),
        CidlType::Double => "double".into(),
        CidlType::String => "string".into(),
        CidlType::Blob => "blob".into(),
        CidlType::Boolean => "bool".into(),
        CidlType::DateIso => "date".into(),
        CidlType::Stream => "stream".into(),
        CidlType::Json => "json".into(),
        CidlType::R2Object => "R2Object".into(),
        CidlType::Env => "env".into(),
        CidlType::Inject { name } => name.to_string(),
        CidlType::Object { name } => name.to_string(),
        CidlType::UnresolvedReference { name } => name.to_string(),
        CidlType::Partial { object_name } => format!("Partial<{}>", object_name),
        CidlType::DataSource { model_name } => format!("DataSource<{}>", model_name),
        CidlType::Array(inner) => format!("Array<{}>", format_cidl_type(inner)),
        CidlType::HttpResult(inner) => format!("HttpResult<{}>", format_cidl_type(inner)),
        CidlType::Nullable(inner) => format!("Option<{}>", format_cidl_type(inner)),
        CidlType::Paginated(inner) => format!("Paginated<{}>", format_cidl_type(inner)),
        CidlType::KvObject(inner) => format!("KvObject<{}>", format_cidl_type(inner)),
    }
}

fn format_sql(input: &str) -> String {
    let opts = FormatOptions::<'_> {
        lines_between_queries: 2,
        ..FormatOptions::default()
    };

    sqlformat::format(input, &QueryParams::None, &opts)
}

fn format_http_verb(verb: HttpVerb) -> &'static str {
    match verb {
        HttpVerb::Get => "get",
        HttpVerb::Post => "post",
        HttpVerb::Put => "put",
        HttpVerb::Delete => "delete",
        HttpVerb::Patch => "patch",
    }
}

fn format_crud(k: &ast::CrudKind) -> String {
    use ast::CrudKind;
    match k {
        CrudKind::Get => "get".into(),
        CrudKind::List => "list".into(),
        CrudKind::Save => "save".into(),
    }
}

fn block_start(b: &AstBlockKind<'_>) -> usize {
    match b {
        AstBlockKind::Model(b) => b.span.start,
        AstBlockKind::Api(b) => b.span.start,
        AstBlockKind::DataSource(b) => b.span.start,
        AstBlockKind::Service(b) => b.span.start,
        AstBlockKind::PlainOldObject(b) => b.span.start,
        AstBlockKind::Env(b) => b.span.start,
        AstBlockKind::Inject(b) => b.span.start,
    }
}

fn block_end(b: &AstBlockKind<'_>) -> usize {
    match b {
        AstBlockKind::Model(b) => b.span.end,
        AstBlockKind::Api(b) => b.span.end,
        AstBlockKind::DataSource(b) => b.span.end,
        AstBlockKind::Service(b) => b.span.end,
        AstBlockKind::PlainOldObject(b) => b.span.end,
        AstBlockKind::Env(b) => b.span.end,
        AstBlockKind::Inject(b) => b.span.end,
    }
}

fn model_block_kind_start(item: &ModelBlockKind<'_>) -> usize {
    match item {
        ModelBlockKind::Column(s) => s.span.start,
        ModelBlockKind::Foreign(fb) => fb.span.start,
        ModelBlockKind::Navigation(nb) => nb.span.start,
        ModelBlockKind::Kv(kv) => kv.span.start,
        ModelBlockKind::R2(r2) => r2.span.start,
        ModelBlockKind::Primary { span, .. } => span.start,
        ModelBlockKind::KeyField { span, .. } => span.start,
        ModelBlockKind::Unique { span, .. } => span.start,
        ModelBlockKind::Paginated { span, .. } => span.start,
        ModelBlockKind::Optional { span, .. } => span.start,
    }
}

fn model_block_kind_end(item: &ModelBlockKind<'_>) -> usize {
    match item {
        ModelBlockKind::Column(s) => s.span.end,
        ModelBlockKind::Foreign(fb) => fb.span.end,
        ModelBlockKind::Navigation(nb) => nb.span.end,
        ModelBlockKind::Kv(kv) => kv.span.end,
        ModelBlockKind::R2(r2) => r2.span.end,
        ModelBlockKind::Primary { span, .. } => span.end,
        ModelBlockKind::KeyField { span, .. } => span.end,
        ModelBlockKind::Unique { span, .. } => span.end,
        ModelBlockKind::Paginated { span, .. } => span.end,
        ModelBlockKind::Optional { span, .. } => span.end,
    }
}
