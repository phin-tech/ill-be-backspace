# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

While the version is below 1.0, rule defaults and thresholds may change in a
minor release. New rules always ship disabled, so an upgrade will not start
failing a build that passed before.

## [Unreleased]

### Added

- A third severity, `note`, and the obligation spelled out on every finding:
  `[MUST fix]`, `[SHOULD fix]`, `[MAY leave as is]`. The reader of a finding is
  often an agent deciding what to change, and "warning" does not tell it whether
  it may decline. `note` never fails a build.
- `agent-tics` preset: phrasing that marks an assistant talking about its own
  work. Unlike `llm-tells`, every entry was measured against a human control
  corpus — 4.3M comment words of Neovim, Emacs and WordPress — and kept only if
  it appeared more in agent-written code. `load-bearing` (agent only),
  `pathological` outside its idiom (19x), `inert` (14x), `stomping` (7x), plus
  the reflexive agreement an assistant opens with.
- `plain-words` preset: `utilize` → `use`, `in order to` → `to`. The finding
  names the replacement rather than only the offence.
- Advisory entries, which report at `note` and explain what a word is properly
  for instead of banning it: `gate` is a conditional guard, `headline` is
  org-mode's word for a heading, `inert` is HTML's non-reactive attribute. An
  advisory stays advice whatever the configured severity.
- `except_before` on a phrase, for words that are only a tic outside a fixed
  idiom. Emacs uses `pathological` ten times and every one is `case`, `cases`,
  `situations` or `behavior`; the agent corpus stretches it over `caller` and
  `span`. Rust's `regex` has no lookaround, so the rule reads the following word
  itself.
- `preset` accepts a list: `preset = ["llm-tells", "agent-tics"]`.
- `backspace prose` takes several paths and walks directories for `.md`,
  `.markdown`, `.txt`, `.rst` and `.adoc`, so documentation can be reviewed the
  way comments already are.
- `explains-what-not-why` rule: flags a comment that both restates the code and
  gives no reason for it — at least `min_lines` (2) of prose, vocabulary overlap
  at or above `threshold` (0.6), and none of the built-in rationale markers
  (`because`, `since`, `so`, `otherwise`, `to avoid`, `must`, `workaround`, …).
  Extend the markers with `extend`, replace them with `markers`. This is the
  precise version of `comment-restates-code`: a comment giving a reason is exempt
  however much vocabulary it shares with the code, which is exactly the case that
  makes the older rule misfire. Measured across four repositories it reports 6
  findings where `comment-restates-code` reports 15. Comments opening with the
  name of the thing declared beneath them are exempt, since godoc requires that
  and Go has no syntax to mark a doc comment. **Off by default.**
- `passive-voice` rule: flags a form of `be` followed by a past participle. By
  default only passives naming their actor (`is set by the caller`), because
  those are the ones with a shorter active rewrite available; `require_agent =
  false` flags every passive. The restriction is what makes the rule usable —
  on a 6,800-file repository it cuts the findings from 3,386 to 361, and the
  difference was almost entirely predicate adjectives (`is unchanged`, `is
  needed`) that no rewrite improves. Works on comments and on `backspace prose`.
  **Off by default, and documented as a suggestion rather than a gate**: style
  rules of this kind systematically penalise people writing in a second
  language.
- `uniform-sentences` rule: flags prose whose sentences are all close to the
  same length, measured as the coefficient of variation of their word counts.
  Needs `min_sentences` (5) to judge a rhythm. Calibrated against this repo's own
  hand-written documents, which score 0.36 to 0.74 against a 0.30 default. It
  catches *unedited* generated text: `docs/harper-integration.md` was written by
  a model, revised, and scores 0.63. **Off by default.**
- `em-dash-habit` rule: flags more than `max_rate` (2.0) em dashes per hundred
  words once `min_count` (2) appear. The same documents peak at 1.3. **Off by
  default.**
- `llm-tells` gains the antithesis constructions — `it's not just X — it's Y`,
  `not only X but Y`, `it isn't about X, it's about Y` — and the current crop of
  vocabulary tics (`delve`, `tapestry`, `testament to`, `In conclusion`). The
  word half of the preset dates fast and is meant to be pruned; the shapes have
  outlasted several model generations. Regex entries can now carry a readable
  name, so a finding quotes `it's not just X — it's Y` rather than the pattern.
- `backspace prose --select`: restricts prose mode to named rules, so
  `--select passive-voice` checks voice without also applying the word list.
- `backspace prose`: checks plain writing rather than source, reading a file or
  stdin, using the same word list that governs comments. Only the rules that
  make sense without code are applied.
- Claude Code hooks in `hooks/`. `backspace-hook.sh` (`PostToolUse`) reports
  findings the moment a file is written, scoped with `--diff` so only the
  current session's comments are raised. `backspace-chat-hook.sh` (`Stop`) runs
  the word list over the assistant's own reply. Both report as context rather
  than blocking by default.

### Changed

- `llm-tells` no longer claims to detect a machine. Measured against the human
  control corpus, its entries point the other way: `Note that` appears 26.14
  times per 100k comment words in human code and 0.51 in agent-written code, and
  it produces more findings than the rest of the preset combined. The preset
  still flags padding, which is worth flagging whoever wrote it; for authorship,
  use `agent-tics`.
- `backspace prose` now honours `select` rather than always applying
  `banned-phrase` and `block-too-long`. A project that selects neither gets
  neither; a configured word list still enables `banned-phrase` on its own.
- The `banned-phrase` message reads "matches banned phrase" rather than "comment
  matches banned phrase", since it now applies to prose as well as comments.
- `llm-tells` findings quote the phrase a reader recognises rather than the
  regex behind it: `Note that` instead of `\bNote that\b`.

- `unapproved-word` rule: flags comment prose outside a configured vocabulary.
  Identifiers from the whole file are approved automatically, so a project's own
  terminology needs no entry. Ships a `plain-code` preset built on the EF 3000
  most-common English words plus systems terms. **Off by default, and currently
  a starting point rather than a finished vocabulary** — measured against three
  real repositories it still reports roughly 1,100 findings, mostly inflections
  (`sorted` where `sort` is listed). A stemmer would close most of that gap.

### Fixed

- A path that does not exist is now an error exiting `2`, rather than reporting
  "0 files checked" and exiting `0`. A typo in a CI invocation previously passed
  while checking nothing.

## [0.1.3] — 2026-07-29

### Added

- `max_line_words`: a budget applied to each comment line on its own, catching a
  single line that over-explains. The block-level `max_words` sums the whole
  comment and so cannot express this without also flagging legitimate multi-line
  blocks. Off by default; `12` is a good starting point — across 907 real
  single-line comments the median is 7 words and the 95th percentile is 12.
  Also available as `--max-line-words`.
- `--audit`: lists every comment in scope and always exits `0`. A review aid
  rather than a gate. Pair with `--diff` to re-read exactly what a change
  introduced, and with `--json` to hand that list to an agent.
- `comment-restates-code` rule: flags a comment whose vocabulary is mostly drawn
  from the code beneath it, splitting identifiers on case and underscores so
  `retry_counter` and "retry counter" compare. Section banners, code samples,
  URL templates and data shapes are excluded. **Off by default** — a good "why"
  comment must name the things it discusses, so it accrues overlap honestly and
  the rule cannot always tell that from genuine restatement. Tuned against
  ~7,400 files; at the default threshold of `0.8` it fires rarely.

### Changed

- `CommentBlock` now carries `following_code`, the text of the code lines
  beneath a comment, not only their count.
- The bundled Claude Code skill lists all five rules and covers single-line
  over-explanation and comment/code redundancy.

## [0.1.2] — 2026-07-29

### Added

- User-level configuration at `~/.config/ill-be-backspace.toml`, applied as the
  weakest file layer so personal settings follow you between projects. Honours
  `$XDG_CONFIG_HOME` and `$BACKSPACE_CONFIG_HOME`. Accepts everything a project
  config does, including custom languages and excludes.
- `words` under `[rules.banned-phrase]`: literal words rather than regexes.
  Entries are escaped and word-bounded, so `substrate` does not fire on
  `substrates` and `c++` is a word rather than a regex syntax error. Boundaries
  are applied only at ends that are word characters, since `\b` after `+` could
  never match.
- `prek` / `pre-commit` hook set covering fmt, clippy, tests, backspace itself
  and the usual file checks, run in CI as well so the two cannot drift.
- `backspace config show` reports the user config path alongside the project
  config path.

### Changed

- Banned phrases accumulate across configuration layers. A project's `words` and
  `extend` are appended to whatever the user config contributed; only `patterns`
  replaces the list outright. This is what keeps a personal word list alive in
  every repository.

### Fixed

- Glob patterns such as `tests/**` now match paths that a directory walk yields
  with a `./` prefix. Previously `exclude` and `[[overrides]]` silently failed to
  match during a recursive scan.

## [0.1.1] — 2026-07-29

### Added

- `aarch64-unknown-linux-gnu` binary, built on a native ARM runner.
- The Homebrew formula is generated by the release workflow, with checksums
  computed from the published assets so it cannot disagree with what is
  downloaded.

### Removed

- crates.io publishing. The crate is marked `publish = false` so it cannot be
  released there by accident.

### Fixed

- `uvx` instructions. The package is `ill-be-backspace` but the command is
  `backspace`, so `uvx --from ill-be-backspace backspace` is required.
- `brew install phin-tech/tap/backspace` works: the formula was advertised
  before it existed.
- `--diff` requires the `=` form when given a revision (`--diff=main`), so
  `backspace --diff .` reads `.` as a path rather than a git ref.
- Directory walks no longer descend into `.git`.
- Diff paths are canonicalised, so `--diff` matches the files a walk produces.

## [0.1.0] — 2026-07-29

First release.

### Added

- Comment scanner: a single-pass character state machine tracking string and
  comment state, so `s = "# not a comment"` is not read as a comment. Handles
  raw strings, template literals, nested block comments, Python docstrings and
  JavaScript regex literals.
- 19 languages defined declaratively in TOML and embedded at compile time:
  Python, Rust, Go, JavaScript, TypeScript, Bash, C/C++, Java, Kotlin, Swift,
  Ruby, PHP, Lua, SQL, YAML, TOML, HCL, Dockerfile, Makefile. Further languages
  can be added at runtime through `[[languages.custom]]` using the same schema.
- Rules `block-too-long` and `comment-code-ratio`, enabled by default, and
  `banned-phrase` with an opt-in `llm-tells` preset.
- Layered configuration from `.backspace.toml`, `backspace.toml`,
  `pyproject.toml`, `package.json` or `Cargo.toml`, with per-language and
  per-path overrides, and `backspace config show` reporting which layer set each
  value.
- `--diff` mode restricting findings to comment blocks the change touched, so
  adopting the tool on an existing repository does not surface its history.
- `text`, `github` and `json` reporters. The JSON payload carries the comment
  text and surrounding counts so a consumer need not re-read the file.
- Inline `backspace: ignore`, `backspace: ignore[rule]` and
  `backspace: ignore-file` directives, with optional
  `require_suppression_reason`.
- Distribution as PyPI wheels, Homebrew formula and GitHub Release binaries.
- A Claude Code skill in `skill/`.

[Unreleased]: https://github.com/phin-tech/ill-be-backspace/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/phin-tech/ill-be-backspace/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/phin-tech/ill-be-backspace/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/phin-tech/ill-be-backspace/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/phin-tech/ill-be-backspace/releases/tag/v0.1.0
