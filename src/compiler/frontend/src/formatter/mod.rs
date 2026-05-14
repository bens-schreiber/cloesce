mod doc;

use std::cell::{Cell, RefCell};

use idl::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, ArgumentLiteral, Ast, AstBlockKind,
    DataSourceBlock, DataSourceBlockMethod, EnvBindingBlock, EnvBindingBlockKind, EnvBlock,
    ForeignBlock, ForeignBlockNav, ForeignQualifier, InjectBlock, Keyword, KvBlock, ModelBlock,
    ModelBlockKind, NavigationBlock, PaginatedBlockKind, ParsedIncludeTree, PlainOldObjectBlock,
    R2Block, ServiceBlock, Spd, SqlBlockKind, Symbol, Tag, fmt_cidl_type, lexer::CommentMap,
};
use doc::{Doc, render};

pub struct Formatter;

impl Formatter {
    pub fn format(ast: &Ast<'_>, comment_map: &CommentMap<'_>, src: &str) -> String {
        let ctx = FmtCtx::new(comment_map, src);
        let doc = ast.to_doc(&ctx);
        render(&doc).trim_start_matches('\n').to_string()
    }
}

struct FmtCtx<'src> {
    cm: &'src CommentMap<'src>,
    src: &'src str,

    /// Byte offset just past the last thing emitted
    cursor: Cell<usize>,

    /// Stack of enclosing spd end offsets.
    /// Necessary for determining inner vs trailing comments.
    node_ends: RefCell<Vec<usize>>,
}

impl<'src> FmtCtx<'src> {
    fn new(cm: &'src CommentMap<'src>, src: &'src str) -> Self {
        Self {
            cm,
            src,
            cursor: Cell::new(0),
            node_ends: RefCell::new(vec![]),
        }
    }

    /// Comments that appear before some node, e.g.
    /// ```cloesce
    /// // leading
    /// // comments
    /// model Foo {}
    /// ```
    fn leading_comments(&self, node_start: usize, indent: usize) -> (Doc<'src>, bool) {
        let prev = self.cursor.get();
        let lo = self.cm.entries.partition_point(|(off, _)| *off < prev);
        let mut doc = Doc::nil();
        let mut cursor = prev;
        let mut emitted = false;

        for &(offset, text) in &self.cm.entries[lo..] {
            if offset >= node_start {
                break;
            }
            let extra_blank = if self.trailing_gap(cursor, offset) >= 2 {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            let line_prefix = if emitted {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            doc = doc
                .then(extra_blank)
                .then(line_prefix)
                .then(Doc::owned(normalize_comment_text(text)));
            emitted = true;
            cursor = offset + text.len();
        }

        if node_start > cursor {
            cursor = node_start;
        }
        self.cursor.set(cursor);
        (doc, emitted)
    }

    /// Comment that appears directly after a node, e.g.
    /// ```cloesce
    /// model Foo {} // trailing comment
    /// ```
    fn trailing_comment(&self, node_end: usize) -> Doc<'src> {
        let min_offset = node_end.max(self.cursor.get());
        let lo = self
            .cm
            .entries
            .partition_point(|(off, _)| *off < min_offset);
        if let Some(&(offset, text)) = self.cm.entries.get(lo)
            && offset >= node_end
        {
            let gap = self.src.get(node_end..offset).unwrap_or("");

            // A trailing comment only belongs to this node when there is no
            // intervening syntax token between the node and the comment.
            if !gap.contains('\n') && gap.chars().all(char::is_whitespace) {
                self.cursor.set(offset + text.len());
                return Doc::text(" ").then(Doc::owned(normalize_comment_text(text)));
            }
        }
        Doc::nil()
    }

    /// Comments that do not lead a node but are in between it's ending, e.g.
    /// ```cloesce
    /// model Foo {
    ///     id: int
    ///     // inner comment
    /// }
    /// ```
    fn inner_comments(&self, indent: usize) -> Doc<'src> {
        let Some(limit) = self.node_ends.borrow().last().copied() else {
            return Doc::nil();
        };

        let prev = self.cursor.get();
        let lo = self.cm.entries.partition_point(|(off, _)| *off < prev);
        let mut doc = Doc::nil();
        let mut cursor = prev;

        for &(offset, text) in &self.cm.entries[lo..] {
            if offset >= limit {
                break;
            }

            let extra_blank = if self.trailing_gap(cursor, offset) >= 2 {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            doc = doc
                .then(extra_blank)
                .then(Doc::hardline(indent))
                .then(Doc::owned(normalize_comment_text(text)));
            cursor = offset + text.len();
        }

        self.cursor.set(cursor);
        doc
    }

    /// Advance the cursor to at least `pos`.
    fn advance(&self, pos: usize) {
        let current = self.cursor.get();
        if pos <= current {
            return;
        }

        let lo = self.cm.entries.partition_point(|(off, _)| *off < current);
        if let Some(&(next_comment_offset, _)) = self.cm.entries.get(lo)
            && next_comment_offset < pos
        {
            return;
        }

        self.cursor.set(pos);
    }

    /// Wraps `inner` in a `{ ... }` block at the given inner indent depth.
    /// Emits ` {`, the inner doc, any inner comments, a hardline at `depth - 1`, then `}`.
    fn block(&self, inner: Doc<'src>, inner_depth: usize) -> Doc<'src> {
        Doc::text(" {")
            .then(inner)
            .then(self.inner_comments(inner_depth))
            .then(Doc::hardline(inner_depth.saturating_sub(1)))
            .then(Doc::text("}"))
    }

    fn gap(&self, from: usize, to: usize) -> usize {
        let text = self.src.get(from..to).unwrap_or("");
        text.chars().filter(|&c| c == '\n').count()
    }

    /// Counts newlines in the trailing whitespace of `src[from..to]`.
    /// Returns >= 2 only when there is a blank line immediately before `to`.
    fn trailing_gap(&self, from: usize, to: usize) -> usize {
        let text = self.src.get(from..to).unwrap_or("");
        text.chars()
            .rev()
            .take_while(|c| c.is_whitespace())
            .filter(|&c| c == '\n')
            .count()
    }

    fn spd_doc<T: ToDoc<'src>>(&self, spd: &'src Spd<T>, indent: usize, inline: bool) -> Doc<'src> {
        let gap = self.gap(self.cursor.get(), spd.span.start);
        let (leading, has_leading_comments) = self.leading_comments(spd.span.start, indent);

        self.node_ends.borrow_mut().push(spd.span.end);
        let content = spd.inner.to_doc(self);
        self.node_ends.borrow_mut().pop();

        let trailing = self.trailing_comment(spd.span.end);
        self.advance(spd.span.end);

        let content_sep = if has_leading_comments {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        if inline {
            let leading_sep = if has_leading_comments {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            return leading_sep
                .then(leading)
                .then(content_sep)
                .then(content)
                .then(trailing);
        }

        // Preserve newlines
        let extra_blank = if gap >= 2 && !has_leading_comments {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        extra_blank
            .then(Doc::hardline(indent))
            .then(leading)
            .then(content_sep)
            .then(content)
            .then(trailing)
    }

    fn _sym_doc(
        &self,
        sym: &'src Symbol<'src>,
        keyword: Option<Keyword>,
        indent: usize,
        inline: bool,
    ) -> Doc<'src> {
        // Tags
        let mut tags_doc = Doc::nil();
        for (idx, tag) in sym.tags.iter().enumerate() {
            let (tag_leading, tag_has_leading) = self.leading_comments(tag.span.start, indent);
            self.node_ends.borrow_mut().push(tag.span.end);
            let tag_content = tag.inner.to_doc(self);
            self.node_ends.borrow_mut().pop();
            let tag_trailing = self.trailing_comment(tag.span.end);
            self.advance(tag.span.end);
            let tag_sep = if tag_has_leading {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            let next_start = sym
                .tags
                .get(idx + 1)
                .map(|t| t.span.start)
                .unwrap_or(sym.span.start);
            let post_sep = if self.gap(tag.span.end, next_start) > 0 {
                Doc::hardline(indent)
            } else {
                Doc::text(" ")
            };

            tags_doc = tags_doc
                .then(tag_leading)
                .then(tag_sep)
                .then(tag_content)
                .then(tag_trailing)
                .then(post_sep);
        }

        let is_top_decl = keyword.is_some();
        let (leading, has_leading_comments) = self.leading_comments(sym.span.start, indent);
        let content = if matches!(sym.cidl_type, CidlType::Void) {
            keyword.map_or_else(
                || Doc::text(sym.name),
                |kw| Doc::kw(kw).then(Doc::text(" ")).then(Doc::text(sym.name)),
            )
        } else {
            Doc::text(sym.name)
                .then(Doc::text(": "))
                .then(Doc::owned(fmt_cidl_type(&sym.cidl_type)))
        };

        let trailing = self.trailing_comment(sym.span.end);
        self.advance(sym.span.end);

        let content_sep = if has_leading_comments {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        let pre_leading = if sym.tags.is_empty() && !is_top_decl {
            Doc::hardline(indent)
        } else {
            Doc::nil()
        };

        if inline {
            let leading_sep = if has_leading_comments && sym.tags.is_empty() {
                Doc::hardline(indent)
            } else {
                Doc::nil()
            };

            return tags_doc
                .then(leading_sep)
                .then(leading)
                .then(content_sep)
                .then(content)
                .then(trailing);
        }

        tags_doc
            .then(pre_leading)
            .then(leading)
            .then(content_sep)
            .then(content)
            .then(trailing)
    }

    /// Emits a top level declaration as a [Doc], including any leading tags or comments.
    fn top_decl_doc(&self, sym: &'src Symbol<'src>, keyword: Keyword) -> Doc<'src> {
        self._sym_doc(sym, Some(keyword), 0, false)
    }

    /// Emits a symbol as a [Doc], including any leading tags or comments.
    fn sym_doc(&self, sym: &'src Symbol<'src>, indent: usize, inline: bool) -> Doc<'src> {
        self._sym_doc(sym, None, indent, inline)
    }
}

fn normalize_comment_text(text: &str) -> String {
    if let Some(content) = text.strip_prefix("//") {
        if content.is_empty() || content.starts_with(char::is_whitespace) {
            text.to_string()
        } else {
            format!("// {}", content)
        }
    } else {
        text.to_string()
    }
}

trait ToDoc<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src>;
}

impl<'src> Ast<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut doc = Doc::nil();

        for spd in &self.blocks {
            doc = doc.then(ctx.spd_doc(spd, 0, false));
        }

        // Consume any dangling comments
        let (trailing_comments, has_trailing_comments) = ctx.leading_comments(usize::MAX, 0);
        if has_trailing_comments {
            doc = doc.then(trailing_comments);
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
        let doc = ctx.top_decl_doc(&self.symbol, Keyword::Model);

        if self.blocks.is_empty() {
            // No content, return empty model
            return doc.then(Doc::text(" {}"));
        }

        let mut inner = Doc::nil();
        for spd in &self.blocks {
            inner = inner.then(ctx.spd_doc(spd, 1, false));
        }
        doc.then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for Tag<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let inner = match self {
            Tag::Validator { name, argument } => {
                let arg = match argument {
                    ArgumentLiteral::Int(s) | ArgumentLiteral::Real(s) => Doc::text(s),
                    ArgumentLiteral::Str(s) => {
                        Doc::text("\"").then(Doc::text(s)).then(Doc::text("\""))
                    }
                    ArgumentLiteral::Regex(s) => {
                        Doc::text("/").then(Doc::text(s)).then(Doc::text("/"))
                    }
                };

                Doc::text(name.as_str()).then(Doc::text(" ")).then(arg)
            }
            Tag::Use { binding } => Doc::kw(Keyword::Use)
                .then(Doc::text(" "))
                .then(Doc::text(binding.inner)),

            Tag::Source { name } => Doc::kw(Keyword::Source)
                .then(Doc::text(" "))
                .then(Doc::text(name.inner)),
            Tag::Internal => Doc::kw(Keyword::Internal),
            Tag::Instance => Doc::kw(Keyword::Instance),

            Tag::Crud { kinds } => Doc::kw(Keyword::Crud)
                .then(Doc::text(" "))
                .then(comma_separated(kinds, |kind| ctx.spd_doc(kind, 0, true))),
            Tag::Inject { bindings: symbols } => Doc::kw(Keyword::Inject)
                .then(Doc::text(" "))
                .then(comma_separated(symbols, |sym| ctx.sym_doc(sym, 0, true))),
        };

        Doc::text("[").then(inner).then(Doc::text("]"))
    }
}

impl<'src> ToDoc<'src> for CrudKind {
    fn to_doc(&'src self, _ctx: &FmtCtx<'src>) -> Doc<'src> {
        let kw = match self {
            CrudKind::Get => Keyword::Get,
            CrudKind::List => Keyword::List,
            CrudKind::Save => Keyword::Save,
        };
        Doc::text(kw.as_str())
    }
}

impl<'src> ToDoc<'src> for ModelBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        fn model_block<'src, T: ToDoc<'src>>(
            keyword: &'static str,
            blocks: &'src [Spd<T>],
            ctx: &FmtCtx<'src>,
        ) -> Doc<'src> {
            let mut inner = Doc::nil();
            for block in blocks {
                inner = inner.then(ctx.spd_doc(block, 2, false));
            }
            Doc::text(keyword).then(ctx.block(inner, 2))
        }

        match self {
            ModelBlockKind::Column(sym) => ctx.sym_doc(sym, 1, true),
            ModelBlockKind::Foreign(fb) => fb.to_doc(ctx),
            ModelBlockKind::Navigation(nb) => nb.to_doc(ctx),
            ModelBlockKind::Kv(kv) => kv.to_doc(ctx),
            ModelBlockKind::R2(r2) => r2.to_doc(ctx),
            ModelBlockKind::Primary(blocks) => model_block(Keyword::Primary.as_str(), blocks, ctx),
            ModelBlockKind::Unique(blocks) => model_block(Keyword::Unique.as_str(), blocks, ctx),
            ModelBlockKind::Optional(blocks) => {
                model_block(Keyword::Optional.as_str(), blocks, ctx)
            }
            ModelBlockKind::Paginated(blocks) => {
                model_block(Keyword::Paginated.as_str(), blocks, ctx)
            }
            ModelBlockKind::KeyField(syms) => {
                let mut inner = Doc::nil();
                for sym in syms {
                    inner = inner.then(ctx.sym_doc(sym, 2, false));
                }
                Doc::kw(Keyword::KeyField).then(ctx.block(inner, 2))
            }
        }
    }
}

impl<'src> ToDoc<'src> for SqlBlockKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            SqlBlockKind::Column(sym) => ctx.sym_doc(sym, 2, true),
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

        let doc = Doc::kw(Keyword::Foreign)
            .then(Doc::text(" ("))
            .then(adjs)
            .then(Doc::text(")"));
        let qualifier = match &self.qualifier {
            Some(ForeignQualifier::Primary) => Doc::text(" ").then(Doc::kw(Keyword::Primary)),
            Some(ForeignQualifier::Optional) => Doc::text(" ").then(Doc::kw(Keyword::Optional)),
            Some(ForeignQualifier::Unique) => Doc::text(" ").then(Doc::kw(Keyword::Unique)),
            None => Doc::nil(),
        };

        let mut inner = Doc::nil();
        for field in &self.fields {
            inner = inner.then(ctx.sym_doc(field, 2, false));
        }

        if let Some(nav) = &self.nav {
            inner = inner.then(ctx.spd_doc(nav, 2, false));
        }

        doc.then(qualifier).then(ctx.block(inner, 2))
    }
}

impl<'src> ToDoc<'src> for NavigationBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let adjs = comma_separated(&self.adj, |adj| {
            let left = ctx.sym_doc(&adj.0, 0, true);
            let right = ctx.sym_doc(&adj.1, 0, true);
            left.then(Doc::text("::")).then(right)
        });

        Doc::kw(Keyword::Nav)
            .then(Doc::text(" ("))
            .then(adjs)
            .then(Doc::text(")"))
            .then(ctx.block(ctx.sym_doc(&self.nav.inner, 2, false), 2))
    }
}

impl<'src> ToDoc<'src> for KvBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" ").then(Doc::kw(Keyword::Paginated))
        } else {
            Doc::nil()
        };
        Doc::kw(Keyword::Kv)
            .then(Doc::text(" ("))
            .then(ctx.sym_doc(&self.env_binding, 0, true))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(ctx.block(ctx.sym_doc(&self.field, 2, false), 2))
    }
}

impl<'src> ToDoc<'src> for R2Block<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let paginated = if self.is_paginated {
            Doc::text(" ").then(Doc::kw(Keyword::Paginated))
        } else {
            Doc::nil()
        };
        Doc::kw(Keyword::R2)
            .then(Doc::text(" ("))
            .then(ctx.sym_doc(&self.env_binding, 0, true))
            .then(Doc::text(", \""))
            .then(Doc::text(self.key_format))
            .then(Doc::text("\")"))
            .then(paginated)
            .then(ctx.block(ctx.sym_doc(&self.field, 2, false), 2))
    }
}

impl<'src> ToDoc<'src> for ApiBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let doc = ctx.top_decl_doc(&self.symbol, Keyword::Api);

        if self.methods.is_empty() {
            return doc.then(Doc::text(" {}"));
        }

        let mut inner = Doc::nil();
        for spd in &self.methods {
            inner = inner.then(ctx.spd_doc(spd, 1, false));
        }
        doc.then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for ApiBlockMethodParamKind<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        match self {
            ApiBlockMethodParamKind::SelfParam(sym) => ctx.sym_doc(sym, 0, true),
            ApiBlockMethodParamKind::Param(sym) => ctx.sym_doc(sym, 0, true),
        }
    }
}

impl<'src> ToDoc<'src> for ApiBlockMethod<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        // Method-level tags precede the verb so they can be parsed back
        // (e.g. `[inject db] post foo(...)`).
        let mut tags_doc = Doc::nil();
        for tag in &self.symbol.tags {
            ctx.node_ends.borrow_mut().push(tag.span.end);
            let tag_content = tag.inner.to_doc(ctx);
            ctx.node_ends.borrow_mut().pop();
            ctx.advance(tag.span.end);

            let post_sep = if ctx.gap(tag.span.end, self.symbol.span.start) > 0 {
                Doc::hardline(1)
            } else {
                Doc::text(" ")
            };
            tags_doc = tags_doc.then(tag_content).then(post_sep);
        }

        let params = comma_separated(&self.parameters, |param| ctx.spd_doc(param, 0, true));

        tags_doc
            .then(Doc::text(match &self.http_verb {
                HttpVerb::Get => Keyword::Get.as_str(),
                HttpVerb::Post => Keyword::Post.as_str(),
                HttpVerb::Put => Keyword::Put.as_str(),
                HttpVerb::Delete => Keyword::Delete.as_str(),
                HttpVerb::Patch => Keyword::Patch.as_str(),
            }))
            .then(Doc::text(" "))
            .then(Doc::text(self.symbol.name))
            .then(Doc::text("("))
            .then(params)
            .then(Doc::text(") -> "))
            .then(Doc::owned(fmt_cidl_type(&self.return_type)))
    }
}

impl<'src> ToDoc<'src> for DataSourceBlockMethod<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let params = comma_separated(&self.parameters, |param| ctx.sym_doc(param, 0, true));

        let sql = Doc::hardline(2)
            .then(Doc::text("\""))
            .then(Doc::text(self.raw_sql))
            .then(Doc::text("\""));
        Doc::text("(")
            .then(params)
            .then(Doc::text(")"))
            .then(ctx.block(sql, 2))
    }
}

impl<'src> ToDoc<'src> for DataSourceBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let source_doc = ctx
            .top_decl_doc(&self.symbol, Keyword::Source)
            .then(Doc::text(" "))
            .then(Doc::kw(Keyword::For))
            .then(Doc::text(" "))
            .then(ctx.sym_doc(&self.model, 0, true));

        let mut include = if self.tree.0.is_empty() {
            Doc::hardline(1)
                .then(Doc::kw(Keyword::Include))
                .then(Doc::text(" {}"))
        } else {
            Doc::hardline(1)
                .then(Doc::kw(Keyword::Include))
                .then(ctx.block(self.tree.to_doc_at(ctx, 2), 2))
        };

        if let Some(get) = &self.get {
            include = include
                .then(Doc::hardline(1))
                .then(Doc::hardline(1))
                .then(Doc::kw(Keyword::Sql))
                .then(Doc::text(" "))
                .then(Doc::kw(Keyword::Get))
                .then(ctx.spd_doc(get, 1, true));
        }
        if let Some(list) = &self.list {
            include = include
                .then(Doc::hardline(1))
                .then(Doc::hardline(1))
                .then(Doc::kw(Keyword::Sql))
                .then(Doc::text(" "))
                .then(Doc::kw(Keyword::List))
                .then(ctx.spd_doc(list, 1, true));
        }

        source_doc.then(ctx.block(include, 1))
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
                .then(ctx.block(subtree.to_doc_at(ctx, depth + 1), depth + 1));
        }
        doc
    }
}

impl<'src> ToDoc<'src> for ServiceBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        if self.symbols.is_empty() {
            return Doc::kw(Keyword::Service).then(Doc::text(" {}"));
        }

        let mut inner = Doc::nil();
        for symbol in &self.symbols {
            inner = inner.then(ctx.sym_doc(symbol, 1, false));
        }
        Doc::kw(Keyword::Service).then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for PlainOldObjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let doc = ctx.top_decl_doc(&self.symbol, Keyword::Poo);

        if self.fields.is_empty() {
            return doc.then(Doc::text(" {}"));
        }

        let mut inner = Doc::nil();
        for field in &self.fields {
            inner = inner.then(ctx.sym_doc(field, 1, false));
        }
        doc.then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for EnvBindingBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let keyword = match self.kind {
            EnvBindingBlockKind::D1 => Keyword::D1.as_str(),
            EnvBindingBlockKind::R2 => Keyword::R2.as_str(),
            EnvBindingBlockKind::Kv => Keyword::Kv.as_str(),
            EnvBindingBlockKind::Var => Keyword::Vars.as_str(),
        };

        let mut inner = Doc::nil();
        for symbol in &self.symbols {
            inner = inner.then(ctx.sym_doc(symbol, 2, false));
        }

        Doc::text(keyword).then(ctx.block(inner, 2))
    }
}

impl<'src> ToDoc<'src> for EnvBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        let mut inner = Doc::nil();
        for spd in &self.blocks {
            inner = inner.then(ctx.spd_doc(spd, 1, false));
        }
        Doc::kw(Keyword::Env).then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for InjectBlock<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        if self.symbols.is_empty() {
            return Doc::kw(Keyword::Inject).then(Doc::text(" {}"));
        }
        let mut inner = Doc::nil();
        for sym in &self.symbols {
            inner = inner.then(ctx.sym_doc(sym, 1, false));
        }
        Doc::kw(Keyword::Inject).then(ctx.block(inner, 1))
    }
}

impl<'src> ToDoc<'src> for ForeignBlockNav<'src> {
    fn to_doc(&'src self, ctx: &FmtCtx<'src>) -> Doc<'src> {
        Doc::kw(Keyword::Nav).then(ctx.block(ctx.sym_doc(&self.symbol, 3, false), 3))
    }
}

fn comma_separated<'src, T, F: FnMut(&'src T) -> Doc<'src>>(
    items: &'src [T],
    mut item_doc: F,
) -> Doc<'src> {
    let mut doc = Doc::nil();
    for (idx, item) in items.iter().enumerate() {
        doc = doc.then(item_doc(item));
        if idx + 1 < items.len() {
            doc = doc.then(Doc::text(", "));
        }
    }
    doc
}
