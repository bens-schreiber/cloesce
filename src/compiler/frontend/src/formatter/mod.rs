mod doc;

use std::cell::Cell;

use ast::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, DataSourceBlock,
    DataSourceBlockMethod, EnvBindingBlock, EnvBindingBlockKind, EnvBlock, ForeignBlock,
    ForeignQualifier, InjectBlock, KvBlock, ModelBlock, ModelBlockKind, NavigationBlock,
    PaginatedBlockKind, ParseAst, ParsedIncludeTree, PlainOldObjectBlock, R2Block, ServiceBlock,
    Spd, SqlBlockKind, Symbol, UseTag, UseTagParamKind, lexer::CommentMap,
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
            // Only return if this comment hasn't been processed yet
            if offset >= self.cursor.get() {
                let gap = self.src.get(node_end..offset).unwrap_or("");
                if !gap.contains('\n') {
                    self.cursor.set(offset + text.len());
                    return Doc::text(" ").then(Doc::text(text));
                }
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

    fn spd_doc<T: ToDoc<'src>>(&self, spd: &'src Spd<T>, indent: usize, inline: bool) -> Doc<'src> {
        let leading = self.leading_comments(spd.span.start, indent);
        let content = spd.block.to_doc(self);
        let trailing = self.trailing_comment(spd.span.end);
        self.advance(spd.span.end);

        if inline {
            return leading.then(content).then(trailing);
        }

        // Allow gaps between nodes, but not larger than one blank line
        let gap = self
            .src
            .get(self.cursor.get()..spd.span.start)
            .unwrap_or("");
        let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        extra_blank
            .then(leading)
            .then(Doc::hardline(indent))
            .then(content)
            .then(trailing)
    }

    fn sym_doc(&self, sym: &'src Symbol<'src>, indent: usize, inline: bool) -> Doc<'src> {
        // Check if there are comments before this symbol
        let prev = self.cursor.get();
        let lo = self.cm.entries.partition_point(|(off, _)| *off < prev);
        let has_leading_comments = self.cm.entries[lo..]
            .iter()
            .any(|&(offset, _)| offset < sym.span.start);

        let leading = self.leading_comments(sym.span.start, indent);
        let content = Doc::text(sym.name);
        let trailing = self.trailing_comment(sym.span.end);
        self.advance(sym.span.end);

        if inline {
            return if has_leading_comments {
                leading
                    .then(Doc::hardline(indent))
                    .then(content)
                    .then(trailing)
            } else {
                leading.then(content).then(trailing)
            };
        }

        // Allow gaps between nodes, but not larger than one blank line
        let gap = self
            .src
            .get(self.cursor.get()..sym.span.start)
            .unwrap_or("");
        let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        extra_blank
            .then(leading)
            .then(Doc::hardline(indent))
            .then(content)
            .then(trailing)
    }

    fn sym_typed_doc(&self, sym: &'src Symbol<'src>, indent: usize, inline: bool) -> Doc<'src> {
        // Check if there are comments before this symbol
        let prev = self.cursor.get();
        let lo = self.cm.entries.partition_point(|(off, _)| *off < prev);
        let has_leading_comments = self.cm.entries[lo..]
            .iter()
            .any(|&(offset, _)| offset < sym.span.start);

        let leading = self.leading_comments(sym.span.start, indent);
        let content = Doc::text(sym.name)
            .then(Doc::text(": "))
            .then(Doc::owned(fmt_cidl_type(&sym.cidl_type)));
        let trailing = self.trailing_comment(sym.span.end);
        self.advance(sym.span.end);

        if inline {
            return if has_leading_comments {
                leading
                    .then(Doc::hardline(indent))
                    .then(content)
                    .then(trailing)
            } else {
                leading.then(content).then(trailing)
            };
        }

        let gap = self
            .src
            .get(self.cursor.get()..sym.span.start)
            .unwrap_or("");
        let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        extra_blank
            .then(leading)
            .then(Doc::hardline(indent))
            .then(content)
            .then(trailing)
    }
}

trait ToDoc<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src>;
}

impl<'src> ParseAst<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::nil();

        for spd in &self.blocks {
            doc = doc.then(ctx.spd_doc(spd, 0, false));
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
            doc = doc.then(ctx.spd_doc(tag, 0, true)).then(Doc::hardline(0));
        }

        doc = doc
            .then(Doc::text("model "))
            .then(ctx.sym_doc(&self.symbol, 0, true));

        if self.blocks.is_empty() {
            // No content, return empty model
            return doc.then(Doc::text(" {}"));
        }

        doc = doc.then(Doc::text(" {"));
        for spd in &self.blocks {
            doc = doc.then(ctx.spd_doc(spd, 1, false));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for UseTag<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.params, |param| match param {
            UseTagParamKind::Crud(spd) => ctx.spd_doc(spd, 0, true),
            UseTagParamKind::EnvBinding(b) => ctx.sym_doc(b, 0, true),
        });

        Doc::text("[use ").then(params).then(Doc::text("]"))
    }
}

impl<'src> ToDoc<'src> for CrudKind {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text(fmt_crud(self))
    }
}

impl<'src> ToDoc<'src> for ModelBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            ModelBlockKind::Column(sym) => ctx.sym_typed_doc(sym, 1, true),
            ModelBlockKind::Foreign(fb) => fb.to_doc(ctx),
            ModelBlockKind::Navigation(nb) => nb.to_doc(ctx),
            ModelBlockKind::Kv(kv) => kv.to_doc(ctx),
            ModelBlockKind::R2(r2) => r2.to_doc(ctx),
            ModelBlockKind::Primary(blocks) => {
                let mut doc = Doc::text("primary {");
                for block in blocks {
                    doc = doc.then(ctx.spd_doc(block, 2, true));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
            ModelBlockKind::Unique(blocks) => {
                let mut doc = Doc::text("unique {");
                for block in blocks {
                    doc = doc.then(ctx.spd_doc(block, 2, true));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
            ModelBlockKind::Optional(blocks) => {
                let mut doc = Doc::text("optional {");
                for block in blocks {
                    doc = doc.then(ctx.spd_doc(block, 2, true));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
            ModelBlockKind::Paginated(blocks) => {
                let mut doc = Doc::text("paginated {");
                for block in blocks {
                    doc = doc.then(ctx.spd_doc(block, 2, true));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
            ModelBlockKind::KeyField(syms) => {
                let mut doc = Doc::text("keyfield {");
                for sym in syms {
                    doc = doc.then(Doc::hardline(2)).then(ctx.sym_doc(sym, 2, true));
                }
                doc.then(Doc::hardline(1)).then(Doc::text("}"))
            }
        }
    }
}

impl<'src> ToDoc<'src> for SqlBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            SqlBlockKind::Column(sym) => ctx.sym_typed_doc(sym, 2, false),
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
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let adjs = comma_separated(&self.adj, |adj| {
            let left = ctx.sym_doc(&adj.0, 0, true);
            let right = ctx.sym_doc(&adj.1, 0, true);
            left.then(Doc::text("::")).then(right)
        });

        let mut doc = Doc::text("foreign (").then(adjs).then(Doc::text(")"));

        let qualifier = match &self.qualifier {
            Some(ForeignQualifier::Primary) => Doc::text(" primary"),
            Some(ForeignQualifier::Optional) => Doc::text(" optional"),
            Some(ForeignQualifier::Unique) => Doc::text(" unique"),
            None => Doc::nil(),
        };
        doc = doc.then(qualifier).then(Doc::text(" {"));

        for field in &self.fields {
            doc = doc.then(ctx.sym_doc(field, 2, false));
        }

        if let Some(nav) = &self.nav {
            doc = doc
                .then(Doc::hardline(2))
                .then(Doc::text("nav { "))
                .then(ctx.sym_doc(&nav.block, 2, true))
                .then(Doc::text(" }"));
        }

        doc.then(Doc::hardline(1)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for NavigationBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let adjs = comma_separated(&self.adj, |adj| {
            let left = ctx.sym_doc(&adj.0, 0, true);
            let right = ctx.sym_doc(&adj.1, 0, true);
            left.then(Doc::text("::")).then(right)
        });

        Doc::text("nav (")
            .then(adjs)
            .then(Doc::text(") {"))
            .then(ctx.sym_doc(&self.nav.block, 2, false))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for KvBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" paginated")
        } else {
            Doc::nil()
        };
        Doc::text("kv (")
            .then(ctx.sym_doc(&self.env_binding, 0, true))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(ctx.sym_typed_doc(&self.field, 2, false))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for R2Block<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" paginated")
        } else {
            Doc::nil()
        };
        Doc::text("r2 (")
            .then(ctx.sym_doc(&self.env_binding, 0, true))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(ctx.sym_doc(&self.field, 2, false))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for ApiBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("api ").then(ctx.sym_doc(&self.symbol, 0, true));

        if self.methods.is_empty() {
            return doc.then(Doc::text(" {}"));
        }

        doc = doc.then(Doc::text(" {"));
        for spd in &self.methods {
            doc = doc.then(ctx.spd_doc(spd, 1, false));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for ApiBlockMethod<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.parameters, |param| ctx.spd_doc(param, 0, true));

        Doc::text(fmt_http_verb(&self.http_verb))
            .then(Doc::text(" "))
            .then(ctx.sym_doc(&self.symbol, 0, true))
            .then(Doc::text("("))
            .then(params)
            .then(Doc::text(") -> "))
            .then(Doc::owned(fmt_cidl_type(&self.return_type)))
    }
}

impl<'src> ToDoc<'src> for ApiBlockMethodParamKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            ApiBlockMethodParamKind::SelfParam {
                symbol,
                data_source,
            } => {
                let ds_doc = if let Some(ds) = data_source {
                    Doc::text("[source ")
                        .then(ctx.sym_doc(ds, 0, true))
                        .then(Doc::text("] "))
                } else {
                    Doc::nil()
                };
                ds_doc.then(ctx.sym_doc(symbol, 0, true))
            }
            ApiBlockMethodParamKind::Field(sym) => ctx.sym_typed_doc(sym, 0, true),
        }
    }
}

impl<'src> ToDoc<'src> for DataSourceBlockMethod<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.parameters, |param| ctx.sym_typed_doc(param, 0, true));

        Doc::text("(")
            .then(params)
            .then(Doc::text(") {"))
            .then(Doc::hardline(2))
            .then(Doc::text("\""))
            .then(Doc::hardline(2))
            .then(Doc::text(self.raw_sql))
            .then(Doc::hardline(2))
            .then(Doc::text("\""))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for DataSourceBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let internal = if self.is_internal {
            Doc::text("[internal]").then(Doc::hardline(0))
        } else {
            Doc::nil()
        };

        let include = if self.tree.0.is_empty() {
            Doc::hardline(1).then(Doc::text("include {}"))
        } else {
            Doc::hardline(1)
                .then(Doc::text("include {"))
                .then(self.tree.to_doc_at(ctx, 2))
                .then(Doc::hardline(1))
                .then(Doc::text("}"))
        };

        let mut doc = internal
            .then(Doc::text("source "))
            .then(ctx.sym_doc(&self.symbol, 0, true))
            .then(Doc::text(" for "))
            .then(ctx.sym_doc(&self.model, 0, true))
            .then(Doc::text(" {"))
            .then(include);

        for (label, spd_opt) in [("get", &self.get), ("list", &self.list)] {
            if let Some(spd) = spd_opt {
                doc = doc
                    .then(Doc::hardline(1))
                    .then(Doc::text("sql "))
                    .then(Doc::text(label))
                    .then(ctx.spd_doc(spd, 2, true))
            }
        }

        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl ParsedIncludeTree<'_> {
    fn to_doc_at<'src>(&'src self, ctx: &FmtCtx<'src>, depth: usize) -> Doc<'src> {
        let leaves = self.0.iter().filter(|(_, v)| v.0.is_empty());
        let branches = self.0.iter().filter(|(_, v)| !v.0.is_empty());

        let mut doc = Doc::nil();
        for (leaf, _) in leaves {
            doc = doc.then(ctx.sym_doc(leaf, depth, false));
        }
        for (name, subtree) in branches {
            doc = doc
                .then(ctx.sym_doc(name, depth, false))
                .then(Doc::text(" {"))
                .then(subtree.to_doc_at(ctx, depth + 1))
                .then(Doc::hardline(depth))
                .then(Doc::text("}"));
        }
        doc
    }
}

impl<'src> ToDoc<'src> for ServiceBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("service ").then(ctx.sym_doc(&self.symbol, 0, true));

        if self.fields.is_empty() {
            return doc.then(Doc::text(" {}"));
        }

        doc = doc.then(Doc::text(" {"));
        for field in &self.fields {
            doc = doc.then(ctx.sym_typed_doc(field, 1, false));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for PlainOldObjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("poo ").then(ctx.sym_doc(&self.symbol, 0, true));

        if self.fields.is_empty() {
            return doc.then(Doc::text(" {}"));
        }

        doc = doc.then(Doc::text(" {"));
        for field in &self.fields {
            doc = doc.then(ctx.sym_typed_doc(field, 1, false));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for EnvBindingBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = match self.kind {
            EnvBindingBlockKind::D1 => Doc::text("d1 {"),
            EnvBindingBlockKind::R2 => Doc::text("r2 {"),
            EnvBindingBlockKind::Kv => Doc::text("kv {"),
            EnvBindingBlockKind::Var => Doc::text("vars {"),
        };

        for symbol in &self.symbols {
            doc = doc.then(Doc::hardline(2));
            if matches!(self.kind, EnvBindingBlockKind::Var) {
                doc = doc.then(ctx.sym_typed_doc(symbol, 2, true));
            } else {
                doc = doc.then(ctx.sym_doc(symbol, 2, true));
            }
        }

        doc.then(Doc::hardline(1)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for EnvBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("env {");
        for spd in &self.blocks {
            doc = doc.then(ctx.spd_doc(spd, 1, false));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for InjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        if self.symbols.is_empty() {
            return Doc::text("inject {}");
        }
        let mut doc = Doc::text("inject {");
        for sym in &self.symbols {
            doc = doc.then(Doc::hardline(1)).then(ctx.sym_doc(sym, 1, true));
        }
        doc.then(Doc::hardline(0)).then(Doc::text("}"))
    }
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

fn fmt_http_verb(verb: &HttpVerb) -> &'static str {
    match verb {
        HttpVerb::Get => "get",
        HttpVerb::Post => "post",
        HttpVerb::Put => "put",
        HttpVerb::Delete => "delete",
        HttpVerb::Patch => "patch",
    }
}

fn fmt_crud(kind: &CrudKind) -> &'static str {
    match kind {
        CrudKind::Get => "get",
        CrudKind::List => "list",
        CrudKind::Save => "save",
    }
}

fn comma_separated<'src, T, F>(items: &'src [T], mut item_doc: F) -> Doc<'src>
where
    F: FnMut(&'src T) -> Doc<'src>,
{
    let mut doc = Doc::nil();
    for (idx, item) in items.iter().enumerate() {
        doc = doc.then(item_doc(item));
        if idx + 1 < items.len() {
            doc = doc.then(Doc::text(", "));
        }
    }
    doc
}
