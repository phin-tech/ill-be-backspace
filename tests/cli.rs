//! End-to-end behaviour of the binary, plus a sweep over the language fixtures.
//!
//! Fixture naming is the contract: `good_*` must be silent and `bad_*` must be
//! flagged. That catches a language spec regression without a bespoke test per
//! language.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

fn bin() -> Command {
    Command::cargo_bin("backspace").unwrap()
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&fixtures(), &mut out);
    out.sort();
    out
}

/// A project directory with a config that keeps fixture expectations stable
/// regardless of what the defaults become.
fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
    dir
}

mod fixture_sweep {
    use super::*;

    #[test]
    fn every_good_fixture_is_silent() {
        for path in fixture_files() {
            let name = path.file_name().unwrap().to_str().unwrap();
            if !name.starts_with("good_") {
                continue;
            }
            let out = bin().arg(&path).arg("--all").output().unwrap();
            assert!(
                out.status.success(),
                "{} should be clean:\n{}",
                path.display(),
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    #[test]
    fn every_bad_fixture_is_flagged() {
        for path in fixture_files() {
            let name = path.file_name().unwrap().to_str().unwrap();
            if !name.starts_with("bad_") {
                continue;
            }
            let out = bin().arg(&path).arg("--all").output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(1),
                "{} should be flagged:\n{}",
                path.display(),
                String::from_utf8_lossy(&out.stdout)
            );
        }
    }

    #[test]
    fn the_sweep_actually_covers_several_languages() {
        // Guards against the sweep silently degrading to zero files.
        let n = fixture_files().len();
        assert!(n >= 15, "expected a broad fixture set, found {n}");
    }
}

mod exit_codes {
    use super::*;

    #[test]
    fn clean_input_exits_zero() {
        let dir = project(&[("a.py", "# short\nx = 1\n")]);
        bin().arg(dir.path()).arg("--all").assert().success();
    }

    #[test]
    fn violations_exit_one() {
        let dir = project(&[("a.py", &"# c\n".repeat(9))]);
        bin().arg(dir.path()).arg("--all").assert().code(1);
    }

    #[test]
    fn warning_severity_reports_but_exits_zero() {
        let dir = project(&[("a.py", &"# c\n".repeat(9))]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--severity", "warning"])
            .output()
            .unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("warning"));
    }

    #[test]
    fn a_bad_config_exits_two() {
        let dir = project(&[(".backspace.toml", "max_linez = 1\n"), ("a.py", "x = 1\n")]);
        bin().arg(dir.path()).arg("--all").assert().code(2);
    }

    #[test]
    fn an_unknown_rule_in_select_exits_two() {
        let dir = project(&[("a.py", "x = 1\n")]);
        bin()
            .arg(dir.path())
            .args(["--all", "--select", "nope"])
            .assert()
            .code(2);
    }
}

mod flags {
    use super::*;

    const SIX: &str = "# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";

    #[test]
    fn max_lines_flag_overrides_the_default() {
        let dir = project(&[("a.py", SIX)]);
        bin()
            .arg(dir.path())
            .args([
                "--all",
                "--max-lines",
                "10",
                "--ignore",
                "comment-code-ratio",
            ])
            .assert()
            .success();
    }

    #[test]
    fn ignore_flag_disables_a_rule() {
        let dir = project(&[("a.py", SIX)]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--ignore", "comment-code-ratio"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(!text.contains("comment-code-ratio"), "{text}");
    }

    #[test]
    fn exclude_glob_skips_files() {
        let dir = project(&[("vendor/a.py", SIX)]);
        bin()
            .arg(dir.path())
            .args(["--all", "--exclude", "**/vendor/**"])
            .assert()
            .success();
    }

    #[test]
    fn include_docstrings_turns_on_docstring_checks() {
        let src = "def f():\n    \"\"\"a\n    b\n    c\n    d\n    e\n    f\n    g\n    \"\"\"\n    return 1\n";
        let dir = project(&[("a.py", src)]);
        bin().arg(dir.path()).arg("--all").assert().success();
        bin()
            .arg(dir.path())
            .args(["--all", "--include-docstrings"])
            .assert()
            .code(1);
    }

    #[test]
    fn stats_prints_a_breakdown() {
        let dir = project(&[("a.py", SIX)]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--stats"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("by rule"), "{text}");
        assert!(text.contains("by language"), "{text}");
    }
}

mod formats {
    use super::*;

    const SIX: &str = "# a\n# b\n# c\n# d\n# e\n# f\nx = 1\n";

    fn stdout(args: &[&str], dir: &TempDir) -> String {
        let out = bin().arg(dir.path()).args(args).output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    #[test]
    fn json_is_valid_and_carries_the_comment_text() {
        let dir = project(&[("a.py", SIX)]);
        let text = stdout(&["--all", "--json"], &dir);
        let v: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(v["version"], 1);
        assert!(v["summary"]["violations"].as_u64().unwrap() > 0);
        let first = &v["violations"][0];
        assert_eq!(first["language"], "python");
        assert!(first["comment"].as_array().unwrap().len() >= 6);
        assert!(first["help"].as_str().unwrap().len() > 10);
    }

    #[test]
    fn json_flag_and_format_json_agree() {
        let dir = project(&[("a.py", SIX)]);
        assert_eq!(
            stdout(&["--all", "--json"], &dir),
            stdout(&["--all", "--format", "json"], &dir)
        );
    }

    #[test]
    fn json_on_clean_input_is_still_a_valid_document() {
        let dir = project(&[("a.py", "x = 1\n")]);
        let text = stdout(&["--all", "--json"], &dir);
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["summary"]["violations"], 0);
    }

    #[test]
    fn github_format_emits_workflow_commands() {
        let dir = project(&[("a.py", SIX)]);
        let text = stdout(&["--all", "--format", "github"], &dir);
        assert!(text.starts_with("::error file="), "{text}");
        assert!(text.contains("title=backspace/"), "{text}");
    }
}

mod subcommands {
    use super::*;

    #[test]
    fn languages_lists_the_built_ins() {
        let out = bin().arg("languages").output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        for lang in ["python", "rust", "go", "typescript", "bash"] {
            assert!(text.contains(lang), "missing {lang} in:\n{text}");
        }
    }

    #[test]
    fn explain_describes_a_known_rule() {
        let out = bin().args(["explain", "block-too-long"]).output().unwrap();
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("max_lines"));
    }

    #[test]
    fn explain_rejects_an_unknown_rule() {
        bin().args(["explain", "nope"]).assert().code(2);
    }

    #[test]
    fn config_show_reports_values_and_provenance() {
        let dir = project(&[(".backspace.toml", "max_lines = 17\n"), ("a.py", "x = 1\n")]);
        let out = bin()
            .current_dir(dir.path())
            .args(["config", "show", "a.py"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("max_lines"), "{text}");
        assert!(text.contains("17"), "{text}");
        assert!(text.contains("config file"), "{text}");
    }

    #[test]
    fn config_show_marks_untouched_values_as_defaults() {
        let dir = project(&[("a.py", "x = 1\n")]);
        let out = bin()
            .current_dir(dir.path())
            .args(["config", "show", "a.py"])
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains("default"));
    }
}

mod diff_mode {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// A repo with one committed file holding a long comment.
    fn repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        git(dir.path(), &["config", "user.email", "t@example.com"]);
        git(dir.path(), &["config", "user.name", "t"]);
        fs::write(
            dir.path().join("a.py"),
            "# a\n# b\n# c\n# d\n# e\n# f\n# g\nx = 1\n",
        )
        .unwrap();
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-qm", "initial"]);
        dir
    }

    #[test]
    fn unchanged_code_is_not_reported() {
        let dir = repo();
        bin()
            .current_dir(dir.path())
            .args(["--diff", "."])
            .assert()
            .success();
    }

    #[test]
    fn the_same_code_is_reported_with_all() {
        let dir = repo();
        bin()
            .current_dir(dir.path())
            .args(["--all", "."])
            .assert()
            .code(1);
    }

    #[test]
    fn touching_a_comment_makes_it_reportable() {
        let dir = repo();
        fs::write(
            dir.path().join("a.py"),
            "# a\n# b CHANGED\n# c\n# d\n# e\n# f\n# g\nx = 1\n",
        )
        .unwrap();
        bin()
            .current_dir(dir.path())
            .args(["--diff", "."])
            .assert()
            .code(1);
    }

    #[test]
    fn changing_unrelated_code_leaves_the_old_comment_alone() {
        let dir = repo();
        fs::write(dir.path().join("b.py"), "y = 2\n").unwrap();
        bin()
            .current_dir(dir.path())
            .args(["--diff", "."])
            .assert()
            .success();
    }

    #[test]
    fn outside_a_repo_it_falls_back_to_whole_files_with_a_warning() {
        let dir = project(&[("a.py", &"# c\n".repeat(9))]);
        let out = bin()
            .current_dir(dir.path())
            .args(["--diff", "."])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&out.stderr).contains("not a git repository"));
    }
}

mod dogfood {
    use super::*;

    /// The tool must pass its own check. This is a real constraint on how the
    /// source is written, and the best test of whether the defaults are livable.
    #[test]
    fn backspace_passes_its_own_lint() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let out = bin()
            .current_dir(root)
            .args(["--all", "src", "languages"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "backspace fails its own lint:\n{}",
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

mod audit_mode {
    use super::*;

    const SRC: &str = "# first note\nx = 1\n\n# second note\n# continues here\ny = 2\n";

    #[test]
    fn lists_every_comment_and_exits_zero() {
        let dir = project(&[("a.py", SRC)]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--audit"])
            .output()
            .unwrap();
        assert!(out.status.success(), "audit must never fail the run");
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("first note"), "{text}");
        assert!(text.contains("second note"), "{text}");
    }

    #[test]
    fn exits_zero_even_when_comments_would_violate() {
        let dir = project(&[("a.py", &"# c\n".repeat(20))]);
        bin()
            .arg(dir.path())
            .args(["--all", "--audit"])
            .assert()
            .success();
    }

    #[test]
    fn json_audit_reports_each_comment_with_its_location() {
        let dir = project(&[("a.py", SRC)]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--audit", "--json"])
            .output()
            .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
        let comments = v["comments"].as_array().expect("comments array");
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0]["start_line"], 1);
        assert_eq!(comments[0]["line_count"], 1);
        assert_eq!(comments[1]["line_count"], 2);
        assert!(comments[0]["language"] == "python");
    }

    #[test]
    fn reports_word_counts_so_a_reviewer_can_spot_the_wordy_ones() {
        let dir = project(&[("a.py", "# one two three four five\nx = 1\n")]);
        let out = bin()
            .arg(dir.path())
            .args(["--all", "--audit", "--json"])
            .output()
            .unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
        assert_eq!(v["comments"][0]["words"], 5);
    }

    #[test]
    fn docstrings_are_listed_only_when_included() {
        let src = "def f():\n    \"\"\"Docs here.\"\"\"\n    return 1\n";
        let dir = project(&[("a.py", src)]);
        let count = |args: &[&str]| -> usize {
            let out = bin().arg(dir.path()).args(args).output().unwrap();
            let v: serde_json::Value =
                serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).unwrap();
            v["comments"].as_array().unwrap().len()
        };
        assert_eq!(count(&["--all", "--audit", "--json"]), 0);
        assert_eq!(
            count(&["--all", "--audit", "--json", "--include-docstrings"]),
            1
        );
    }
}

mod line_word_budget {
    use super::*;

    #[test]
    fn max_line_words_flag_catches_a_one_line_essay() {
        let src = "# Set the user's name to the provided value if it is not None otherwise keep it\nx = 1\n";
        let dir = project(&[("a.py", src)]);
        bin().arg(dir.path()).arg("--all").assert().success();
        bin()
            .arg(dir.path())
            .args(["--all", "--max-line-words", "12"])
            .assert()
            .code(1);
    }
}
