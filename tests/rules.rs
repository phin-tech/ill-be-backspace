//! Rule behaviour and inline suppression, exercised through the public
//! `check_source` entry point so the tests cover the wiring as well as the rules.

use backspace::config::{ResolvedConfig, Severity};
use backspace::lang::Registry;
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
            banned_phrases: pats.iter().map(|s| s.to_string()).collect(),
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
        assert!(backspace::rules::compile_phrases(&["([".to_string()]).is_err());
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
