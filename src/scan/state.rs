//! The comment-extraction state machine.
//!
//! Two passes: classify every line as code/comment/blank while tracking string and
//! comment state, then group runs of comment lines into blocks. Tracking string
//! state is the whole point — `s = "# not a comment"` must not read as a comment.

use crate::lang::{DocstringStyle, LanguageSpec};
use crate::scan::{CommentBlock, CommentKind, ScanOptions};

pub(crate) fn scan_impl(
    src: &str,
    spec: &LanguageSpec,
    opts: &ScanOptions,
) -> (Vec<CommentBlock>, String) {
    let lines = tokenize(src, spec);
    let code = lines
        .iter()
        .filter(|l| l.has_code)
        .map(|l| l.code_text.trim())
        .collect::<Vec<_>>()
        .join(" ");
    (group(&lines, opts), code)
}

#[derive(Debug, Clone)]
struct CommentPiece {
    text: String,
    kind: CommentKind,
    column: u32,
}

#[derive(Debug, Default, Clone)]
struct LineInfo {
    /// Non-whitespace characters outside of comments.
    has_code: bool,
    /// Code text only, used to spot `def`/`class` headers for docstrings.
    code_text: String,
    comment: Option<CommentPiece>,
    /// A comment preceded by code on the same line is a trailing aside, not part
    /// of a block with the lines above it.
    code_before_comment: bool,
}

impl LineInfo {
    fn is_blank(&self) -> bool {
        !self.has_code && self.comment.is_none()
    }

    fn is_block_joinable(&self) -> bool {
        self.comment.is_some() && !self.code_before_comment
    }

    fn is_doc(&self) -> bool {
        matches!(
            self.comment.as_ref().map(|c| c.kind),
            Some(CommentKind::Doc)
        )
    }
}

enum St {
    Code,
    Str {
        closer: String,
        escape: Option<char>,
        multiline: bool,
        /// A Python docstring: recorded as a comment rather than skipped as code.
        doc: bool,
    },
    Line,
    Block {
        close: String,
        nested: bool,
        open: String,
        depth: u32,
    },
    /// A JavaScript-family `/regex/`, which must not be read as a comment.
    Regex,
}

#[allow(unused_assignments)]
fn tokenize(src: &str, spec: &LanguageSpec) -> Vec<LineInfo> {
    let mut lines: Vec<LineInfo> = Vec::new();
    let mut cur = LineInfo::default();
    let mut st = St::Code;

    // Buffer for the comment text on the current line, carried across the newline
    // for multi-line block comments and docstrings.
    let mut buf = String::new();
    let mut pending_kind = CommentKind::Line;
    let mut pending_col = 1u32;
    // Buffer emptiness is not a proxy for this: a line holding only `*/` has no
    // content but is still a comment line.
    let mut in_comment_line = false;

    let mut col = 1u32;
    let mut prev_significant: Option<char> = None;
    let mut docstring_ctx = spec.docstrings == DocstringStyle::Python;

    let bytes = src.as_bytes();
    let mut i = 0usize;

    macro_rules! finish_line {
        () => {{
            if in_comment_line {
                push_piece(&mut cur, &buf, pending_kind, pending_col);
                buf.clear();
                in_comment_line = false;
            }
            update_docstring_ctx(&cur, &mut docstring_ctx, spec);
            lines.push(std::mem::take(&mut cur));
            col = 1;
        }};
    }

    while i < src.len() {
        let ch = char_at(src, i);

        if ch == '\n' {
            finish_line!();
            // A line comment ends at the newline; every other state carries over.
            if matches!(st, St::Line | St::Regex) {
                st = St::Code;
            }
            if let St::Str {
                multiline: false, ..
            } = st
            {
                // An unterminated single-line string must not swallow the file.
                st = St::Code;
            }
            i += 1;
            continue;
        }
        if ch == '\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }

        match &mut st {
            St::Code => {
                let rest = &src[i..];

                // Block opens are tried before line comments so Lua's `--[[` wins
                // over `--`, which would otherwise never find its closer.
                if let Some(bc) = spec
                    .block_comments
                    .iter()
                    .find(|b| rest.starts_with(&b.open))
                {
                    let kind = if spec.is_doc_comment(rest) {
                        CommentKind::Doc
                    } else {
                        CommentKind::Block
                    };
                    let strip = doc_marker_len(spec, rest).unwrap_or(bc.open.len());
                    begin_comment(&mut cur, &mut pending_kind, &mut pending_col, kind, col);
                    in_comment_line = true;
                    st = St::Block {
                        close: bc.close.clone(),
                        nested: bc.nested,
                        open: bc.open.clone(),
                        depth: 1,
                    };
                    advance(&mut i, &mut col, src, strip);
                    continue;
                }

                if let Some(marker) = spec
                    .line_comments
                    .iter()
                    .find(|m| rest.starts_with(m.as_str()))
                {
                    let kind = if spec.is_doc_comment(rest) {
                        CommentKind::Doc
                    } else {
                        CommentKind::Line
                    };
                    let strip = doc_marker_len(spec, rest).unwrap_or(marker.len());
                    begin_comment(&mut cur, &mut pending_kind, &mut pending_col, kind, col);
                    in_comment_line = true;
                    st = St::Line;
                    advance(&mut i, &mut col, src, strip);
                    continue;
                }

                if spec.regex_literals && ch == '/' && regex_can_start_here(prev_significant) {
                    cur.has_code = true;
                    cur.code_text.push(ch);
                    prev_significant = Some(ch);
                    st = St::Regex;
                    advance(&mut i, &mut col, src, 1);
                    continue;
                }

                if let Some(s) = spec.strings.iter().find(|s| rest.starts_with(&s.delim)) {
                    let doc =
                        spec.docstrings == DocstringStyle::Python && docstring_ctx && !cur.has_code;
                    if doc {
                        begin_comment(
                            &mut cur,
                            &mut pending_kind,
                            &mut pending_col,
                            CommentKind::Doc,
                            col,
                        );
                        in_comment_line = true;
                    } else {
                        cur.has_code = true;
                        cur.code_text.push_str(&s.delim);
                        prev_significant = s.delim.chars().last();
                    }
                    st = St::Str {
                        closer: s.closer().to_string(),
                        escape: s.escape_char(),
                        multiline: s.multiline,
                        doc,
                    };
                    advance(&mut i, &mut col, src, s.delim.len());
                    continue;
                }

                if !ch.is_whitespace() {
                    cur.has_code = true;
                    prev_significant = Some(ch);
                }
                cur.code_text.push(ch);
                advance(&mut i, &mut col, src, ch.len_utf8());
            }

            St::Line => {
                in_comment_line = true;
                buf.push(ch);
                advance(&mut i, &mut col, src, ch.len_utf8());
            }

            St::Block {
                close,
                nested,
                open,
                depth,
            } => {
                in_comment_line = true;
                let rest = &src[i..];
                if *nested && rest.starts_with(open.as_str()) {
                    *depth += 1;
                    buf.push_str(open);
                    advance(&mut i, &mut col, src, open.len());
                    continue;
                }
                if rest.starts_with(close.as_str()) {
                    *depth -= 1;
                    let len = close.len();
                    if *depth == 0 {
                        st = St::Code;
                    } else {
                        buf.push_str(close);
                    }
                    advance(&mut i, &mut col, src, len);
                    continue;
                }
                buf.push(ch);
                advance(&mut i, &mut col, src, ch.len_utf8());
            }

            St::Str {
                closer,
                escape,
                doc,
                ..
            } => {
                if *doc {
                    in_comment_line = true;
                }
                if Some(ch) == *escape {
                    // Consume the escape and whatever it escapes, so `\"` cannot
                    // close the string.
                    let n = ch.len_utf8();
                    let next = src.get(i + n..).and_then(|s| s.chars().next());
                    if *doc {
                        buf.push(ch);
                        if let Some(c) = next {
                            buf.push(c);
                        }
                    }
                    advance(&mut i, &mut col, src, n + next.map_or(0, |c| c.len_utf8()));
                    continue;
                }
                if src[i..].starts_with(closer.as_str()) {
                    let len = closer.len();
                    let was_doc = *doc;
                    st = St::Code;
                    if was_doc {
                        docstring_ctx = false;
                    }
                    advance(&mut i, &mut col, src, len);
                    continue;
                }
                if *doc {
                    buf.push(ch);
                } else {
                    cur.code_text.push(ch);
                }
                advance(&mut i, &mut col, src, ch.len_utf8());
            }

            St::Regex => {
                if ch == '\\' {
                    let n = ch.len_utf8();
                    let extra = src
                        .get(i + n..)
                        .and_then(|s| s.chars().next())
                        .map_or(0, |c| c.len_utf8());
                    advance(&mut i, &mut col, src, n + extra);
                    continue;
                }
                if ch == '/' {
                    st = St::Code;
                    prev_significant = Some('/');
                }
                cur.code_text.push(ch);
                advance(&mut i, &mut col, src, ch.len_utf8());
            }
        }
    }

    if !src.is_empty() && !src.ends_with('\n') {
        finish_line!();
    }

    lines
}

fn advance(i: &mut usize, col: &mut u32, src: &str, bytes: usize) {
    let end = (*i + bytes).min(src.len());
    // Column is in characters, so multibyte source does not skew reported columns.
    *col += src[*i..end].chars().count() as u32;
    *i = end;
}

fn char_at(src: &str, i: usize) -> char {
    src[i..].chars().next().unwrap_or('\0')
}

fn begin_comment(
    cur: &mut LineInfo,
    pending_kind: &mut CommentKind,
    pending_col: &mut u32,
    kind: CommentKind,
    col: u32,
) {
    if cur.comment.is_none() {
        cur.code_before_comment = cur.has_code;
        *pending_col = col;
    }
    *pending_kind = kind;
}

/// Length of the longest doc marker matching here, so `///` is stripped whole
/// rather than leaving a stray `/` in the text.
fn doc_marker_len(spec: &LanguageSpec, rest: &str) -> Option<usize> {
    spec.doc_markers
        .iter()
        .find(|m| rest.starts_with(m.as_str()))
        .map(|m| m.len())
}

fn push_piece(cur: &mut LineInfo, raw: &str, kind: CommentKind, column: u32) {
    let text = strip_content(raw, kind);
    match &mut cur.comment {
        Some(existing) => {
            if !text.is_empty() {
                if !existing.text.is_empty() {
                    existing.text.push(' ');
                }
                existing.text.push_str(&text);
            }
        }
        None => cur.comment = Some(CommentPiece { text, kind, column }),
    }
}

fn strip_content(raw: &str, kind: CommentKind) -> String {
    let t = raw.trim();
    if matches!(kind, CommentKind::Block | CommentKind::Doc) {
        // Continuation lines of a block comment are conventionally ` * text`.
        if let Some(rest) = t.strip_prefix('*') {
            if !rest.starts_with('/') {
                return rest.trim().to_string();
            }
        }
    }
    t.to_string()
}

fn update_docstring_ctx(line: &LineInfo, ctx: &mut bool, spec: &LanguageSpec) {
    if spec.docstrings != DocstringStyle::Python {
        return;
    }
    // Blank and comment-only lines leave the context alone, so a comment between
    // `def` and its docstring does not disqualify the docstring.
    if !line.has_code {
        return;
    }
    let t = line.code_text.trim();
    *ctx = t.ends_with(':')
        && (t.starts_with("def ") || t.starts_with("class ") || t.starts_with("async def "));
}

/// A `/` starts a regex only where a value may begin. After an identifier or a
/// closing bracket it is division.
fn regex_can_start_here(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => matches!(
            c,
            '(' | ','
                | '='
                | ':'
                | '['
                | '!'
                | '&'
                | '|'
                | '?'
                | '{'
                | '}'
                | ';'
                | '+'
                | '-'
                | '*'
                | '%'
                | '<'
                | '>'
                | '~'
                | '^'
                | '\n'
        ),
    }
}

fn group(lines: &[LineInfo], opts: &ScanOptions) -> Vec<CommentBlock> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        let line = &lines[i];
        let Some(piece) = &line.comment else {
            i += 1;
            continue;
        };

        // A comment trailing code on the same line stands alone.
        if !line.is_block_joinable() {
            let following = count_following_code(lines, i, true);
            out.push(CommentBlock {
                start_line: i as u32 + 1,
                end_line: i as u32 + 1,
                text: vec![piece.text.clone()],
                kind: piece.kind,
                following_code_lines: following.len() as u32,
                following_code: following,
                column: piece.column,
            });
            i += 1;
            continue;
        }

        let doc = line.is_doc();
        let mut text = Vec::new();
        let mut end;
        let mut j = i;

        loop {
            text.push(lines[j].comment.as_ref().unwrap().text.clone());
            end = j;
            j += 1;

            let mut k = j;
            if opts.merge_across_blank_lines {
                while k < lines.len() && lines[k].is_blank() {
                    k += 1;
                }
            }
            // Doc comments and plain comments have different budgets, so a change
            // of kind ends the block.
            if k < lines.len() && lines[k].is_block_joinable() && lines[k].is_doc() == doc {
                j = k;
            } else {
                break;
            }
        }

        let following = count_following_code(lines, end, false);
        out.push(CommentBlock {
            start_line: i as u32 + 1,
            end_line: end as u32 + 1,
            text,
            kind: lines[i].comment.as_ref().unwrap().kind,
            following_code_lines: following.len() as u32,
            following_code: following,
            column: piece.column,
        });
        i = j;
    }

    out
}

fn count_following_code(lines: &[LineInfo], end: usize, code_on_end_line: bool) -> Vec<String> {
    let mut k = if code_on_end_line { end } else { end + 1 };
    if !code_on_end_line {
        // One blank line between a comment and the code it describes is ordinary
        // formatting, not a sign the comment stands alone.
        if lines.get(k).is_some_and(|l| l.is_blank()) {
            k += 1;
        }
    }
    let mut out = Vec::new();
    while let Some(l) = lines.get(k) {
        if !l.has_code || (l.comment.is_some() && !code_on_end_line) {
            break;
        }
        out.push(l.code_text.trim().to_string());
        k += 1;
    }
    out
}
