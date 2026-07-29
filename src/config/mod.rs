//! Configuration discovery and resolution.
//!
//! Settings come from several layers. [`ResolvedConfig`] is the flattened result
//! for one specific file, so rules never have to know the layering existed, and
//! [`Provenance`] records which layer produced each value — deep configurability
//! is only usable if it is inspectable.

pub mod schema;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context as _, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

use crate::lang::Registry;
use crate::rules::{self, ALL_RULES};
use schema::{ConfigFile, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// Where a resolved value came from. Ordered weakest to strongest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Default,
    File,
    Language,
    Override(usize),
    Cli,
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Layer::Default => write!(f, "default"),
            Layer::File => write!(f, "config file"),
            Layer::Language => write!(f, "language override"),
            Layer::Override(i) => write!(f, "overrides[{i}]"),
            Layer::Cli => write!(f, "command line"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Provenance {
    entries: BTreeMap<&'static str, Layer>,
}

impl Provenance {
    fn set(&mut self, key: &'static str, layer: Layer) {
        self.entries.insert(key, layer);
    }

    pub fn layer_of(&self, key: &str) -> Option<Layer> {
        self.entries.get(key).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&&'static str, &Layer)> {
        self.entries.iter()
    }
}

#[derive(Debug, Clone, Default)]
pub struct CliOverrides {
    pub max_lines: Option<usize>,
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,
    pub max_ratio: Option<f64>,
    pub include_docstrings: Option<bool>,
    pub severity: Option<Severity>,
    pub select: Vec<String>,
    pub ignore: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// Maximum consecutive lines of comment prose in one block.
    pub max_lines: usize,
    /// Optional prose-volume budgets, for the single very long wrapped line that
    /// a line-count budget cannot see.
    pub max_words: Option<usize>,
    pub max_chars: Option<usize>,

    /// Docstrings and doc comments are legitimate API documentation and often
    /// long, so they are exempt unless explicitly opted in.
    pub include_docstrings: bool,
    pub merge_across_blank_lines: bool,

    pub max_ratio: f64,
    pub ratio_min_lines: usize,

    pub banned_phrases: Vec<String>,

    pub select: BTreeSet<String>,
    pub ignore: BTreeSet<String>,
    pub severity: Severity,
    pub require_suppression_reason: bool,
}

impl Default for ResolvedConfig {
    fn default() -> Self {
        Self {
            max_lines: 5,
            max_words: None,
            max_chars: None,
            include_docstrings: false,
            merge_across_blank_lines: true,
            max_ratio: 1.5,
            ratio_min_lines: 3,
            banned_phrases: Vec::new(),
            select: ["block-too-long", "comment-code-ratio"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ignore: BTreeSet::new(),
            severity: Severity::Error,
            require_suppression_reason: false,
        }
    }
}

impl ResolvedConfig {
    /// `ignore` always wins over `select`, so a broad selection can be narrowed
    /// without rewriting it.
    pub fn rule_enabled(&self, id: &str) -> bool {
        !self.ignore.contains(id) && self.select.contains(id)
    }

    pub fn scan_options(&self) -> crate::scan::ScanOptions {
        crate::scan::ScanOptions {
            merge_across_blank_lines: self.merge_across_blank_lines,
        }
    }
}

/// Config-file names checked in each directory, strongest first. Dedicated files
/// beat package-manager files so a project can opt out of an inherited setting.
const DEDICATED: &[&str] = &[".backspace.toml", "backspace.toml"];

#[derive(Debug, Clone)]
pub struct Config {
    file: ConfigFile,
    source: Option<PathBuf>,
    registry: Registry,
    exclude: GlobSet,
    override_globs: Vec<GlobSet>,
    pub cli: CliOverrides,
    pub diff_only: Option<bool>,
}

impl Config {
    /// Walks up from `start` looking for a config file. Returns defaults if none
    /// is found — a project with no config is a supported state, not an error.
    pub fn discover(start: &Path) -> Result<Config> {
        let start = if start.is_dir() {
            start.to_path_buf()
        } else {
            start.parent().unwrap_or(Path::new(".")).to_path_buf()
        };

        for dir in start.ancestors() {
            if let Some((file, path)) = load_from_dir(dir)? {
                return Config::build(file, Some(path));
            }
        }
        Config::build(ConfigFile::default(), None)
    }

    pub fn from_file(path: &Path) -> Result<Config> {
        let file = parse_any(path)?
            .with_context(|| format!("{} contains no backspace configuration", path.display()))?;
        Config::build(file, Some(path.to_path_buf()))
    }

    pub fn defaults() -> Config {
        Config::build(ConfigFile::default(), None).expect("default config is always valid")
    }

    fn build(file: ConfigFile, source: Option<PathBuf>) -> Result<Config> {
        let mut registry = Registry::builtin().clone();
        if let Some(langs) = &file.languages {
            for spec in &langs.custom {
                registry
                    .insert(spec.clone())
                    .map_err(|e| anyhow::anyhow!("invalid custom language: {e}"))?;
            }
        }

        let exclude = build_globset(file.exclude.as_deref().unwrap_or(&[]))?;
        let override_globs = file
            .overrides
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|o| build_globset(&o.paths))
            .collect::<Result<Vec<_>>>()?;

        let cfg = Config {
            diff_only: file.diff_only,
            file,
            source,
            registry,
            exclude,
            override_globs,
            cli: CliOverrides::default(),
        };
        cfg.validate()?;
        Ok(cfg)
    }

    /// Rejects unknown rule ids and uncompilable phrase patterns up front, so a
    /// typo fails loudly at startup instead of silently disabling a check.
    fn validate(&self) -> Result<()> {
        let mut settings: Vec<&Settings> = Vec::new();
        let top = self.file.settings();
        settings.push(&top);
        if let Some(l) = &self.file.languages {
            settings.extend(l.overrides.values());
        }
        if let Some(o) = &self.file.overrides {
            settings.extend(o.iter().map(|o| &o.settings));
        }

        for s in settings {
            for id in s.select.iter().chain(s.ignore.iter()).flatten() {
                if !ALL_RULES.contains(&id.as_str()) {
                    bail!(
                        "unknown rule `{id}` (known rules: {})",
                        ALL_RULES.join(", ")
                    );
                }
            }
            if let Some(p) = s.rules.as_ref().and_then(|r| r.banned_phrase.as_ref()) {
                if let Some(preset) = &p.preset {
                    if preset != "llm-tells" {
                        bail!("unknown banned-phrase preset `{preset}` (known: llm-tells)");
                    }
                }
                let all: Vec<String> = p
                    .patterns
                    .iter()
                    .chain(p.extend.iter())
                    .flatten()
                    .cloned()
                    .collect();
                rules::compile_phrases(&all).map_err(|e| anyhow::anyhow!(e))?;
            }
        }
        Ok(())
    }

    pub fn source(&self) -> Option<&Path> {
        self.source.as_deref()
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Adds exclude globs from the command line on top of the configured ones.
    pub fn add_excludes(&mut self, patterns: &[String]) -> Result<()> {
        if patterns.is_empty() {
            return Ok(());
        }
        let mut all: Vec<String> = self.file.exclude.clone().unwrap_or_default();
        all.extend(patterns.iter().cloned());
        self.exclude = build_globset(&all)?;
        self.file.exclude = Some(all);
        Ok(())
    }

    /// Rejects rule ids that came from the command line, so a typo fails loudly
    /// instead of silently disabling a check.
    pub fn validate_rule_ids(ids: &[String]) -> Result<()> {
        for id in ids {
            if !ALL_RULES.contains(&id.as_str()) {
                bail!(
                    "unknown rule `{id}` (known rules: {})",
                    ALL_RULES.join(", ")
                );
            }
        }
        Ok(())
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        self.exclude.is_match(normalize(path))
    }

    pub fn resolve(&self, path: &Path, language: &str) -> ResolvedConfig {
        self.resolve_verbose(path, language).0
    }

    pub fn resolve_verbose(&self, path: &Path, language: &str) -> (ResolvedConfig, Provenance) {
        let mut rc = ResolvedConfig::default();
        let mut prov = Provenance::default();
        for key in TRACKED_KEYS {
            prov.set(key, Layer::Default);
        }

        apply(&mut rc, &self.file.settings(), Layer::File, &mut prov);

        if let Some(langs) = &self.file.languages {
            if let Some(s) = langs.overrides.get(language) {
                apply(&mut rc, s, Layer::Language, &mut prov);
            }
        }

        // Later overrides win, so the most specific rule is written last.
        if let Some(overrides) = &self.file.overrides {
            for (i, o) in overrides.iter().enumerate() {
                if self.override_globs[i].is_match(normalize(path)) {
                    apply(&mut rc, &o.settings, Layer::Override(i), &mut prov);
                }
            }
        }

        apply_cli(&mut rc, &self.cli, &mut prov);
        (rc, prov)
    }
}

const TRACKED_KEYS: &[&str] = &[
    "max_lines",
    "max_words",
    "max_chars",
    "include_docstrings",
    "merge_across_blank_lines",
    "max_ratio",
    "ratio_min_lines",
    "banned_phrases",
    "select",
    "ignore",
    "severity",
    "require_suppression_reason",
];

macro_rules! set {
    ($rc:expr, $prov:expr, $layer:expr, $field:ident, $value:expr) => {
        if let Some(v) = $value {
            $rc.$field = v;
            $prov.set(stringify!($field), $layer);
        }
    };
}

fn apply(rc: &mut ResolvedConfig, s: &Settings, layer: Layer, prov: &mut Provenance) {
    set!(rc, prov, layer, max_lines, s.max_lines);
    set!(rc, prov, layer, include_docstrings, s.include_docstrings);
    set!(
        rc,
        prov,
        layer,
        merge_across_blank_lines,
        s.merge_across_blank_lines
    );
    set!(rc, prov, layer, severity, s.severity);
    set!(
        rc,
        prov,
        layer,
        require_suppression_reason,
        s.require_suppression_reason
    );

    if s.max_words.is_some() {
        rc.max_words = s.max_words;
        prov.set("max_words", layer);
    }
    if s.max_chars.is_some() {
        rc.max_chars = s.max_chars;
        prov.set("max_chars", layer);
    }
    if let Some(v) = &s.select {
        rc.select = v.iter().cloned().collect();
        prov.set("select", layer);
    }
    if let Some(v) = &s.ignore {
        rc.ignore = v.iter().cloned().collect();
        prov.set("ignore", layer);
    }

    let Some(r) = &s.rules else { return };
    if let Some(l) = &r.block_too_long {
        set!(rc, prov, layer, max_lines, l.max_lines);
        if l.max_words.is_some() {
            rc.max_words = l.max_words;
            prov.set("max_words", layer);
        }
        if l.max_chars.is_some() {
            rc.max_chars = l.max_chars;
            prov.set("max_chars", layer);
        }
    }
    if let Some(x) = &r.comment_code_ratio {
        set!(rc, prov, layer, max_ratio, x.max_ratio);
        set!(rc, prov, layer, ratio_min_lines, x.ratio_min_lines);
    }
    if let Some(p) = &r.banned_phrase {
        // `patterns` replaces the preset outright; `extend` always adds on top.
        let mut phrases = match (&p.patterns, &p.preset) {
            (Some(pats), _) => pats.clone(),
            (None, Some(_)) => rules::llm_tells_preset(),
            (None, None) => rc.banned_phrases.clone(),
        };
        if let Some(extra) = &p.extend {
            phrases.extend(extra.iter().cloned());
        }
        rc.banned_phrases = phrases;
        prov.set("banned_phrases", layer);

        // A phrase list with nothing selected would be inert; turn the rule on.
        if !rc.banned_phrases.is_empty() && !rc.ignore.contains(rules::BANNED_PHRASE) {
            rc.select.insert(rules::BANNED_PHRASE.to_string());
        }
    }
}

fn apply_cli(rc: &mut ResolvedConfig, cli: &CliOverrides, prov: &mut Provenance) {
    let layer = Layer::Cli;
    set!(rc, prov, layer, max_lines, cli.max_lines);
    set!(rc, prov, layer, max_ratio, cli.max_ratio);
    set!(rc, prov, layer, include_docstrings, cli.include_docstrings);
    set!(rc, prov, layer, severity, cli.severity);

    if cli.max_words.is_some() {
        rc.max_words = cli.max_words;
        prov.set("max_words", layer);
    }
    if cli.max_chars.is_some() {
        rc.max_chars = cli.max_chars;
        prov.set("max_chars", layer);
    }
    if !cli.select.is_empty() {
        rc.select = cli.select.iter().cloned().collect();
        prov.set("select", layer);
    }
    if !cli.ignore.is_empty() {
        rc.ignore.extend(cli.ignore.iter().cloned());
        prov.set("ignore", layer);
    }
}

/// Drops `./` segments so a glob like `tests/**` matches a walked `./tests/a.py`.
fn normalize(path: &Path) -> PathBuf {
    path.components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .collect()
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("invalid glob `{p}`"))?);
    }
    b.build().context("failed to build glob set")
}

fn load_from_dir(dir: &Path) -> Result<Option<(ConfigFile, PathBuf)>> {
    for name in DEDICATED {
        let path = dir.join(name);
        if path.is_file() {
            let file = parse_dedicated(&path)?;
            return Ok(Some((file, path)));
        }
    }
    for name in ["pyproject.toml", "package.json", "Cargo.toml"] {
        let path = dir.join(name);
        if path.is_file() {
            if let Some(file) = parse_any(&path)? {
                return Ok(Some((file, path)));
            }
        }
    }
    Ok(None)
}

fn parse_dedicated(path: &Path) -> Result<ConfigFile> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
}

/// Reads a config from any supported file, returning `None` when the file exists
/// but carries no backspace section.
fn parse_any(path: &Path) -> Result<Option<ConfigFile>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let ctx = || format!("failed to parse {}", path.display());

    match name {
        "package.json" => {
            let v: serde_json::Value = serde_json::from_str(&text).with_context(ctx)?;
            match v.get("backspace") {
                Some(section) => Ok(Some(
                    serde_json::from_value(section.clone()).with_context(ctx)?,
                )),
                None => Ok(None),
            }
        }
        "pyproject.toml" => {
            let v: toml::Value = toml::from_str(&text).with_context(ctx)?;
            extract(v.get("tool").and_then(|t| t.get("backspace")), ctx)
        }
        "Cargo.toml" => {
            let v: toml::Value = toml::from_str(&text).with_context(ctx)?;
            extract(
                v.get("package")
                    .and_then(|p| p.get("metadata"))
                    .and_then(|m| m.get("backspace")),
                ctx,
            )
        }
        _ => Ok(Some(parse_dedicated(path)?)),
    }
}

fn extract<F, S>(section: Option<&toml::Value>, ctx: F) -> Result<Option<ConfigFile>>
where
    F: FnOnce() -> S,
    S: std::fmt::Display + Send + Sync + 'static,
{
    match section {
        Some(s) => Ok(Some(s.clone().try_into().with_context(ctx)?)),
        None => Ok(None),
    }
}
