/// Document IR for canonical formatting
pub enum Doc<'src> {
    Nil,

    Text(&'src str),
    OwnedText(String),

    /// A mandatory newline followed by `depth` levels of indentation.
    HardLine {
        depth: usize,
    },

    /// Two documents in sequence.
    Concat(Box<Doc<'src>>, Box<Doc<'src>>),
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

    pub fn then(self, other: Doc<'src>) -> Doc<'src> {
        match (&self, &other) {
            (Doc::Nil, _) => other,
            (_, Doc::Nil) => self,
            _ => Doc::Concat(Box::new(self), Box::new(other)),
        }
    }
}

/// Render a `Doc` to a `String`.
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
        Doc::Concat(a, b) => {
            render_into(a, out);
            render_into(b, out);
        }
    }
}
