//! Serde types for `.backspace.toml` and the package-manager equivalents.

use std::collections::HashMap;

use serde::Deserialize;

use crate::config::Severity;
use crate::lang::LanguageSpec;

/// Every key that a language or path override may set. Kept separate from
/// [`ConfigFile`] so overrides and the top level cannot drift apart.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub max_lines: Option<usize>,
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,
    pub include_docstrings: Option<bool>,
    pub merge_across_blank_lines: Option<bool>,
    pub severity: Option<Severity>,
    pub require_suppression_reason: Option<bool>,
    pub select: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub rules: Option<RulesSection>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub max_lines: Option<usize>,
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,
    pub include_docstrings: Option<bool>,
    pub merge_across_blank_lines: Option<bool>,
    pub severity: Option<Severity>,
    pub require_suppression_reason: Option<bool>,
    pub select: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
    pub rules: Option<RulesSection>,

    pub exclude: Option<Vec<String>>,
    pub diff_only: Option<bool>,
    pub languages: Option<LanguagesSection>,
    pub overrides: Option<Vec<PathOverride>>,
}

impl ConfigFile {
    /// The top-level keys viewed as an override layer.
    pub fn settings(&self) -> Settings {
        Settings {
            max_lines: self.max_lines,
            max_words: self.max_words,
            max_chars: self.max_chars,
            include_docstrings: self.include_docstrings,
            merge_across_blank_lines: self.merge_across_blank_lines,
            severity: self.severity,
            require_suppression_reason: self.require_suppression_reason,
            select: self.select.clone(),
            ignore: self.ignore.clone(),
            rules: self.rules.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LanguagesSection {
    /// User-defined languages, using the same schema as the shipped ones.
    #[serde(default)]
    pub custom: Vec<LanguageSpec>,
    /// `[languages.<name>]` tables tweaking a language's budgets.
    #[serde(flatten)]
    pub overrides: HashMap<String, Settings>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathOverride {
    pub paths: Vec<String>,
    #[serde(flatten)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesSection {
    #[serde(rename = "comment-restates-code")]
    pub comment_restates_code: Option<RestateRule>,
    #[serde(rename = "block-too-long")]
    pub block_too_long: Option<LengthRule>,
    #[serde(rename = "comment-code-ratio")]
    pub comment_code_ratio: Option<RatioRule>,
    #[serde(rename = "banned-phrase")]
    pub banned_phrase: Option<PhraseRule>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LengthRule {
    pub max_lines: Option<usize>,
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestateRule {
    /// Overlap at or above this fraction counts as restating the code.
    pub threshold: Option<f64>,
    /// Comments with fewer content words than this are not judged.
    pub min_words: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatioRule {
    pub max_ratio: Option<f64>,
    pub ratio_min_lines: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhraseRule {
    /// A named bundle, currently only `llm-tells`.
    pub preset: Option<String>,
    /// Replaces the preset entirely.
    pub patterns: Option<Vec<String>>,
    /// Regexes added on top of whatever the preset or `patterns` produced.
    pub extend: Option<Vec<String>>,
    /// Literal words, escaped and word-bounded. The friendly option: a word list
    /// is not a regex list, so `c++` is a word rather than a syntax error.
    pub words: Option<Vec<String>>,
}
