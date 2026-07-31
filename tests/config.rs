//! Config discovery, layering and provenance.
//!
//! Layering is the part most likely to harbour quiet bugs, so these tests assert
//! the resolved value *and*, where it matters, which layer produced it.

use std::fs;
use std::path::Path;

use backspace::config::{Config, Layer, ResolvedConfig};
use tempfile::TempDir;

/// Builds a project tree from `(relative path, contents)` pairs.
fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
    dir
}

fn discover(dir: &Path) -> Config {
    Config::discover(dir).expect("discovery failed")
}

mod defaults {
    use super::*;

    #[test]
    fn apply_when_no_config_exists() {
        let dir = project(&[("src/a.py", "")]);
        let cfg = discover(dir.path()).resolve(Path::new("src/a.py"), "python");
        assert_eq!(cfg.max_lines, 5);
        assert_eq!(cfg.max_ratio, 1.5);
        assert!(!cfg.include_docstrings);
        assert!(cfg.merge_across_blank_lines);
    }

    #[test]
    fn banned_phrases_are_empty_so_the_tool_is_not_preachy_out_of_the_box() {
        let dir = project(&[("src/a.py", "")]);
        let cfg = discover(dir.path()).resolve(Path::new("src/a.py"), "python");
        assert!(cfg.banned_phrases.is_empty());
    }
}

mod sources {
    use super::*;

    #[test]
    fn reads_dot_backspace_toml() {
        let dir = project(&[(".backspace.toml", "max_lines = 12\n")]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.py"), "python")
                .max_lines,
            12
        );
    }

    #[test]
    fn reads_backspace_toml_without_the_dot() {
        let dir = project(&[("backspace.toml", "max_lines = 11\n")]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.py"), "python")
                .max_lines,
            11
        );
    }

    #[test]
    fn reads_pyproject_tool_table() {
        let dir = project(&[(
            "pyproject.toml",
            "[project]\nname = \"x\"\n\n[tool.backspace]\nmax_lines = 9\n",
        )]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.py"), "python")
                .max_lines,
            9
        );
    }

    #[test]
    fn reads_package_json_backspace_key() {
        let dir = project(&[(
            "package.json",
            r#"{"name":"x","backspace":{"max_lines":8}}"#,
        )]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.ts"), "typescript")
                .max_lines,
            8
        );
    }

    #[test]
    fn reads_cargo_package_metadata() {
        let dir = project(&[(
            "Cargo.toml",
            "[package]\nname = \"x\"\nversion = \"0.1.0\"\n\n[package.metadata.backspace]\nmax_lines = 7\n",
        )]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.rs"), "rust")
                .max_lines,
            7
        );
    }

    #[test]
    fn a_package_manager_file_without_our_key_is_not_treated_as_config() {
        let dir = project(&[("pyproject.toml", "[project]\nname = \"x\"\n")]);
        let config = discover(dir.path());
        assert!(config.source().is_none());
        assert_eq!(config.resolve(Path::new("a.py"), "python").max_lines, 5);
    }

    #[test]
    fn a_dedicated_file_wins_over_a_package_manager_file() {
        let dir = project(&[
            (".backspace.toml", "max_lines = 3\n"),
            ("pyproject.toml", "[tool.backspace]\nmax_lines = 30\n"),
        ]);
        assert_eq!(
            discover(dir.path())
                .resolve(Path::new("a.py"), "python")
                .max_lines,
            3
        );
    }

    #[test]
    fn config_is_found_by_walking_up_from_a_nested_directory() {
        let dir = project(&[
            (".backspace.toml", "max_lines = 4\n"),
            ("a/b/c/keep.py", ""),
        ]);
        let start = dir.path().join("a/b/c");
        assert_eq!(
            discover(&start)
                .resolve(Path::new("a/b/c/keep.py"), "python")
                .max_lines,
            4
        );
    }

    #[test]
    fn an_explicit_config_path_skips_discovery() {
        let dir = project(&[
            (".backspace.toml", "max_lines = 3\n"),
            ("other.toml", "max_lines = 21\n"),
        ]);
        let cfg = Config::from_file(&dir.path().join("other.toml")).unwrap();
        assert_eq!(cfg.resolve(Path::new("a.py"), "python").max_lines, 21);
    }
}

mod layering {
    use super::*;

    const BASE: &str = r#"
max_lines = 5

[languages.go]
max_lines = 8

[[overrides]]
paths = ["tests/**"]
max_lines = 20

[[overrides]]
paths = ["tests/critical/**"]
max_lines = 2
"#;

    fn cfg() -> Config {
        let dir = project(&[(".backspace.toml", BASE)]);
        // Leaked so the returned Config outlives the temp dir in these tests.
        let cfg = discover(dir.path());
        std::mem::forget(dir);
        cfg
    }

    #[test]
    fn language_override_beats_the_top_level_value() {
        assert_eq!(cfg().resolve(Path::new("main.go"), "go").max_lines, 8);
    }

    #[test]
    fn language_override_does_not_leak_to_other_languages() {
        assert_eq!(cfg().resolve(Path::new("main.py"), "python").max_lines, 5);
    }

    #[test]
    fn path_override_beats_a_language_override() {
        assert_eq!(cfg().resolve(Path::new("tests/a.go"), "go").max_lines, 20);
    }

    #[test]
    fn a_later_path_override_beats_an_earlier_one() {
        assert_eq!(
            cfg()
                .resolve(Path::new("tests/critical/a.py"), "python")
                .max_lines,
            2
        );
    }

    #[test]
    fn unmatched_paths_keep_the_top_level_value() {
        assert_eq!(cfg().resolve(Path::new("src/a.py"), "python").max_lines, 5);
    }

    #[test]
    fn cli_flags_beat_every_file_layer() {
        let mut c = cfg();
        c.cli.max_lines = Some(1);
        assert_eq!(c.resolve(Path::new("tests/a.go"), "go").max_lines, 1);
    }

    #[test]
    fn an_override_only_changes_the_keys_it_names() {
        // `tests/**` sets max_lines only; max_ratio must survive from defaults.
        assert_eq!(cfg().resolve(Path::new("tests/a.go"), "go").max_ratio, 1.5);
    }
}

mod rules_section {
    use super::*;

    #[test]
    fn rule_settings_are_read_from_their_own_table() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.comment-code-ratio]\nmax_ratio = 4.0\nratio_min_lines = 6\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.max_ratio, 4.0);
        assert_eq!(cfg.ratio_min_lines, 6);
    }

    #[test]
    fn the_llm_tells_preset_can_be_enabled_by_name() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.banned-phrase]\npreset = \"llm-tells\"\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert!(!cfg.banned_phrases.is_empty());
    }

    #[test]
    fn extend_adds_to_the_preset_rather_than_replacing_it() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.banned-phrase]\npreset = \"llm-tells\"\nextend = [\"(?i)as an ai\"]\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert!(cfg
            .banned_phrases
            .iter()
            .any(|p| p.display.contains("as an ai")));
        assert!(cfg.banned_phrases.len() > 1);
    }

    #[test]
    fn what_not_why_settings_are_read_from_their_own_table() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.explains-what-not-why]\nthreshold = 0.5\nmin_lines = 3\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.what_not_why_threshold, 0.5);
        assert_eq!(cfg.what_not_why_min_lines, 3);
    }

    #[test]
    fn rationale_markers_extend_the_built_in_list() {
        let built_in = ResolvedConfig::default().rationale_markers.len();
        let dir = project(&[(
            ".backspace.toml",
            "[rules.explains-what-not-why]\nextend = [\"rationale\"]\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.rationale_markers.len(), built_in + 1);
        assert!(cfg.rationale_markers.iter().any(|m| m == "rationale"));
    }

    #[test]
    fn markers_replace_the_built_in_list_outright() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.explains-what-not-why]\nmarkers = [\"rationale\"]\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.rationale_markers, ["rationale"]);
    }

    #[test]
    fn passive_voice_can_be_widened_to_agentless_passives() {
        assert!(ResolvedConfig::default().passive_requires_agent);
        let dir = project(&[(
            ".backspace.toml",
            "[rules.passive-voice]\nrequire_agent = false\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert!(!cfg.passive_requires_agent);
    }

    #[test]
    fn select_and_ignore_are_read() {
        let dir = project(&[(
            ".backspace.toml",
            "select = [\"block-too-long\"]\nignore = [\"comment-code-ratio\"]\n",
        )]);
        let cfg = discover(dir.path()).resolve(Path::new("a.py"), "python");
        assert!(cfg.rule_enabled("block-too-long"));
        assert!(!cfg.rule_enabled("comment-code-ratio"));
    }
}

mod validation {
    use super::*;

    /// The full error chain, which is what the CLI prints.
    fn error(dir: &Path) -> String {
        format!("{:#}", Config::discover(dir).unwrap_err())
    }

    #[test]
    fn an_unknown_key_is_an_error_not_a_silent_no_op() {
        let dir = project(&[(".backspace.toml", "max_linez = 3\n")]);
        let err = error(dir.path());
        assert!(err.contains("max_linez"), "{err}");
    }

    #[test]
    fn an_unknown_rule_id_in_select_is_an_error() {
        let dir = project(&[(".backspace.toml", "select = [\"no-such-rule\"]\n")]);
        let err = error(dir.path());
        assert!(err.contains("no-such-rule"), "{err}");
    }

    #[test]
    fn malformed_toml_reports_the_file_it_came_from() {
        let dir = project(&[(".backspace.toml", "max_lines = \n")]);
        let err = error(dir.path());
        assert!(err.contains(".backspace.toml"), "{err}");
    }

    #[test]
    fn an_invalid_banned_phrase_regex_is_rejected() {
        let dir = project(&[(
            ".backspace.toml",
            "[rules.banned-phrase]\nextend = [\"([\"]\n",
        )]);
        assert!(Config::discover(dir.path()).is_err());
    }
}

mod custom_languages {
    use super::*;

    const NIX: &str = r##"
[[languages.custom]]
name = "nix"
extensions = [".nix"]
line_comments = ["#"]
"##;

    #[test]
    fn a_user_defined_language_is_registered() {
        let dir = project(&[(".backspace.toml", NIX)]);
        let cfg = discover(dir.path());
        assert!(cfg.registry().get("nix").is_some());
    }

    #[test]
    fn a_user_defined_language_is_detected_by_extension() {
        let dir = project(&[(".backspace.toml", NIX)]);
        let cfg = discover(dir.path());
        let lang = cfg.registry().detect(Path::new("x.nix"), "").unwrap();
        assert_eq!(lang.name, "nix");
    }

    #[test]
    fn a_user_definition_can_replace_a_built_in_one() {
        let dir = project(&[(
            ".backspace.toml",
            "[[languages.custom]]\nname = \"python\"\nextensions = [\".py\"]\nline_comments = [\"//\"]\n",
        )]);
        let cfg = discover(dir.path());
        let lang = cfg.registry().get("python").unwrap();
        assert_eq!(lang.line_comments, ["//"]);
    }
}

mod provenance {
    use super::*;

    #[test]
    fn reports_the_default_layer_when_nothing_overrides() {
        let dir = project(&[("a.py", "")]);
        let (_, prov) = discover(dir.path()).resolve_verbose(Path::new("a.py"), "python");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::Default));
    }

    #[test]
    fn reports_the_config_file_layer() {
        let dir = project(&[(".backspace.toml", "max_lines = 9\n")]);
        let (_, prov) = discover(dir.path()).resolve_verbose(Path::new("a.py"), "python");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::File));
    }

    #[test]
    fn reports_the_language_layer() {
        let dir = project(&[(".backspace.toml", "[languages.go]\nmax_lines = 9\n")]);
        let (_, prov) = discover(dir.path()).resolve_verbose(Path::new("a.go"), "go");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::Language));
    }

    #[test]
    fn reports_the_override_layer_with_its_index() {
        let dir = project(&[(
            ".backspace.toml",
            "[[overrides]]\npaths = [\"a/**\"]\nmax_lines = 9\n",
        )]);
        let (_, prov) = discover(dir.path()).resolve_verbose(Path::new("a/b.py"), "python");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::Override(0)));
    }

    #[test]
    fn reports_the_cli_layer() {
        let dir = project(&[(".backspace.toml", "max_lines = 9\n")]);
        let mut c = discover(dir.path());
        c.cli.max_lines = Some(2);
        let (_, prov) = c.resolve_verbose(Path::new("a.py"), "python");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::Cli));
    }
}

mod excludes {
    use super::*;

    #[test]
    fn an_excluded_path_is_reported_as_excluded() {
        let dir = project(&[(
            ".backspace.toml",
            "exclude = [\"**/vendor/**\", \"**/*.generated.*\"]\n",
        )]);
        let cfg = discover(dir.path());
        assert!(cfg.is_excluded(Path::new("a/vendor/b.py")));
        assert!(cfg.is_excluded(Path::new("a/x.generated.ts")));
        assert!(!cfg.is_excluded(Path::new("src/main.py")));
    }

    #[test]
    fn a_leading_dot_slash_does_not_defeat_a_glob() {
        // Directory walks yield `./a/b.py`; globs are written without the prefix.
        let dir = project(&[(".backspace.toml", "exclude = [\"tests/**\"]\n")]);
        let cfg = discover(dir.path());
        assert!(cfg.is_excluded(Path::new("./tests/a.py")));
    }

    #[test]
    fn overrides_also_tolerate_a_leading_dot_slash() {
        let dir = project(&[(
            ".backspace.toml",
            "[[overrides]]\npaths = [\"tests/**\"]\nmax_lines = 42\n",
        )]);
        let cfg = discover(dir.path());
        assert_eq!(
            cfg.resolve(Path::new("./tests/a.py"), "python").max_lines,
            42
        );
    }
}

mod user_config {
    use super::*;

    /// Points config discovery at a scratch directory so these tests never read
    /// or write the developer's real `~/.config`.
    fn with_user_config(contents: &str) -> (TempDir, TempDir) {
        let home = tempfile::tempdir().unwrap();
        fs::write(home.path().join("ill-be-backspace.toml"), contents).unwrap();
        let proj = tempfile::tempdir().unwrap();
        (home, proj)
    }

    fn discover_with(home: &Path, proj: &Path) -> Config {
        Config::discover_in(proj, Some(home)).expect("discovery failed")
    }

    #[test]
    fn user_config_applies_when_no_project_config_exists() {
        let (home, proj) = with_user_config("max_lines = 9\n");
        let cfg = discover_with(home.path(), proj.path());
        assert_eq!(cfg.resolve(Path::new("a.py"), "python").max_lines, 9);
    }

    #[test]
    fn a_project_config_overrides_the_user_config() {
        let (home, proj) = with_user_config("max_lines = 9\n");
        fs::write(proj.path().join(".backspace.toml"), "max_lines = 3\n").unwrap();
        let cfg = discover_with(home.path(), proj.path());
        assert_eq!(cfg.resolve(Path::new("a.py"), "python").max_lines, 3);
    }

    #[test]
    fn keys_the_project_does_not_set_still_come_from_the_user() {
        let (home, proj) = with_user_config("max_lines = 9\nrequire_suppression_reason = true\n");
        fs::write(proj.path().join(".backspace.toml"), "max_lines = 3\n").unwrap();
        let cfg = discover_with(home.path(), proj.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.max_lines, 3);
        assert!(cfg.require_suppression_reason);
    }

    #[test]
    fn personal_banned_words_apply_everywhere() {
        let (home, proj) =
            with_user_config("[rules.banned-phrase]\nextend = [\"delve into\", \"leverage\"]\n");
        let cfg = discover_with(home.path(), proj.path()).resolve(Path::new("a.py"), "python");
        assert!(cfg.banned_phrases.iter().any(|p| p.display == "delve into"));
        assert!(cfg.rule_enabled("banned-phrase"));
    }

    #[test]
    fn a_project_can_add_words_without_losing_the_users() {
        let (home, proj) = with_user_config("[rules.banned-phrase]\nextend = [\"delve into\"]\n");
        fs::write(
            proj.path().join(".backspace.toml"),
            "[rules.banned-phrase]\nextend = [\"synergy\"]\n",
        )
        .unwrap();
        let cfg = discover_with(home.path(), proj.path()).resolve(Path::new("a.py"), "python");
        assert!(cfg.banned_phrases.iter().any(|p| p.display == "delve into"));
        assert!(cfg.banned_phrases.iter().any(|p| p.display == "synergy"));
    }

    #[test]
    fn a_project_can_replace_the_word_list_outright() {
        let (home, proj) = with_user_config("[rules.banned-phrase]\nextend = [\"delve into\"]\n");
        fs::write(
            proj.path().join(".backspace.toml"),
            "[rules.banned-phrase]\npatterns = [\"only-this\"]\n",
        )
        .unwrap();
        let cfg = discover_with(home.path(), proj.path()).resolve(Path::new("a.py"), "python");
        assert_eq!(cfg.banned_phrases[0].display, "only-this");
        assert_eq!(cfg.banned_phrases.len(), 1);
    }

    #[test]
    fn a_user_defined_language_is_available_to_every_project() {
        let (home, proj) = with_user_config(
            "[[languages.custom]]\nname = \"nix\"\nextensions = [\".nix\"]\nline_comments = [\"#\"]\n",
        );
        let cfg = discover_with(home.path(), proj.path());
        assert!(cfg.registry().get("nix").is_some());
    }

    #[test]
    fn user_excludes_are_honoured() {
        let (home, proj) = with_user_config("exclude = [\"**/scratch/**\"]\n");
        let cfg = discover_with(home.path(), proj.path());
        assert!(cfg.is_excluded(Path::new("a/scratch/b.py")));
    }

    #[test]
    fn provenance_names_the_user_layer() {
        let (home, proj) = with_user_config("max_lines = 9\n");
        let (_, prov) =
            discover_with(home.path(), proj.path()).resolve_verbose(Path::new("a.py"), "python");
        assert_eq!(prov.layer_of("max_lines"), Some(Layer::User));
    }

    #[test]
    fn a_missing_user_config_is_not_an_error() {
        let home = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        let cfg = discover_with(home.path(), proj.path());
        assert_eq!(cfg.resolve(Path::new("a.py"), "python").max_lines, 5);
        assert!(cfg.user_source().is_none());
    }

    #[test]
    fn a_broken_user_config_reports_the_file_it_came_from() {
        let (home, proj) = with_user_config("max_linez = 3\n");
        let err = format!(
            "{:#}",
            Config::discover_in(proj.path(), Some(home.path())).unwrap_err()
        );
        assert!(err.contains("ill-be-backspace.toml"), "{err}");
    }
}
