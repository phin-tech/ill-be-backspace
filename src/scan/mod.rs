//! Extracts comment blocks from source text.

mod state;

use crate::lang::LanguageSpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    /// `// like this` — a run of single-line comments.
    Line,
    /// `/* like this */`.
    Block,
    /// API documentation: `///`, `/** */`, or a Python docstring.
    Doc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentBlock {
    /// 1-indexed, inclusive. Spans blank lines absorbed into the block.
    pub start_line: u32,
    pub end_line: u32,
    /// Comment content with markers stripped, one entry per comment line.
    /// Blank lines absorbed by `merge_across_blank_lines` are not included, so
    /// `text.len()` is the number of lines that actually carry prose.
    pub text: Vec<String>,
    pub kind: CommentKind,
    /// Non-blank code lines following the block, up to the next blank line or
    /// comment. Input to the comment:code ratio rule.
    pub following_code_lines: u32,
    /// The text of those lines, so a rule can compare the comment's vocabulary
    /// against the code it describes.
    pub following_code: Vec<String>,
    /// Column of the first comment marker, 1-indexed. Used for reporting.
    pub column: u32,
}

impl CommentBlock {
    pub fn line_count(&self) -> usize {
        self.text.len()
    }

    pub fn joined(&self) -> String {
        self.text.join("\n")
    }

    /// True if any line of the block falls within `changed`.
    pub fn intersects(&self, changed: &std::collections::BTreeSet<u32>) -> bool {
        changed
            .range(self.start_line..=self.end_line)
            .next()
            .is_some()
    }
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Treat a blank line between two comment lines as part of one block. LLM
    /// comments use paragraph breaks, so splitting there would defeat the tool.
    pub merge_across_blank_lines: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            merge_across_blank_lines: true,
        }
    }
}

pub fn scan(source: &str, spec: &LanguageSpec, opts: &ScanOptions) -> Vec<CommentBlock> {
    state::scan_impl(source, spec, opts)
}
