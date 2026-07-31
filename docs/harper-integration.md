# Design note: Harper, and STE-style grammar

Status: proposed, not built. Written at the end of the session that measured the
vocabulary rule, so the next session does not have to rediscover the numbers.

## Why Harper

[harper-core](https://crates.io/crates/harper-core) 2.7.0, Apache-2.0, actively
maintained. Measured locally rather than assumed:

| | |
|---|---|
| binary, linked and stripped | 1.6 MB (backspace today is 3.8 MB) |
| bundled dictionary | 134,704 words, FST-compressed |
| required dependencies | 27, including `harper-brill` (Brill POS tagger) |
| compile cost | 64 s wall, 258 s CPU, cold |

The cost is compile time, not runtime weight. It needs no model downloads and
works offline, so it does not break the single-binary promise.

## What it fixes

`unapproved-word` currently reports ~1,155 findings across roux, clod and orca.
The residue is almost entirely inflections: `sorted`, `keeps`, `writes`,
`callers`, `explicitly`. The EF 3000 list holds base forms only. A 134k-word
dictionary has the inflections, so swapping the vocabulary source should do more
than any further hand-curation.

It also reframes the rule usefully. "Only these approved words" is STE's model
and the measurements say it does not fit code comments — the domain is too
broad. "This word is neither real English nor anywhere in your codebase" is a
better-shaped rule: it catches typos and genuine jargon, and people would leave
it on.

## Plan

1. Add `harper-core` behind a **non-default cargo feature** (`grammar`). Keep a
   build without it, so the compile cost is opt-in for contributors.
2. Publish the wheel and release binaries **with** the feature on. The 64 s is
   CI's problem, not a user's.
3. Point `unapproved-word` at Harper's dictionary when the feature is on,
   keeping the file-wide code-identifier approval that already exists. Retire
   `vocabularies/plain-code.txt` to a fallback for `--no-default-features`.
4. Delete the `inflections_are_not_yet_recognised` test once it passes; it exists
   to record a known gap, not to be preserved.

## STE-style grammar rules: which are worth it

STE has ~60 writing rules. They were written so a technician cannot misread an
aircraft maintenance procedure. Most do not transfer to code comments.

**Worth building:**

- **Sentence length cap.** STE caps procedural sentences at 20 words. Needs no
  Harper at all — split on `.!?` and count. Highest value per line of code here,
  and it generalises the existing `max_line_words`.
- **Passive voice**, if Harper exposes it. "The value is set by the caller" →
  "the caller sets the value". Genuinely clearer in comments.

**Not worth building:**

- **Article usage**, **present tense**, **no gerunds as nouns**. Pedantic for
  comments, and the noise would swamp the signal.
- **One instruction per sentence.** Aimed at procedures, not explanations.

## The caution that must survive into the docs

Grammar rules of this kind systematically penalise non-native English speakers,
who often write more formally and completely than native speakers. STE was
designed to *help* that group read; enforcing its style rules on the same group's
writing inverts the intent.

So: ship these **off by default**, document why, and never let them reach
`severity = "error"` in the shipped config. The existing pre-1.0 policy already
says new rules ship disabled — this is the case that policy exists for.

## Also considered, and dropped

Harvesting approved words from dependency manifests (`package.json`,
`Cargo.toml`, `pyproject.toml`). It addressed about 5% of the measured gaps —
`eslint`, `typescript`, `vitest` — which file-wide identifier approval and a real
dictionary already cover. Not worth the manifest-parsing surface.

## "Why, not what" — the rule this is all circling

Asked at the end of the session, and it deserves its own entry because it is the
principle the whole tool exists to serve.

It cannot be detected directly — no local check knows whether a fact was
derivable from the code. But it can be approximated well by combining two
signals we can measure, one of which already exists:

1. **Does it restate the code?** `comment-restates-code` already answers this.
   High vocabulary overlap is the "what" signal.
2. **Does it give a reason?** Rationale has vocabulary: `because`, `so that`,
   `since`, `otherwise`, `to avoid`, `prevents`, `needed for`, `due to`,
   `workaround`, `historically`, `must`, `cannot`, `would`. A comment carrying
   none of these is not explaining anything.

Neither alone is precise. **Together they are:** a comment that is long, shares
most of its vocabulary with the code beneath it, *and* contains no rationale
marker is almost certainly narrating the what. That conjunction should have a
far better false-positive rate than `comment-restates-code` has on its own —
which matters, because the measured weakness of that rule is exactly that a good
"why" comment must name the things it discusses, and so scores high overlap
honestly. Requiring the *absence* of a reason marker is what separates them.

Proposed as `explains-what-not-why`, off by default, needing no new dependency:

```toml
[rules.explains-what-not-why]
min_lines = 2          # one line is rarely worth the argument
restate_threshold = 0.6
markers = []           # extend the built-in rationale list
```

Build it before the Harper work. It is cheaper, it needs no new dependency, and
it targets the thing the README leads with — unlike the vocabulary rule, which
measurement showed to be a poor fit for code comments.
