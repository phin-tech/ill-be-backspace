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
    let ctx = rules::Context::new(cfg)?;
    let blocks = scan(source, spec, &cfg.scan_options());
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
