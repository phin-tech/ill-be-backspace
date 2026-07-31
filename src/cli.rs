//! Command-line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::Severity;
use crate::report::Format;

#[derive(Debug, Parser)]
#[command(
    name = "backspace",
    version,
    about = "Flags comment blocks that have outgrown the code they describe.",
    long_about = None,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub check: CheckArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Show the fully resolved configuration for a path, and where each value
    /// came from.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// List the languages this build understands.
    Languages,
    /// Explain what a rule checks and why.
    Explain { rule: String },
    /// Check prose rather than source: reads a file or stdin and applies the
    /// word list to plain writing. Same list that governs comments.
    Prose {
        /// File to read. Omit to read stdin.
        file: Option<PathBuf>,
        /// Maximum words on a single line.
        #[arg(long)]
        max_line_words: Option<usize>,
        /// Enable only these rules. Rules needing code are ignored here.
        #[arg(long, value_name = "RULE")]
        select: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    Show { path: PathBuf },
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Files or directories to check. Pre-commit passes these itself.
    pub paths: Vec<PathBuf>,

    /// Maximum lines of comment prose in one block.
    #[arg(long)]
    pub max_lines: Option<usize>,

    /// Maximum words in one comment block.
    #[arg(long)]
    pub max_words: Option<usize>,

    /// Maximum characters in one comment block.
    #[arg(long)]
    pub max_chars: Option<usize>,

    /// Maximum words on any single comment line. Catches the one-line essay.
    #[arg(long)]
    pub max_line_words: Option<usize>,

    /// Maximum ratio of comment lines to the code lines they precede.
    #[arg(long)]
    pub max_ratio: Option<f64>,

    /// Also check docstrings and doc comments, which are exempt by default.
    #[arg(long)]
    pub include_docstrings: bool,

    /// Enable only these rules.
    #[arg(long, value_name = "RULE")]
    pub select: Vec<String>,

    /// Disable these rules. Wins over --select.
    #[arg(long, value_name = "RULE")]
    pub ignore: Vec<String>,

    /// Skip paths matching these globs.
    #[arg(long, value_name = "GLOB")]
    pub exclude: Vec<String>,

    /// Only report comments the diff touched. Bare: uncommitted changes.
    /// With a ref: everything since the merge base with that ref.
    /// Written `--diff` or `--diff=REF`; the `=` is required so that
    /// `backspace --diff .` reads `.` as a path rather than a revision.
    #[arg(long, value_name = "REF", num_args = 0..=1, require_equals = true,
          default_missing_value = "")]
    pub diff: Option<String>,

    /// Check whole files, overriding a `diff_only` setting in config.
    #[arg(long, conflicts_with = "diff")]
    pub all: bool,

    /// Use this config file instead of discovering one.
    #[arg(long, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[arg(long, value_name = "FORMAT", default_value = "text")]
    pub format: Format,

    /// Shorthand for --format json.
    #[arg(long, conflicts_with = "format")]
    pub json: bool,

    /// Report findings but always exit 0.
    #[arg(long, value_name = "LEVEL")]
    pub severity: Option<SeverityArg>,

    /// List every comment instead of checking it. Always exits 0 — this is a
    /// review aid, not a gate. Pair with --diff to review only what you changed.
    #[arg(long)]
    pub audit: bool,

    /// Print counts by rule and language.
    #[arg(long)]
    pub stats: bool,

    /// Fail on files whose type is not recognised, instead of skipping them.
    #[arg(long)]
    pub fail_on_unknown: bool,

    /// Worker threads. Defaults to the number of cores.
    #[arg(long, short)]
    pub jobs: Option<usize>,
}

impl CheckArgs {
    pub fn format(&self) -> Format {
        if self.json {
            Format::Json
        } else {
            self.format
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum SeverityArg {
    Error,
    Warning,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::Error => Severity::Error,
            SeverityArg::Warning => Severity::Warning,
        }
    }
}

impl clap::builder::ValueParserFactory for Format {
    type Parser = clap::builder::ValueParser;
    fn value_parser() -> Self::Parser {
        clap::builder::ValueParser::new(|s: &str| s.parse::<Format>())
    }
}
