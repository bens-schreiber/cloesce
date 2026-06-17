//! A trie for detecting overlapping binding key formats within a single
//! namespace.
//!
//! Two key formats overlap when a prefix list query over one would return keys
//! produced by the other. We model each format as its literal prefix (the run
//! of bytes before the first `{` placeholder) followed by either a terminal
//! wildcard (if a placeholder is present) or a literal end.

use frontend::Symbol;

enum Terminal<'src, 'p> {
    /// A placeholder was reached here; everything beyond is a wildcard.
    ///
    /// e.g. `foo/{id}` and `foo/{name}` both reach the same node, which is a wildcard
    Wildcard(&'p Symbol<'src>),

    /// A literal end was reached here; only this exact key is claimed.
    End(&'p Symbol<'src>),
}

impl<'src, 'p> Terminal<'src, 'p> {
    fn symbol(&self) -> &'p Symbol<'src> {
        match self {
            Terminal::Wildcard(s) | Terminal::End(s) => s,
        }
    }
}

/// A node in the [PrefixTrie]: literal child edges plus an optional terminal.
struct PrefixTrieNode<'src, 'p> {
    children: Vec<(u8, PrefixTrieNode<'src, 'p>)>,
    terminal: Option<Terminal<'src, 'p>>,
}

impl<'src, 'p> PrefixTrieNode<'src, 'p> {
    fn new() -> Self {
        Self {
            children: Vec::new(),
            terminal: None,
        }
    }

    fn get_or_insert(&mut self, byte: u8) -> &mut PrefixTrieNode<'src, 'p> {
        let idx = match self.children.iter().position(|(b, _)| *b == byte) {
            Some(idx) => idx,
            None => {
                self.children.push((byte, PrefixTrieNode::new()));
                self.children.len() - 1
            }
        };
        &mut self.children[idx].1
    }
}

pub struct PrefixTrie<'src, 'p> {
    root: PrefixTrieNode<'src, 'p>,
}

impl<'src, 'p> PrefixTrie<'src, 'p> {
    pub fn new() -> Self {
        Self {
            root: PrefixTrieNode::new(),
        }
    }

    /// Inserts `key_format` into the trie, attributing it to `symbol`.
    ///
    /// Returns [Some] if the insertion overlaps with some other symbol.
    pub fn insert(
        &mut self,
        key_format: &'src str,
        symbol: &'p Symbol<'src>,
    ) -> Option<&'p Symbol<'src>> {
        let wildcard = key_format.find('{');

        // All bytes up to the first placeholder (or full str if none)
        let bytes = &key_format.as_bytes()[..wildcard.unwrap_or(key_format.len())];

        // Traverse the trie
        let mut node = &mut self.root;
        for &byte in bytes {
            if let Some(Terminal::Wildcard(first)) = &node.terminal {
                // A wildcard always indicates an overlap, because it claims everything below it.
                return Some(first);
            }

            node = node.get_or_insert(byte);
        }

        match wildcard {
            Some(_) => {
                // Wildcard; any existing terminal at this node is an overlap.
                if let Some(first) = node.terminal.as_ref().map(|t| t.symbol()) {
                    return Some(first);
                }

                // Any existing child edge is also an overlap.
                if let Some((_, child)) = node.children.first() {
                    return Some(find_symbol(child));
                }

                node.terminal = Some(Terminal::Wildcard(symbol));
            }

            None => {
                // Literal format; only an existing terminal at this exact node overlaps.
                if let Some(first) = node.terminal.as_ref().map(|t| t.symbol()) {
                    return Some(first);
                }
                node.terminal = Some(Terminal::End(symbol));
            }
        }

        None
    }
}

impl Default for PrefixTrie<'_, '_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Traverses until a terminal is found
fn find_symbol<'src, 'p>(node: &PrefixTrieNode<'src, 'p>) -> &'p Symbol<'src> {
    if let Some(t) = &node.terminal {
        return t.symbol();
    }
    find_symbol(&node.children.first().expect("branch is non-empty").1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_symbol(name: &str) -> Symbol<'_> {
        Symbol {
            name,
            ..Default::default()
        }
    }

    /// Returns the name of the first format that overlaps,
    /// or [None] if all formats are distinct.
    fn first_overlap(formats: &[(&'static str, &'static str)]) -> Option<&'static str> {
        let symbols = formats
            .iter()
            .map(|(name, _)| mock_symbol(name))
            .collect::<Vec<_>>();

        let mut trie = PrefixTrie::new();
        for (i, (_, key_format)) in formats.iter().enumerate() {
            if let Some(first) = trie.insert(key_format, &symbols[i]) {
                return Some(first.name);
            }
        }
        None
    }

    #[test]
    fn identical_literals_overlap() {
        assert_eq!(first_overlap(&[("a", "foo"), ("b", "foo")]), Some("a"));
    }

    #[test]
    fn wildcards_with_same_prefix_overlap() {
        assert_eq!(
            first_overlap(&[("a", "foo/{id}"), ("b", "foo/{name}")]),
            Some("a")
        );
    }

    #[test]
    fn wildcard_contains_longer_wildcard() {
        assert_eq!(
            first_overlap(&[("a", "foo/{id}"), ("b", "foo/{id}/bar")]),
            Some("a")
        );
    }

    #[test]
    fn wildcard_contains_literal() {
        assert_eq!(
            first_overlap(&[("a", "foo/{id}"), ("b", "foo/bar")]),
            Some("a")
        );
    }

    #[test]
    fn literal_under_wildcard_overlaps_regardless_of_order() {
        assert_eq!(
            first_overlap(&[("a", "foo/bar"), ("b", "foo/{id}")]),
            Some("a")
        );
    }

    #[test]
    fn literal_and_deeper_wildcard_do_not_overlap() {
        assert_eq!(first_overlap(&[("a", "foo"), ("b", "foo/{id}")]), None);

        // ...reversed
        assert_eq!(first_overlap(&[("a", "foo/{id}"), ("b", "foo")]), None);
    }

    #[test]
    fn distinct_prefixes_do_not_overlap() {
        assert_eq!(
            first_overlap(&[
                ("a", "foo/{id}"),
                ("b", "bar/{id}"),
                ("c", "baz"),
                ("d", "qux/{id}/extra"),
            ]),
            None
        );
    }

    #[test]
    fn empty_prefix_wildcards_overlap() {
        assert_eq!(first_overlap(&[("a", "{id}"), ("b", "{other}")]), Some("a"));
    }

    #[test]
    fn shared_literal_prefix_then_diverging_branches_ok() {
        assert_eq!(first_overlap(&[("a", "foo/a"), ("b", "foo/b")]), None);
    }

    #[test]
    fn reports_the_earlier_of_three_on_overlap() {
        assert_eq!(
            first_overlap(&[("a", "foo/{id}"), ("b", "bar/{id}"), ("c", "foo/{x}")]),
            Some("a")
        );
    }
}
