/// Document IR for canonical formatting
pub enum Doc<'src> {
    Nil,

    Text(&'src str),
    OwnedText(String),

    /// A mandatory newline followed by `depth` levels of indentation.
    HardLine {
        depth: usize,
    },

    Seq(Vec<Doc<'src>>),
}

impl<'src> Doc<'src> {
    pub fn text(s: &'src str) -> Self {
        Doc::Text(s)
    }

    pub fn owned(s: String) -> Self {
        Doc::OwnedText(s)
    }

    pub fn nil() -> Self {
        Doc::Nil
    }

    pub fn hardline(depth: usize) -> Self {
        Doc::HardLine { depth }
    }

    // TODO: revisit this if it ends up being a noticeable bottleneck.
    pub fn then(self, other: Doc<'src>) -> Doc<'src> {
        match (self, other) {
            (Doc::Nil, rhs) => rhs,
            (lhs, Doc::Nil) => lhs,

            (Doc::Seq(mut lhs), Doc::Seq(mut rhs)) => {
                lhs.append(&mut rhs);
                Doc::Seq(lhs)
            }
            (Doc::Seq(mut lhs), rhs) => {
                lhs.push(rhs);
                Doc::Seq(lhs)
            }
            (lhs, Doc::Seq(mut rhs)) => {
                let mut docs = Vec::with_capacity(1 + rhs.len());
                docs.push(lhs);
                docs.append(&mut rhs);
                Doc::Seq(docs)
            }

            (lhs, rhs) => Doc::Seq(vec![lhs, rhs]),
        }
    }
}

pub fn render(doc: &Doc<'_>) -> String {
    let mut out = String::new();
    render_into(doc, &mut out);
    out
}

fn render_into(doc: &Doc<'_>, out: &mut String) {
    match doc {
        Doc::Nil => {}
        Doc::Text(s) => out.push_str(s),
        Doc::OwnedText(s) => out.push_str(s),
        Doc::HardLine { depth } => {
            out.push('\n');
            for _ in 0..*depth {
                out.push_str("    ");
            }
        }
        Doc::Seq(docs) => {
            for doc in docs {
                render_into(doc, out);
            }
        }
    }
}
