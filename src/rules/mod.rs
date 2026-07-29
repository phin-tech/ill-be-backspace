//! The checks applied to each comment block.

use std::path::PathBuf;

use regex::Regex;

use crate::config::{ResolvedConfig, Severity};
use crate::scan::CommentBlock;

pub const BLOCK_TOO_LONG: &str = "block-too-long";
pub const COMMENT_CODE_RATIO: &str = "comment-code-ratio";
pub const BANNED_PHRASE: &str = "banned-phrase";
pub const SUPPRESSION_NEEDS_REASON: &str = "suppression-needs-reason";

/// Every rule id the tool knows about, for `--select` validation and `explain`.
pub const ALL_RULES: &[&str] = &[
    BLOCK_TOO_LONG,
    COMMENT_CODE_RATIO,
    BANNED_PHRASE,
    SUPPRESSION_NEEDS_REASON,
];

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
}

/// Phrases that reliably mark comments written to sound thorough rather than to
/// inform. Opt-in: enabling this by default would make the tool preachy.
pub fn llm_tells_preset() -> Vec<Phrase> {
    [
        r"Verified \d{4}-\d{2}-\d{2}",
        r"it does NOT\b",
        r"\bNote that\b",
        r"\bIt'?s worth noting\b",
        r"\bIn other words\b",
        r"\bThis is important because\b",
        r"\bAs mentioned above\b",
        r"\bKeep in mind that\b",
    ]
    .iter()
    .map(|p| Phrase::pattern(p))
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
}

impl Context {
    pub fn new(cfg: &ResolvedConfig) -> Result<Self, String> {
        Ok(Self {
            phrases: compile_phrases(&cfg.banned_phrases)?,
        })
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
    out
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

    let message = if lines > cfg.max_lines {
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
                format!("comment matches banned phrase `{pattern}`"),
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
        SUPPRESSION_NEEDS_REASON => {
            "Fires when `require_suppression_reason` is set and a `backspace: \
             ignore` directive carries no justification."
        }
        _ => return None,
    })
}
