---
name: backspace
description: >
  Check source comments for over-long, over-explaining or redundant comments, and
  guidance on writing proportionate ones. Use when writing or reviewing code
  comments, when re-reading comments you just wrote, when the user mentions comment
  bloat or verbose comments, or when they mention backspace.
---

# Writing comments that survive

The failure mode this catches is a comment that grew into a narrative: a
multi-paragraph aside with changelog entries, verification dates, and
"does X; does NOT do Y" contrasts, sitting above two lines of code.

## What to write

Comment the invariant a reader cannot derive from the code.

- **Why, not what.** If the comment restates the next line in prose, delete it.
- **History belongs in git.** Verification dates, "I tried X and it failed",
  and incident write-ups go in the commit message, where they stay attached to
  the change that motivated them and do not rot in the source.
- **Length should track the code.** A comment longer than the code it describes
  is a smell. If the explanation genuinely needs six lines, the code probably
  needs a named function instead.
- **One fact per comment.** Contrasting what something does with what it does
  not do usually means two separate things need names.
- **A single line can over-explain too.** Length is not only about line count.
  If one line needs more than a dozen words, it is usually narrating the code
  rather than adding to it. Real single-line comments run about seven words.
- **Never restate the identifiers.** `# increment the retry counter` above
  `retry_counter += 1` uses the code's own vocabulary and adds nothing.

Rewriting the example above:

```python
# Compose changes need `up`; sync only refreshes files on disk.
sync(service)
compose_up(service)
```

## Checking

```bash
backspace <files> --all      # whole files
backspace --diff             # only what the current change touched
backspace <files> --json     # machine-readable, includes the comment text
```

Exit codes: `0` clean, `1` violations, `2` bad usage or config.

## Reviewing your own comments

After writing code, re-read what you wrote:

```bash
backspace --diff --audit --json
```

This lists every comment the change introduced and always exits 0. Use it to
check each one against the guidance above before handing the work over.

## Reading a finding

Each violation names a rule:

| rule | what it means |
|---|---|
| `block-too-long` | too many lines, words, or words on one line |
| `comment-code-ratio` | the comment is longer than the code beneath it |
| `comment-restates-code` | the comment's words already appear in that code |
| `banned-phrase` | matched a configured word or pattern |
| `suppression-needs-reason` | an `ignore` directive with no justification |

`backspace explain <rule>` describes any of them in full.

## When a long comment is genuinely right

Wire formats, protocol quirks and licence headers sometimes need the words. Say
so explicitly rather than deleting the check:

```python
# backspace: ignore[block-too-long] — frame layout is fixed by RFC 9114 §4.2
```

A bare `backspace: ignore` works too, and `backspace: ignore-file` near the top
of a file exempts the whole file. Prefer the narrowest form, and always give a
reason: the next reader needs to know whether the exemption still applies.

## Personal word lists

A user can ban words for themselves in `~/.config/ill-be-backspace.toml`:

```toml
[rules.banned-phrase]
words = ["substrate", "delve into", "leverage"]
```

These are literal words, escaped and whole-word matched. A project's own list
adds to them rather than replacing them, so a personal list survives across
repos.

The same list applies to plain writing, not just comments:

```bash
echo "some text" | backspace prose
```

If the user has banned a word, avoid it in your replies to them as well as in
the code you write.

Full configuration schema and rule list: see `reference.md` in this skill.
