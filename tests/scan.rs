//! Scanner behaviour, one module per concern.
//!
//! The traps that matter are comment markers appearing inside string literals —
//! getting those wrong produces false positives on ordinary code, which is the
//! fastest way to make a linter get uninstalled.

use backspace::lang::Registry;
use backspace::{scan, CommentBlock, CommentKind, ScanOptions};

fn blocks(source: &str, lang: &str) -> Vec<CommentBlock> {
    let spec = Registry::builtin()
        .get(lang)
        .unwrap_or_else(|| panic!("no such language: {lang}"));
    scan(source, spec, &ScanOptions::default())
}

fn blocks_unmerged(source: &str, lang: &str) -> Vec<CommentBlock> {
    let spec = Registry::builtin().get(lang).unwrap();
    let opts = ScanOptions {
        merge_across_blank_lines: false,
    };
    scan(source, spec, &opts)
}

/// `(start_line, end_line, line_count)` — the shape assertions most tests need.
fn shape(blocks: &[CommentBlock]) -> Vec<(u32, u32, usize)> {
    blocks
        .iter()
        .map(|b| (b.start_line, b.end_line, b.line_count()))
        .collect()
}

mod basics {
    use super::*;

    #[test]
    fn empty_source_has_no_blocks() {
        assert!(blocks("", "python").is_empty());
        assert!(blocks("\n\n\n", "python").is_empty());
    }

    #[test]
    fn code_without_comments_has_no_blocks() {
        assert!(blocks("x = 1\ny = 2\n", "python").is_empty());
    }

    #[test]
    fn single_line_comment() {
        let b = blocks("# hello\nx = 1\n", "python");
        assert_eq!(shape(&b), [(1, 1, 1)]);
        assert_eq!(b[0].text, ["hello"]);
        assert_eq!(b[0].kind, CommentKind::Line);
    }

    #[test]
    fn consecutive_comments_form_one_block() {
        let b = blocks("# one\n# two\n# three\nx = 1\n", "python");
        assert_eq!(shape(&b), [(1, 3, 3)]);
        assert_eq!(b[0].text, ["one", "two", "three"]);
    }

    #[test]
    fn code_between_comments_splits_blocks() {
        let b = blocks("# one\nx = 1\n# two\n", "python");
        assert_eq!(shape(&b), [(1, 1, 1), (3, 3, 1)]);
    }

    #[test]
    fn trailing_comment_after_code_is_its_own_block() {
        let b = blocks("x = 1  # trailing\n", "python");
        assert_eq!(shape(&b), [(1, 1, 1)]);
        assert_eq!(b[0].text, ["trailing"]);
    }

    #[test]
    fn markers_are_stripped_with_surrounding_whitespace() {
        let b = blocks("#no space\n#   lots of space\n", "python");
        assert_eq!(b[0].text, ["no space", "lots of space"]);
    }

    #[test]
    fn empty_comment_lines_are_kept_as_empty_strings() {
        let b = blocks("# one\n#\n# two\n", "python");
        assert_eq!(shape(&b), [(1, 3, 3)]);
        assert_eq!(b[0].text, ["one", "", "two"]);
    }

    #[test]
    fn column_records_the_marker_position() {
        let b = blocks("    # indented\n", "python");
        assert_eq!(b[0].column, 5);
    }

    #[test]
    fn file_without_trailing_newline_still_closes_the_block() {
        let b = blocks("# one\n# two", "python");
        assert_eq!(shape(&b), [(1, 2, 2)]);
    }

    #[test]
    fn crlf_line_endings_do_not_leak_into_text() {
        let b = blocks("# one\r\n# two\r\n", "python");
        assert_eq!(b[0].text, ["one", "two"]);
    }
}

mod blank_line_merging {
    use super::*;

    const PARAGRAPHS: &str = "# para one\n\n# para two\nx = 1\n";

    #[test]
    fn merges_across_blank_lines_by_default() {
        let b = blocks(PARAGRAPHS, "python");
        assert_eq!(shape(&b), [(1, 3, 2)]);
        assert_eq!(b[0].text, ["para one", "para two"]);
    }

    #[test]
    fn splits_across_blank_lines_when_disabled() {
        let b = blocks_unmerged(PARAGRAPHS, "python");
        assert_eq!(shape(&b), [(1, 1, 1), (3, 3, 1)]);
    }

    #[test]
    fn trailing_blank_lines_are_not_absorbed_into_the_block() {
        let b = blocks("# one\n\n\nx = 1\n", "python");
        assert_eq!(shape(&b), [(1, 1, 1)]);
    }

    #[test]
    fn blank_run_between_paragraphs_is_spanned_but_not_counted() {
        let b = blocks("# one\n\n\n\n# two\n", "python");
        assert_eq!(shape(&b), [(1, 5, 2)]);
    }
}

mod following_code {
    use super::*;

    #[test]
    fn counts_code_lines_until_a_blank() {
        let b = blocks("# c\nx = 1\ny = 2\n\nz = 3\n", "python");
        assert_eq!(b[0].following_code_lines, 2);
    }

    #[test]
    fn counts_zero_when_the_next_line_is_another_comment_block() {
        let b = blocks("# c\nx = 1\n", "python");
        assert_eq!(b[0].following_code_lines, 1);
    }

    #[test]
    fn tolerates_one_blank_line_between_comment_and_code() {
        let b = blocks("# c\n\nx = 1\ny = 2\n", "python");
        assert_eq!(b[0].following_code_lines, 2);
    }

    #[test]
    fn is_zero_at_end_of_file() {
        let b = blocks("x = 1\n# trailing thought\n", "python");
        assert_eq!(b[0].following_code_lines, 0);
    }

    #[test]
    fn a_trailing_comment_counts_its_own_line_as_code() {
        // `x = 1  # note` has code on the same line; the ratio rule should not
        // treat this as a comment floating above nothing.
        let b = blocks("x = 1  # note\n", "python");
        assert_eq!(b[0].following_code_lines, 1);
    }
}

mod string_literals {
    use super::*;

    #[test]
    fn hash_inside_a_python_string_is_not_a_comment() {
        assert!(blocks("s = \"# not a comment\"", "python").is_empty());
        assert!(blocks("s = '# not a comment'", "python").is_empty());
    }

    #[test]
    fn url_inside_a_go_string_is_not_a_comment() {
        assert!(blocks(r#"u := "https://example.com/x""#, "go").is_empty());
    }

    #[test]
    fn marker_inside_a_go_raw_string_is_not_a_comment() {
        assert!(blocks("s := `line // not a comment`\n", "go").is_empty());
    }

    #[test]
    fn escaped_quote_does_not_end_the_string() {
        assert!(blocks(r#"s = "he said \" # nope""#, "python").is_empty());
    }

    #[test]
    fn backslash_is_literal_in_a_raw_string_so_the_string_still_ends() {
        // In Go raw strings there is no escape, so the quote closes and the
        // trailing `//` is a real comment.
        let b = blocks("s := `a\\` // real\n", "go");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
    }

    #[test]
    fn multiline_string_spans_lines_and_hides_markers() {
        let src = "s = '''\n# not a comment\nstill not\n'''\n# real\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
        assert_eq!(b[0].start_line, 5);
    }

    #[test]
    fn unterminated_single_line_string_does_not_swallow_the_file() {
        // A stray quote is a syntax error, but it must not silence every
        // subsequent comment in the file.
        let b = blocks("s = \"oops\nx = 1\n# real\n", "python");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
    }

    #[test]
    fn rust_raw_string_with_hashes() {
        assert!(blocks("let s = r#\"a // b\"#;", "rust").is_empty());
    }

    #[test]
    fn template_literal_hides_markers() {
        assert!(blocks("const s = `a // b`;\n", "typescript").is_empty());
    }
}

mod block_comments {
    use super::*;

    #[test]
    fn single_line_block_comment() {
        let b = blocks("/* hello */\n", "go");
        assert_eq!(shape(&b), [(1, 1, 1)]);
        assert_eq!(b[0].text, ["hello"]);
        assert_eq!(b[0].kind, CommentKind::Block);
    }

    #[test]
    fn multi_line_block_comment_counts_every_line() {
        let src = "/*\n * one\n * two\n */\nx := 1\n";
        let b = blocks(src, "go");
        assert_eq!(shape(&b), [(1, 4, 4)]);
        assert_eq!(b[0].text, ["", "one", "two", ""]);
    }

    #[test]
    fn leading_stars_are_stripped_from_continuation_lines() {
        let b = blocks("/*\n * kept\n */\n", "go");
        assert_eq!(b[0].text[1], "kept");
    }

    #[test]
    fn rust_block_comments_nest() {
        let src = "/* outer /* inner */ still comment */\nlet x = 1;\n";
        let b = blocks(src, "rust");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].end_line, 1);
    }

    #[test]
    fn go_block_comments_do_not_nest() {
        // The first `*/` closes it, so `still` is code, not comment.
        let src = "/* outer /* inner */ still\n";
        let b = blocks(src, "go");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["outer /* inner"]);
    }

    #[test]
    fn unterminated_block_comment_runs_to_end_of_file() {
        let b = blocks("/* never closed\nmore\n", "go");
        assert_eq!(shape(&b), [(1, 2, 2)]);
    }

    #[test]
    fn adjacent_line_and_block_comments_merge_into_one_block() {
        let b = blocks("// one\n/* two */\n", "go");
        assert_eq!(shape(&b), [(1, 2, 2)]);
    }

    #[test]
    fn block_comment_marker_inside_a_string_is_ignored() {
        assert!(blocks(r#"s := "/* not a comment */""#, "go").is_empty());
    }
}

mod doc_comments {
    use super::*;

    #[test]
    fn rust_slash_slash_slash_is_a_doc_comment() {
        let b = blocks("/// docs\nfn f() {}\n", "rust");
        assert_eq!(b[0].kind, CommentKind::Doc);
        assert_eq!(b[0].text, ["docs"]);
    }

    #[test]
    fn rust_inner_doc_comment() {
        assert_eq!(
            blocks("//! module docs\n", "rust")[0].kind,
            CommentKind::Doc
        );
    }

    #[test]
    fn plain_rust_comment_is_not_a_doc_comment() {
        assert_eq!(blocks("// aside\n", "rust")[0].kind, CommentKind::Line);
    }

    #[test]
    fn jsdoc_is_a_doc_comment() {
        let b = blocks("/** docs */\n", "typescript");
        assert_eq!(b[0].kind, CommentKind::Doc);
    }

    #[test]
    fn doc_and_plain_comments_do_not_merge_into_one_block() {
        // Different kinds have different budgets, so they must stay separable.
        let b = blocks("// aside\n/// docs\nfn f() {}\n", "rust");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].kind, CommentKind::Line);
        assert_eq!(b[1].kind, CommentKind::Doc);
    }
}

mod python_docstrings {
    use super::*;

    #[test]
    fn module_docstring_is_a_doc_block() {
        let b = blocks("\"\"\"Module docs.\"\"\"\nimport os\n", "python");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, CommentKind::Doc);
        assert_eq!(b[0].text, ["Module docs."]);
    }

    #[test]
    fn function_docstring_is_a_doc_block() {
        let src = "def f():\n    \"\"\"Does a thing.\"\"\"\n    return 1\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].kind, CommentKind::Doc);
        assert_eq!(b[0].start_line, 2);
    }

    #[test]
    fn multiline_docstring_spans_its_lines() {
        let src = "def f():\n    \"\"\"Summary.\n\n    Detail.\n    \"\"\"\n    return 1\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].start_line, b[0].end_line), (2, 5));
    }

    #[test]
    fn class_docstring_is_a_doc_block() {
        let src = "class C:\n    \"\"\"Docs.\"\"\"\n    x = 1\n";
        assert_eq!(blocks(src, "python")[0].kind, CommentKind::Doc);
    }

    #[test]
    fn a_string_assigned_to_a_variable_is_not_a_docstring() {
        assert!(blocks("x = \"\"\"not docs\"\"\"\n", "python").is_empty());
    }

    #[test]
    fn a_string_after_real_code_is_not_a_docstring() {
        let src = "import os\nx = 1\n\"\"\"not docs\"\"\"\n";
        assert!(blocks(src, "python").is_empty());
    }

    #[test]
    fn docstring_after_a_comment_and_def_still_counts() {
        let src = "def f():\n    # note\n    \"\"\"Docs.\"\"\"\n    return 1\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].kind, CommentKind::Doc);
    }

    #[test]
    fn docstring_and_comment_do_not_merge() {
        let src = "\"\"\"Docs.\"\"\"\n# aside\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].kind, CommentKind::Doc);
        assert_eq!(b[1].kind, CommentKind::Line);
    }
}

mod js_regex_literals {
    use super::*;

    #[test]
    fn regex_literal_containing_a_slash_is_not_a_comment() {
        assert!(blocks("const re = /a\\/b/;\nconst x = 1;\n", "typescript").is_empty());
    }

    #[test]
    fn regex_after_an_operator_is_a_regex() {
        assert!(blocks("if (s.match(/x\\/y/)) { }\n", "javascript").is_empty());
    }

    #[test]
    fn division_is_not_mistaken_for_a_regex() {
        let b = blocks("const x = a / b; // real\n", "javascript");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
    }

    #[test]
    fn comment_after_a_regex_literal_is_still_found() {
        let b = blocks("const re = /a/; // real\n", "javascript");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
    }

    #[test]
    fn a_regex_is_not_assumed_in_languages_without_them() {
        // Go has no regex literals, so `/` is only ever division or a comment.
        let b = blocks("x := a / b // real\n", "go");
        assert_eq!(b[0].text, ["real"]);
    }
}

mod other_languages {
    use super::*;

    #[test]
    fn bash_comments_and_strings() {
        let b = blocks("# real\necho \"# not a comment\"\n", "bash");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, ["real"]);
    }

    #[test]
    fn bash_shebang_line_is_a_comment_line() {
        // It matches `#`, and treating it as a comment is correct — it just
        // never accumulates into a long block on its own.
        let b = blocks("#!/bin/bash\necho hi\n", "bash");
        assert_eq!(shape(&b), [(1, 1, 1)]);
    }

    #[test]
    fn sql_double_dash_comments() {
        let b = blocks("-- one\n-- two\nSELECT 1;\n", "sql");
        assert_eq!(shape(&b), [(1, 2, 2)]);
    }

    #[test]
    fn ruby_begin_end_block_comment() {
        let b = blocks("=begin\none\ntwo\n=end\nx = 1\n", "ruby");
        assert_eq!(shape(&b), [(1, 4, 4)]);
    }

    #[test]
    fn yaml_comments() {
        let b = blocks("# one\nkey: value  # two\n", "yaml");
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn lua_block_comment_beats_line_comment_prefix() {
        // `--[[` must win over `--`, or the block close is never found.
        let b = blocks("--[[\none\n]]\nx = 1\n", "lua");
        assert_eq!(shape(&b), [(1, 3, 3)]);
    }

    #[test]
    fn php_supports_both_line_comment_markers() {
        let b = blocks("// one\n# two\n", "php");
        assert_eq!(shape(&b), [(1, 2, 2)]);
    }
}

mod unicode {
    use super::*;

    #[test]
    fn multibyte_characters_do_not_break_line_numbering() {
        let src = "x = \"héllo — wörld\"\n# real\n";
        let b = blocks(src, "python");
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].start_line, 2);
    }

    #[test]
    fn multibyte_characters_in_comments_are_preserved() {
        let b = blocks("# héllo — wörld\n", "python");
        assert_eq!(b[0].text, ["héllo — wörld"]);
    }

    #[test]
    fn column_is_reported_in_characters_not_bytes() {
        let b = blocks("x = \"é\"  # note\n", "python");
        assert_eq!(b[0].column, 10);
    }
}
