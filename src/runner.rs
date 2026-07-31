//! Collecting files, checking them in parallel, and assembling a report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use rayon::prelude::*;

use crate::config::Config;
use crate::diff::ChangedLines;
use crate::rules::Violation;
use crate::scan::CommentKind;
use crate::{rules, scan, suppress};

/// One comment as surfaced by `--audit`.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub path: PathBuf,
    pub language: String,
    pub start_line: u32,
    pub end_line: u32,
    pub kind: &'static str,
    pub text: Vec<String>,
    pub words: usize,
    pub following_code_lines: u32,
}

#[derive(Debug, Default, Clone)]
pub struct RunOptions {
    /// When set, only blocks overlapping these lines are reported.
    pub changed: Option<ChangedLines>,
    /// Fail rather than silently skipping files of unknown type.
    pub fail_on_unknown: bool,
    /// Collect every comment for review instead of applying rules.
    pub audit: bool,
}

#[derive(Debug, Default)]
pub struct Report {
    pub violations: Vec<Violation>,
    pub comments: Vec<AuditEntry>,
    pub files_checked: usize,
    pub files_skipped: usize,
}

impl Report {
    pub fn errors(&self) -> usize {
        self.violations
            .iter()
            .filter(|v| v.severity == crate::Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.violations.len() - self.errors()
    }

    /// Counts by rule id, for `--stats`.
    pub fn by_rule(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for v in &self.violations {
            *m.entry(v.rule).or_insert(0) += 1;
        }
        m
    }

    pub fn by_language(&self) -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        for v in &self.violations {
            *m.entry(v.language.clone()).or_insert(0) += 1;
        }
        m
    }
}

/// Extensions `backspace prose` walks a directory for. Named explicitly rather
/// than "everything that is not source", so pointing it at a repository root
/// reviews the documentation instead of the lockfiles.
pub const PROSE_EXTENSIONS: &[&str] = &["md", "markdown", "txt", "rst", "adoc"];

pub fn is_prose_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PROSE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Expands the given paths into concrete files, honouring gitignore and the
/// configured excludes.
pub fn collect_files(paths: &[PathBuf], config: &Config) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_file() {
            out.push(path.clone());
            continue;
        }
        // `hidden(false)` so dotfiles like `.backspace.toml` are checkable, but
        // the git database itself is never source.
        let walker = WalkBuilder::new(path)
            .hidden(false)
            .filter_entry(|e| e.file_name() != ".git")
            .build();
        for entry in walker.flatten() {
            if entry.file_type().is_some_and(|t| t.is_file()) {
                out.push(entry.into_path());
            }
        }
    }
    out.retain(|p| !config.is_excluded(p));
    out.sort();
    out.dedup();
    out
}

pub fn check_paths(paths: &[PathBuf], config: &Config, opts: &RunOptions) -> Result<Report> {
    let files = collect_files(paths, config);

    // Files are independent, so this parallelises cleanly. Results are sorted
    // afterwards to keep output deterministic regardless of scheduling.
    let results: Vec<Result<FileOutcome>> = files
        .par_iter()
        .map(|path| check_file(path, config, opts))
        .collect();

    let mut report = Report::default();
    for r in results {
        let outcome = r?;
        match outcome {
            FileOutcome::Skipped => report.files_skipped += 1,
            FileOutcome::Checked(v, c) => {
                report.files_checked += 1;
                report.violations.extend(v);
                report.comments.extend(c);
            }
        }
    }
    report
        .violations
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.start_line.cmp(&b.start_line)));
    report
        .comments
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.start_line.cmp(&b.start_line)));
    Ok(report)
}

enum FileOutcome {
    Skipped,
    Checked(Vec<Violation>, Vec<AuditEntry>),
}

fn check_file(path: &Path, config: &Config, opts: &RunOptions) -> Result<FileOutcome> {
    // Binary and unreadable files are not an error; they are simply not source.
    let Ok(source) = std::fs::read_to_string(path) else {
        return Ok(FileOutcome::Skipped);
    };

    let Some(spec) = config.registry().detect(path, &source) else {
        if opts.fail_on_unknown {
            anyhow::bail!("unrecognised file type: {}", path.display());
        }
        return Ok(FileOutcome::Skipped);
    };

    let cfg = config.resolve(path, &spec.name);
    let (blocks, code) = scan::scan_with_code(&source, spec, &cfg.scan_options());
    let ctx = rules::Context::new(&cfg)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid configuration for {}", path.display()))?
        .with_code(&code);
    let mut out = Vec::new();
    let mut audit = Vec::new();

    for block in &blocks {
        if block.kind == CommentKind::Doc && !cfg.include_docstrings {
            continue;
        }
        if let Some(changed) = &opts.changed {
            // Diff paths are absolute; walked paths are usually relative.
            let key = path.canonicalize();
            let key = key.as_deref().unwrap_or(path);
            match changed.get(key) {
                Some(lines) if block.intersects(lines) => {}
                _ => continue,
            }
        }

        if opts.audit {
            audit.push(AuditEntry {
                path: path.to_path_buf(),
                language: spec.name.clone(),
                start_line: block.start_line,
                end_line: block.end_line,
                kind: match block.kind {
                    CommentKind::Line => "line",
                    CommentKind::Block => "block",
                    CommentKind::Doc => "doc",
                },
                words: block
                    .text
                    .iter()
                    .map(|l| l.split_whitespace().count())
                    .sum(),
                text: block.text.clone(),
                following_code_lines: block.following_code_lines,
            });
            continue;
        }

        let directives = suppress::parse(&block.text);
        if directives.iter().any(|d| {
            d.scope == suppress::Scope::File && block.start_line <= suppress::IGNORE_FILE_MAX_LINE
        }) {
            return Ok(FileOutcome::Checked(Vec::new(), Vec::new()));
        }

        let text: Vec<String> = block
            .text
            .iter()
            .enumerate()
            .filter(|(i, _)| !directives.iter().any(|d| d.line_index == *i))
            .map(|(_, l)| l.clone())
            .collect();

        let block_directives: Vec<_> = directives
            .iter()
            .filter(|d| d.scope == suppress::Scope::Block)
            .collect();

        if cfg.require_suppression_reason && block_directives.iter().any(|d| d.reason.is_none()) {
            out.push(rules::suppression_needs_reason(block, &text, &cfg));
        }

        out.extend(
            rules::check_block(block, &text, &cfg, &ctx)
                .into_iter()
                .filter(|v| !block_directives.iter().any(|d| d.covers(v.rule))),
        );
    }

    for v in &mut out {
        v.path = path.to_path_buf();
        v.language = spec.name.clone();
    }
    Ok(FileOutcome::Checked(out, audit))
}
