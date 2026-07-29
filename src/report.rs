//! Output formats.

use std::io::Write;

use anstyle::{AnsiColor, Style};
use anyhow::Result;
use serde::Serialize;

use crate::config::Severity;
use crate::rules::Violation;
use crate::runner::Report;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Text,
    /// GitHub Actions workflow commands, which render as inline annotations.
    Github,
    Json,
}

impl std::str::FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "text" => Ok(Format::Text),
            "github" => Ok(Format::Github),
            "json" => Ok(Format::Json),
            other => Err(format!("unknown format `{other}` (text, github, json)")),
        }
    }
}

pub fn write(out: &mut impl Write, report: &Report, format: Format, stats: bool) -> Result<()> {
    match format {
        Format::Text => text(out, report, stats),
        Format::Github => github(out, report),
        Format::Json => json(out, report),
    }
}

/// How many lines of the offending comment to show before eliding the middle.
const PREVIEW_HEAD: usize = 2;
const PREVIEW_TAIL: usize = 1;

fn text(out: &mut impl Write, report: &Report, stats: bool) -> Result<()> {
    let bold = Style::new().bold();
    let red = Style::new().fg_color(Some(AnsiColor::Red.into())).bold();
    let yellow = Style::new().fg_color(Some(AnsiColor::Yellow.into())).bold();
    let dim = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
    let cyan = Style::new().fg_color(Some(AnsiColor::Cyan.into()));

    for v in &report.violations {
        let (sev_style, sev) = match v.severity {
            Severity::Error => (red, "error"),
            Severity::Warning => (yellow, "warning"),
        };
        writeln!(
            out,
            "{bold}{}:{}:{}{bold:#}: {sev_style}{sev}{sev_style:#}: {}: {}",
            v.path.display(),
            v.start_line,
            v.column,
            v.rule,
            v.message
        )?;

        for (label, line) in preview(v) {
            match label {
                Some(n) => writeln!(out, "{dim}{n:>5} |{dim:#} {line}")?,
                None => writeln!(out, "{dim}      | {line}{dim:#}")?,
            }
        }
        writeln!(out, "{cyan}      = help:{cyan:#} {}", wrap_help(&v.help))?;
        writeln!(out)?;
    }

    if stats {
        writeln!(out, "{bold}by rule{bold:#}")?;
        for (rule, n) in report.by_rule() {
            writeln!(out, "  {rule:<28} {n}")?;
        }
        writeln!(out, "{bold}by language{bold:#}")?;
        for (lang, n) in report.by_language() {
            writeln!(out, "  {lang:<28} {n}")?;
        }
    }

    let n = report.violations.len();
    writeln!(
        out,
        "{} file{} checked, {n} violation{}",
        report.files_checked,
        plural(report.files_checked),
        plural(n)
    )?;
    Ok(())
}

/// Numbered source lines for the head and tail of the comment, with the middle
/// elided so a 40-line comment does not print 40 lines to complain about length.
fn preview(v: &Violation) -> Vec<(Option<u32>, String)> {
    let n = v.text.len();
    if n <= PREVIEW_HEAD + PREVIEW_TAIL + 1 {
        return v
            .text
            .iter()
            .enumerate()
            .map(|(i, l)| (Some(v.start_line + i as u32), l.clone()))
            .collect();
    }
    let mut out: Vec<(Option<u32>, String)> = v.text[..PREVIEW_HEAD]
        .iter()
        .enumerate()
        .map(|(i, l)| (Some(v.start_line + i as u32), l.clone()))
        .collect();
    out.push((
        None,
        format!("... {} more lines", n - PREVIEW_HEAD - PREVIEW_TAIL),
    ));
    for (i, l) in v.text[n - PREVIEW_TAIL..].iter().enumerate() {
        out.push((Some(v.end_line - (PREVIEW_TAIL - 1 - i) as u32), l.clone()));
    }
    out
}

fn wrap_help(help: &str) -> String {
    let mut out = String::new();
    let mut width = 0;
    for word in help.split_whitespace() {
        if width + word.len() > 64 {
            out.push_str("\n             ");
            width = 0;
        } else if !out.is_empty() {
            out.push(' ');
            width += 1;
        }
        out.push_str(word);
        width += word.len();
    }
    out
}

fn github(out: &mut impl Write, report: &Report) -> Result<()> {
    for v in &report.violations {
        let level = match v.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        // Newlines are encoded as %0A per the workflow-command format.
        writeln!(
            out,
            "::{level} file={},line={},endLine={},col={},title=backspace/{}::{}",
            v.path.display(),
            v.start_line,
            v.end_line,
            v.column,
            v.rule,
            v.message.replace('\n', "%0A")
        )?;
    }
    Ok(())
}

/// Machine-readable output. Carries the comment text and the surrounding counts
/// so an agent can act on a finding without re-reading the file.
#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    summary: Summary,
    violations: Vec<JsonViolation<'a>>,
}

#[derive(Serialize)]
struct Summary {
    files_checked: usize,
    files_skipped: usize,
    violations: usize,
    errors: usize,
    warnings: usize,
}

#[derive(Serialize)]
struct JsonViolation<'a> {
    rule: &'a str,
    severity: &'a str,
    file: String,
    start_line: u32,
    end_line: u32,
    column: u32,
    message: &'a str,
    help: &'a str,
    language: &'a str,
    comment: &'a [String],
    comment_line_count: usize,
    following_code_lines: u32,
}

fn json(out: &mut impl Write, report: &Report) -> Result<()> {
    let doc = JsonReport {
        version: 1,
        summary: Summary {
            files_checked: report.files_checked,
            files_skipped: report.files_skipped,
            violations: report.violations.len(),
            errors: report.errors(),
            warnings: report.warnings(),
        },
        violations: report
            .violations
            .iter()
            .map(|v| JsonViolation {
                rule: v.rule,
                severity: v.severity.as_str(),
                file: v.path.display().to_string(),
                start_line: v.start_line,
                end_line: v.end_line,
                column: v.column,
                message: &v.message,
                help: &v.help,
                language: &v.language,
                comment: &v.text,
                comment_line_count: v.text.len(),
                following_code_lines: v.following_code_lines,
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut *out, &doc)?;
    writeln!(out)?;
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// `--audit`: every comment in scope, for a human or an agent to re-read.
/// Never a pass/fail judgement, so it carries no severity and no rule id.
pub fn write_audit(out: &mut impl Write, report: &Report, format: Format) -> Result<()> {
    match format {
        Format::Json => audit_json(out, report),
        _ => audit_text(out, report),
    }
}

fn audit_text(out: &mut impl Write, report: &Report) -> Result<()> {
    let dim = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
    let bold = Style::new().bold();

    for c in &report.comments {
        writeln!(
            out,
            "{bold}{}:{}{bold:#} {dim}({}, {} line{}, {} word{}){dim:#}",
            c.path.display(),
            c.start_line,
            c.kind,
            c.text.len(),
            plural(c.text.len()),
            c.words,
            plural(c.words),
        )?;
        for (i, line) in c.text.iter().enumerate() {
            writeln!(out, "{dim}{:>5} |{dim:#} {line}", c.start_line as usize + i)?;
        }
        writeln!(out)?;
    }

    let n = report.comments.len();
    writeln!(
        out,
        "{} file{} checked, {n} comment{}",
        report.files_checked,
        plural(report.files_checked),
        plural(n)
    )?;
    Ok(())
}

#[derive(Serialize)]
struct AuditReport<'a> {
    version: u32,
    mode: &'a str,
    summary: AuditSummary,
    comments: Vec<AuditComment<'a>>,
}

#[derive(Serialize)]
struct AuditSummary {
    files_checked: usize,
    comments: usize,
}

#[derive(Serialize)]
struct AuditComment<'a> {
    file: String,
    language: &'a str,
    kind: &'a str,
    start_line: u32,
    end_line: u32,
    line_count: usize,
    words: usize,
    following_code_lines: u32,
    text: &'a [String],
}

fn audit_json(out: &mut impl Write, report: &Report) -> Result<()> {
    let doc = AuditReport {
        version: 1,
        mode: "audit",
        summary: AuditSummary {
            files_checked: report.files_checked,
            comments: report.comments.len(),
        },
        comments: report
            .comments
            .iter()
            .map(|c| AuditComment {
                file: c.path.display().to_string(),
                language: &c.language,
                kind: c.kind,
                start_line: c.start_line,
                end_line: c.end_line,
                line_count: c.text.len(),
                words: c.words,
                following_code_lines: c.following_code_lines,
                text: &c.text,
            })
            .collect(),
    };
    serde_json::to_writer_pretty(&mut *out, &doc)?;
    writeln!(out)?;
    Ok(())
}
