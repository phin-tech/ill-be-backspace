//! The checks applied to each comment block.

pub mod restate;
pub mod rhythm;
pub mod voice;
pub mod why;

use std::path::PathBuf;

use regex::Regex;

use crate::config::{ResolvedConfig, Severity};
use crate::scan::CommentBlock;

pub const BLOCK_TOO_LONG: &str = "block-too-long";
pub const COMMENT_CODE_RATIO: &str = "comment-code-ratio";
pub const BANNED_PHRASE: &str = "banned-phrase";
pub const COMMENT_RESTATES_CODE: &str = "comment-restates-code";
pub const EXPLAINS_WHAT_NOT_WHY: &str = "explains-what-not-why";
pub const PASSIVE_VOICE: &str = "passive-voice";
pub const UNIFORM_SENTENCES: &str = "uniform-sentences";
pub const EM_DASH_HABIT: &str = "em-dash-habit";
pub const UNAPPROVED_WORD: &str = "unapproved-word";
pub const SUPPRESSION_NEEDS_REASON: &str = "suppression-needs-reason";

/// Every rule id the tool knows about, for `--select` validation and `explain`.
pub const ALL_RULES: &[&str] = &[
    BLOCK_TOO_LONG,
    COMMENT_CODE_RATIO,
    BANNED_PHRASE,
    COMMENT_RESTATES_CODE,
    EXPLAINS_WHAT_NOT_WHY,
    PASSIVE_VOICE,
    UNIFORM_SENTENCES,
    EM_DASH_HABIT,
    UNAPPROVED_WORD,
    SUPPRESSION_NEEDS_REASON,
];

/// An approved vocabulary for comment prose, in the spirit of Simplified
/// Technical English without reproducing its licensed dictionary. Small on
/// purpose: words in the code beneath a comment are approved automatically, so
/// only the prose around them needs listing.
pub fn plain_code_vocabulary() -> Vec<String> {
    include_str!("../../vocabularies/plain-code.txt")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_ascii_lowercase())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub path: PathBuf,
    pub start_line: u32,
    pub end_line: u32,
    pub column: u32,
    pub message: String,
    /// A concrete next action, not a restatement of the message.
    pub help: String,
    pub severity: Severity,
    /// Detected language, for grouping and machine consumers.
    pub language: String,
    /// The offending comment, so machine consumers need not re-read the file.
    pub text: Vec<String>,
    pub following_code_lines: u32,
}

/// A thing to look for in a comment. `display` is what the user wrote and what
/// the violation message quotes; `pattern` is what actually gets compiled. They
/// differ for word entries, where `substrate` becomes `\bsubstrate\b`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    pub display: String,
    pub pattern: String,
}

impl Phrase {
    /// A literal word or multi-word phrase. Regex metacharacters are escaped so
    /// `c++` is a word rather than a syntax error.
    ///
    /// Word boundaries are only applied at ends that are themselves word
    /// characters: `\b` after the `+` of `c++` could never match, since a
    /// boundary needs a word character on one side.
    pub fn word(w: &str) -> Self {
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        let lead = w.chars().next().is_some_and(is_word);
        let trail = w.chars().last().is_some_and(is_word);
        Self {
            display: w.to_string(),
            pattern: format!(
                "{}{}{}",
                if lead { r"\b" } else { "" },
                regex::escape(w),
                if trail { r"\b" } else { "" },
            ),
        }
    }

    /// A raw regex, used as written.
    pub fn pattern(p: &str) -> Self {
        Self {
            display: p.to_string(),
            pattern: p.to_string(),
        }
    }

    /// A regex a reader would not want quoted at them. The finding shows
    /// `display`; the match is still done by `pattern`.
    pub fn named(display: &str, pattern: &str) -> Self {
        Self {
            display: display.to_string(),
            pattern: pattern.to_string(),
        }
    }
}

/// Phrases that reliably mark comments written to sound thorough rather than to
/// inform. Opt-in: enabling this by default would make the tool preachy.
pub fn llm_tells_preset() -> Vec<Phrase> {
    // Word-level entries go through `Phrase::word` so a finding quotes the
    // phrase a reader recognises rather than the regex behind it.
    //
    // These date fast. Every model generation has its own favourites, and a word
    // that marked generated text in 2025 is just a word in 2027. Prune them.
    let words = [
        "Note that",
        "It's worth noting",
        "In other words",
        "This is important because",
        "As mentioned above",
        "Keep in mind that",
        "delve",
        "tapestry",
        "testament to",
        "navigate the complexities",
        "In conclusion",
        "It is important to note",
        "at the end of the day",
    ];

    // Constructions rather than words, and they have outlasted several model
    // generations: the antithesis that inflates one claim into two, and the
    // correlative pair used for the same effect.
    let patterns = [
        ("Verified <date>", r"Verified \d{4}-\d{2}-\d{2}"),
        // The tell is the shouting, not the negation. Case-sensitive, or it
        // fires on `a project adds to your list, it does not replace it`.
        ("it does NOT", r"(?-i)\bit does NOT\b"),
        (
            "it's not just X — it's Y",
            r"(?i)\b(?:it'?s|this is|that'?s|they'?re|we'?re)\s+not\s+just\b[^.!?]{0,60}?(?:\u{2014}|--|,\s*it'?s\b|\bbut\b)",
        ),
        ("not only X but Y", r"(?i)\bnot only\b[^.!?]{0,60}\bbut\b"),
        (
            "it isn't about X, it's about Y",
            r"(?i)\bis(?:n'?t|\s+not)\s+about\b[^.!?]{0,60}\bit'?s about\b",
        ),
    ];

    words
        .iter()
        .map(|w| Phrase::word(w))
        .chain(patterns.iter().map(|(d, p)| Phrase::named(d, p)))
        .collect()
}

/// Compiles phrase patterns, defaulting to case-insensitive unless the pattern
/// sets its own flags. An invalid pattern is a config error, not something to
/// silently drop.
pub fn compile_phrases(phrases: &[Phrase]) -> Result<Vec<(String, Regex)>, String> {
    phrases
        .iter()
        .map(|p| {
            let source = if p.pattern.starts_with("(?") {
                p.pattern.clone()
            } else {
                format!("(?i){}", p.pattern)
            };
            Regex::new(&source)
                .map(|re| (p.display.clone(), re))
                .map_err(|e| format!("invalid banned-phrase pattern `{}`: {e}", p.display))
        })
        .collect()
}

pub(crate) struct Context {
    pub phrases: Vec<(String, Regex)>,
    /// Rationale markers, compiled once: word-bounded and case-insensitive, so
    /// `since` does not match `sincerely`.
    pub rationale: Vec<Regex>,
    pub approved: std::collections::HashSet<String>,
    /// Identifiers from the whole file, treated as approved vocabulary.
    pub code_vocabulary: std::collections::HashSet<String>,
}

impl Context {
    pub fn new(cfg: &ResolvedConfig) -> Result<Self, String> {
        let markers: Vec<Phrase> = cfg
            .rationale_markers
            .iter()
            .map(|m| Phrase::word(m))
            .collect();
        Ok(Self {
            phrases: compile_phrases(&cfg.banned_phrases)?,
            rationale: compile_phrases(&markers)?
                .into_iter()
                .map(|(_, re)| re)
                .collect(),
            approved: cfg
                .approved_words
                .iter()
                .map(|w| w.to_ascii_lowercase())
                .collect(),
            code_vocabulary: Default::default(),
        })
    }

    /// Records the file's identifiers, so a project's own terminology is
    /// approved without anyone restating it in configuration.
    pub fn with_code(mut self, code: &str) -> Self {
        self.code_vocabulary = restate::words_of(code).into_iter().collect();
        self
    }
}

pub(crate) fn check_block(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
    ctx: &Context,
) -> Vec<Violation> {
    let mut out = Vec::new();

    if cfg.rule_enabled(BLOCK_TOO_LONG) {
        out.extend(block_too_long(block, text, cfg));
    }
    if cfg.rule_enabled(COMMENT_CODE_RATIO) {
        out.extend(comment_code_ratio(block, text, cfg));
    }
    if cfg.rule_enabled(BANNED_PHRASE) {
        out.extend(banned_phrase(block, text, cfg, ctx));
    }
    if cfg.rule_enabled(COMMENT_RESTATES_CODE) {
        out.extend(comment_restates_code(block, text, cfg));
    }
    if cfg.rule_enabled(EXPLAINS_WHAT_NOT_WHY) {
        out.extend(explains_what_not_why(block, text, cfg, ctx));
    }
    if cfg.rule_enabled(PASSIVE_VOICE) {
        out.extend(passive_voice(block, text, cfg));
    }
    if cfg.rule_enabled(UNIFORM_SENTENCES) {
        out.extend(uniform_sentences(block, text, cfg));
    }
    if cfg.rule_enabled(EM_DASH_HABIT) {
        out.extend(em_dash_habit(block, text, cfg));
    }
    if cfg.rule_enabled(UNAPPROVED_WORD) {
        out.extend(unapproved_word(block, text, cfg, ctx));
    }
    out
}

/// Flags prose outside a configured vocabulary. An empty list means no
/// vocabulary was configured, which is not the same as banning everything.
fn unapproved_word(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
    ctx: &Context,
) -> Option<Violation> {
    if ctx.approved.is_empty() {
        return None;
    }

    let mut allowed = ctx.approved.clone();
    if cfg.approve_code_words {
        // A project's identifiers define its own terminology, so there is no
        // reason to make someone restate it in configuration.
        allowed.extend(restate::words_of(&block.following_code.join(" ")));
        allowed.extend(ctx.code_vocabulary.iter().cloned());
    }

    let mut unknown: Vec<String> = restate::words_of(&text.join(" "))
        .into_iter()
        // A bare number or a version like `500ms` carries no vocabulary.
        .filter(|w| !w.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .filter(|w| !allowed.contains(w))
        .collect();
    unknown.sort();
    unknown.dedup();
    if unknown.is_empty() {
        return None;
    }

    Some(violation(
        UNAPPROVED_WORD,
        block,
        text,
        cfg,
        format!("outside the approved vocabulary: {}", unknown.join(", ")),
        "Use a word from the approved list, or add this one with \
         `[rules.unapproved-word] extend = [...]` if it is part of the \
         project's vocabulary."
            .to_string(),
    ))
}

fn comment_restates_code(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
) -> Option<Violation> {
    let score = restate::overlap(text, &block.following_code, cfg.restate_min_words)?;
    if score < cfg.restate_threshold {
        return None;
    }
    Some(violation(
        COMMENT_RESTATES_CODE,
        block,
        text,
        cfg,
        format!(
            "{:.0}% of the comment's words already appear in the code it describes",
            score * 100.0
        ),
        "This comment names what the code names. Either delete it, or replace \
         it with the reason the code is written this way."
            .to_string(),
    ))
}

/// Flags a comment that draws its words from the code beneath it *and* gives no
/// reason for any of it.
///
/// Neither half is decisive alone. `comment-restates-code` fires on good "why"
/// comments because they have to name the things they discuss; a missing
/// rationale marker means little in a one-line note. The conjunction is what
/// isolates narration: several lines, mostly the code's own vocabulary, and
/// nothing that answers "why".
fn explains_what_not_why(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
    ctx: &Context,
) -> Option<Violation> {
    let prose = restate::prose_lines(text);
    if prose.len() < cfg.what_not_why_min_lines {
        return None;
    }
    // A godoc comment names its function because the convention says it must.
    if why::opens_with_declared_name(text, &block.following_code) {
        return None;
    }
    let score = restate::overlap(text, &block.following_code, cfg.restate_min_words)?;
    if score < cfg.what_not_why_threshold {
        return None;
    }
    // Joined rather than checked line by line, so `so\n// that` still reads as
    // one marker.
    let joined = prose
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    if ctx.rationale.iter().any(|re| re.is_match(&joined)) {
        return None;
    }

    Some(violation(
        EXPLAINS_WHAT_NOT_WHY,
        block,
        text,
        cfg,
        format!(
            "{} lines drawn {:.0}% from the code below, with no reason given",
            prose.len(),
            score * 100.0
        ),
        "This says what the code does. Say why it does it — the constraint, the \
         bug it avoids, what breaks without it. If there is no why, delete it."
            .to_string(),
    ))
}

/// Flags the first passive construction in a comment. One finding per block:
/// three quotes from the same paragraph is nagging, not information.
fn passive_voice(block: &CommentBlock, text: &[String], cfg: &ResolvedConfig) -> Option<Violation> {
    let phrase = restate::prose_lines(text)
        .into_iter()
        .find_map(|l| voice::passive_phrase(l, cfg.passive_requires_agent))?;

    Some(violation(
        PASSIVE_VOICE,
        block,
        text,
        cfg,
        format!("passive voice: `{phrase}`"),
        "Passive voice hides who acts. `the caller sets the value` rather than \
         `the value is set by the caller`."
            .to_string(),
    ))
}

/// Flags prose whose sentences are all the same length.
///
/// People write a six-word sentence next to a thirty-word one. Text that never
/// does is either generated or edited until it reads like it was.
fn uniform_sentences(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
) -> Option<Violation> {
    let joined = restate::prose_lines(text)
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let (cv, count) = rhythm::variation(&joined, cfg.min_sentences)?;
    if cv >= cfg.min_variation {
        return None;
    }

    Some(violation(
        UNIFORM_SENTENCES,
        block,
        text,
        cfg,
        format!(
            "{count} sentences of near-identical length (variation {cv:.2}, min {:.2})",
            cfg.min_variation
        ),
        "Vary the rhythm. Cut one sentence to four words, let another run long. \
         Uniform length reads as generated even when it is not."
            .to_string(),
    ))
}

/// Flags a heavy em-dash habit, the punctuation an extra thought gets bolted on
/// with.
fn em_dash_habit(block: &CommentBlock, text: &[String], cfg: &ResolvedConfig) -> Option<Violation> {
    let joined = text.join(" ");
    let (rate, count) = rhythm::em_dash_rate(&joined);
    // A rate alone would fire on a four-word line with one dash in it.
    if count < cfg.min_em_dashes || rate <= cfg.max_em_dash_rate {
        return None;
    }

    Some(violation(
        EM_DASH_HABIT,
        block,
        text,
        cfg,
        format!(
            "{count} em dashes in {} words ({rate:.1} per 100, max {:.1})",
            joined.split_whitespace().count(),
            cfg.max_em_dash_rate
        ),
        "An em dash usually joins two thoughts that wanted to be two sentences. \
         Use a full stop, or a comma if they belong together."
            .to_string(),
    ))
}

fn violation(
    rule: &'static str,
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
    message: String,
    help: String,
) -> Violation {
    Violation {
        rule,
        path: PathBuf::new(),
        start_line: block.start_line,
        end_line: block.end_line,
        column: block.column,
        message,
        help,
        severity: cfg.severity,
        language: String::new(),
        text: text.to_vec(),
        following_code_lines: block.following_code_lines,
    }
}

/// One violation per block even when several budgets are exceeded — three
/// findings on the same comment is noise, not information.
fn block_too_long(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
) -> Option<Violation> {
    let lines = text.len();
    let words = text
        .iter()
        .map(|l| l.split_whitespace().count())
        .sum::<usize>();
    let chars = text.iter().map(|l| l.chars().count()).sum::<usize>();

    // Checked before the block budgets: a single runaway line is a more
    // specific finding than "the block is long".
    let worst_line = text
        .iter()
        .map(|l| l.split_whitespace().count())
        .max()
        .unwrap_or(0);

    let message = if cfg.max_line_words.is_some_and(|m| worst_line > m) {
        format!(
            "comment line is {worst_line} words (max {})",
            cfg.max_line_words.unwrap()
        )
    } else if lines > cfg.max_lines {
        format!("comment block is {lines} lines (max {})", cfg.max_lines)
    } else if cfg.max_words.is_some_and(|m| words > m) {
        format!(
            "comment block is {words} words (max {})",
            cfg.max_words.unwrap()
        )
    } else if cfg.max_chars.is_some_and(|m| chars > m) {
        format!(
            "comment block is {chars} characters (max {})",
            cfg.max_chars.unwrap()
        )
    } else {
        return None;
    };

    Some(violation(
        BLOCK_TOO_LONG,
        block,
        text,
        cfg,
        message,
        "Keep the invariant a reader cannot derive from the code; move history, \
         dates and rejected approaches into the commit message."
            .to_string(),
    ))
}

fn comment_code_ratio(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
) -> Option<Violation> {
    let lines = text.len();
    // Short comments are exempt whatever the ratio: two lines over one is fine.
    if lines < cfg.ratio_min_lines {
        return None;
    }
    let code = block.following_code_lines.max(1);
    let ratio = lines as f64 / code as f64;
    if ratio <= cfg.max_ratio {
        return None;
    }

    Some(violation(
        COMMENT_CODE_RATIO,
        block,
        text,
        cfg,
        format!(
            "{lines} comment lines describe {} line{} of code (ratio {ratio:.1}, max {:.1})",
            block.following_code_lines,
            if block.following_code_lines == 1 {
                ""
            } else {
                "s"
            },
            cfg.max_ratio
        ),
        "A comment longer than the code it describes usually restates the code. \
         Say what the code cannot."
            .to_string(),
    ))
}

fn banned_phrase(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
    ctx: &Context,
) -> Vec<Violation> {
    let joined = text.join("\n");
    ctx.phrases
        .iter()
        .filter(|(_, re)| re.is_match(&joined))
        .map(|(pattern, _)| {
            violation(
                BANNED_PHRASE,
                block,
                text,
                cfg,
                format!("matches banned phrase `{pattern}`"),
                "This phrasing usually introduces narration rather than \
                 information. Delete it or state the fact directly."
                    .to_string(),
            )
        })
        .collect()
}

pub(crate) fn suppression_needs_reason(
    block: &CommentBlock,
    text: &[String],
    cfg: &ResolvedConfig,
) -> Violation {
    violation(
        SUPPRESSION_NEEDS_REASON,
        block,
        text,
        cfg,
        "suppression directive has no reason".to_string(),
        "Write `backspace: ignore[rule] — why` so the next reader knows whether \
         the exemption still applies."
            .to_string(),
    )
}

/// Human-readable explanation for `backspace explain <rule>`.
pub fn explain(rule: &str) -> Option<&'static str> {
    Some(match rule {
        BLOCK_TOO_LONG => {
            "Flags a run of consecutive comment lines longer than `max_lines`. \
             Optional `max_words` and `max_chars` budgets catch a single very long \
             wrapped line that a line count cannot see."
        }
        COMMENT_CODE_RATIO => {
            "Flags a comment block longer than the code it introduces, by a factor \
             of `max_ratio`. Blocks shorter than `ratio_min_lines` are exempt."
        }
        BANNED_PHRASE => {
            "Flags comments matching configured regexes. The `llm-tells` preset \
             targets phrasing that narrates rather than informs."
        }
        COMMENT_RESTATES_CODE => {
            "Flags a comment whose vocabulary is mostly drawn from the code \
             beneath it. Identifiers are split on case and underscores before \
             comparison. Controlled by `restate_threshold` and \
             `restate_min_words`; off by default."
        }
        EXPLAINS_WHAT_NOT_WHY => {
            "Flags a comment that both restates the code and offers no reason for \
             it: at least `min_lines` of prose, vocabulary overlap at or above \
             `threshold`, and none of the rationale markers (`because`, `so \
             that`, `to avoid`, `otherwise`, …). Narrower than \
             `comment-restates-code`, because a comment giving a reason is \
             exempt however much vocabulary it shares. Off by default."
        }
        PASSIVE_VOICE => {
            "Flags a form of `be` followed by a past participle: `the value is \
             set by the caller`. Naming the actor is usually shorter and always \
             clearer. Off by default, and deliberately not an error: see the \
             note in the README about whose writing style rules like this \
             penalise."
        }
        UNIFORM_SENTENCES => {
            "Flags prose whose sentences are all close to the same length, \
             measured as the coefficient of variation of their word counts. \
             Needs at least `min_sentences` (5) to judge a rhythm at all. Most \
             useful on `backspace prose`, since few comments are that long. Off \
             by default."
        }
        EM_DASH_HABIT => {
            "Flags more than `max_rate` (2.0) em dashes per hundred words, once \
             at least `min_count` (2) appear. The em dash is where a second \
             thought gets bolted onto a first; a habit of it is the most \
             reliable punctuation tell there is. Off by default."
        }
        UNAPPROVED_WORD => {
            "Flags comment prose using words outside a configured vocabulary. \
             Words appearing in the code beneath the comment are approved \
             automatically, so project terminology needs no entry. The \
             `plain-code` preset ships a small starter list; off by default."
        }
        SUPPRESSION_NEEDS_REASON => {
            "Fires when `require_suppression_reason` is set and a `backspace: \
             ignore` directive carries no justification."
        }
        _ => return None,
    })
}
