<p align="center">
  <img src="docs/img/100-percent-vibed.png" alt="100% vibed" width="640">
</p>

<h1 align="center">I'll Be Backspace</h1>

<p align="center">
  <em>The comment linter that says what everybody's thinking.</em>
</p>

---

## Friend, Can We Talk About Your Comments?

Let me paint you a picture. You open a file. You're a busy person. And there,
sitting on top of a two-line function like a wedding cake on a bicycle, is
*this*:

```python
# Sync pulls files; it does NOT recreate containers. That's enough for
# config a service reads live (Traefik file-watches dynamic/, Dashy
# re-reads conf.yml per request), but a compose change needs an `up` to
# take effect. Verified 2026-07-29: fixing postgres's data mount and
# force-syncing left the same container id running the old mount, so the
# crash loop continued. Hence the two-step below.
sync(service)
compose_up(service)
```

Six lines of comment. Two lines of code. Somewhere in there is one useful fact,
and it is doing hard time.

Now — what if I told you there was a **better way**?

```console
$ backspace deploy.py
deploy.py:2:5: error: comment-code-ratio: 6 comment lines describe 2 lines of code (ratio 3.0, max 1.5)
    2 | Sync pulls files; it does NOT recreate containers. That's enough for
    3 | config a service reads live (Traefik file-watches dynamic/, Dashy
      | ... 3 more lines
    7 | crash loop continued. Hence the two-step below.
      = help: A comment longer than the code it describes usually restates the
             code. Say what the code cannot.

1 file checked, 1 violation
```

That's it. That's the product. And folks, it is **fast**.

## What's In The Box

- **Nineteen languages out of the box.** Python, Rust, Go, TypeScript,
  JavaScript, Bash, C/C++, Java, Kotlin, Swift, Ruby, PHP, Lua, SQL, YAML,
  TOML, HCL, Dockerfile, Makefile. Adding another is *one TOML file*.
- **It knows a string from a comment.** `s = "# not a comment"` does not fool
  it. Neither do raw strings, template literals, nested block comments, or
  JavaScript regex literals. This is the part everyone else gets wrong.
- **Diff-aware.** Point it at a mature repo and it won't hand you four hundred
  findings on code you didn't write. It checks what *you* touched.
- **Docstrings are safe.** Your API documentation is supposed to be long. We
  leave it alone unless you ask.
- **A `--json` mode built for robots.** Ships the offending comment text right
  in the payload, so your agent can fix it without opening the file again.
- **Configurable six ways from Sunday**, and — here's the kicker — a
  `config show` command that tells you *which layer* set every single value.
  No guessing. Never guessing.

## Get It Installed. Right Now. Today.

**Pre-commit** — and this is where it really sings:

```yaml
repos:
  - repo: https://github.com/phin-tech/ill-be-backspace
    rev: v0.1.1
    hooks:
      - id: backspace
```

**Or any of these**, whichever suits your operation:

```console
$ uvx --from ill-be-backspace backspace   # try before you buy
$ pipx install ill-be-backspace           # a wheel, no Rust required
$ brew install phin-tech/tap/backspace
```

The package is `ill-be-backspace`; the command it installs is `backspace`.

## Kick The Tires

```console
$ backspace                       # current directory, changed lines only
$ backspace src/ --all            # the whole lot
$ backspace --diff=main           # everything your branch changed
$ backspace src/ --json           # for machines
$ backspace --stats               # who's the worst offender?
```

Exit codes are `0` clean, `1` violations, `2` you've configured something
strange. Your CI will know exactly what to do.

## Three Rules. That's All It Takes.

| Rule | What it catches |
|---|---|
| `block-too-long` | More than `max_lines` (5) consecutive lines of comment. |
| `comment-code-ratio` | A comment longer than the code beneath it. **This is the one that finds the real offenders.** |
| `banned-phrase` | Regexes you pick. There's an `llm-tells` preset for `Verified 2026-…`, `Note that`, `it does NOT`, and friends. Opt-in — we're not here to preach. |

`backspace explain <rule>` if you want the long version.

## Make It Yours

It reads config from wherever you already keep it — `.backspace.toml`,
`pyproject.toml`, `package.json`, or `Cargo.toml`. No new file unless you want
one.

```toml
max_lines = 5
exclude = ["**/vendor/**"]

[rules.banned-phrase]
preset = "llm-tells"

[languages.go]
max_lines = 8          # licence headers, we understand

[[overrides]]
paths = ["tests/**"]
max_lines = 15         # tests get a little room to breathe
```

## Ban Your Own Words

Got a word you never want to see again? Put it in
`~/.config/ill-be-backspace.toml` and it follows you into every repo you touch:

```toml
[rules.banned-phrase]
words = ["substrate", "delve into", "leverage", "seamless", "robust"]
```

`words` are literal, not regexes — they're escaped for you, matched
whole-word and case-insensitively. So `substrate` won't fire on `substrates`,
and `c++` is a word rather than a syntax error. If you *want* a regex, use
`extend = ['Verified \d{4}-\d{2}-\d{2}']`.

### Your Words Are Safe

Here's the important part. A project **adds to** your list, it does not replace
it:

```toml
# some repo's .backspace.toml
[rules.banned-phrase]
words = ["synergy"]
```

You still get `substrate`, `delve into`, `leverage`, `seamless`, `robust` —
*plus* `synergy`. The only thing that wipes your list is a project explicitly
saying `patterns = [...]`, which is the documented "start from scratch" switch.

| key | takes | what it does |
|---|---|---|
| `words` | plain text | escaped, whole-word, case-insensitive — **start here** |
| `extend` | regex | appended to whatever's accumulated |
| `patterns` | regex | **replaces** the accumulated list |
| `preset` | `"llm-tells"` | the built-in bundle |

## Who Set What, And Where

Values stack up in this order, each one beating the last:

```
defaults → ~/.config/ill-be-backspace.toml → project config
         → [languages.*] → [[overrides]] → CLI flags → inline directives
```

Every layer only changes the keys it names. Set `max_lines` in a project and
your personal `require_suppression_reason` still applies.

And when you can't remember why a value is what it is, just ask:

```console
$ backspace config show src/api/handlers.py
src/api/handlers.py  (python)
  user config:    /Users/you/.config/ill-be-backspace.toml
  project config: .backspace.toml

  max_lines                  = 15            overrides[0]
  max_ratio                  = 1.50          default
  banned_phrases             = 6 pattern(s)  user config
  severity                   = error         command line
```

No guessing. Never guessing.

Full schema in [`skill/reference.md`](skill/reference.md).

## Sometimes The Long Comment Is Right

Wire formats. Protocol quirks. We're reasonable people:

```python
# backspace: ignore[block-too-long] — frame layout is fixed by RFC 9114 §4.2
```

`backspace: ignore` for everything, `backspace: ignore-file` near the top of a
file for the whole thing. Set `require_suppression_reason = true` and it'll
insist you explain yourself — which, frankly, you should.

## For The Robots

There's a Claude Code skill in [`skill/`](skill/). Drop it in and your agent
writes shorter comments *in the first place*, which beats catching them at
commit time every day of the week:

```console
$ cp -r skill ~/.claude/skills/backspace
```

## Under The Hood

A clanker-rolled character state machine, one pass, no parser generators, no
grammar downloads. Language definitions are plain TOML embedded at compile time
— and you can add your own at *runtime*, same schema, no recompile.

Two things it's honest about: JavaScript regex-literal detection and Python
docstring position are heuristics, not a parser. They're tested against the
cases that matter and documented where they aren't perfect.

197 tests. It passes its own lint. We wouldn't dare ship it otherwise.

## Contributing

New language? One file in `languages/`, one line in `src/lang/mod.rs`, a
`good_` and a `bad_` fixture in `tests/fixtures/`. That's the whole ceremony.

```console
$ cargo test
$ cargo run -- --all .
```

## License

MIT. Go build something.
