//! Inline `backspace:` directives.
//!
//! Matched against comment text with markers already stripped, so the same syntax
//! works in every language without per-language parsing.

use std::sync::OnceLock;

use regex::Regex;

/// Directives are only honoured this close to the top of a file, so a stray
/// `ignore-file` deep in a vendored blob cannot silence everything above it.
pub const IGNORE_FILE_MAX_LINE: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directive {
    pub scope: Scope,
    /// Empty means "every rule".
    pub rules: Vec<String>,
    /// Free text after the directive, used when a reason is required.
    pub reason: Option<String>,
    /// Index into the block's text lines, so the directive can be excluded from
    /// length budgets.
    pub line_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Block,
    File,
}

fn directive_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)backspace:\s*(ignore-file|ignore)\s*(?:\[([^\]]*)\])?\s*(.*)$").unwrap()
    })
}

/// Extracts every directive in a block's comment text.
pub fn parse(text: &[String]) -> Vec<Directive> {
    text.iter()
        .enumerate()
        .filter_map(|(i, line)| parse_line(line, i))
        .collect()
}

fn parse_line(line: &str, line_index: usize) -> Option<Directive> {
    let caps = directive_re().captures(line)?;
    let scope = if caps[1].eq_ignore_ascii_case("ignore-file") {
        Scope::File
    } else {
        Scope::Block
    };
    let rules = caps
        .get(2)
        .map(|m| {
            m.as_str()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // A reason may be introduced by a dash, em dash or colon; none of those on
    // their own count as justification.
    let reason = caps
        .get(3)
        .map(|m| {
            m.as_str().trim_matches(|c: char| {
                c.is_whitespace() || c == '-' || c == '\u{2014}' || c == '\u{2013}' || c == ':'
            })
        })
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Some(Directive {
        scope,
        rules,
        reason,
        line_index,
    })
}

impl Directive {
    pub fn covers(&self, rule: &str) -> bool {
        self.rules.is_empty() || self.rules.iter().any(|r| r == rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(line: &str) -> Directive {
        parse(&[line.to_string()])
            .pop()
            .expect("no directive found")
    }

    #[test]
    fn parses_a_bare_ignore() {
        let d = one("backspace: ignore");
        assert_eq!(d.scope, Scope::Block);
        assert!(d.rules.is_empty());
        assert!(d.reason.is_none());
    }

    #[test]
    fn parses_a_targeted_ignore() {
        assert_eq!(
            one("backspace: ignore[block-too-long]").rules,
            ["block-too-long"]
        );
    }

    #[test]
    fn parses_several_rules() {
        assert_eq!(
            one("backspace: ignore[block-too-long, comment-code-ratio]").rules,
            ["block-too-long", "comment-code-ratio"]
        );
    }

    #[test]
    fn parses_a_file_scope_directive() {
        assert_eq!(one("backspace: ignore-file").scope, Scope::File);
    }

    #[test]
    fn captures_a_reason_after_punctuation() {
        assert_eq!(
            one("backspace: ignore — the wire format demands it")
                .reason
                .as_deref(),
            Some("the wire format demands it")
        );
    }

    #[test]
    fn punctuation_alone_is_not_a_reason() {
        assert!(one("backspace: ignore --").reason.is_none());
    }

    #[test]
    fn finds_a_directive_mid_sentence() {
        assert!(parse(&["see below, backspace: ignore".to_string()]).len() == 1);
    }

    #[test]
    fn ordinary_prose_is_not_a_directive() {
        assert!(parse(&["this is about the backspace key".to_string()]).is_empty());
    }

    #[test]
    fn a_bare_ignore_covers_every_rule() {
        assert!(one("backspace: ignore").covers("anything"));
    }

    #[test]
    fn a_targeted_ignore_covers_only_its_rule() {
        let d = one("backspace: ignore[block-too-long]");
        assert!(d.covers("block-too-long"));
        assert!(!d.covers("comment-code-ratio"));
    }
}
