//! The vocabulary of reasons.
//!
//! Whether a comment explains *why* cannot be measured directly — no local check
//! knows if a fact was derivable from the code. It can be approximated by
//! conjunction: a comment that draws its words from the code beneath it *and*
//! offers no reason is narrating. This module supplies the second half, the
//! reason. The first half is [`super::restate`].

/// Words and phrases that introduce a reason. Deliberately generous: every
/// marker present is a comment the rule leaves alone, so a broad list buys a low
/// false-positive rate at the cost of missing some narration.
const RATIONALE_MARKERS: &[&str] = &[
    "because",
    "since",
    // Bare `so`, not just `so that`: measurement found three real "why"
    // comments flagged only because they used `so the caller can …`. `and so
    // on` and `so far` will exempt a comment they should not, which is the
    // cheaper mistake.
    "so",
    "why",
    "in order to",
    "otherwise",
    "to avoid",
    "avoids",
    "avoid",
    "prevents",
    "prevent",
    "needed for",
    "needed to",
    "required for",
    "requires",
    "due to",
    "workaround",
    "historically",
    "must",
    "cannot",
    "can't",
    "would",
    "unless",
    "until",
    "ensures",
    "guarantees",
    "assumes",
    "relies on",
    "depends on",
    "upstream",
    "bug",
    "breaks",
    "fails",
    "deliberately",
    "intentionally",
    "on purpose",
];

/// The built-in marker list, as configuration sees it.
pub fn rationale_markers() -> Vec<String> {
    RATIONALE_MARKERS.iter().map(|s| s.to_string()).collect()
}

/// Whether a comment opens by naming the thing declared beneath it, the way
/// godoc requires and JSDoc encourages: `// NewFromConfig constructs …` above
/// `func NewFromConfig(…)`.
///
/// Go writes these with plain `//`, so no syntactic marker distinguishes them
/// from an ordinary comment. The convention itself is the marker. The opening
/// word must also *look* like an identifier — a capital, an underscore or a
/// digit somewhere in it — or `// retry the fetch` above `retry_count += 1`
/// would exempt itself, which is the narration this rule exists to catch.
pub fn opens_with_declared_name(text: &[String], code: &[String]) -> bool {
    let first_word = |s: &str| {
        s.split_whitespace()
            .next()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .filter(|w| w.len() > 2)
            .filter(|w| w.contains(|c: char| c.is_uppercase() || c == '_' || c.is_ascii_digit()))
            .map(str::to_string)
    };
    let Some(name) = text.first().and_then(|l| first_word(l)) else {
        return false;
    };
    let Some(decl) = code.first() else {
        return false;
    };
    decl.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|t| t == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_lowercase_and_unique() {
        // Matching is case-insensitive, so an uppercase entry would only be a
        // duplicate wearing a hat.
        let mut sorted = rationale_markers();
        sorted.sort();
        let len = sorted.len();
        sorted.dedup();
        assert_eq!(sorted.len(), len);
        assert!(sorted.iter().all(|m| m.to_lowercase() == *m));
    }
}
