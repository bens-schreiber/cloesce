mod doc;

use std::cell::Cell;

use ast::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, DataSourceBlock,
    EnvBindingBlock, EnvBindingBlockKind, EnvBlock, ForeignBlock, ForeignQualifier, InjectBlock,
    KvBlock, ModelBlock, ModelBlockKind, NavigationBlock, PaginatedBlockKind, ParseAst,
    ParsedIncludeTree, PlainOldObjectBlock, R2Block, ServiceBlock, Spd, SqlBlockKind, Symbol,
    UseTag, UseTagParamKind, lexer::CommentMap,
};
use doc::{Doc, render};

pub struct Formatter;

impl Formatter {
    pub fn format(ast: &ParseAst<'_>, comment_map: &CommentMap<'_>, src: &str) -> String {
        let ctx = FmtCtx::new(comment_map, src);
        let doc = ast.to_doc(&ctx);
        render(&doc)
    }
}

struct FmtCtx<'src> {
    cm: &'src CommentMap<'src>,
    src: &'src str,

    /// Byte offset just past the last thing emitted
    cursor: Cell<usize>,
}

impl<'src> FmtCtx<'src> {
    fn new(cm: &'src CommentMap<'src>, src: &'src str) -> Self {
        Self {
            cm,
            src,
            cursor: Cell::new(0),
        }
    }

    /// Collect leading comment docs between the current cursor and `node_start`
    fn leading_comments(&self, node_start: usize, indent: usize) -> Doc<'src> {
        let prev = self.cursor.get();
        let lo = self.cm.entries.partition_point(|(off, _)| *off < prev);
        let mut doc = Doc::nil();
        let mut cursor = prev;

        for &(offset, text) in &self.cm.entries[lo..] {
            if offset >= node_start {
                break;
            }
            let gap = self.src.get(cursor..offset).unwrap_or("");
            let is_leading = gap.is_empty() || gap.contains('\n');
            if is_leading {
                doc = doc.then(Doc::hardline(indent)).then(Doc::text(text));
            }
            cursor = offset + text.len();
        }

        if node_start > cursor {
            cursor = node_start;
        }
        self.cursor.set(cursor);
        doc
    }

    /// Emit a trailing comment immediately after `node_end` if one exists on
    /// the same source line
    fn trailing_comment(&self, node_end: usize) -> Doc<'src> {
        let lo = self.cm.entries.partition_point(|(off, _)| *off < node_end);
        if let Some(&(offset, text)) = self.cm.entries.get(lo) {
            let gap = self.src.get(node_end..offset).unwrap_or("");
            if !gap.contains('\n') {
                self.cursor.set(offset + text.len());
                return Doc::text(" ").then(Doc::text(text));
            }
        }
        Doc::nil()
    }

    /// Advance the cursor to at least `pos`.
    fn advance(&self, pos: usize) {
        if pos > self.cursor.get() {
            self.cursor.set(pos);
        }
    }
}

trait ToDoc<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src>;
}

fn spd_doc<'src, T: ToDoc<'src>>(
    spd: &'src Spd<T>,
    ctx: &FmtCtx<'src>,
    indent: usize,
) -> Doc<'src> {
    // Allow gaps between nodes, but not larger than one blank line
    let gap = ctx.src.get(ctx.cursor.get()..spd.span.start).unwrap_or("");
    let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
        Doc::hardline(0)
    } else {
        Doc::nil()
    };

    let leading = ctx.leading_comments(spd.span.start, indent);
    let content = spd.block.to_doc(ctx);
    let trailing = ctx.trailing_comment(spd.span.end);
    ctx.advance(spd.span.end);
    extra_blank
        .then(leading)
        .then(Doc::hardline(indent))
        .then(content)
        .then(trailing)
}

impl<'src> ParseAst<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::nil();
        let mut first = true;
        for spd in &self.blocks {
            // Leading file level comments
            let leading = {
                let prev = ctx.cursor.get();
                let lo = ctx.cm.entries.partition_point(|(off, _)| *off < prev);
                let mut ldoc = Doc::nil();
                let mut cursor = prev;
                for &(offset, text) in &ctx.cm.entries[lo..] {
                    if offset >= spd.span.start {
                        break;
                    }
                    let gap = ctx.src.get(cursor..offset).unwrap_or("");
                    if gap.is_empty() || gap.contains('\n') {
                        ldoc = ldoc.then(Doc::text(text)).then(Doc::hardline(0));
                    }
                    cursor = offset + text.len();
                }
                if spd.span.start > cursor {
                    cursor = spd.span.start;
                }
                ctx.cursor.set(cursor);
                ldoc
            };

            if !first {
                doc = doc.then(Doc::hardline(0));
            }
            first = false;

            doc = doc.then(leading).then(spd.block.to_doc(ctx));
            ctx.advance(spd.span.end);
        }

        // Trailing file level comments after the last block
        let cursor = ctx.cursor.get();
        let lo = ctx.cm.entries.partition_point(|(off, _)| *off < cursor);
        for &(_, text) in &ctx.cm.entries[lo..] {
            doc = doc.then(Doc::text(text)).then(Doc::hardline(0));
        }
        doc
    }
}

impl<'src> ToDoc<'src> for AstBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            AstBlockKind::Model(b) => b.to_doc(ctx),
            AstBlockKind::Api(b) => b.to_doc(ctx),
            AstBlockKind::DataSource(b) => b.to_doc(ctx),
            AstBlockKind::Service(b) => b.to_doc(ctx),
            AstBlockKind::PlainOldObject(b) => b.to_doc(ctx),
            AstBlockKind::Env(b) => b.to_doc(ctx),
            AstBlockKind::Inject(b) => b.to_doc(ctx),
        }
    }
}

impl<'src> ToDoc<'src> for ModelBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::nil();
        for tag in &self.use_tags {
            doc = doc.then(spd_doc(tag, ctx, 0)).then(Doc::hardline(0));
        }

        doc = doc
            .then(Doc::text("model "))
            .then(Doc::text(self.symbol.name));

        if self.blocks.is_empty() {
            return doc.then(Doc::text(" {}")).then(Doc::hardline(0));
        }

        doc = doc.then(Doc::text(" {"));

        let mut prev_is_column = true;
        let mut first = true;
        for spd in &self.blocks {
            let is_column = matches!(spd.block, ModelBlockKind::Column(_));
            if !first && (!is_column || !prev_is_column) {
                doc = doc.then(Doc::hardline(0));
            }
            doc = doc.then(spd_doc(spd, ctx, 1));
            first = false;
            prev_is_column = is_column;
        }

        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

impl<'src> ToDoc<'src> for UseTag<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|p| match p {
                UseTagParamKind::Crud(spd) => spd.block.to_keyword().to_string(),
                UseTagParamKind::EnvBinding(b) => b.name.to_string(),
            })
            .collect();
        Doc::text("[use ")
            .then(Doc::owned(params.join(", ")))
            .then(Doc::text("]"))
    }
}

impl<'src> ToDoc<'src> for ModelBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            ModelBlockKind::Column(sym) => typed_field_doc(sym),
            ModelBlockKind::Foreign(fb) => fb.to_doc(ctx),
            ModelBlockKind::Navigation(nb) => nb.to_doc(ctx),
            ModelBlockKind::Kv(kv) => kv.to_doc(ctx),
            ModelBlockKind::R2(r2) => r2.to_doc(ctx),
            ModelBlockKind::Primary(blocks) => sql_group_doc("primary", blocks, ctx),
            ModelBlockKind::Unique(blocks) => sql_group_doc("unique", blocks, ctx),
            ModelBlockKind::Optional(blocks) => sql_group_doc("optional", blocks, ctx),
            ModelBlockKind::Paginated(blocks) => {
                if blocks.is_empty() {
                    return Doc::text("paginated {}");
                }
                let mut doc = Doc::text("paginated {");
                for pb in blocks {
                    doc = doc.then(spd_doc(pb, ctx, 2));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
            ModelBlockKind::KeyField(fields) => {
                if fields.is_empty() {
                    return Doc::text("keyfield {}");
                }
                let mut doc = Doc::text("keyfield {");
                for sym in fields {
                    doc = doc.then(Doc::hardline(2)).then(Doc::text(sym.name));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
        }
    }
}

impl<'src> ToDoc<'src> for SqlBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            SqlBlockKind::Column(sym) => typed_field_doc(sym),
            SqlBlockKind::Foreign(fb) => fb.to_doc(ctx),
        }
    }
}

impl<'src> ToDoc<'src> for PaginatedBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            PaginatedBlockKind::R2(r2) => r2.to_doc(ctx),
            PaginatedBlockKind::Kv(kv) => kv.to_doc(ctx),
        }
    }
}

impl<'src> ToDoc<'src> for ForeignBlock<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let qualifier = match &self.qualifier {
            Some(ForeignQualifier::Primary) => Doc::text(" primary"),
            Some(ForeignQualifier::Optional) => Doc::text(" optional"),
            Some(ForeignQualifier::Unique) => Doc::text(" unique"),
            None => Doc::nil(),
        };

        let mut doc = Doc::text("foreign (")
            .then(Doc::owned(fmt_adj(&self.adj)))
            .then(Doc::text(")"))
            .then(qualifier)
            .then(Doc::text(" {"));

        for field in &self.fields {
            doc = doc.then(Doc::hardline(2)).then(Doc::text(field.name));
        }
        if let Some(nav) = &self.nav {
            doc = doc
                .then(Doc::hardline(2))
                .then(Doc::text("nav { "))
                .then(Doc::text(nav.name))
                .then(Doc::text(" }"));
        }
        doc.then(Doc::hardline(1)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for NavigationBlock<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text("nav (")
            .then(Doc::owned(fmt_adj(&self.adj)))
            .then(Doc::text(") {"))
            .then(Doc::hardline(2))
            .then(Doc::text(self.symbol.name))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for KvBlock<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" paginated")
        } else {
            Doc::nil()
        };
        Doc::text("kv (")
            .then(Doc::text(self.env_binding.name))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(Doc::hardline(2))
            .then(typed_field_doc(&self.field))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for R2Block<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" paginated")
        } else {
            Doc::nil()
        };
        Doc::text("r2 (")
            .then(Doc::text(self.env_binding.name))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(Doc::hardline(2))
            .then(Doc::text(self.field.name))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for ApiBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("api ").then(Doc::text(self.symbol.name));

        if self.methods.is_empty() {
            return doc.then(Doc::text(" {}")).then(Doc::hardline(0));
        }

        doc = doc.then(Doc::text(" {"));
        for spd in &self.methods {
            doc = doc.then(spd_doc(spd, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

impl<'src> ToDoc<'src> for ApiBlockMethod<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
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

        Doc::text(self.http_verb.to_keyword())
            .then(Doc::text(" "))
            .then(Doc::text(self.symbol.name))
            .then(Doc::text("("))
            .then(Doc::owned(params.join(", ")))
            .then(Doc::text(") -> "))
            .then(Doc::owned(fmt_cidl_type(&self.return_type)))
    }
}

impl<'src> ToDoc<'src> for DataSourceBlock<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let internal = if self.is_internal {
            Doc::text("[internal]").then(Doc::hardline(0))
        } else {
            Doc::nil()
        };

        let include = if self.tree.0.is_empty() {
            Doc::hardline(1)
                .then(Doc::text("include {}"))
                .then(Doc::hardline(0))
        } else {
            Doc::hardline(1)
                .then(Doc::text("include {"))
                .then(self.tree.to_doc_at(2))
                .then(Doc::hardline(1))
                .then(Doc::text("}"))
                .then(Doc::hardline(0))
        };

        let mut doc = internal
            .then(Doc::text("source "))
            .then(Doc::text(self.symbol.name))
            .then(Doc::text(" for "))
            .then(Doc::text(self.model.name))
            .then(Doc::text(" {"))
            .then(include);

        for (label, spd_opt) in [("get", &self.get), ("list", &self.list)] {
            if let Some(spd) = spd_opt {
                doc = doc
                    .then(Doc::hardline(1))
                    .then(Doc::text("sql "))
                    .then(Doc::text(label))
                    .then(Doc::text("("))
                    .then(Doc::owned(fmt_sql_params(&spd.block.parameters)))
                    .then(Doc::text(") {"))
                    .then(fmt_sql_string_doc(spd.block.raw_sql, 2))
                    .then(Doc::hardline(1))
                    .then(Doc::text("}"))
                    .then(Doc::hardline(0));
            }
        }

        doc.then(Doc::text("}")).then(Doc::hardline(0))
    }
}

impl ParsedIncludeTree<'_> {
    fn to_doc_at(&self, depth: usize) -> Doc<'_> {
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

        let mut doc = Doc::nil();
        for leaf in &leaves {
            doc = doc.then(Doc::hardline(depth)).then(Doc::text(leaf));
        }
        for (name, subtree) in branches {
            doc = doc
                .then(Doc::hardline(depth))
                .then(Doc::text(name))
                .then(Doc::text(" {"))
                .then(subtree.to_doc_at(depth + 1))
                .then(Doc::hardline(depth))
                .then(Doc::text("}"));
        }
        doc
    }
}

impl<'src> ToDoc<'src> for ServiceBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text("service ")
            .then(Doc::text(self.symbol.name))
            .then(symbol_block_doc(&self.fields, ctx))
    }
}

impl<'src> ToDoc<'src> for PlainOldObjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text("poo ")
            .then(Doc::text(self.symbol.name))
            .then(symbol_block_doc(&self.fields, ctx))
    }
}

impl<'src> ToDoc<'src> for EnvBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("env {");
        for spd in &self.blocks {
            doc = doc.then(spd_doc(spd, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

impl<'src> ToDoc<'src> for EnvBindingBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let keyword = match self.kind {
            EnvBindingBlockKind::D1 => "d1",
            EnvBindingBlockKind::R2 => "r2",
            EnvBindingBlockKind::Kv => "kv",
            EnvBindingBlockKind::Var => "vars",
        };

        let mut doc = Doc::text(keyword).then(Doc::text(" {"));
        for sym in &self.symbols {
            let leading = ctx.leading_comments(sym.span.start, 2);
            doc = doc.then(leading).then(Doc::hardline(2));
            doc = match self.kind {
                EnvBindingBlockKind::Var => doc.then(typed_field_doc(sym)),
                _ => doc.then(Doc::text(sym.name)),
            };
            let trailing = ctx.trailing_comment(sym.span.end);
            ctx.advance(sym.span.end);
            doc = doc.then(trailing);
        }
        doc.then(Doc::hardline(1)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for InjectBlock<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        if self.symbols.is_empty() {
            return Doc::text("inject {}").then(Doc::hardline(0));
        }
        let mut doc = Doc::text("inject {");
        for sym in &self.symbols {
            doc = doc.then(Doc::hardline(1)).then(Doc::text(sym.name));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

fn typed_field_doc<'src>(sym: &'src Symbol<'src>) -> Doc<'src> {
    Doc::text(sym.name)
        .then(Doc::text(": "))
        .then(Doc::owned(fmt_cidl_type(&sym.cidl_type)))
}

fn symbol_block_doc<'src>(fields: &'src [Symbol<'src>], _ctx: &FmtCtx<'src>) -> Doc<'src> {
    if fields.is_empty() {
        return Doc::text(" {}").then(Doc::hardline(0));
    }
    let mut doc = Doc::text(" {");
    for field in fields {
        doc = doc.then(Doc::hardline(1)).then(typed_field_doc(field));
    }
    doc.then(Doc::hardline(0))
        .then(Doc::text("}"))
        .then(Doc::hardline(0))
}

fn sql_group_doc<'src>(
    keyword: &'src str,
    blocks: &'src [Spd<SqlBlockKind<'src>>],
    ctx: &FmtCtx<'src>,
) -> Doc<'src> {
    if blocks.is_empty() {
        return Doc::text(keyword).then(Doc::text(" {}"));
    }
    let mut doc = Doc::text(keyword).then(Doc::text(" {"));
    for b in blocks {
        doc = doc.then(spd_doc(b, ctx, 2));
    }
    doc.then(Doc::hardline(1)).then(Doc::text("}"))
}

fn fmt_sql_params(params: &[Symbol<'_>]) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, fmt_cidl_type(&p.cidl_type)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn fmt_sql_string_doc(raw_sql: &str, indent_depth: usize) -> Doc<'_> {
    let lines: Vec<&str> = raw_sql
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if lines.is_empty() {
        Doc::hardline(indent_depth).then(Doc::text("\"\""))
    } else {
        let mut doc = Doc::hardline(indent_depth).then(Doc::text("\""));
        for line in lines {
            doc = doc
                .then(Doc::hardline(indent_depth))
                .then(Doc::owned(line.to_string()));
        }
        doc.then(Doc::hardline(indent_depth)).then(Doc::text("\""))
    }
}

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
