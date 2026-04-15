use ast::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, DataSourceBlock, EnvBlock,
    EnvBlockKind, ForeignBlock, ForeignQualifier, InjectBlock, KvBlock, ModelBlock, ModelBlockKind,
    NavigationBlock, PaginatedBlockKind, ParseAst, ParsedIncludeTree, PlainOldObjectBlock, R2Block,
    ServiceBlock, Spd, SqlBlockKind, Symbol, UseTag, UseTagParamKind, lexer::CommentMap,
};

trait Format<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>);
}

pub struct Formatter<'src> {
    comment_map: &'src CommentMap<'src>,
    src: &'src str,
    out: String,

    /// Byte offset of the end of the last AST node emitted.
    cursor: usize,
}

impl<'src> Formatter<'src> {
    pub fn format(ast: &ParseAst<'_>, comment_map: &'src CommentMap<'_>, src: &'src str) -> String {
        let mut f = Self {
            comment_map,
            src,
            out: String::with_capacity(src.len()),
            cursor: 0,
        };
        ast.fmt(&mut f);
        f.out
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

    /// Emit leading comments between `self.cursor` and `node_start.
    fn emit_leading_comments(&mut self, node_start: usize, indent_depth: usize) {
        let comments: Vec<(usize, &str)> =
            self.comment_map.between(self.cursor, node_start).to_vec();
        for (offset, text) in comments {
            // A leading comment either has a newline between the cursor and its
            // start, or it sits at the very beginning of the file (nothing
            // precedes it, so it can't be trailing anything).
            let gap = &self.src[self.cursor..offset];
            let is_leading = gap.is_empty() || gap.contains('\n');
            if is_leading {
                self.indent(indent_depth);
                self.push(text);
                self.newline();
            }
            self.cursor = offset + text.len();
        }
        if node_start > self.cursor {
            self.cursor = node_start;
        }
    }

    /// If there is a comment on the same source line immediately after `after`,
    /// emit it inline (` // ...`) and advance the cursor past it.
    fn emit_trailing_comment(&mut self, after: usize) {
        // Find the first comment whose offset >= after
        let lo = self
            .comment_map
            .entries
            .partition_point(|(off, _)| *off < after);
        if let Some(&(offset, text)) = self.comment_map.entries.get(lo) {
            // Only inline if there's no newline between `after` and the comment
            let gap = self.src.get(after..offset).unwrap_or("");
            if !gap.contains('\n') {
                self.push(" ");
                self.push(text);
                self.cursor = offset + text.len();
            }
        }
    }

    /// Emit leading comments, emit any trailing comment, then emit a newline.
    fn emit_spd<T: Format<'src>>(&mut self, spd: &'src Spd<T>, indent_depth: usize) {
        self.emit_leading_comments(spd.span.start, indent_depth);
        spd.block.fmt(self);
        self.emit_trailing_comment(spd.span.end);
        self.newline();
        if spd.span.end > self.cursor {
            self.cursor = spd.span.end;
        }
    }

    /// emit leading comments, emit any trailing comment, then emit a newline.
    fn emit_sym(
        &mut self,
        sym: &'src Symbol<'src>,
        indent_depth: usize,
        f: impl FnOnce(&mut Self),
    ) {
        self.emit_leading_comments(sym.span.start, indent_depth);
        f(self);
        self.emit_trailing_comment(sym.span.end);
        self.newline();
        if sym.span.end > self.cursor {
            self.cursor = sym.span.end;
        }
    }
}

impl<'src> Format<'src> for ParseAst<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        let mut first = true;
        for spd in &self.blocks {
            f.emit_leading_comments(spd.span.start, 0);
            if !first {
                f.newline();
            }
            first = false;
            spd.block.fmt(f);
            if spd.span.end > f.cursor {
                f.cursor = spd.span.end;
            }
        }
        // trailing comments after the last block
        f.emit_leading_comments(usize::MAX, 0);
    }
}

impl<'src> Format<'src> for AstBlockKind<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        match self {
            AstBlockKind::Model(b) => b.fmt(f),
            AstBlockKind::Api(b) => b.fmt(f),
            AstBlockKind::DataSource(b) => b.fmt(f),
            AstBlockKind::Service(b) => b.fmt(f),
            AstBlockKind::PlainOldObject(b) => b.fmt(f),
            AstBlockKind::Env(blocks) => blocks.as_slice().fmt(f),
            AstBlockKind::Inject(b) => b.fmt(f),
        }
    }
}

impl<'src> Format<'src> for ModelBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        for tag in &self.use_tags {
            f.emit_spd(tag, 0);
        }
        f.push("model ");
        f.push(self.symbol.name);

        if self.blocks.is_empty() {
            f.push(" {}");
            f.newline();
            return;
        }

        f.push(" {");
        f.newline();
        for spd in &self.blocks {
            f.emit_leading_comments(spd.span.start, 1);
            f.indent(1);
            spd.block.fmt(f);
            f.emit_trailing_comment(spd.span.end);
            f.newline();
            if spd.span.end > f.cursor {
                f.cursor = spd.span.end;
            }
        }
        f.push("}");
        f.newline();
    }
}

impl<'src> Format<'src> for UseTag<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("[use ");
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| match p {
                UseTagParamKind::Crud(spd) => spd.block.to_keyword().to_string(),
                UseTagParamKind::EnvBinding(b) => b.name.to_string(),
            })
            .collect();
        f.push(&params.join(", "));
        f.push("]");
    }
}

impl<'src> Format<'src> for ModelBlockKind<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        match self {
            ModelBlockKind::Column(sym) => fmt_typed_field(sym, f),
            ModelBlockKind::Foreign(fb) => fb.fmt(f),
            ModelBlockKind::Navigation(nb) => nb.fmt(f),
            ModelBlockKind::Kv(kv) => kv.fmt(f),
            ModelBlockKind::R2(r2) => r2.fmt(f),
            ModelBlockKind::Primary(blocks) => fmt_sql_block_group("primary", blocks, f),
            ModelBlockKind::Unique(blocks) => fmt_sql_block_group("unique", blocks, f),
            ModelBlockKind::Optional(blocks) => fmt_sql_block_group("optional", blocks, f),
            ModelBlockKind::Paginated(blocks) => {
                if blocks.is_empty() {
                    f.push("paginated {}");
                } else {
                    f.push("paginated {");
                    f.newline();
                    for pb in blocks {
                        f.indent(2);
                        match pb {
                            PaginatedBlockKind::R2(r2) => r2.fmt(f),
                            PaginatedBlockKind::Kv(kv) => kv.fmt(f),
                        }
                        f.newline();
                    }
                    f.indent(1);
                    f.push("}");
                }
            }
            ModelBlockKind::KeyField(fields) => {
                if fields.is_empty() {
                    f.push("keyfield {}");
                } else {
                    f.push("keyfield {");
                    f.newline();
                    for sym in fields {
                        f.indent(2);
                        f.push(sym.name);
                        f.newline();
                    }
                    f.indent(1);
                    f.push("}");
                }
            }
        }
    }
}

impl<'src> Format<'src> for ForeignBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("foreign (");
        f.push(&fmt_adj(&self.adj));
        f.push(")");

        if let Some(q) = &self.qualifier {
            f.push(" ");
            f.push(match q {
                ForeignQualifier::Primary => "primary",
                ForeignQualifier::Optional => "optional",
                ForeignQualifier::Unique => "unique",
            });
        }

        f.push(" {");
        f.newline();
        for field in &self.fields {
            f.indent(2);
            f.push(field.name);
            f.newline();
        }
        if let Some(nav) = &self.nav {
            f.indent(2);
            f.push("nav { ");
            f.push(nav.name);
            f.push(" }");
            f.newline();
        }
        f.indent(1);
        f.push("}");
    }
}

impl<'src> Format<'src> for NavigationBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("nav (");
        f.push(&fmt_adj(&self.adj));
        f.push(") {");
        f.newline();
        f.indent(2);
        f.push(self.symbol.name);
        f.newline();
        f.indent(1);
        f.push("}");
    }
}

impl<'src> Format<'src> for KvBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("kv (");
        f.push(self.env_binding.name);
        f.push(", \"");
        f.push(self.key_format);
        f.push("\")");
        if self.is_paginated {
            f.push(" paginated");
        }
        f.push(" {");
        f.newline();
        f.indent(2);
        fmt_typed_field(&self.field, f);
        f.newline();
        f.indent(1);
        f.push("}");
    }
}

impl<'src> Format<'src> for R2Block<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("r2 (");
        f.push(self.env_binding.name);
        f.push(", \"");
        f.push(self.key_format);
        f.push("\")");
        if self.is_paginated {
            f.push(" paginated");
        }
        f.push(" {");
        f.newline();
        f.indent(2);
        f.push(self.field.name);
        f.newline();
        f.indent(1);
        f.push("}");
    }
}

impl<'src> Format<'src> for ApiBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("api ");
        f.push(self.symbol.name);

        if self.methods.is_empty() {
            f.push(" {}");
            f.newline();
            return;
        }

        f.push(" {");
        f.newline();
        for spd in &self.methods {
            f.emit_leading_comments(spd.span.start, 1);
            f.indent(1);
            spd.block.fmt(f);
            f.emit_trailing_comment(spd.span.end);
            f.newline();
            if spd.span.end > f.cursor {
                f.cursor = spd.span.end;
            }
        }
        f.push("}");
        f.newline();
    }
}

impl<'src> Format<'src> for ApiBlockMethod<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push(self.http_verb.to_keyword());
        f.push(" ");
        f.push(self.symbol.name);
        f.push("(");

        let params: Vec<String> = self
            .parameters
            .iter()
            .map(|spd| match &spd.block {
                ApiBlockMethodParamKind::SelfParam {
                    symbol: _,
                    data_source,
                } => match data_source {
                    Some(ds) => format!("[source {}] self", ds.name),
                    None => "self".to_string(),
                },
                ApiBlockMethodParamKind::Field(sym) => {
                    format!("{}: {}", sym.name, fmt_cidl_type(&sym.cidl_type))
                }
            })
            .collect();

        f.push(&params.join(", "));
        f.push(") -> ");
        f.push(&fmt_cidl_type(&self.return_type));
    }
}

impl<'src> Format<'src> for DataSourceBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        if self.is_internal {
            f.push("[internal]");
            f.newline();
        }

        f.push("source ");
        f.push(self.symbol.name);
        f.push(" for ");
        f.push(self.model.name);
        f.push(" {");
        f.newline();

        if self.tree.0.is_empty() {
            f.indent(1);
            f.push("include {}");
            f.newline();
        } else {
            f.indent(1);
            f.push("include {");
            f.newline();
            self.tree.fmt_at(f, 2);
            f.indent(1);
            f.push("}");
            f.newline();
        }

        for (label, spd) in [("get", &self.get), ("list", &self.list)] {
            if let Some(spd) = spd {
                f.indent(1);
                f.push("sql ");
                f.push(label);
                f.push("(");
                f.push(&fmt_sql_params(&spd.block.parameters));
                f.push(") {");
                f.newline();
                fmt_sql_string(spd.block.raw_sql, 2, f);
                f.indent(1);
                f.push("}");
                f.newline();
            }
        }

        f.push("}");
        f.newline();
    }
}

impl ParsedIncludeTree<'_> {
    fn fmt_at(&self, f: &mut Formatter<'_>, depth: usize) {
        let leaves: Vec<&str> = self
            .0
            .iter()
            .filter(|(_, v)| v.0.is_empty())
            .map(|(k, _)| k.name)
            .collect();
        let branches: Vec<(&str, &ParsedIncludeTree<'_>)> = self
            .0
            .iter()
            .filter(|(_, v)| !v.0.is_empty())
            .map(|(k, v)| (k.name, v))
            .collect();

        if !leaves.is_empty() {
            f.indent(depth);
            f.push(&leaves.join(", "));
            f.newline();
        }

        for (name, subtree) in branches {
            f.indent(depth);
            f.push(name);
            f.push(" {");
            f.newline();
            subtree.fmt_at(f, depth + 1);
            f.indent(depth);
            f.push("}");
            f.newline();
        }
    }
}

impl<'src> Format<'src> for ServiceBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("service ");
        f.push(self.symbol.name);
        fmt_symbol_block(&self.fields, f);
    }
}

impl<'src> Format<'src> for PlainOldObjectBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        f.push("poo ");
        f.push(self.symbol.name);
        fmt_symbol_block(&self.fields, f);
    }
}

impl<'src> Format<'src> for [Spd<EnvBlock<'src>>] {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        if self.is_empty() {
            f.push("env {}");
            f.newline();
            return;
        }

        f.push("env {");
        f.newline();

        for spd in self {
            f.emit_leading_comments(spd.span.start, 1);
            f.indent(1);
            spd.block.fmt(f);
            f.emit_trailing_comment(spd.span.end);
            f.newline();
            if spd.span.end > f.cursor {
                f.cursor = spd.span.end;
            }
        }

        f.push("}");
        f.newline();
    }
}

impl<'src> Format<'src> for EnvBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        match &self.kind {
            EnvBlockKind::D1 => fmt_env_inline("d1", &self.symbols, f),
            EnvBlockKind::R2 => fmt_env_inline("r2", &self.symbols, f),
            EnvBlockKind::Kv => fmt_env_inline("kv", &self.symbols, f),
            EnvBlockKind::Var => {
                f.push("vars {");
                f.newline();
                for sym in &self.symbols {
                    f.emit_sym(sym, 2, |f| {
                        f.indent(2);
                        fmt_typed_field(sym, f);
                    });
                }
                f.indent(1);
                f.push("}");
            }
        }
    }
}

impl<'src> Format<'src> for InjectBlock<'src> {
    fn fmt(&'src self, f: &mut Formatter<'src>) {
        if self.symbols.is_empty() {
            f.push("inject {}");
            f.newline();
            return;
        }

        f.push("inject {");
        f.newline();
        for sym in &self.symbols {
            f.emit_sym(sym, 1, |f| {
                f.indent(1);
                f.push(sym.name);
            });
        }
        f.push("}");
        f.newline();
    }
}

/// Format a typed `name: Type` field.
fn fmt_typed_field(sym: &Symbol<'_>, f: &mut Formatter<'_>) {
    f.push(sym.name);
    f.push(": ");
    f.push(&fmt_cidl_type(&sym.cidl_type));
}

fn fmt_symbol_block<'src>(fields: &'src [Symbol<'src>], f: &mut Formatter<'src>) {
    if fields.is_empty() {
        f.push(" {}");
        f.newline();
        return;
    }
    f.push(" {");
    f.newline();
    for field in fields {
        f.emit_sym(field, 1, |f| {
            f.indent(1);
            fmt_typed_field(field, f);
        });
    }
    f.push("}");
    f.newline();
}

fn fmt_sql_block_group<'src>(
    keyword: &str,
    blocks: &'src [SqlBlockKind<'src>],
    f: &mut Formatter<'src>,
) {
    if blocks.is_empty() {
        f.push(keyword);
        f.push(" {}");
    } else {
        f.push(keyword);
        f.push(" {");
        f.newline();
        for b in blocks {
            f.indent(2);
            match b {
                SqlBlockKind::Column(sym) => fmt_typed_field(sym, f),
                SqlBlockKind::Foreign(fb) => fb.fmt(f),
            }
            f.newline();
        }
        f.indent(1);
        f.push("}");
    }
}

fn fmt_env_inline(keyword: &str, symbols: &[Symbol<'_>], f: &mut Formatter<'_>) {
    f.push(keyword);
    f.push(" { ");
    f.push(
        &symbols
            .iter()
            .map(|s| s.name)
            .collect::<Vec<_>>()
            .join(", "),
    );
    f.push(" }");
}

fn fmt_sql_params(params: &[Symbol<'_>]) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, fmt_cidl_type(&p.cidl_type)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Places each quotation mark on its own line with the SQL body indented.
/// Each line is trimmed before re-indenting so the formatter owns all indentation.
fn fmt_sql_string(raw_sql: &str, indent_depth: usize, f: &mut Formatter<'_>) {
    let lines: Vec<&str> = raw_sql
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        f.indent(indent_depth);
        f.push("\"\"");
        f.newline();
    } else {
        f.indent(indent_depth);
        f.push("\"");
        f.newline();
        for line in lines {
            f.indent(indent_depth);
            f.push(line);
            f.newline();
        }
        f.indent(indent_depth);
        f.push("\"");
        f.newline();
    }
}

/// Format a list of `(model, field)` adjacency pairs as `Model::field, …`.
fn fmt_adj(adj: &[(Symbol<'_>, Symbol<'_>)]) -> String {
    adj.iter()
        .map(|(m, field)| format!("{}::{}", m.name, field.name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_cidl_type(ty: &CidlType<'_>) -> String {
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
        CidlType::Inject { name }
        | CidlType::Object { name }
        | CidlType::UnresolvedReference { name } => name.to_string(),
        CidlType::Partial { object_name } => format!("Partial<{}>", object_name),
        CidlType::DataSource { model_name } => format!("DataSource<{}>", model_name),
        CidlType::Array(inner) => format!("Array<{}>", fmt_cidl_type(inner)),
        CidlType::HttpResult(inner) => format!("HttpResult<{}>", fmt_cidl_type(inner)),
        CidlType::Nullable(inner) => format!("Option<{}>", fmt_cidl_type(inner)),
        CidlType::Paginated(inner) => format!("Paginated<{}>", fmt_cidl_type(inner)),
        CidlType::KvObject(inner) => format!("KvObject<{}>", fmt_cidl_type(inner)),
    }
}

trait ToKeyword {
    fn to_keyword(&self) -> &'static str;
}

impl ToKeyword for HttpVerb {
    fn to_keyword(&self) -> &'static str {
        match self {
            HttpVerb::Get => "get",
            HttpVerb::Post => "post",
            HttpVerb::Put => "put",
            HttpVerb::Delete => "delete",
            HttpVerb::Patch => "patch",
        }
    }
}

impl ToKeyword for CrudKind {
    fn to_keyword(&self) -> &'static str {
        match self {
            CrudKind::Get => "get",
            CrudKind::List => "list",
            CrudKind::Save => "save",
        }
    }
}
