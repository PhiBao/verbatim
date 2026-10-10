//! Canonical text form used by the verifier.
//!
//! The verifier's central check is *quote containment*: does the quote an agent
//! claims it took from a source actually appear in that source? A raw substring
//! test fails on smart quotes, non-breaking spaces, soft hyphenation, HTML
//! entities and Unicode composition, so we compare a **canonical** form instead.
//!
//! The canonical form also carries a per-character map back to the original
//! bytes, so a match is reported as real byte offsets into the text a human is
//! reading. Offsets must be honest: a receipt that highlights the wrong passage
//! is worse than no receipt.

use unicode_normalization::UnicodeNormalization;

/// A canonical rendering of a source, with a map back to the original bytes.
#[derive(Debug, Clone)]
pub struct Canonical {
    /// The original text, unmodified.
    pub raw: String,
    /// The canonical text (see [`canonical`]).
    pub canon: String,
    /// Byte offset in `raw` where canon char `i` begins.
    starts: Vec<usize>,
    /// Byte offset in `raw` just past canon char `i`.
    ends: Vec<usize>,
}

impl Canonical {
    /// Finds a canonical `needle` inside this canonical text and returns the
    /// span **in the original bytes** of `raw`.
    pub fn find(&self, needle: &str) -> Option<Span> {
        let needle_canon = canonical(needle);
        let needle_str = needle_canon.canon.as_str();
        if needle_str.is_empty() {
            return None;
        }
        let byte_start = self.canon.find(needle_str)?;
        let char_start = self.canon[..byte_start].chars().count();
        let char_end = char_start + needle_str.chars().count();

        let raw_start = *self.starts.get(char_start)?;
        let raw_end = self
            .ends
            .get(char_end - 1)
            .copied()
            .unwrap_or_else(|| self.raw.len());
        Some(Span {
            start: raw_start,
            end: raw_end,
        })
    }

    /// True if the canonical `needle` occurs anywhere in this text.
    pub fn contains(&self, needle: &str) -> bool {
        self.find(needle).is_some()
    }
}

/// A half-open byte range `[start, end)` into an original string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Removes renderer-inserted hyphenation so that a word broken across a line
/// still matches the word as the agent wrote it.
fn dehyphenate(input: &str) -> String {
    // Soft hyphen: invisible, always removable.
    let no_soft: String = input.chars().filter(|c| *c != '\u{00AD}').collect();
    // A hard hyphen immediately before a line break is line-break hyphenation:
    // drop the hyphen and the break, so "inter-\noperable" reads as one word.
    let chars: Vec<char> = no_soft.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '-' {
            let mut j = i + 1;
            while j < chars.len() && (chars[j] == '\n' || chars[j] == '\r') {
                j += 1;
            }
            if j > i + 1 {
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Combining diacritical marks, stripped so accented and unaccented text match.
fn is_combining_mark(c: char) -> bool {
    ('\u{0300}'..='\u{036F}').contains(&c)
}

/// Fold a single character toward its comparison form.
///
/// Returns `None` when the character should become a separator, which is how
/// punctuation is neutralised: "req/min" and "req min" compare equal.
fn fold_char(c: char) -> Option<char> {
    let mapped = match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' | '\u{02BC}' | '\u{0060}' | '\u{00B4}' => {
            '\''
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' | '\u{00AB}' | '\u{00BB}' | '\u{2039}'
        | '\u{203A}' => '"',
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}'
        | '\u{FE58}' | '\u{FE63}' | '\u{FF0D}' => '-',
        '\u{2026}' => '.',
        '\u{00A0}' | '\u{2007}' | '\u{202F}' | '\u{2009}' | '\u{200A}' | '\u{2002}' | '\u{2003}'
        | '\u{3000}' => ' ',
        _ => c,
    };

    // Hyphen is a separator, not a kept character: sources write
    // "requests-per-minute", "requests per minute" and "req/min" for the same
    // thing, and all three must compare equal.
    let keep = mapped.is_alphanumeric();
    if keep {
        Some(mapped)
    } else {
        None
    }
}

/// Build the canonical form of `raw` plus the index map back to `raw`.
pub fn canonical(raw: &str) -> Canonical {
    let decoded = decode_entities(raw);
    let dehyph = dehyphenate(&decoded);

    let mut canon = String::with_capacity(dehyph.len());
    let mut starts: Vec<usize> = Vec::with_capacity(dehyph.len());
    let mut ends: Vec<usize> = Vec::with_capacity(dehyph.len());
    let mut pending_space = false;

    for (byte_idx, ch) in dehyph.char_indices() {
        // NFD then drop combining marks, so "Café" and "Cafe" compare equal.
        for nc in ch.to_lowercase().nfc().nfd() {
            if is_combining_mark(nc) {
                continue;
            }
            match fold_char(nc) {
                Some(c) => {
                    let char_len = c.len_utf8();
                    if pending_space && !canon.is_empty() {
                        canon.push(' ');
                        starts.push(byte_idx);
                        ends.push(byte_idx + char_len);
                    }
                    pending_space = false;
                    canon.push(c);
                    starts.push(byte_idx);
                    ends.push(byte_idx + char_len);
                }
                None => {
                    if !canon.is_empty() {
                        pending_space = true;
                    }
                }
            }
        }
    }

    Canonical {
        raw: raw.to_string(),
        canon,
        starts,
        ends,
    }
}

/// Decodes the HTML entities that matter for text comparison.
pub fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        rest = &rest[idx..];
        let end = rest[1..].find(';').map(|i| i + 1);
        let entity = match end {
            Some(e) if e <= 12 => &rest[..=e],
            _ => {
                out.push('&');
                rest = &rest[1..];
                continue;
            }
        };
        let replacement = match &entity[1..entity.len() - 1] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            "hellip" => Some('\u{2026}'),
            "mdash" => Some('\u{2014}'),
            "ndash" => Some('\u{2013}'),
            "lsquo" => Some('\u{2018}'),
            "rsquo" => Some('\u{2019}'),
            "ldquo" => Some('\u{201C}'),
            "rdquo" => Some('\u{201D}'),
            "middot" => Some('\u{00B7}'),
            "deg" => Some('\u{00B0}'),
            "copy" => Some('\u{00A9}'),
            "reg" => Some('\u{00AE}'),
            "trade" => Some('\u{2122}'),
            "times" => Some('\u{00D7}'),
            other => {
                if let Some(num) = other.strip_prefix("#x").or_else(|| other.strip_prefix("#X")) {
                    u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
                } else if let Some(num) = other.strip_prefix('#') {
                    num.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    None
                }
            }
        };
        match replacement {
            Some(c) => {
                out.push(c);
                rest = &rest[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_quotes_and_nbsp_compare_equal() {
        let source = "The limit is 100\u{00A0}requests\u{2013}per\u{2013}minute, period.";
        let c = canonical(source);
        assert!(c.contains("The limit is 100 requests-per-minute"), "{:?}", c.canon);
        assert!(c.contains("the limit is 100 requests per minute"));
    }

    #[test]
    fn punctuation_is_a_separator() {
        let c = canonical("60 req/min, no exceptions.");
        assert!(c.contains("60 req min"));
        assert!(c.contains("60 req/min"));
    }

    #[test]
    fn html_entities_are_decoded() {
        let c = canonical("cats &amp; dogs &mdash; together");
        assert!(c.contains("cats dogs together"), "{:?}", c.canon);
    }

    #[test]
    fn offsets_map_back_to_the_exact_words() {
        let raw = "PREFIX noise. The exact answer is 42, and more noise follows.";
        let c = canonical(raw);
        let span = c.find("the exact answer is 42").expect("must match");
        assert_eq!(&raw[span.start..span.end], "The exact answer is 42");
    }

    #[test]
    fn altered_quote_does_not_match() {
        let c = canonical("The limit is 100 requests per minute.");
        assert!(!c.contains("The limit is 500 requests per minute."));
        assert!(!c.contains("The limit is 100 requests per hour."));
    }

    #[test]
    fn soft_hyphenation_matches() {
        let c = canonical("inter\u{00AD}operable systems work well");
        assert!(c.contains("interoperable systems"), "{:?}", c.canon);
    }

    #[test]
    fn line_break_hyphenation_matches() {
        let c = canonical("inter-\noperable systems work well");
        assert!(c.contains("interoperable systems"), "{:?}", c.canon);
    }

    #[test]
    fn empty_needles_never_match() {
        let c = canonical("something");
        assert!(c.find("").is_none());
    }

    #[test]
    fn unicode_text_is_comparable() {
        let c = canonical("Café reservations cost €5 — cheap.");
        assert!(c.contains("cafe reservations cost 5 cheap"), "{:?}", c.canon);
    }
}
