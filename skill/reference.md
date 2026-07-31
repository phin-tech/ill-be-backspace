# backspace reference

## Rules

| id | fires when | key settings |
|---|---|---|
| `block-too-long` | a comment block exceeds a budget | `max_lines` (5), `max_words` (off), `max_chars` (off), `max_line_words` (off) |
| `comment-code-ratio` | comment lines / following code lines exceeds a ratio | `max_ratio` (1.5), `ratio_min_lines` (3) |
| `comment-restates-code` | the comment's words are mostly drawn from the code below it | `threshold` (0.8), `min_words` (6) — **off by default** |
| `banned-phrase` | comment text matches a word or regex | `words`, `preset`, `extend`, `patterns` |
| `suppression-needs-reason` | a suppression directive has no justification | `require_suppression_reason` (false) |

Docstrings and doc comments (`"""..."""`, `///`, `//!`, `/** */`) are exempt
unless `include_docstrings` is set.

## Configuration sources

**User config**, read first and applied as the weakest file layer:

- `$BACKSPACE_CONFIG_HOME/ill-be-backspace.toml`, else
- `$XDG_CONFIG_HOME/ill-be-backspace.toml`, else
- `~/.config/ill-be-backspace.toml`

`backspace.toml` is accepted as an alternative name in the same directory.

**Project config**, checked in this order, first match wins, walking up from the
target directory:

1. `.backspace.toml`
2. `backspace.toml`
3. `pyproject.toml` → `[tool.backspace]`
4. `package.json` → `"backspace"` key
5. `Cargo.toml` → `[package.metadata.backspace]`

Layers, later beating earlier:

```
defaults → user config → project config → [languages.<name>]
         → [[overrides]] (later entries win) → CLI flags → inline directives
```

Each layer only overrides the keys it names, so a project setting `max_lines`
leaves the user's `require_suppression_reason` intact.

**Banned phrases accumulate rather than replace.** `words` and `extend` from a
project are appended to whatever the user config already contributed; only
`patterns` discards the accumulated list. That is what keeps a personal word
list alive across every repo.

`backspace config show <path>` prints the resolved value of every key and which
layer produced it.

## Full schema

```toml
max_lines = 5
max_words = 40                     # optional, whole block
max_chars = 300                    # optional, whole block
max_line_words = 12                # optional, any single line
include_docstrings = false
merge_across_blank_lines = true    # a blank line does not split a comment block
require_suppression_reason = false
severity = "error"                 # or "warning": reports but exits 0
diff_only = true
select = ["block-too-long", "comment-code-ratio"]
ignore = []
exclude = ["**/vendor/**", "**/*.generated.*"]

[rules.block-too-long]
max_lines = 5

[rules.comment-code-ratio]
max_ratio = 1.5
ratio_min_lines = 3

[rules.comment-restates-code]      # off unless added to `select`
threshold = 0.8                    # vocabulary overlap that counts as restating
min_words = 6                      # shorter comments are not judged

[rules.banned-phrase]
preset = "llm-tells"               # the only preset; omit for none
words = ["substrate", "c++"]       # literal, escaped, whole-word, added on top
extend = ["(?i)as an ai"]          # regexes, added on top
patterns = []                      # regexes, replaces the accumulated list

[languages.go]                     # per-language budgets
max_lines = 8

[[overrides]]                      # per-path, later entries win
paths = ["tests/**", "scripts/**"]
max_lines = 15

[[languages.custom]]               # teach it a language it does not ship
name = "nix"
extensions = [".nix"]
line_comments = ["#"]
block_comments = [{ open = "/*", close = "*/" }]
```

## Language spec fields

`name`, `extensions`, `filenames`, `shebangs`, `line_comments`,
`block_comments` (`open`, `close`, `nested`), `doc_markers`, `strings`
(`delim`, `close`, `raw`, `multiline`, `escape`), `docstrings` (`none` or
`python`), `regex_literals`.

`backspace languages` lists what the current build understands.

## CLI

```
backspace [PATHS...] [--max-lines N] [--max-ratio F] [--max-words N] [--max-chars N]
          [--include-docstrings] [--select RULE] [--ignore RULE] [--exclude GLOB]
          [--diff | --diff=REF | --all] [--config PATH]
          [--format text|github|json] [--json] [--severity error|warning]
          [--stats] [--fail-on-unknown] [--jobs N]

backspace [PATHS...] --audit        # list comments, never fails

backspace prose [FILE]             # check writing, not source; stdin by default
       [--max-line-words N] [--json]

backspace config show <PATH>
backspace languages
backspace explain <RULE>
```

`--diff` requires the `=` form when given a ref (`--diff=main`), so that
`backspace --diff .` reads `.` as a path.

## JSON output

```json
{
  "version": 1,
  "summary": { "files_checked": 1, "violations": 2, "errors": 2, "warnings": 0 },
  "violations": [
    {
      "rule": "comment-code-ratio",
      "severity": "error",
      "file": "src/deploy.py",
      "start_line": 2, "end_line": 7, "column": 5,
      "message": "6 comment lines describe 2 lines of code (ratio 3.0, max 1.5)",
      "help": "...",
      "language": "python",
      "comment": ["...", "..."],
      "comment_line_count": 6,
      "following_code_lines": 2
    }
  ]
}
```

`comment` carries the offending text, so a consumer can rewrite the comment
without re-reading the file.

## Audit output

`--audit --json` reports every comment rather than every violation, and always
exits 0:

```json
{
  "version": 1,
  "mode": "audit",
  "summary": { "files_checked": 1, "comments": 2 },
  "comments": [
    {
      "file": "src/api.py",
      "language": "python",
      "kind": "line",
      "start_line": 41, "end_line": 41,
      "line_count": 1,
      "words": 18,
      "following_code_lines": 1,
      "text": ["Set the user's name to the provided value if it is not None…"]
    }
  ]
}
```

Pair with `--diff` to review only the comments the current change introduced.
