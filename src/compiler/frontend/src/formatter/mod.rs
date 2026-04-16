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

    // Collect comments before and after the spd
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

fn sym_doc<'src>(sym: &'src Symbol<'src>, ctx: &FmtCtx<'src>, indent: usize) -> Doc<'src> {
    // Allow gaps between nodes, but not larger than one blank line
    let gap = ctx.src.get(ctx.cursor.get()..sym.span.start).unwrap_or("");
    let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
        Doc::hardline(0)
    } else {
        Doc::nil()
    };

    // Collect comments before and after the symbol
    let leading = ctx.leading_comments(sym.span.start, indent);
    let content = Doc::text(sym.name);
    let trailing = ctx.trailing_comment(sym.span.end);
    ctx.advance(sym.span.end);

    extra_blank
        .then(leading)
        .then(Doc::hardline(indent))
        .then(content)
        .then(trailing)
}

fn sym_typed_doc<'src>(sym: &'src Symbol<'src>, ctx: &FmtCtx<'src>, indent: usize) -> Doc<'src> {
    let gap = ctx.src.get(ctx.cursor.get()..sym.span.start).unwrap_or("");
    let extra_blank = if gap.chars().filter(|&c| c == '\n').count() >= 2 {
        Doc::hardline(0)
    } else {
        Doc::nil()
    };

    // Collect comments before and after the symbol
    let leading = ctx.leading_comments(sym.span.start, indent);
    let content = Doc::text(sym.name)
        .then(Doc::text(": "))
        .then(Doc::owned(fmt_cidl_type(&sym.cidl_type)));
    let trailing = ctx.trailing_comment(sym.span.end);
    ctx.advance(sym.span.end);

    extra_blank
        .then(leading)
        .then(Doc::hardline(indent))
        .then(content)
        .then(trailing)
}

impl<'src> ParseAst<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::nil();

        for spd in &self.blocks {
            doc = doc.then(spd_doc(spd, ctx, 0));
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
            .then(sym_doc(&self.symbol, ctx, 0));

        if self.blocks.is_empty() {
            // No content, return empty model
            return doc.then(Doc::text(" {}")).then(Doc::hardline(0));
        }

        doc = doc.then(Doc::text(" {"));
        for spd in &self.blocks {
            doc = doc.then(spd_doc(spd, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

impl<'src> ToDoc<'src> for UseTag<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.params, |param| match param {
            UseTagParamKind::Crud(spd) => spd_doc(spd, ctx, 0),
            UseTagParamKind::EnvBinding(b) => sym_doc(b, ctx, 0),
        });

        Doc::text("[use ").then(params).then(Doc::text("]"))
    }
}

impl<'src> ToDoc<'src> for CrudKind {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text(fmt_crud(self))
    }
}

impl<'src> ToDoc<'src> for Symbol<'src> {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::text(self.name)
    }
}

impl<'src> ToDoc<'src> for ModelBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            ModelBlockKind::Column(sym) => sym_typed_doc(sym, ctx, 1),
            ModelBlockKind::Foreign(fb) => fb.to_doc(ctx),
            ModelBlockKind::Navigation(nb) => nb.to_doc(ctx),
            ModelBlockKind::Kv(kv) => kv.to_doc(ctx),
            ModelBlockKind::R2(r2) => r2.to_doc(ctx),
            ModelBlockKind::Primary(blocks)
            | ModelBlockKind::Unique(blocks)
            | ModelBlockKind::Optional(blocks) => {
                let mut doc = Doc::nil();
                for block in blocks {
                    doc = doc.then(spd_doc(block, ctx, 2));
                }
                doc
            }
            ModelBlockKind::Paginated(blocks) => {
                let mut doc = Doc::nil();
                for block in blocks {
                    doc = doc.then(spd_doc(block, ctx, 2));
                }
                doc
            }
            ModelBlockKind::KeyField(syms) => {
                let mut doc = Doc::nil();
                for sym in syms {
                    doc = doc.then(sym_doc(sym, ctx, 2));
                }
                doc
            }
        }
    }
}

impl<'src> ToDoc<'src> for SqlBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            SqlBlockKind::Column(sym) => sym_typed_doc(sym, ctx, 2),
            SqlBlockKind::Foreign(fb) => fb.to_doc(ctx),
        }
    }
}

impl<'src> ToDoc<'src> for PaginatedBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("paginated {");
        doc = doc.then(match self {
            PaginatedBlockKind::R2(r2) => r2.to_doc(ctx),
            PaginatedBlockKind::Kv(kv) => kv.to_doc(ctx),
        });
        doc.then(Doc::hardline(1)).then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for ForeignBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let adjs = comma_separated(&self.adj, |adj| {
            let left = sym_doc(&adj.0, ctx, 0);
            let right = sym_doc(&adj.1, ctx, 0);
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
            doc = doc.then(sym_doc(field, ctx, 2)).then(Doc::hardline(1));
        }

        if let Some(nav) = &self.nav {
            doc = doc.then(spd_doc(nav, ctx, 2)).then(Doc::hardline(1));
        }

        doc.then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for NavigationBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let adjs = comma_separated(&self.adj, |adj| {
            let left = sym_doc(&adj.0, ctx, 0);
            let right = sym_doc(&adj.1, ctx, 0);
            left.then(Doc::text("::")).then(right)
        });

        Doc::text("nav (")
            .then(adjs)
            .then(Doc::text(") {"))
            .then(spd_doc(&self.nav, ctx, 2))
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
            .then(sym_doc(&self.env_binding, ctx, 0))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(Doc::hardline(2))
            .then(sym_typed_doc(&self.field, ctx, 2))
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
            .then(sym_doc(&self.env_binding, ctx, 0))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(Doc::text(" {"))
            .then(Doc::hardline(2))
            .then(sym_doc(&self.field, ctx, 2))
            .then(Doc::hardline(1))
            .then(Doc::text("}"))
    }
}

impl<'src> ToDoc<'src> for ApiBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("api ").then(sym_doc(&self.symbol, ctx, 0));

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
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.parameters, |param| spd_doc(param, ctx, 0));

        Doc::text(fmt_http_verb(&self.http_verb))
            .then(Doc::text(" "))
            .then(sym_doc(&self.symbol, ctx, 0))
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
                        .then(sym_doc(ds, ctx, 0))
                        .then(Doc::text("] "))
                } else {
                    Doc::nil()
                };
                ds_doc.then(sym_doc(symbol, ctx, 0))
            }
            ApiBlockMethodParamKind::Field(sym) => sym_typed_doc(sym, ctx, 0),
        }
    }
}

impl<'src> ToDoc<'src> for DataSourceBlockMethod<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.parameters, |param| sym_typed_doc(param, ctx, 0));

        Doc::text("sql (")
            .then(params)
            .then(Doc::text(") {\""))
            .then(Doc::text(self.raw_sql))
            .then(Doc::text("\"}"))
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
            Doc::hardline(1)
                .then(Doc::text("include {}"))
                .then(Doc::hardline(0))
        } else {
            Doc::hardline(1)
                .then(Doc::text("include {"))
                .then(self.tree.to_doc_at(ctx, 2))
                .then(Doc::hardline(1))
                .then(Doc::text("}"))
                .then(Doc::hardline(0))
        };

        let mut doc = internal
            .then(Doc::text("source "))
            .then(sym_doc(&self.symbol, ctx, 0))
            .then(Doc::text(" for "))
            .then(sym_doc(&self.model, ctx, 0))
            .then(Doc::text(" {"))
            .then(include);

        for (label, spd_opt) in [("get", &self.get), ("list", &self.list)] {
            if let Some(spd) = spd_opt {
                doc = doc
                    .then(Doc::text(label))
                    .then(Doc::text(" {"))
                    .then(spd_doc(spd, ctx, 2))
                    .then(Doc::hardline(1))
                    .then(Doc::text("}"));
            }
        }

        doc.then(Doc::text("}")).then(Doc::hardline(0))
    }
}

impl ParsedIncludeTree<'_> {
    fn to_doc_at<'src>(&'src self, ctx: &FmtCtx<'src>, depth: usize) -> Doc<'src> {
        let leaves = self.0.iter().filter(|(_, v)| v.0.is_empty());
        let branches = self.0.iter().filter(|(_, v)| !v.0.is_empty());

        let mut doc = Doc::nil();
        for (leaf, _) in leaves {
            doc = doc.then(sym_doc(leaf, ctx, depth));
        }
        for (name, subtree) in branches {
            doc = doc
                .then(sym_doc(name, ctx, depth))
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
        let mut doc = Doc::text("service ").then(sym_doc(&self.symbol, ctx, 0));

        if self.fields.is_empty() {
            return doc.then(Doc::text(" {}")).then(Doc::hardline(0));
        }

        doc = doc.then(Doc::text(" {"));
        for field in &self.fields {
            doc = doc.then(sym_typed_doc(field, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
    }
}

impl<'src> ToDoc<'src> for PlainOldObjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::text("poo ").then(sym_doc(&self.symbol, ctx, 0));

        if self.fields.is_empty() {
            return doc.then(Doc::text(" {}")).then(Doc::hardline(0));
        }

        doc = doc.then(Doc::text(" {"));
        for field in &self.fields {
            doc = doc.then(sym_typed_doc(field, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
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
            if matches!(self.kind, EnvBindingBlockKind::Var) {
                doc = doc.then(sym_typed_doc(symbol, ctx, 2));
            } else {
                doc = doc.then(sym_doc(symbol, ctx, 2));
            }
        }

        doc.then(Doc::hardline(1)).then(Doc::text("}"))
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

impl<'src> ToDoc<'src> for InjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        if self.symbols.is_empty() {
            return Doc::text("inject {}").then(Doc::hardline(0));
        }
        let mut doc = Doc::text("inject {");
        for sym in &self.symbols {
            doc = doc.then(sym_doc(sym, ctx, 1));
        }
        doc.then(Doc::hardline(0))
            .then(Doc::text("}"))
            .then(Doc::hardline(0))
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
