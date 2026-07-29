//! Detects comments that only restate the code beneath them.
//!
//! `# increment the retry counter` above `retry_counter += 1` carries no
//! information a reader could not get from the line itself. Measured by
//! vocabulary overlap: identifiers are split on case and underscores, so the
//! comment's words and the code's words end up in the same space.

use std::collections::HashSet;

/// Words too common to signal anything. Overlap on `the` means nothing.
const STOPWORDS: &[&str] = &[
    "the", "and", "for", "with", "that", "this", "these", "those", "from", "into", "onto", "are",
    "was", "were", "been", "being", "have", "has", "had", "not", "but", "its", "it's", "you",
    "your", "our", "we", "us", "they", "them", "their", "then", "than", "when", "while", "which",
    "what", "who", "whom", "here", "there", "will", "would", "shall", "should", "can", "could",
    "may", "might", "must", "any", "all", "each", "every", "some", "one", "two", "also", "just",
    "only", "very", "much", "more", "most", "less", "least", "same", "other", "such", "how", "why",
    "does", "did", "done", "let", "via", "per", "out", "off", "over", "under", "about",
];

/// Splits text into lowercase content words: three characters or more, not a
/// stopword.
fn content_words(text: &str) -> Vec<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    text.split(|c: char| !c.is_alphanumeric())
        .flat_map(split_case)
        .map(|w| w.to_lowercase())
        .filter(|w| w.chars().count() >= 3 && !stop.contains(w.as_str()))
        .collect()
}

/// Splits `updateItemCount` into `update`, `Item`, `Count` so a camelCase
/// identifier is comparable with prose. Runs of capitals stay whole, keeping
/// `HTTPServer` as `HTTP` and `Server`.
fn split_case(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = token.chars().collect();

    for (i, &c) in chars.iter().enumerate() {
        let starts_word = i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if starts_word && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A section divider like `── JSON-RPC types ──────`. These name the code below
/// them because that is their whole purpose; they are navigation, not
/// explanation.
fn is_banner(line: &str) -> bool {
    let mut run = 0;
    let mut prev = '\0';
    for c in line.chars() {
        if !c.is_alphanumeric() && !c.is_whitespace() && c == prev {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 1;
            prev = c;
        }
    }
    false
}

/// A comment quoting a literal snippet, URL template or data shape rather than
/// prose about the code: `` `adb push <jar>` ``, `/orgs/{owner}/projects/{n}`,
/// `rows: [host-a, group-a1, item]`. These share vocabulary with the code
/// because they *are* code, so overlap says nothing about them.
fn is_code_sample(line: &str) -> bool {
    let bracketed = |open, close| line.contains(open) && line.contains(close);
    if line.contains('`') || bracketed('<', '>') || bracketed('{', '}') || bracketed('[', ']') {
        return true;
    }
    // Prose is mostly letters and spaces. A high punctuation density means the
    // line is structure, not sentences.
    let total = line.chars().count();
    if total == 0 {
        return false;
    }
    let punct = line
        .chars()
        .filter(|c| {
            !c.is_alphanumeric() && !c.is_whitespace() && *c != '\'' && *c != '.' && *c != ','
        })
        .count();
    punct * 4 > total
}

/// Fraction of the comment's content words that also appear in the code, or
/// `None` when there is not enough prose to judge.
pub fn overlap(comment: &[String], code: &[String], min_words: usize) -> Option<f64> {
    if code.is_empty() {
        return None;
    }
    // Judge only the lines that are actually prose.
    let prose: Vec<&String> = comment
        .iter()
        .filter(|l| !is_banner(l) && !is_code_sample(l))
        .collect();
    if prose.is_empty() {
        return None;
    }
    let joined = prose
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let words = content_words(&joined);
    if words.len() < min_words {
        return None;
    }
    let code_words: HashSet<String> = content_words(&code.join(" ")).into_iter().collect();
    if code_words.is_empty() {
        return None;
    }

    // Counted over distinct words so a repeated term cannot dominate the score.
    let distinct: HashSet<&String> = words.iter().collect();
    let hits = distinct.iter().filter(|w| code_words.contains(**w)).count();
    Some(hits as f64 / distinct.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        vec![s.to_string()]
    }

    #[test]
    fn splits_camel_case() {
        assert_eq!(split_case("updateItemCount"), ["update", "Item", "Count"]);
    }

    #[test]
    fn keeps_acronyms_together() {
        assert_eq!(split_case("HTTPServer"), ["HTTP", "Server"]);
    }

    #[test]
    fn leaves_a_plain_word_alone() {
        assert_eq!(split_case("retry"), ["retry"]);
    }

    #[test]
    fn drops_stopwords_and_short_words() {
        assert_eq!(
            content_words("the a of retry counter"),
            ["retry", "counter"]
        );
    }

    #[test]
    fn total_restatement_scores_one() {
        let o = overlap(&words("set the user name"), &words("user_name = set(x)"), 3);
        assert_eq!(o, Some(1.0));
    }

    #[test]
    fn unrelated_prose_scores_zero() {
        let o = overlap(
            &words("upstream flakes on cold boot"),
            &words("fetch(url, retries=1)"),
            3,
        );
        assert_eq!(o, Some(0.0));
    }

    #[test]
    fn code_with_no_content_words_is_not_judged() {
        // `x = 1` offers no vocabulary to compare against, so the rule abstains
        // rather than scoring zero and implying it checked.
        assert_eq!(
            overlap(&words("a comment about things"), &words("x = 1"), 3),
            None
        );
    }

    #[test]
    fn a_short_comment_is_not_judged() {
        assert_eq!(overlap(&words("todo"), &words("x = 1"), 3), None);
    }

    #[test]
    fn no_code_means_no_judgement() {
        assert_eq!(overlap(&words("a comment about things"), &[], 3), None);
    }

    #[test]
    fn banners_are_not_judged() {
        assert!(is_banner("── JSON-RPC types ──────────"));
        assert!(is_banner("--------------------------"));
        assert!(!is_banner("a normal sentence about things"));
    }

    #[test]
    fn code_samples_are_not_judged() {
        assert!(is_code_sample("`adb -s <serial> push <jar>`"));
        assert!(is_code_sample("/orgs/{owner}/projects/{n}"));
        assert!(is_code_sample(
            "rows: [host-a, group-a1, item, item, host-b]"
        ));
        assert!(!is_code_sample("Upstream returns 502 on cold start."));
        assert!(!is_code_sample("Buffered so a slow producer cannot stall."));
    }

    #[test]
    fn a_block_of_only_decoration_is_skipped() {
        let o = overlap(&words("──────────────────"), &words("fn thing() {}"), 3);
        assert_eq!(o, None);
    }

    #[test]
    fn a_repeated_word_cannot_dominate() {
        // `retry` five times against one matching identifier is still one hit.
        let o = overlap(
            &words("retry retry retry retry counter"),
            &words("retry_thing"),
            2,
        );
        assert_eq!(o, Some(0.5));
    }
}
