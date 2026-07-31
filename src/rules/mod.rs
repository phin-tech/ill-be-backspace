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
    /// The plain word to use instead, when there is one. A finding that can name
    /// the replacement is a finding the reader can act on without thinking.
    pub suggest: Option<String>,
    /// What the word properly means, for an entry that advises rather than
    /// bans.
    pub note: Option<String>,
    /// How hard this entry pushes, overriding the configured severity.
    ///
    /// Some words are right in one domain and a tic in another — `gate` is a
    /// conditional guard, `headline` is an org-mode heading — and the useful
    /// thing to say about them is what they mean, not that they are banned.
    pub severity: Option<Severity>,
    /// Words that make this match legitimate when they follow it.
    ///
    /// Some words are only a tic outside a fixed idiom. `pathological case` is
    /// standard, `pathological span` is the idiom stretched over any noun to
    /// hand — measured against Emacs, all ten human uses are the idiom and none
    /// are the stretch. Rust's `regex` has no lookaround, so the exception
    /// cannot live in the pattern; the rule reads the following word instead.
    pub except_before: Vec<String>,
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
            suggest: None,
            note: None,
            severity: None,
            except_before: Vec::new(),
        }
    }

    /// A raw regex, used as written.
    pub fn pattern(p: &str) -> Self {
        Self {
            display: p.to_string(),
            pattern: p.to_string(),
            suggest: None,
            note: None,
            severity: None,
            except_before: Vec::new(),
        }
    }

    /// A regex a reader would not want quoted at them. The finding shows
    /// `display`; the match is still done by `pattern`.
    pub fn named(display: &str, pattern: &str) -> Self {
        Self {
            display: display.to_string(),
            pattern: pattern.to_string(),
            suggest: None,
            note: None,
            severity: None,
            except_before: Vec::new(),
        }
    }

    /// A word with the plain one to use instead: `utilize` → `use`.
    pub fn replacing(w: &str, plain: &str) -> Self {
        Self {
            suggest: Some(plain.to_string()),
            ..Phrase::word(w)
        }
    }

    /// A word worth a word about, rather than a word to remove. Reports at
    /// `note`, which never fails a build, and says what the word is for.
    pub fn advisory(w: &str, means: &str) -> Self {
        Self {
            severity: Some(Severity::Note),
            note: Some(means.to_string()),
            ..Phrase::word(w)
        }
    }

    /// A word that is only a tic outside a fixed idiom: `pathological` is fine
    /// before `case`, and a stretch before anything else.
    pub fn word_unless_followed_by(w: &str, allowed: &[&str]) -> Self {
        Self {
            except_before: allowed.iter().map(|s| s.to_string()).collect(),
            ..Phrase::word(w)
        }
    }
}

/// Phrases that mark comments written to sound thorough rather than to inform.
///
/// **This preset finds narration, not authorship, and the measurement is
/// unambiguous about it.** Per 100,000 comment words, across 4.3M words of
/// Neovim, Emacs and WordPress against 586k words of agent-written code:
///
/// | phrase | human | agent |
/// |---|---|---|
/// | `Note that` | 26.14 | 0.51 |
/// | `In other words` | 1.05 | 0.00 |
/// | `not only X but Y` | 0.68 | 0.00 |
/// | `Keep in mind that` | 0.40 | 0.00 |
/// | `delve` | 0.05 | 0.00 |
///
/// Every one of them points the wrong way. `Note that` — the entry that
/// produces more findings than the rest of the preset combined — appears fifty
/// times more often in human code. The folklore words are folklore: `delve` and
/// `In conclusion` did not appear in the agent corpus at all.
///
/// So keep this preset for what it does do — flagging phrasing that pads a
/// comment without adding to it, which is a real thing to want whoever wrote it
/// — and do not read a finding as evidence of a machine. For that, see
/// [`agent_tics_preset`], whose entries were chosen because they measured the
/// other way round.
///
/// One caveat on the numbers: the agent corpus is seven times smaller, so a
/// phrase with one or two human hits proves nothing. `Note that`, with over a
/// thousand expected occurrences under the null, is not in that category.
pub fn llm_tells_preset() -> Vec<Phrase> {
    // Word-level entries go through `Phrase::word` so a finding quotes the
    // phrase a reader recognises rather than the regex behind it.
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

/// Long words with short ones that mean the same thing.
///
/// Every pair here is a straight substitution: the plain word carries the whole
/// meaning of the fancy one in the register comments are written in. Words with
/// a real distinction stay out — `terminate` is not `end` when you are talking
/// about signals, and `abort` is not `stop` in a transaction.
pub fn plain_words_preset() -> Vec<Phrase> {
    let pairs = [
        ("utilize", "use"),
        ("utilise", "use"),
        ("leverage", "use"),
        ("facilitate", "help"),
        ("commence", "start"),
        ("initiate", "start"),
        ("endeavor", "try"),
        ("endeavour", "try"),
        ("ascertain", "find out"),
        ("demonstrate", "show"),
        ("sufficient", "enough"),
        ("additional", "more"),
        ("numerous", "many"),
        ("methodology", "method"),
        ("functionality", "features"),
        ("approximately", "about"),
        ("subsequently", "later"),
        ("prior to", "before"),
        ("subsequent to", "after"),
        ("in the event that", "if"),
        ("in order to", "to"),
        ("due to the fact that", "because"),
        ("at this point in time", "now"),
        ("has the ability to", "can"),
        ("is able to", "can"),
        ("a large number of", "many"),
        ("in the vicinity of", "near"),
        ("with regard to", "about"),
        ("in an effort to", "to"),
    ];
    pairs
        .iter()
        .map(|(long, plain)| Phrase::replacing(long, plain))
        .collect()
}

/// The bundles `[rules.banned-phrase] preset` accepts.
pub const PHRASE_PRESETS: &[&str] = &["llm-tells", "agent-tics", "plain-words"];

/// The phrases of a named preset; empty for a name that does not exist, which
/// configuration validation rejects before it gets here.
pub fn preset_named(name: &str) -> Vec<Phrase> {
    match name {
        "llm-tells" => llm_tells_preset(),
        "agent-tics" => agent_tics_preset(),
        "plain-words" => plain_words_preset(),
        _ => Vec::new(),
    }
}

/// Phrasing that marks an assistant talking about its own work rather than
/// documenting anything: the reflexive agreement, the self-congratulation, the
/// metaphors that arrive in place of a measurement.
///
/// Aimed at `backspace prose` and the chat hook more than at comments.
///
/// Unlike [`llm_tells_preset`], every entry here earned its place against a
/// control corpus. Rates per 100,000 comment words, 4.3M words of Neovim, Emacs
/// and WordPress against 586k words of agent-written code:
///
/// | word | human | agent | |
/// |---|---|---|---|
/// | `load-bearing` | 0.00 | 6.14 | agent only |
/// | `pathological` (outside its idiom) | 0.12 | 2.22 | 19x |
/// | `inert` | 0.35 | 4.95 | 14x |
/// | `stomping` | 0.07 | 0.51 | 7x |
///
/// What separates these from the folklore words is that they are metaphors
/// standing in for a specific statement, and the specific statement is what a
/// comment is for.
///
/// Counting alone could not have settled any of it. The agent corpus is
/// agent-written throughout, so a high count there is as likely to be the tic
/// repeating as the word being ordinary — `load-bearing` appears 36 times and
/// every one is the same move. Only the human control makes the count mean
/// something.
///
/// `pathological` and `inert` are the close calls. `pathological input` is a
/// fixed idiom from the algorithms literature and legitimate; `pathological
/// caller`, `pathological span`, `pathological scoped history` are the idiom
/// stretched over any noun to hand, and they outnumber it four to two. The
/// exception cannot be expressed as a pattern — Rust's `regex` has no
/// lookaround, so `pathological` cannot be matched only when `input` does not
/// follow — so this is all or nothing, and it is in. Drop it with
/// `ignore = [...]` if your domain uses the idiom often.
///
/// Two words the control corpus removed. `gate` is `is gated on`, `auth gate`,
/// `visibility gate` — a conditional guard, a thing rather than a metaphor.
/// `headline` appears 249 times in Emacs, because it is org-mode's word for a
/// heading. Domain vocabulary always beats a tic list, and no amount of
/// agent-corpus counting would have shown either of them.
pub fn agent_tics_preset() -> Vec<Phrase> {
    let words = [
        // Reflexive agreement. None of these ever document anything.
        "You're right",
        "You are right",
        "You're absolutely right",
        "You're right to call that out",
        "Good catch",
        "Great question",
        "I need to own this",
        "let me be honest",
        // Claiming a state of understanding rather than stating what is known.
        "complete picture",
        "clear picture",
        "honest caveat",
        // Metaphor standing in for a measurement.
        "the crux",
        "load-bearing",
        "inert",
        "belt and suspenders",
        "spine",
        "lever",
        "soak",
        "stomping",
    ];

    // Only a tic outside its idiom. Emacs uses `pathological` ten times and
    // every one is `case`, `cases`, `situations` or `behavior`; the agent-written
    // corpus stretches it over `caller`, `span` and `scoped history`.
    let idiomatic = [(
        "pathological",
        &[
            "case",
            "cases",
            "behavior",
            "behaviour",
            "situation",
            "situations",
            "input",
            "inputs",
            "example",
            "examples",
        ][..],
    )];

    // Words the control corpus cleared, kept as advice rather than dropped.
    // Each is right in some domain and a tic outside it, so the useful thing to
    // say is what it means. These report at `note` and never fail a build.
    let advisory = [
        (
            "gate",
            "a gate is a conditional guard — `gated on the flag`. If you mean \
             `controls` or `limits`, say that",
        ),
        (
            "headline",
            "a headline is org-mode's word for a heading. If you mean `the main \
             point`, say that",
        ),
        (
            "inert",
            "inert means present but non-reactive, as in HTML's `inert` \
             attribute. If you mean `disabled` or `does nothing`, say that",
        ),
        (
            "soak",
            "a soak test runs for a long time under load. If you mean `absorb` \
             or `tolerate`, say that",
        ),
    ];

    words
        .iter()
        .map(|w| Phrase::word(w))
        .chain(
            idiomatic
                .iter()
                .map(|(w, allowed)| Phrase::word_unless_followed_by(w, allowed)),
        )
        .chain(advisory.iter().map(|(w, means)| Phrase::advisory(w, means)))
        .collect()
}

/// Compiles phrase patterns, defaulting to case-insensitive unless the pattern
/// sets its own flags. An invalid pattern is a config error, not something to
/// silently drop.
pub fn compile_phrases(phrases: &[Phrase]) -> Result<Vec<(Phrase, Regex)>, String> {
    phrases
        .iter()
        .map(|p| {
            let source = if p.pattern.starts_with("(?") {
                p.pattern.clone()
            } else {
                format!("(?i){}", p.pattern)
            };
            Regex::new(&source)
                .map(|re| (p.clone(), re))
                .map_err(|e| format!("invalid banned-phrase pattern `{}`: {e}", p.display))
        })
        .collect()
}

pub(crate) struct Context {
    pub phrases: Vec<(Phrase, Regex)>,
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
        .filter(|(p, re)| matches_outside_its_idiom(p, re, &joined))
        .map(|(phrase, _)| {
            let help = match (&phrase.suggest, &phrase.note) {
                (Some(plain), _) => format!(
                    "Write `{plain}`. The shorter word is not a lesser one, and \
                     every reader knows it."
                ),
                (None, Some(means)) => format!("Check the sense: {means}."),
                (None, None) => "This phrasing usually introduces narration rather \
                                 than information. Delete it or state the fact \
                                 directly."
                    .to_string(),
            };
            let mut v = violation(
                BANNED_PHRASE,
                block,
                text,
                cfg,
                format!("matches banned phrase `{}`", phrase.display),
                help,
            );
            // An advisory entry outranks the configured severity downwards only:
            // a project running everything at `warning` does not get errors back
            // from a preset, and an advisory stays advice at any setting.
            if let Some(s) = phrase.severity {
                v.severity = s;
            }
            v
        })
        .collect()
}

/// Whether the phrase appears somewhere its `except_before` list does not
/// excuse. A phrase with no exceptions is just a match.
fn matches_outside_its_idiom(phrase: &Phrase, re: &Regex, text: &str) -> bool {
    if phrase.except_before.is_empty() {
        return re.is_match(text);
    }
    re.find_iter(text).any(|m| {
        let next = text[m.end()..]
            .split_whitespace()
            .next()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .unwrap_or_default();
        !phrase.except_before.contains(&next)
    })
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
