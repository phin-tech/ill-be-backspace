//! Rule behaviour and inline suppression, exercised through the public
//! `check_source` entry point so the tests cover the wiring as well as the rules.

use backspace::config::{ResolvedConfig, Severity};
use backspace::lang::Registry;
use backspace::rules::Phrase;
use backspace::{check_source, Violation};

fn check(source: &str, lang: &str, cfg: &ResolvedConfig) -> Vec<Violation> {
    let spec = Registry::builtin().get(lang).unwrap();
    check_source(source, spec, cfg)
}

fn rule_ids(v: &[Violation]) -> Vec<&str> {
    v.iter().map(|x| x.rule).collect()
}

/// A config that only ever fires `block-too-long`, so ratio noise does not
/// contaminate length tests.
fn length_only(max_lines: usize) -> ResolvedConfig {
    ResolvedConfig {
        max_lines,
        select: ["block-too-long"].iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn comment_block(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!("# line {i}\n"));
    }
    s.push_str("x = 1\n");
    s
}

mod block_too_long {
    use super::*;

    #[test]
    fn passes_at_the_threshold() {
        let v = check(&comment_block(5), "python", &length_only(5));
        assert!(v.is_empty(), "5 lines with max_lines=5 should pass");
    }

    #[test]
    fn fails_one_line_over_the_threshold() {
        let v = check(&comment_block(6), "python", &length_only(5));
        assert_eq!(rule_ids(&v), ["block-too-long"]);
    }

    #[test]
    fn reports_the_full_span_of_the_block() {
        let v = check(&comment_block(8), "python", &length_only(5));
        assert_eq!((v[0].start_line, v[0].end_line), (1, 8));
    }

    #[test]
    fn message_states_actual_and_limit() {
        let v = check(&comment_block(9), "python", &length_only(5));
        assert!(v[0].message.contains('9'), "{}", v[0].message);
        assert!(v[0].message.contains('5'), "{}", v[0].message);
    }

    #[test]
    fn two_long_blocks_produce_two_violations() {
        let src = format!("{}\n{}", comment_block(7), comment_block(7));
        let v = check(&src, "python", &length_only(5));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn blank_lines_inside_a_block_do_not_count_toward_length() {
        // Six comment lines split by a blank is still six lines of prose.
        let src = "# a\n# b\n# c\n\n# d\n# e\n# f\nx = 1\n";
        assert_eq!(check(src, "python", &length_only(6)).len(), 0);
        assert_eq!(check(src, "python", &length_only(5)).len(), 1);
    }

    #[test]
    fn applies_to_every_language() {
        for (lang, line) in [
            ("go", "// x\n"),
            ("rust", "// x\n"),
            ("typescript", "// x\n"),
            ("bash", "# x\n"),
            ("sql", "-- x\n"),
        ] {
            let src = line.repeat(7);
            let v = check(&src, lang, &length_only(5));
            assert_eq!(v.len(), 1, "{lang} should flag a 7-line comment");
        }
    }
}

mod max_words_and_chars {
    use super::*;

    /// One very long wrapped line evades a line-count budget entirely.
    const ONE_FAT_LINE: &str = "# aaa bbb ccc ddd eee fff ggg hhh iii jjj\nx = 1\n";

    #[test]
    fn word_budget_is_off_by_default() {
        assert!(check(ONE_FAT_LINE, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn word_budget_flags_a_single_long_line() {
        let cfg = ResolvedConfig {
            max_words: Some(5),
            ..length_only(5)
        };
        assert_eq!(
            rule_ids(&check(ONE_FAT_LINE, "python", &cfg)),
            ["block-too-long"]
        );
    }

    #[test]
    fn char_budget_flags_a_single_long_line() {
        let cfg = ResolvedConfig {
            max_chars: Some(10),
            ..length_only(5)
        };
        assert_eq!(check(ONE_FAT_LINE, "python", &cfg).len(), 1);
    }

    #[test]
    fn a_block_is_reported_once_even_if_several_budgets_bust() {
        let cfg = ResolvedConfig {
            max_words: Some(1),
            max_chars: Some(1),
            ..length_only(1)
        };
        assert_eq!(check(ONE_FAT_LINE, "python", &cfg).len(), 1);
    }
}

mod comment_code_ratio {
    use super::*;

    fn ratio_only() -> ResolvedConfig {
        ResolvedConfig {
            select: ["comment-code-ratio"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            max_ratio: 1.5,
            ratio_min_lines: 3,
            ..Default::default()
        }
    }

    #[test]
    fn flags_a_comment_longer_than_its_code() {
        // 6 comment lines over 2 code lines = 3.0
        let src = "# a\n# b\n# c\n# d\n# e\n# f\nx = 1\ny = 2\n";
        assert_eq!(
            rule_ids(&check(src, "python", &ratio_only())),
            ["comment-code-ratio"]
        );
    }

    #[test]
    fn allows_a_proportionate_comment() {
        // 3 comment lines over 4 code lines = 0.75
        let src = "# a\n# b\n# c\nw = 0\nx = 1\ny = 2\nz = 3\n";
        assert!(check(src, "python", &ratio_only()).is_empty());
    }

    #[test]
    fn ignores_short_blocks_regardless_of_ratio() {
        // 2 lines over 1 = 2.0, but below ratio_min_lines so it is left alone.
        let src = "# a\n# b\nx = 1\n";
        assert!(check(src, "python", &ratio_only()).is_empty());
    }

    #[test]
    fn respects_the_min_lines_boundary() {
        let src = "# a\n# b\n# c\nx = 1\n";
        assert_eq!(check(src, "python", &ratio_only()).len(), 1);
    }

    #[test]
    fn a_comment_with_no_following_code_is_measured_against_one_line() {
        let src = "x = 1\n\n# a\n# b\n# c\n# d\n";
        assert_eq!(check(src, "python", &ratio_only()).len(), 1);
    }

    #[test]
    fn message_reports_both_counts() {
        let src = "# a\n# b\n# c\n# d\n# e\n# f\nx = 1\ny = 2\n";
        let v = check(src, "python", &ratio_only());
        assert!(v[0].message.contains('6'), "{}", v[0].message);
        assert!(v[0].message.contains('2'), "{}", v[0].message);
    }
}

mod banned_phrase {
    use super::*;

    fn with_patterns(pats: &[&str]) -> ResolvedConfig {
        ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: pats.iter().map(|s| Phrase::pattern(s)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn is_inert_with_no_patterns() {
        assert!(check("# Note that x\n", "python", &with_patterns(&[])).is_empty());
    }

    #[test]
    fn matches_a_literal_phrase() {
        let v = check(
            "# Note that x is y\n",
            "python",
            &with_patterns(&["Note that"]),
        );
        assert_eq!(rule_ids(&v), ["banned-phrase"]);
    }

    #[test]
    fn is_case_insensitive_by_default() {
        let v = check("# note THAT x\n", "python", &with_patterns(&["Note that"]));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn matches_a_regex_pattern() {
        let src = "# Verified 2026-07-29: it works\n";
        let v = check(
            src,
            "python",
            &with_patterns(&[r"Verified \d{4}-\d{2}-\d{2}"]),
        );
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn message_names_the_phrase_that_matched() {
        let v = check("# Note that x\n", "python", &with_patterns(&["Note that"]));
        assert!(v[0].message.contains("Note that"), "{}", v[0].message);
    }

    #[test]
    fn matches_across_lines_of_one_block() {
        let src = "# first line\n# second says Note that\nx = 1\n";
        assert_eq!(
            check(src, "python", &with_patterns(&["Note that"])).len(),
            1
        );
    }

    #[test]
    fn reports_each_distinct_phrase_once() {
        let src = "# Note that it does NOT work\n";
        let v = check(src, "python", &with_patterns(&["Note that", "does NOT"]));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn the_llm_tells_preset_catches_a_realistic_comment() {
        let cfg = ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: backspace::rules::llm_tells_preset(),
            ..Default::default()
        };
        let src = "# Sync pulls files; it does NOT recreate containers.\n\
                   # Verified 2026-07-29: the crash loop continued.\n";
        assert!(!check(src, "python", &cfg).is_empty());
    }

    #[test]
    fn an_invalid_regex_is_reported_at_config_time_not_ignored() {
        assert!(backspace::rules::compile_phrases(&[Phrase::pattern("([")]).is_err());
    }
}

mod docstrings {
    use super::*;

    const LONG_DOCSTRING: &str = "def f():\n    \"\"\"One.\n    Two.\n    Three.\n    Four.\n    Five.\n    Six.\n    Seven.\n    \"\"\"\n    return 1\n";

    #[test]
    fn are_exempt_by_default() {
        assert!(check(LONG_DOCSTRING, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn are_checked_when_opted_in() {
        let cfg = ResolvedConfig {
            include_docstrings: true,
            ..length_only(5)
        };
        assert_eq!(check(LONG_DOCSTRING, "python", &cfg).len(), 1);
    }

    #[test]
    fn rust_doc_comments_are_exempt_by_default() {
        let src = "/// a\n/// b\n/// c\n/// d\n/// e\n/// f\n/// g\nfn f() {}\n";
        assert!(check(src, "rust", &length_only(5)).is_empty());
    }

    #[test]
    fn a_plain_comment_next_to_a_docstring_is_still_checked() {
        let src = "def f():\n    # a\n    # b\n    # c\n    # d\n    # e\n    # f\n    \"\"\"Docs.\"\"\"\n    return 1\n";
        assert_eq!(check(src, "python", &length_only(5)).len(), 1);
    }
}

mod suppression {
    use super::*;

    #[test]
    fn bare_ignore_suppresses_every_rule_in_the_block() {
        let src = "# backspace: ignore\n# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn targeted_ignore_suppresses_only_that_rule() {
        let src = "# backspace: ignore[comment-code-ratio]\n# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";
        assert_eq!(
            rule_ids(&check(src, "python", &length_only(5))),
            ["block-too-long"]
        );
    }

    #[test]
    fn targeted_ignore_matching_the_rule_suppresses_it() {
        let src = "# backspace: ignore[block-too-long]\n# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn directive_works_anywhere_in_the_block_not_just_the_first_line() {
        let src = "# a\n# b\n# c\n# d\n# e\n# f\n# backspace: ignore\nx = 1\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn directive_syntax_is_language_agnostic() {
        let src = "// backspace: ignore\n// a\n// b\n// c\n// d\n// e\n// f\n";
        assert!(check(src, "go", &length_only(5)).is_empty());
    }

    #[test]
    fn ignore_file_suppresses_the_whole_file() {
        let src = "# backspace: ignore-file\n\n# a\n# b\n# c\n# d\n# e\n# f\n# g\nx = 1\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn ignore_file_is_only_honoured_near_the_top() {
        let mut src = "x = 1\n".repeat(20);
        src.push_str("# backspace: ignore-file\n");
        src.push_str(&comment_block(7));
        assert_eq!(check(&src, "python", &length_only(5)).len(), 1);
    }

    #[test]
    fn a_reason_may_follow_the_directive() {
        let src = "# backspace: ignore[block-too-long] — protocol quirk, see RFC 9114\n# a\n# b\n# c\n# d\n# e\n# f\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn requiring_a_reason_rejects_a_bare_directive() {
        let cfg = ResolvedConfig {
            require_suppression_reason: true,
            ..length_only(5)
        };
        let src = "# backspace: ignore\n# a\n# b\n# c\n# d\n# e\n# f\n";
        let v = check(src, "python", &cfg);
        assert!(
            rule_ids(&v).contains(&"suppression-needs-reason"),
            "{:?}",
            rule_ids(&v)
        );
    }

    #[test]
    fn requiring_a_reason_accepts_a_justified_directive() {
        let cfg = ResolvedConfig {
            require_suppression_reason: true,
            ..length_only(5)
        };
        let src = "# backspace: ignore — the wire format demands this much prose\n# a\n# b\n# c\n# d\n# e\n# f\n";
        assert!(check(src, "python", &cfg).is_empty());
    }

    #[test]
    fn the_directive_line_itself_does_not_count_toward_length() {
        // Otherwise adding a suppression could push a passing block over the limit.
        let src = "# backspace: ignore[comment-code-ratio]\n# a\n# b\n# c\n# d\n# e\nx = 1\n";
        assert!(check(src, "python", &length_only(5)).is_empty());
    }
}

mod select_and_ignore {
    use super::*;

    const OFFENDS_BOTH: &str = "# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";

    #[test]
    fn both_rules_fire_when_both_are_selected() {
        let cfg = ResolvedConfig {
            select: ["block-too-long", "comment-code-ratio"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ..Default::default()
        };
        let found = check(OFFENDS_BOTH, "python", &cfg);
        let mut ids = rule_ids(&found);
        ids.sort_unstable();
        assert_eq!(ids, ["block-too-long", "comment-code-ratio"]);
    }

    #[test]
    fn ignore_wins_over_select() {
        let cfg = ResolvedConfig {
            select: ["block-too-long", "comment-code-ratio"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ignore: ["comment-code-ratio"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ..Default::default()
        };
        assert_eq!(
            rule_ids(&check(OFFENDS_BOTH, "python", &cfg)),
            ["block-too-long"]
        );
    }

    #[test]
    fn violations_carry_the_configured_severity() {
        let cfg = ResolvedConfig {
            severity: Severity::Warning,
            ..length_only(5)
        };
        assert_eq!(
            check(&comment_block(7), "python", &cfg)[0].severity,
            Severity::Warning
        );
    }

    #[test]
    fn violations_carry_the_comment_text_for_machine_consumers() {
        let v = check(&comment_block(7), "python", &length_only(5));
        assert_eq!(v[0].text.len(), 7);
        assert_eq!(v[0].text[0], "line 0");
    }
}

mod banned_words {
    use super::*;

    fn with_words(words: &[&str]) -> ResolvedConfig {
        ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: words.iter().map(|w| Phrase::word(w)).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn bans_a_plain_word() {
        let v = check(
            "# runs on the substrate layer\n",
            "python",
            &with_words(&["substrate"]),
        );
        assert_eq!(rule_ids(&v), ["banned-phrase"]);
    }

    #[test]
    fn is_case_insensitive() {
        assert_eq!(
            check("# The Substrate\n", "python", &with_words(&["substrate"])).len(),
            1
        );
    }

    #[test]
    fn matches_whole_words_only() {
        // `substrate` must not fire on `substrates` or `subsubstrate`.
        assert!(check(
            "# substrates everywhere\n",
            "python",
            &with_words(&["substrate"])
        )
        .is_empty());
        assert!(check("# a subsubstrate\n", "python", &with_words(&["substrate"])).is_empty());
    }

    #[test]
    fn matches_a_multi_word_phrase() {
        let v = check(
            "# we delve into the cache\n",
            "python",
            &with_words(&["delve into"]),
        );
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn regex_metacharacters_are_literal_not_syntax() {
        // A word list is not a regex list; `c++` must not be a parse error.
        let v = check("# written in c++ here\n", "python", &with_words(&["c++"]));
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn the_message_names_the_word_not_a_regex() {
        let v = check("# the substrate\n", "python", &with_words(&["substrate"]));
        assert!(v[0].message.contains("substrate"), "{}", v[0].message);
        assert!(!v[0].message.contains("\\b"), "{}", v[0].message);
    }

    #[test]
    fn words_and_regex_patterns_coexist() {
        let cfg = ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: vec![
                Phrase::word("substrate"),
                Phrase::pattern(r"Verified \d{4}"),
            ],
            ..Default::default()
        };
        let v = check("# substrate, Verified 2026\n", "python", &cfg);
        assert_eq!(v.len(), 2);
    }
}

mod comment_restates_code {
    use super::*;

    /// `min_words` is lowered so the examples can stay short and readable; the
    /// shipped default of 6 needs more prose than a doc example wants.
    fn restates_only() -> ResolvedConfig {
        ResolvedConfig {
            select: ["comment-restates-code"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            restate_min_words: 2,
            restate_threshold: 0.6,
            ..Default::default()
        }
    }

    #[test]
    fn flags_a_comment_that_names_what_the_code_names() {
        let src = "# increment the retry counter\nretry_counter += 1\n";
        assert_eq!(
            rule_ids(&check(src, "python", &restates_only())),
            ["comment-restates-code"]
        );
    }

    #[test]
    fn splits_snake_case_identifiers() {
        let src = "# set the user name\nuser_name = value\n";
        assert_eq!(check(src, "python", &restates_only()).len(), 1);
    }

    #[test]
    fn splits_camel_case_identifiers() {
        let src = "// update the item count\nupdateItemCount();\n";
        assert_eq!(check(src, "typescript", &restates_only()).len(), 1);
    }

    #[test]
    fn leaves_a_comment_that_adds_information() {
        // Explains *why*; shares almost no vocabulary with the code.
        let src = "# Upstream returns 502 on cold start.\nfetch(url, retries=1)\n";
        assert!(check(src, "python", &restates_only()).is_empty());
    }

    #[test]
    fn leaves_a_comment_naming_a_constraint_the_code_cannot_state() {
        let src = "// Buffered so a slow producer cannot stall.\nch := make(chan int, 64)\n";
        assert!(check(src, "go", &restates_only()).is_empty());
    }

    #[test]
    fn ignores_very_short_comments() {
        // `# TODO` has nothing to measure.
        assert!(check("# todo\nx = 1\n", "python", &restates_only()).is_empty());
    }

    #[test]
    fn ignores_a_comment_with_no_code_beneath_it() {
        assert!(check(
            "x = 1\n\n# a note about the thing\n",
            "python",
            &restates_only()
        )
        .is_empty());
    }

    #[test]
    fn stopwords_do_not_inflate_the_overlap() {
        // Only `frobnicate` is a content word, and it is absent from the code.
        let src = "# it is the one that we frobnicate\nx = 1\n";
        assert!(check(src, "python", &restates_only()).is_empty());
    }

    #[test]
    fn the_threshold_is_configurable() {
        let src = "# increment the retry counter\nretry_counter += 1\n";
        let strict = ResolvedConfig {
            restate_threshold: 0.99,
            ..restates_only()
        };
        assert!(check(src, "python", &strict).is_empty());
    }

    #[test]
    fn message_reports_the_overlap() {
        let src = "# set the user name\nuser_name = value\n";
        let v = check(src, "python", &restates_only());
        assert!(v[0].message.contains('%'), "{}", v[0].message);
    }

    #[test]
    fn is_off_by_default() {
        let src = "# increment the retry counter\nretry_counter += 1\n";
        let found = check(src, "python", &ResolvedConfig::default());
        let ids = rule_ids(&found);
        assert!(!ids.contains(&"comment-restates-code"), "{ids:?}");
    }
}

mod explains_what_not_why {
    use super::*;

    /// A comment drawn from the code beneath it, saying nothing about why.
    const NARRATES: &str = "# set the user name and the user email\n\
                            # from the user record fields\n\
                            user_name = record.name\n\
                            user_email = record.email\n";

    /// The same restatement, with a reason attached.
    const NARRATES_WITH_REASON: &str = "# set the user name and the user email\n\
                                        # from the user record; the record must set both\n\
                                        user_name = record.name\n\
                                        user_email = record.email\n";

    /// `restate_min_words` is lowered so the examples can stay readable.
    fn why_only() -> ResolvedConfig {
        ResolvedConfig {
            select: ["explains-what-not-why"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            restate_min_words: 2,
            ..Default::default()
        }
    }

    #[test]
    fn flags_a_restatement_with_no_reason() {
        assert_eq!(
            rule_ids(&check(NARRATES, "python", &why_only())),
            ["explains-what-not-why"]
        );
    }

    #[test]
    fn a_reason_exempts_a_comment_however_much_it_restates() {
        assert!(check(NARRATES_WITH_REASON, "python", &why_only()).is_empty());

        // The same comment still trips the blunter rule. That difference is the
        // whole reason this rule exists.
        let blunt = ResolvedConfig {
            select: ["comment-restates-code"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            restate_min_words: 2,
            restate_threshold: 0.6,
            ..Default::default()
        };
        assert_eq!(
            rule_ids(&check(NARRATES_WITH_REASON, "python", &blunt)),
            ["comment-restates-code"]
        );
    }

    #[test]
    fn a_marker_is_matched_whole_word() {
        // `bug` is a marker; `debugger` is not one wearing a disguise.
        let src = "# update the retry counter\n\
                   # in the retry debugger\n\
                   retry_counter += 1\n\
                   retry_debugger.log()\n";
        assert_eq!(check(src, "python", &why_only()).len(), 1);
    }

    #[test]
    fn one_line_is_not_worth_the_argument() {
        let src = "# increment the retry counter\nretry_counter += 1\n";
        assert!(check(src, "python", &why_only()).is_empty());
    }

    #[test]
    fn min_lines_is_configurable() {
        let src = "# increment the retry counter\nretry_counter += 1\n";
        let cfg = ResolvedConfig {
            what_not_why_min_lines: 1,
            ..why_only()
        };
        assert_eq!(check(src, "python", &cfg).len(), 1);
    }

    #[test]
    fn a_godoc_comment_naming_its_function_is_exempt() {
        // Go has no syntax for a doc comment, only the convention that it opens
        // with the declared name. Honouring the convention is not narration.
        let src = "// NewFromConfig builds a config client from a config file.\n\
                   // The config client is cached.\n\
                   func NewFromConfig(config Config) *Client {\n";
        assert!(check(src, "go", &why_only()).is_empty());
    }

    #[test]
    fn prose_that_merely_starts_with_a_code_word_is_not_exempt() {
        let src = "# retry the request and retry the parse\n\
                   # then retry the write\n\
                   retry(request)\n\
                   retry(parse)\n";
        assert_eq!(check(src, "python", &why_only()).len(), 1);
    }

    #[test]
    fn leaves_a_comment_that_adds_information() {
        let src = "# Upstream returns 502 on cold start.\n\
                   # One retry is enough to clear it.\n\
                   fetch(url, retries=1)\n";
        assert!(check(src, "python", &why_only()).is_empty());
    }

    #[test]
    fn markers_can_be_replaced_and_extended() {
        // With the built-in list gone, `must` no longer rescues the comment.
        let bare = ResolvedConfig {
            rationale_markers: Vec::new(),
            ..why_only()
        };
        assert_eq!(check(NARRATES_WITH_REASON, "python", &bare).len(), 1);

        let extended = ResolvedConfig {
            rationale_markers: vec!["fields".to_string()],
            ..why_only()
        };
        assert!(check(NARRATES, "python", &extended).is_empty());
    }

    #[test]
    fn the_message_names_both_halves() {
        let v = check(NARRATES, "python", &why_only());
        assert!(v[0].message.contains('%'), "{}", v[0].message);
        assert!(v[0].message.contains("no reason"), "{}", v[0].message);
    }

    #[test]
    fn is_off_by_default() {
        let found = check(NARRATES, "python", &ResolvedConfig::default());
        let ids = rule_ids(&found);
        assert!(!ids.contains(&"explains-what-not-why"), "{ids:?}");
    }
}

mod passive_voice {
    use super::*;

    fn passive_only() -> ResolvedConfig {
        ResolvedConfig {
            select: ["passive-voice"].iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn flags_a_passive_construction() {
        let src = "# The value is set by the caller.\nvalue = None\n";
        assert_eq!(
            rule_ids(&check(src, "python", &passive_only())),
            ["passive-voice"]
        );
    }

    #[test]
    fn the_message_quotes_the_phrase() {
        let src = "# The value is set by the caller.\nvalue = None\n";
        let v = check(src, "python", &passive_only());
        assert!(v[0].message.contains("is set"), "{}", v[0].message);
    }

    #[test]
    fn leaves_active_voice_alone() {
        let src = "// The caller sets the value before the first read.\nvar v int\n";
        assert!(check(src, "go", &passive_only()).is_empty());
    }

    #[test]
    fn reports_once_per_block() {
        let src = "# The value is set by the caller.\n\
                   # The header is written first.\n\
                   value = None\n";
        assert_eq!(check(src, "python", &passive_only()).len(), 1);
    }

    #[test]
    fn skips_code_samples() {
        // A shell transcript is not prose, whatever its verbs look like.
        let src = "# `adb -s <serial> push <jar>` is run by the harness\nrun()\n";
        assert!(check(src, "python", &passive_only()).is_empty());
    }

    #[test]
    fn is_off_by_default() {
        let src = "# The value is set by the caller.\nvalue = None\n";
        let found = check(src, "python", &ResolvedConfig::default());
        let ids = rule_ids(&found);
        assert!(!ids.contains(&"passive-voice"), "{ids:?}");
    }
}

mod rhythm_rules {
    use super::*;

    fn only(rule: &str) -> ResolvedConfig {
        ResolvedConfig {
            select: [rule.to_string()].into_iter().collect(),
            ..Default::default()
        }
    }

    /// Five sentences, all four or five words long.
    const FLAT: &str = "# One two three four. Five six seven eight.\n\
                        # Nine ten more words. Twelve thirteen four teen.\n\
                        # Sixteen seventeen more here.\n\
                        x = 1\n";

    #[test]
    fn flags_prose_with_no_rhythm() {
        assert_eq!(
            rule_ids(&check(FLAT, "python", &only("uniform-sentences"))),
            ["uniform-sentences"]
        );
    }

    #[test]
    fn leaves_prose_that_varies() {
        let src = "# No. It failed because the upstream server rejected the second\n\
                   # request after the retry budget ran out and nothing retried it.\n\
                   # Twice. That was the whole bug, and it took a week to find. Fixed.\n\
                   x = 1\n";
        assert!(check(src, "python", &only("uniform-sentences")).is_empty());
    }

    #[test]
    fn too_few_sentences_are_not_judged() {
        let src = "# One two three four. Five six seven eight.\nx = 1\n";
        assert!(check(src, "python", &only("uniform-sentences")).is_empty());
    }

    #[test]
    fn flags_an_em_dash_habit() {
        let src = "# Retry once \u{2014} the upstream 502s \u{2014} then give up.\nx = 1\n";
        assert_eq!(
            rule_ids(&check(src, "python", &only("em-dash-habit"))),
            ["em-dash-habit"]
        );
    }

    #[test]
    fn one_em_dash_is_not_a_habit() {
        let src = "# Retry once \u{2014} the upstream 502s on cold start.\nx = 1\n";
        assert!(check(src, "python", &only("em-dash-habit")).is_empty());
    }

    #[test]
    fn a_low_rate_over_long_prose_is_not_a_habit() {
        let mut src = String::from("# Two dashes \u{2014} spread \u{2014} thin.\n");
        for i in 0..30 {
            src.push_str(&format!(
                "# filler line number {i} with several plain words\n"
            ));
        }
        src.push_str("x = 1\n");
        assert!(check(&src, "python", &only("em-dash-habit")).is_empty());
    }

    #[test]
    fn both_are_off_by_default() {
        let found = check(FLAT, "python", &ResolvedConfig::default());
        let ids = rule_ids(&found);
        assert!(!ids.contains(&"uniform-sentences"), "{ids:?}");
        assert!(!ids.contains(&"em-dash-habit"), "{ids:?}");
    }
}

mod llm_tells {
    use super::*;

    fn preset() -> ResolvedConfig {
        ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: backspace::rules::llm_tells_preset(),
            ..Default::default()
        }
    }

    #[test]
    fn catches_the_antithesis_construction() {
        let src = "# It's not just a cache \u{2014} it's a contract.\nx = 1\n";
        let v = check(src, "python", &preset());
        assert_eq!(v.len(), 1);
        // The reader is shown the shape, not the regex that found it.
        assert!(v[0].message.contains("not just X"), "{}", v[0].message);
        assert!(!v[0].message.contains(r"\b"), "{}", v[0].message);
    }

    #[test]
    fn catches_the_correlative_pair() {
        let src = "# Not only is it faster but it is smaller.\nx = 1\n";
        assert_eq!(check(src, "python", &preset()).len(), 1);
    }

    #[test]
    fn leaves_a_plain_negation_alone() {
        let src = "# Retry once; the upstream 502s on cold start.\nx = 1\n";
        assert!(check(src, "python", &preset()).is_empty());
    }
}

mod presets {
    use super::*;

    fn with(phrases: Vec<Phrase>) -> ResolvedConfig {
        ResolvedConfig {
            select: ["banned-phrase"].iter().map(|s| s.to_string()).collect(),
            banned_phrases: phrases,
            ..Default::default()
        }
    }

    #[test]
    fn an_idiom_is_left_alone_but_the_stretch_is_not() {
        let cfg = with(backspace::rules::agent_tics_preset());
        // Emacs writes this; every human use measured was the idiom.
        assert!(check(
            "# Guards a pathological case here.\nx = 1\n",
            "python",
            &cfg
        )
        .is_empty());
        // The agent corpus stretches it over any noun to hand.
        assert_eq!(
            check(
                "# Guards a pathological caller here.\nx = 1\n",
                "python",
                &cfg
            )
            .len(),
            1
        );
    }

    #[test]
    fn the_exception_only_excuses_the_use_that_earned_it() {
        // One idiomatic use must not excuse a stretch elsewhere in the block.
        let cfg = with(backspace::rules::agent_tics_preset());
        let src = "# A pathological case, and a pathological span.\nx = 1\n";
        assert_eq!(check(src, "python", &cfg).len(), 1);
    }

    #[test]
    fn a_plain_word_is_named_in_the_help() {
        let cfg = with(backspace::rules::plain_words_preset());
        let v = check("# We utilize a cache here.\nx = 1\n", "python", &cfg);
        assert_eq!(v.len(), 1);
        assert!(v[0].help.contains("`use`"), "{}", v[0].help);
    }

    #[test]
    fn a_borderline_word_advises_rather_than_bans() {
        let cfg = with(backspace::rules::agent_tics_preset());
        let v = check("# We gate the retry here.\nx = 1\n", "python", &cfg);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].severity, Severity::Note);
        // The advice says what the word is for, not merely that it was seen.
        assert!(v[0].help.contains("conditional guard"), "{}", v[0].help);
    }

    #[test]
    fn an_advisory_stays_advice_even_at_error_severity() {
        // Otherwise a project's global severity would turn a note into a gate,
        // which is the one thing an advisory must never become.
        let cfg = ResolvedConfig {
            severity: Severity::Error,
            ..with(backspace::rules::agent_tics_preset())
        };
        let v = check("# We gate the retry here.\nx = 1\n", "python", &cfg);
        assert_eq!(v[0].severity, Severity::Note);
    }

    #[test]
    fn obligations_are_distinct_per_level() {
        assert_eq!(Severity::Error.obligation(), "MUST fix");
        assert_eq!(Severity::Warning.obligation(), "SHOULD fix");
        assert_eq!(Severity::Note.obligation(), "MAY leave as is");
    }

    #[test]
    fn a_quoted_word_is_named_not_used() {
        let cfg = with(vec![Phrase::word("utilize")]);
        assert!(check(
            "# Prefer `use` over `utilize` here.\nx = 1\n",
            "python",
            &cfg
        )
        .is_empty());
        assert_eq!(
            check("# We utilize a cache.\nx = 1\n", "python", &cfg).len(),
            1
        );
    }

    #[test]
    fn a_stray_backtick_does_not_silence_the_block() {
        let cfg = with(vec![Phrase::word("utilize")]);
        assert_eq!(
            check(
                "# A ` stray tick, and we utilize it.\nx = 1\n",
                "python",
                &cfg
            )
            .len(),
            1
        );
    }

    #[test]
    fn no_preset_lists_a_word_twice() {
        // A duplicate reports the same match twice, and if the two entries
        // disagree about severity the reader gets both an error and a note for
        // one word. Caught in the wild by running the tool over this repo.
        for name in backspace::rules::PHRASE_PRESETS {
            let mut seen: Vec<String> = backspace::rules::preset_named(name)
                .iter()
                .map(|p| p.display.to_lowercase())
                .collect();
            let before = seen.len();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), before, "{name} lists a phrase twice");
        }
    }

    #[test]
    fn presets_are_all_reachable_by_name() {
        for name in backspace::rules::PHRASE_PRESETS {
            assert!(
                !backspace::rules::preset_named(name).is_empty(),
                "{name} resolved to nothing"
            );
        }
    }
}

mod max_line_words {
    use super::*;

    /// One line that over-explains, and one that does its job.
    const WORDY: &str = "# Set the user's name to the provided value if it is not None, otherwise keep the existing one.\nuser.name = name\n";
    const TERSE: &str = "# Cached: the upstream rate-limits at 10rps.\nreturn fetch(x)\n";

    fn per_line(n: usize) -> ResolvedConfig {
        ResolvedConfig {
            max_line_words: Some(n),
            select: ["block-too-long"].iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn is_off_by_default() {
        assert!(check(WORDY, "python", &length_only(5)).is_empty());
    }

    #[test]
    fn flags_a_single_line_that_runs_long() {
        assert_eq!(
            rule_ids(&check(WORDY, "python", &per_line(12))),
            ["block-too-long"]
        );
    }

    #[test]
    fn leaves_a_terse_single_line_alone() {
        assert!(check(TERSE, "python", &per_line(12)).is_empty());
    }

    #[test]
    fn measures_per_line_not_per_block() {
        // Five short lines total well over the budget, but no single line does.
        let src = "# alpha beta gamma\n# delta epsilon zeta\n# eta theta iota\n# kappa lambda mu\n# nu xi omicron\nx = 1\n";
        assert!(check(src, "python", &per_line(12)).is_empty());
    }

    #[test]
    fn catches_one_bloated_line_inside_a_short_block() {
        let src = "# short\n# this particular line goes on and on and on and on and well past the budget\nx = 1\n";
        assert_eq!(check(src, "python", &per_line(12)).len(), 1);
    }

    #[test]
    fn message_reports_the_word_count_and_budget() {
        let v = check(WORDY, "python", &per_line(12));
        assert!(v[0].message.contains("12"), "{}", v[0].message);
        assert!(
            v[0].message.to_lowercase().contains("word"),
            "{}",
            v[0].message
        );
    }

    #[test]
    fn the_block_budget_still_works_independently() {
        let cfg = ResolvedConfig {
            max_words: Some(5),
            ..per_line(100)
        };
        // Six words across the block, none on a single line near the budget.
        let src = "# alpha beta gamma\n# delta epsilon zeta\nx = 1\n";
        assert_eq!(check(src, "python", &cfg).len(), 1);
    }
}

mod unapproved_word {
    use super::*;

    /// Known limitation: the vocabulary holds base forms, so inflections are
    /// reported as unapproved. `sort` is listed, `sorted` is not. Stemming would
    /// close this; until then the preset needs `extend` for real use.
    #[test]
    fn inflections_are_not_yet_recognised() {
        let cfg = ResolvedConfig {
            select: ["unapproved-word"].iter().map(|s| s.to_string()).collect(),
            approved_words: backspace::rules::plain_code_vocabulary(),
            ..Default::default()
        };
        let v = check("# sorted before the search\nx = 1\n", "python", &cfg);
        assert_eq!(v.len(), 1, "documents the gap rather than hiding it");
        assert!(v[0].message.contains("sorted"), "{}", v[0].message);
    }

    fn approved(words: &[&str]) -> ResolvedConfig {
        ResolvedConfig {
            select: ["unapproved-word"].iter().map(|s| s.to_string()).collect(),
            approved_words: words.iter().map(|w| w.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn is_off_by_default() {
        let found = check(
            "# xyzzy plugh\nx = 1\n",
            "python",
            &ResolvedConfig::default(),
        );
        assert!(
            !rule_ids(&found).contains(&"unapproved-word"),
            "{:?}",
            rule_ids(&found)
        );
    }

    #[test]
    fn is_inert_with_an_empty_list() {
        // An empty allow-list means "no vocabulary configured", not "ban everything".
        assert!(check("# anything at all here\nx = 1\n", "python", &approved(&[])).is_empty());
    }

    #[test]
    fn accepts_a_comment_within_the_vocabulary() {
        let cfg = approved(&["retry", "once", "server", "fails"]);
        assert!(check("# retry once if the server fails\nx = 1\n", "python", &cfg).is_empty());
    }

    #[test]
    fn flags_a_word_outside_the_vocabulary() {
        let cfg = approved(&["retry", "once"]);
        let v = check("# retry once, then obfuscate\nx = 1\n", "python", &cfg);
        assert_eq!(rule_ids(&v), ["unapproved-word"]);
        assert!(v[0].message.contains("obfuscate"), "{}", v[0].message);
    }

    #[test]
    fn stopwords_and_short_words_are_always_allowed() {
        // Nobody wants to enumerate "the", "a", "is" in an approved list.
        let cfg = approved(&["retry"]);
        assert!(check("# the retry is on\nx = 1\n", "python", &cfg).is_empty());
    }

    #[test]
    fn is_case_insensitive() {
        let cfg = approved(&["retry"]);
        assert!(check("# Retry RETRY retry\nx = 1\n", "python", &cfg).is_empty());
    }

    #[test]
    fn words_from_the_surrounding_code_are_approved() {
        // A project's own vocabulary should not need restating in config.
        let cfg = approved(&["the", "holds", "state"]);
        let src = "# the idempotency_token holds state\nidempotency_token = mint()\n";
        assert!(check(src, "python", &cfg).is_empty());
    }

    #[test]
    fn code_approval_splits_identifiers() {
        let cfg = approved(&["reset"]);
        let src = "# reset the retryCounter\nretryCounter = 0\n";
        assert!(check(src, "python", &cfg).is_empty());
    }

    #[test]
    fn code_approval_can_be_disabled() {
        let cfg = ResolvedConfig {
            approve_code_words: false,
            ..approved(&["reset"])
        };
        let src = "# reset the retryCounter\nretryCounter = 0\n";
        assert_eq!(check(src, "python", &cfg).len(), 1);
    }

    #[test]
    fn numbers_and_versions_are_allowed() {
        let cfg = approved(&["retry", "after"]);
        assert!(check("# retry after 500ms\nx = 1\n", "python", &cfg).is_empty());
    }

    #[test]
    fn reports_every_unapproved_word_in_one_finding() {
        let cfg = approved(&["retry"]);
        let v = check("# retry obfuscate defenestrate\nx = 1\n", "python", &cfg);
        assert_eq!(v.len(), 1, "one finding per block, not per word");
        assert!(v[0].message.contains("obfuscate"), "{}", v[0].message);
        assert!(v[0].message.contains("defenestrate"), "{}", v[0].message);
    }

    #[test]
    fn the_plain_code_preset_accepts_common_technical_prose() {
        let cfg = ResolvedConfig {
            select: ["unapproved-word"].iter().map(|s| s.to_string()).collect(),
            approved_words: backspace::rules::plain_code_vocabulary(),
            ..Default::default()
        };
        let comment = "# retry once: the upstream returns an error on cold start";
        let src = format!("{comment}\nx = 1\n");
        let v = check(&src, "python", &cfg);
        assert!(
            v.is_empty(),
            "{comment}\n  -> {:?}",
            v.first().map(|x| &x.message)
        );
    }
}
