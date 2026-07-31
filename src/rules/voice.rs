//! Passive voice, detected without a part-of-speech tagger.
//!
//! "The value is set by the caller" hides who acts; "the caller sets the value"
//! does not. The shape is a form of *be* followed by a past participle, which is
//! recognisable from the words alone: a participle either ends in `-ed` or is one
//! of a closed list of irregulars.

use std::sync::OnceLock;

use regex::Regex;

/// Past participles that do not end in `-ed`. English has a few hundred; these
/// are the ones that turn up in prose about software.
///
/// Participles of intransitive verbs are deliberately absent — `gone`, `come`,
/// `fallen`, `slept` cannot form a passive, so `the pane is gone` is a predicate
/// adjective and listing them only produced false positives.
const IRREGULAR: &[&str] = &[
    "begun",
    "bought",
    "brought",
    "built",
    "caught",
    "chosen",
    "cut",
    "dealt",
    "done",
    "drawn",
    "driven",
    "eaten",
    "fed",
    "felt",
    "fought",
    "found",
    "forgotten",
    "frozen",
    "given",
    "grown",
    "held",
    "hidden",
    "hit",
    "hurt",
    "kept",
    "known",
    "laid",
    "led",
    "left",
    "lent",
    "let",
    "lost",
    "made",
    "meant",
    "met",
    "overridden",
    "overwritten",
    "paid",
    "put",
    "read",
    "rebuilt",
    "rerun",
    "rewritten",
    "run",
    "said",
    "seen",
    "sent",
    "set",
    "shown",
    "shut",
    "sold",
    "sought",
    "spent",
    "split",
    "spread",
    "stolen",
    "stood",
    "struck",
    "sung",
    "swept",
    "taken",
    "taught",
    "thought",
    "thrown",
    "told",
    "torn",
    "understood",
    "undone",
    "woken",
    "won",
    "worn",
    "written",
];

/// A form of *be*, up to two intervening adverbs, then a candidate participle.
/// The adverbs are limited: an open `\w+` there would match "is a set of flags".
fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(is|are|was|were|be|been|being)\b((?:\s+(?:not|never|always|already|only|also|still|now|then|usually|generally|[a-z]+ly))*)\s+([a-z]+)\b",
        )
        .expect("passive-voice pattern is a literal and compiles")
    })
}

fn is_participle(word: &str) -> bool {
    let w = word.to_ascii_lowercase();
    // `-ed` alone would match `red` and `bed`; four characters is the shortest
    // real participle (`used`, `sent` is irregular).
    (w.len() >= 4 && w.ends_with("ed")) || IRREGULAR.contains(&w.as_str())
}

/// Words that may sit between the participle and its `by`, so `is set once by
/// the caller` still reads as agentive.
const AGENT_WINDOW: usize = 4;

/// The passive construction in a line, as the reader would quote it, or `None`.
///
/// With `require_agent`, only passives naming their actor count. That is the
/// case where an active rewrite is guaranteed to exist and to be shorter, and
/// measurement says the rest is mostly predicate adjectives — `is unchanged`,
/// `is needed` — which no rewrite improves.
pub fn passive_phrase(line: &str, require_agent: bool) -> Option<String> {
    for caps in pattern().captures_iter(line) {
        if !is_participle(&caps[3]) {
            continue;
        }
        let phrase = format!("{}{} {}", &caps[1], &caps[2], &caps[3]);
        if !require_agent {
            return Some(phrase);
        }
        let rest = &line[caps.get(3).expect("group 3 always participates").end()..];
        if let Some(agent) = agent_of(rest) {
            return Some(format!("{phrase} by {agent}"));
        }
    }
    None
}

/// The actor in `… by the caller`, if one follows within [`AGENT_WINDOW`] words.
fn agent_of(rest: &str) -> Option<String> {
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for (i, token) in tokens.iter().take(AGENT_WINDOW).enumerate() {
        let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
        if bare.eq_ignore_ascii_case("by") {
            // Two words is enough to identify the actor. Punctuation ends it, so
            // the quote reads as prose rather than as a fragment of the line.
            let agent: Vec<String> = tokens[i + 1..]
                .iter()
                .take(2)
                .take_while(|w| w.starts_with(|c: char| c.is_alphanumeric()))
                .map(|w| {
                    w.trim_end_matches(|c: char| !c.is_alphanumeric())
                        .to_string()
                })
                .collect();
            if agent.is_empty() {
                return None;
            }
            return Some(agent.join(" "));
        }
        // A full stop ends the clause: the next sentence's `by` is not this
        // verb's agent.
        if token.ends_with('.') {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default: only passives that name their actor.
    fn agentive(line: &str) -> Option<String> {
        passive_phrase(line, true)
    }

    /// Strict mode: any passive at all.
    fn any(line: &str) -> Option<String> {
        passive_phrase(line, false)
    }

    #[test]
    fn finds_a_plain_passive() {
        assert_eq!(
            agentive("The value is set by the caller.").as_deref(),
            Some("is set by the caller")
        );
    }

    #[test]
    fn finds_an_irregular_participle() {
        assert_eq!(
            agentive("This is called by the runtime.").as_deref(),
            Some("is called by the runtime")
        );
        assert_eq!(
            any("The header was written first.").as_deref(),
            Some("was written")
        );
    }

    #[test]
    fn carries_the_adverb_into_the_quote() {
        assert_eq!(
            any("The cache is not invalidated here.").as_deref(),
            Some("is not invalidated")
        );
        assert_eq!(
            any("It is deliberately buffered.").as_deref(),
            Some("is deliberately buffered")
        );
    }

    #[test]
    fn leaves_active_voice_alone() {
        assert_eq!(any("The caller sets the value."), None);
        assert_eq!(any("Upstream returns 502 on cold start."), None);
    }

    #[test]
    fn a_noun_after_be_is_not_a_participle() {
        assert_eq!(any("This is a set of flags."), None);
        assert_eq!(any("The bed is red."), None);
    }

    #[test]
    fn a_modal_passive_still_counts() {
        assert_eq!(
            agentive("The lock must be held by the writer.").as_deref(),
            Some("be held by the writer")
        );
    }

    #[test]
    fn an_agentless_passive_is_left_alone_by_default() {
        // Nothing to rewrite it to: the sentence never says who acts.
        assert_eq!(agentive("The cache is invalidated here."), None);
        assert_eq!(
            any("The cache is invalidated here."),
            Some("is invalidated".into())
        );
    }

    #[test]
    fn a_predicate_adjective_is_not_a_passive() {
        // `gone` and `come` are intransitive; no actor can be named.
        assert_eq!(any("The pane is gone."), None);
        assert_eq!(any("The value is unchanged."), Some("is unchanged".into()));
    }

    #[test]
    fn the_agent_search_stops_at_a_sentence_boundary() {
        assert_eq!(agentive("The value is cached. Sorted by the caller."), None);
    }

    #[test]
    fn an_adverb_may_separate_the_participle_from_its_agent() {
        assert_eq!(
            agentive("The value is set once by the caller.").as_deref(),
            Some("is set by the caller")
        );
    }
}
