//! Sentence rhythm and punctuation habits.
//!
//! Two tells that survive measurement. Human writing varies its sentence length
//! sharply — a six-word sentence next to a thirty-word one. Generated prose
//! regresses to the mean and keeps almost every sentence the same size. And it
//! reaches for the em dash to bolt a second thought onto a first, far more often
//! than people do.

/// Splits prose into sentences on terminal punctuation.
///
/// Abbreviations (`e.g.`, `i.e.`, `etc.`) and decimals would each split a
/// sentence in two, so a full stop only ends a sentence when whitespace and a
/// capital or digit follow it.
pub fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();

    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        if !matches!(c, '.' | '!' | '?') {
            continue;
        }
        // Consume any run of terminators, then look at what follows.
        let mut j = i + 1;
        while chars
            .get(j)
            .is_some_and(|n| matches!(n, '.' | '!' | '?' | '"' | '\'' | ')'))
        {
            j += 1;
        }
        let ends = match chars.get(j) {
            None => true,
            Some(n) if n.is_whitespace() => chars
                .get(j + 1..)
                .and_then(|rest| rest.iter().find(|r| !r.is_whitespace()))
                .is_none_or(|next| next.is_uppercase() || next.is_ascii_digit()),
            _ => false,
        };
        if ends && !cur.trim().is_empty() {
            out.push(std::mem::take(&mut cur).trim().to_string());
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Coefficient of variation of sentence length: the standard deviation over the
/// mean, so it compares alike across a terse paragraph and a florid one.
///
/// `None` when there are too few sentences to have a rhythm at all.
pub fn variation(text: &str, min_sentences: usize) -> Option<(f64, usize)> {
    let lengths: Vec<f64> = sentences(text)
        .iter()
        .map(|s| s.split_whitespace().count() as f64)
        .filter(|n| *n > 0.0)
        .collect();
    if lengths.len() < min_sentences.max(2) {
        return None;
    }
    let n = lengths.len() as f64;
    let mean = lengths.iter().sum::<f64>() / n;
    if mean == 0.0 {
        return None;
    }
    let variance = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / n;
    Some((variance.sqrt() / mean, lengths.len()))
}

/// Em dashes per hundred words, with the raw count.
///
/// A rate rather than a count: one em dash in a two-line comment is a habit, one
/// in four paragraphs is a sentence that needed it.
pub fn em_dash_rate(text: &str) -> (f64, usize) {
    let count = text.chars().filter(|c| *c == '\u{2014}').count();
    let words = text.split_whitespace().count().max(1);
    (count as f64 * 100.0 / words as f64, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_terminal_punctuation() {
        assert_eq!(
            sentences("One thing. Then another! And a third?"),
            ["One thing.", "Then another!", "And a third?"]
        );
    }

    #[test]
    fn an_abbreviation_does_not_end_a_sentence() {
        // Lowercase after the stop, so the sentence continues.
        assert_eq!(sentences("Use e.g. a cache here.").len(), 1);
        assert_eq!(sentences("The timeout is 1.5 seconds now.").len(), 1);
    }

    #[test]
    fn trailing_text_without_a_stop_is_still_a_sentence() {
        assert_eq!(sentences("no full stop here"), ["no full stop here"]);
    }

    #[test]
    fn uniform_lengths_score_near_zero() {
        let text = "One two three four. Five six seven eight. Nine ten more words. \
                    Twelve thirteen four teen. Sixteen seventeen more here.";
        let (cv, n) = variation(text, 5).unwrap();
        assert_eq!(n, 5);
        assert!(cv < 0.1, "uniform prose should vary little, got {cv}");
    }

    #[test]
    fn varied_lengths_score_high() {
        let text = "No. It failed because the upstream server rejected the second \
                    request after the retry budget ran out and nothing retried it. \
                    Twice. That was the whole bug, and it took a week to find. Fixed.";
        let (cv, _) = variation(text, 5).unwrap();
        assert!(cv > 0.4, "varied prose should score high, got {cv}");
    }

    #[test]
    fn too_few_sentences_have_no_rhythm() {
        assert_eq!(variation("One thing. Then another.", 5), None);
    }

    #[test]
    fn counts_em_dashes_per_hundred_words() {
        let (rate, count) = em_dash_rate("a b c \u{2014} d e f g h i");
        assert_eq!(count, 1);
        assert_eq!(rate, 10.0);
    }

    #[test]
    fn a_hyphen_is_not_an_em_dash() {
        assert_eq!(em_dash_rate("well-known - and fine").1, 0);
    }
}
