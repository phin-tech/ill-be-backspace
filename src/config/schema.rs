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
    pub max_line_words: Option<usize>,
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
    pub max_line_words: Option<usize>,
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
            max_line_words: self.max_line_words,
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
    #[serde(rename = "explains-what-not-why")]
    pub explains_what_not_why: Option<WhyRule>,
    #[serde(rename = "passive-voice")]
    pub passive_voice: Option<PassiveRule>,
    #[serde(rename = "uniform-sentences")]
    pub uniform_sentences: Option<RhythmRule>,
    #[serde(rename = "em-dash-habit")]
    pub em_dash_habit: Option<EmDashRule>,
    #[serde(rename = "block-too-long")]
    pub block_too_long: Option<LengthRule>,
    #[serde(rename = "comment-code-ratio")]
    pub comment_code_ratio: Option<RatioRule>,
    #[serde(rename = "banned-phrase")]
    pub banned_phrase: Option<PhraseRule>,
    #[serde(rename = "unapproved-word")]
    pub unapproved_word: Option<VocabularyRule>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VocabularyRule {
    /// A named vocabulary, currently only `plain-code`.
    pub preset: Option<String>,
    /// Replaces the preset entirely.
    pub words: Option<Vec<String>>,
    /// Added on top of the preset.
    pub extend: Option<Vec<String>>,
    /// Treat identifiers in the code beneath a comment as approved.
    pub approve_code_words: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LengthRule {
    pub max_lines: Option<usize>,
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,
    pub max_line_words: Option<usize>,
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
pub struct WhyRule {
    /// Overlap at or above this fraction counts as restating the code. Lower
    /// than `comment-restates-code` uses, because the missing rationale marker
    /// is carrying half the judgement.
    pub threshold: Option<f64>,
    /// Comment blocks with fewer prose lines than this are not judged.
    pub min_lines: Option<usize>,
    /// Replaces the built-in rationale markers entirely.
    pub markers: Option<Vec<String>>,
    /// Added on top of the built-in markers.
    pub extend: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RhythmRule {
    /// Variation below this counts as uniform.
    pub min_variation: Option<f64>,
    /// Fewer sentences than this have no rhythm to measure.
    pub min_sentences: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmDashRule {
    /// Em dashes per hundred words that count as a habit.
    pub max_rate: Option<f64>,
    /// Below this many, a rate says nothing.
    pub min_count: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PassiveRule {
    /// Flag only passives that name their actor (`set by the caller`). Turning
    /// this off flags every passive construction, which measurement says is
    /// mostly predicate adjectives.
    pub require_agent: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatioRule {
    pub max_ratio: Option<f64>,
    pub ratio_min_lines: Option<usize>,
}

/// One preset name or several. `preset = "llm-tells"` and
/// `preset = ["llm-tells", "agent-tics"]` are both valid, so a project can take
/// two bundles without either one replacing the other.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Presets {
    One(String),
    Many(Vec<String>),
}

impl Presets {
    pub fn names(&self) -> Vec<&str> {
        match self {
            Presets::One(s) => vec![s.as_str()],
            Presets::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhraseRule {
    /// Named bundles: `llm-tells`, `agent-tics`.
    pub preset: Option<Presets>,
    /// Replaces the preset entirely.
    pub patterns: Option<Vec<String>>,
    /// Regexes added on top of whatever the preset or `patterns` produced.
    pub extend: Option<Vec<String>>,
    /// Literal words, escaped and word-bounded. The friendly option: a word list
    /// is not a regex list, so `c++` is a word rather than a syntax error.
    pub words: Option<Vec<String>>,
}
