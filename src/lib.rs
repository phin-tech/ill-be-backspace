//! Flags comment blocks that have outgrown the code they describe.
//!
//! ```no_run
//! use backspace::{check_source, config::ResolvedConfig, lang::Registry};
//!
//! let spec = Registry::builtin().get("python").unwrap();
//! let violations = check_source("# a\n# b\n", spec, &ResolvedConfig::default());
//! ```

pub mod cli;
pub mod config;
pub mod diff;
pub mod lang;
pub mod report;
pub mod rules;
pub mod runner;
pub mod scan;
pub mod suppress;

pub use config::{ResolvedConfig, Severity};
pub use lang::{LanguageSpec, Registry};
pub use rules::Violation;
pub use scan::{scan, CommentBlock, CommentKind, ScanOptions};

use suppress::Scope;

/// Checks plain prose with the same rules that govern comments.
///
/// Each line becomes its own block so findings carry accurate line numbers, and
/// only the rules that make sense without code are applied — a comment:code
/// ratio is meaningless when there is no code.
pub fn check_prose(text: &str, cfg: &ResolvedConfig) -> Result<Vec<Violation>, String> {
    let ctx = rules::Context::new(cfg)?;
    // The rules that still mean something with no code beneath the words. The
    // rest are dropped rather than reported as clean.
    const PROSE_RULES: &[&str] = &[
        rules::BANNED_PHRASE,
        rules::BLOCK_TOO_LONG,
        rules::PASSIVE_VOICE,
    ];
    // Rhythm and punctuation habits are properties of a passage, not of a line,
    // so they are judged once over the whole text rather than per line.
    const WHOLE_TEXT_RULES: &[&str] = &[rules::UNIFORM_SENTENCES, rules::EM_DASH_HABIT];
    let whole: Vec<String> = WHOLE_TEXT_RULES
        .iter()
        .filter(|r| cfg.rule_enabled(r))
        .map(|r| r.to_string())
        .collect();
    let selected: Vec<String> = PROSE_RULES
        .iter()
        .filter(|r| cfg.rule_enabled(r))
        .map(|r| r.to_string())
        .collect();
    let mut cfg = cfg.clone();
    cfg.select = selected.into_iter().collect();
    // A block budget over one line would double-report the line budget.
    cfg.max_lines = usize::MAX;
    cfg.max_words = None;
    cfg.max_chars = None;

    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let block = CommentBlock {
            start_line: i as u32 + 1,
            end_line: i as u32 + 1,
            text: vec![line.to_string()],
            kind: CommentKind::Line,
            following_code_lines: 0,
            following_code: Vec::new(),
            column: 1,
        };
        out.extend(rules::check_block(&block, &block.text, &cfg, &ctx));
    }

    if !whole.is_empty() {
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let block = CommentBlock {
            start_line: 1,
            end_line: lines.len().max(1) as u32,
            text: lines,
            kind: CommentKind::Line,
            following_code_lines: 0,
            following_code: Vec::new(),
            column: 1,
        };
        let mut whole_cfg = cfg.clone();
        whole_cfg.select = whole.into_iter().collect();
        out.extend(rules::check_block(&block, &block.text, &whole_cfg, &ctx));
    }
    Ok(out)
}

/// Checks one source string. Returns violations in source order.
///
/// Panics only on an invalid banned-phrase pattern; use
/// [`rules::compile_phrases`] to validate configuration up front.
pub fn check_source(source: &str, spec: &LanguageSpec, cfg: &ResolvedConfig) -> Vec<Violation> {
    try_check_source(source, spec, cfg).unwrap_or_else(|e| panic!("invalid configuration: {e}"))
}

pub fn try_check_source(
    source: &str,
    spec: &LanguageSpec,
    cfg: &ResolvedConfig,
) -> Result<Vec<Violation>, String> {
    let (blocks, code) = scan::scan_with_code(source, spec, &cfg.scan_options());
    let ctx = rules::Context::new(cfg)?.with_code(&code);
    let mut out = Vec::new();

    for block in &blocks {
        if block.kind == CommentKind::Doc && !cfg.include_docstrings {
            continue;
        }

        let directives = suppress::parse(&block.text);

        // A file-scope directive is only honoured near the top, so a stray one
        // deep in a vendored blob cannot silence everything above it.
        if directives
            .iter()
            .any(|d| d.scope == Scope::File && block.start_line <= suppress::IGNORE_FILE_MAX_LINE)
        {
            return Ok(Vec::new());
        }

        // Directive lines are prose about the linter, not about the code, so they
        // must not push an otherwise-passing block over its budget.
        let text: Vec<String> = block
            .text
            .iter()
            .enumerate()
            .filter(|(i, _)| !directives.iter().any(|d| d.line_index == *i))
            .map(|(_, l)| l.clone())
            .collect();

        let block_directives: Vec<_> = directives
            .iter()
            .filter(|d| d.scope == Scope::Block)
            .collect();

        if cfg.require_suppression_reason {
            if let Some(d) = block_directives.iter().find(|d| d.reason.is_none()) {
                let _ = d;
                out.push(rules::suppression_needs_reason(block, &text, cfg));
            }
        }

        out.extend(
            rules::check_block(block, &text, cfg, &ctx)
                .into_iter()
                .filter(|v| !block_directives.iter().any(|d| d.covers(v.rule))),
        );
    }

    Ok(out)
}
