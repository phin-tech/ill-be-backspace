//! Declarative language definitions.
//!
//! Both the built-in `languages/*.toml` files and user-supplied languages in
//! `.backspace.toml` deserialize into [`LanguageSpec`], so adding a language never
//! requires a new release.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageSpec {
    pub name: String,

    /// Matched against the file extension, with the leading dot (`".rs"`).
    #[serde(default)]
    pub extensions: Vec<String>,

    /// Matched against the whole file name, for extensionless files like `Makefile`.
    #[serde(default)]
    pub filenames: Vec<String>,

    /// Matched as a substring of a `#!` line, for extensionless scripts.
    #[serde(default)]
    pub shebangs: Vec<String>,

    #[serde(default)]
    pub line_comments: Vec<String>,

    #[serde(default)]
    pub block_comments: Vec<BlockComment>,

    /// Prefixes that mark a comment as API documentation rather than an aside.
    /// Matched against the comment lexeme including its opening marker, so `///`
    /// and `//!` classify correctly without competing with `//` for the match.
    #[serde(default)]
    pub doc_markers: Vec<String>,

    #[serde(default)]
    pub strings: Vec<StringSpec>,

    /// Whether a string literal in the first-statement position counts as a doc
    /// comment. Python-style only; every other language uses `doc_markers`.
    #[serde(default)]
    pub docstrings: DocstringStyle,

    /// Enables the `/`-is-maybe-a-regex disambiguation the JS family needs.
    #[serde(default)]
    pub regex_literals: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockComment {
    pub open: String,
    pub close: String,
    /// Rust's `/* /* */ */` nests; the C family's does not.
    #[serde(default)]
    pub nested: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StringSpec {
    /// Opening delimiter. Longer delimiters win, so `"""` is preferred over `"`.
    pub delim: String,
    /// Closing delimiter, when it differs from the opener (`r#"` closes with `"#`).
    #[serde(default)]
    pub close: Option<String>,
    /// Raw strings ignore the escape character.
    #[serde(default)]
    pub raw: bool,
    /// Non-multiline strings are terminated by a newline as well as by their closer,
    /// which keeps one unbalanced quote from swallowing the rest of the file.
    #[serde(default)]
    pub multiline: bool,
    #[serde(default)]
    pub escape: Option<String>,
}

impl StringSpec {
    pub fn closer(&self) -> &str {
        self.close.as_deref().unwrap_or(&self.delim)
    }

    pub fn escape_char(&self) -> Option<char> {
        if self.raw {
            return None;
        }
        self.escape.as_ref().and_then(|e| e.chars().next())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocstringStyle {
    #[default]
    None,
    Python,
}

impl LanguageSpec {
    /// Sorts every delimiter list longest-first so that prefix matching in the
    /// scanner is unambiguous, and validates that the spec can actually match
    /// something.
    pub fn normalize(mut self) -> Result<Self, String> {
        if self.line_comments.is_empty()
            && self.block_comments.is_empty()
            && self.docstrings == DocstringStyle::None
        {
            return Err(format!(
                "language `{}` defines no comment syntax",
                self.name
            ));
        }
        if let Some(s) = self.strings.iter().find(|s| s.delim.is_empty()) {
            return Err(format!(
                "language `{}` has a string with an empty delimiter: {s:?}",
                self.name
            ));
        }

        self.line_comments
            .sort_by_key(|m| std::cmp::Reverse(m.len()));
        self.doc_markers.sort_by_key(|m| std::cmp::Reverse(m.len()));
        self.block_comments
            .sort_by_key(|b| std::cmp::Reverse(b.open.len()));
        self.strings
            .sort_by_key(|s| std::cmp::Reverse(s.delim.len()));

        for ext in &mut self.extensions {
            if !ext.starts_with('.') {
                ext.insert(0, '.');
            }
            *ext = ext.to_ascii_lowercase();
        }
        Ok(self)
    }

    /// True if the comment lexeme (including its opening marker) is documentation.
    pub fn is_doc_comment(&self, lexeme: &str) -> bool {
        self.doc_markers
            .iter()
            .any(|m| lexeme.starts_with(m.as_str()))
    }
}
