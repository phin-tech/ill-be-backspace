use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use backspace::cli::{Cli, Command, ConfigAction};
use backspace::config::Config;
use backspace::diff::{self, DiffSpec};
use backspace::report;
use backspace::rules;
use backspace::runner::{self, RunOptions};

/// Exit codes are part of the contract: pre-commit and CI branch on them.
const OK: u8 = 0;
const VIOLATIONS: u8 = 1;
const USAGE: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("backspace: {e:#}");
            ExitCode::from(USAGE)
        }
    }
}

fn run() -> Result<u8> {
    let cli = Cli::parse();

    if let Some(command) = &cli.command {
        return subcommand(command, &cli);
    }

    let args = &cli.check;
    let paths: Vec<PathBuf> = if args.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        args.paths.clone()
    };

    Config::validate_rule_ids(&args.select)?;
    Config::validate_rule_ids(&args.ignore)?;

    let mut config = load_config(&cli)?;
    config.add_excludes(&args.exclude)?;
    apply_cli_overrides(&mut config, &cli);

    if let Some(jobs) = args.jobs {
        rayon::ThreadPoolBuilder::new()
            .num_threads(jobs)
            .build_global()
            .ok();
    }

    let opts = RunOptions {
        changed: resolve_diff(&cli, &paths)?,
        fail_on_unknown: args.fail_on_unknown,
    };

    let report = runner::check_paths(&paths, &config, &opts)?;

    let mut out = anstream::stdout().lock();
    report::write(&mut out, &report, args.format(), args.stats)?;
    out.flush()?;

    // Warnings are reported but never fail the run; that is what makes
    // `severity = "warning"` usable as a soft rollout.
    Ok(if report.errors() > 0 { VIOLATIONS } else { OK })
}

fn load_config(cli: &Cli) -> Result<Config> {
    match &cli.check.config {
        Some(path) => Config::from_file(path)
            .with_context(|| format!("failed to load config from {}", path.display())),
        None => {
            let start = cli
                .check
                .paths
                .first()
                .cloned()
                .unwrap_or_else(|| PathBuf::from("."));
            Config::discover(&start)
        }
    }
}

fn apply_cli_overrides(config: &mut Config, cli: &Cli) {
    let a = &cli.check;
    config.cli.max_lines = a.max_lines;
    config.cli.max_words = a.max_words;
    config.cli.max_chars = a.max_chars;
    config.cli.max_ratio = a.max_ratio;
    config.cli.include_docstrings = a.include_docstrings.then_some(true);
    config.cli.severity = a.severity.map(Into::into);
    config.cli.select = a.select.clone();
    config.cli.ignore = a.ignore.clone();
}

/// Decides whether to restrict to changed lines, honouring `--all`, `--diff` and
/// the `diff_only` config key in that order.
fn resolve_diff(cli: &Cli, paths: &[PathBuf]) -> Result<Option<diff::ChangedLines>> {
    let a = &cli.check;
    if a.all {
        return Ok(None);
    }

    let spec = match &a.diff {
        Some(r) if r.is_empty() => Some(DiffSpec::Working),
        Some(r) => Some(DiffSpec::MergeBase(r.clone())),
        None => None,
    };
    let Some(spec) = spec else { return Ok(None) };

    let start = paths
        .first()
        .map(PathBuf::as_path)
        .unwrap_or(Path::new("."));
    let anchor = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(Path::new("."))
    };

    if !diff::is_git_repo(anchor) {
        eprintln!("backspace: not a git repository; checking whole files");
        return Ok(None);
    }
    let root = diff::repo_root(anchor)?;
    Ok(Some(diff::changed_lines(&spec, &root)?))
}

fn subcommand(command: &Command, cli: &Cli) -> Result<u8> {
    match command {
        Command::Languages => {
            let config = load_config(cli)?;
            for lang in config.registry().iter() {
                println!("{:<12} {}", lang.name, lang.extensions.join(" "));
            }
            Ok(OK)
        }
        Command::Explain { rule } => match rules::explain(rule) {
            Some(text) => {
                println!("{rule}\n\n{text}");
                Ok(OK)
            }
            None => {
                eprintln!(
                    "backspace: unknown rule `{rule}` (known: {})",
                    rules::ALL_RULES.join(", ")
                );
                Ok(USAGE)
            }
        },
        Command::Config {
            action: ConfigAction::Show { path },
        } => {
            let mut config = load_config(cli)?;
            apply_cli_overrides(&mut config, cli);

            let source = std::fs::read_to_string(path).unwrap_or_default();
            let language = config
                .registry()
                .detect(path, &source)
                .map(|l| l.name.clone())
                .unwrap_or_else(|| "unknown".to_string());

            let (resolved, prov) = config.resolve_verbose(path, &language);
            let show = |p: Option<&Path>| {
                p.map(|p| p.display().to_string())
                    .unwrap_or_else(|| "none".into())
            };
            println!("{}  ({language})", path.display());
            println!("  user config:    {}", show(config.user_source()));
            println!("  project config: {}", show(config.source()));
            println!();
            for (key, layer) in prov.iter() {
                println!("  {key:<26} = {:<24} {layer}", value_of(&resolved, key));
            }
            Ok(OK)
        }
    }
}

fn value_of(c: &backspace::config::ResolvedConfig, key: &str) -> String {
    match key {
        "max_lines" => c.max_lines.to_string(),
        "max_words" => opt(c.max_words),
        "max_chars" => opt(c.max_chars),
        "include_docstrings" => c.include_docstrings.to_string(),
        "merge_across_blank_lines" => c.merge_across_blank_lines.to_string(),
        "max_ratio" => format!("{:.2}", c.max_ratio),
        "ratio_min_lines" => c.ratio_min_lines.to_string(),
        "restate_threshold" => format!("{:.2}", c.restate_threshold),
        "restate_min_words" => c.restate_min_words.to_string(),
        "banned_phrases" => format!("{} pattern(s)", c.banned_phrases.len()),
        "select" => c.select.iter().cloned().collect::<Vec<_>>().join(","),
        "ignore" => c.ignore.iter().cloned().collect::<Vec<_>>().join(","),
        "severity" => c.severity.as_str().to_string(),
        "require_suppression_reason" => c.require_suppression_reason.to_string(),
        _ => String::new(),
    }
}

fn opt(v: Option<usize>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "none".into())
}
