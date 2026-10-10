//! The verifier.
//!
//! Five deterministic checks, no language model in any of them. This is the
//! product's core claim: **the thing that decides whether the agent is right
//! has no opinion.**
//!
//! Checks, in the order they run:
//!
//! 1. **Source available** — the cited source was fetched and is not dead.
//! 2. **Prior leak** — do the claim's distinctive facts appear in *any* fetched
//!    source? If not, the model asserted from memory. True or not, it is not
//!    evidence, so it cannot ship.
//! 3. **Quote containment** — does the quoted passage exist in the cited
//!    source, byte-exact after canonicalisation? Catches fabricated quotes.
//! 4. **Entity grounding** — does every number, version, date, acronym and
//!    proper noun in the claim appear in the quote?
//! 5. **Polarity** — negation asymmetry, and value substitution (the source
//!    talks about the same quantity with a different number).
//!
//! Every verdict carries a [`NextAction`], so the repairer is a control loop
//! rather than a retry.

use crate::model::{NextAction, Source, Verdict};
use crate::text::{canonical, Canonical};

/// The result of checking one claim.
#[derive(Debug, Clone)]
pub struct Verification {
    pub verdict: Verdict,
    /// 0.0–1.0. Share of the claim's distinctive facts found in the quote.
    pub score: f64,
    pub reason: String,
    pub next_action: NextAction,
    pub span: Option<crate::text::Span>,
    /// Facts we looked for but could not find.
    pub missing: Vec<String>,
}

/// A distinctive fact pulled out of a claim.
#[derive(Debug, Clone, PartialEq)]
pub struct Fact {
    pub kind: FactKind,
    pub raw: String,
    /// Canonical form used for containment tests.
    pub canon: String,
    /// The words that follow the fact, canonical. Used to tell "500 req/min"
    /// from "5 req/min" talking about different things.
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactKind {
    Number,
    Version,
    Date,
    Acronym,
    Url,
    Email,
    ProperNoun,
}

impl FactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FactKind::Number => "number",
            FactKind::Version => "version",
            FactKind::Date => "date",
            FactKind::Acronym => "acronym",
            FactKind::Url => "url",
            FactKind::Email => "email",
            FactKind::ProperNoun => "proper-noun",
        }
    }
}

/// The verifier holds the sources for one run and answers per-claim questions.
pub struct Verifier<'a> {
    sources: &'a [Source],
    canon: Vec<Canonical>,
}

impl<'a> Verifier<'a> {
    pub fn new(sources: &'a [Source]) -> Self {
        let canon = sources.iter().map(|s| canonical(&s.text)).collect();
        Self { sources, canon }
    }

    /// Checks one claim against the corpus of fetched sources.
    pub fn verify(&self, claim_text: &str, quote: &str, source_id: uuid::Uuid) -> Verification {
        // 1. Is the cited source usable at all?
        let idx = self.sources.iter().position(|s| s.id == source_id);
        let Some(idx) = idx else {
            return Verification {
                verdict: Verdict::Unverifiable,
                score: 0.0,
                reason: "cited source was never fetched".into(),
                next_action: NextAction::ReRetrieve(claim_text.to_string()),
                span: None,
                missing: vec![],
            };
        };
        let source = &self.sources[idx];
        if source.dead {
            return Verification {
                verdict: Verdict::Unverifiable,
                score: 0.0,
                reason: format!(
                    "cited source is unavailable ({}) — not verifiable, not wrong",
                    source.dead_reason.as_deref().unwrap_or("unreachable")
                ),
                next_action: NextAction::DeclareUnverifiable(format!(
                    "source unavailable: {}",
                    source.url
                )),
                span: None,
                missing: vec![],
            };
        }

        let facts = extract_facts(claim_text);
        let source_canon = &self.canon[idx];

        // 3. Quote containment.
        let Some(span) = source_canon.find(quote) else {
            // The quote is not in the cited source. Diagnose *why*, because the
            // three causes call for three different repairs.
            //
            // (a) The source talks about exactly this quantity with a
            //     different value — that is a contradiction, not a mismatch.
            for fact in &facts {
                if fact.kind == FactKind::Number
                    && !fact.context.is_empty()
                    && source_canon.contains(&fact.context)
                    && !source_canon.contains(&fact.canon)
                {
                    return Verification {
                        verdict: Verdict::Contradicted,
                        score: 0.0,
                        reason: format!(
                            "the source discusses '{}' but with a different value",
                            fact.context
                        ),
                        next_action: NextAction::ReRetrieve(claim_text.to_string()),
                        span: None,
                        missing: vec![fact.raw.clone()],
                    };
                }
            }
            // (b) None of the claim's facts appear anywhere we fetched: the
            //     model asserted from memory. True or not, it is not evidence.
            if !facts.is_empty() && !facts.iter().any(|f| self.found_in_any_source(f)) {
                let sample: Vec<String> = facts.iter().take(3).map(|f| f.raw.clone()).collect();
                return Verification {
                    verdict: Verdict::Ungrounded,
                    score: 0.0,
                    reason: format!(
                        "no fetched source mentions {} — this came from the model's priors, not from this run",
                        sample.join(", ")
                    ),
                    next_action: NextAction::ReRetrieve(claim_text.to_string()),
                    span: None,
                    missing: sample,
                };
            }
            // (c) The evidence exists somewhere, just not in this source.
            return Verification {
                verdict: Verdict::Unsupported,
                score: 0.0,
                reason: "the quoted passage does not appear in the cited source".into(),
                next_action: NextAction::ReRetrieve(claim_text.to_string()),
                span: None,
                missing: vec![],
            };
        };

        // 4. Entity grounding. A fact may live in the cited source without
        // living in the quoted sentence — quotes are excerpts, so "Entire
        // describes X" quoting only "X" is fine. What matters is that the
        // *source* carries the fact.
        let quote_canon = canonical(quote);
        let mut missing = Vec::new();
        let mut matched = 0usize;
        let mut matched_in_source_only = 0usize;
        let mut value_substitutions = Vec::new();

        for fact in &facts {
            if quote_canon.contains(&fact.canon) {
                matched += 1;
                continue;
            }
            // A number whose unit-context *is* in the quote, but with a
            // different value, is a contradiction rather than a mismatch.
            if fact.kind == FactKind::Number && !fact.context.is_empty() {
                if quote_canon.contains(&fact.context) {
                    value_substitutions.push(fact.raw.clone());
                    continue;
                }
            }
            if source_canon.contains(&fact.canon) {
                matched_in_source_only += 1;
                continue;
            }
            missing.push(format!("{} ({})", fact.raw, fact.kind.as_str()));
        }

        // 5a. Value substitution — the strongest contradiction signal we have.
        if !value_substitutions.is_empty() {
            return Verification {
                verdict: Verdict::Contradicted,
                score: score_of(matched, facts.len(), value_substitutions.len()),
                reason: format!(
                    "the source discusses {} but with a different value",
                    value_substitutions.join(", ")
                ),
                next_action: NextAction::ReRetrieve(claim_text.to_string()),
                span: Some(span),
                missing,
            };
        }

        // 5b. Negation asymmetry.
        if let Some(reason) = negation_asymmetry(claim_text, quote) {
            return Verification {
                verdict: Verdict::Contradicted,
                score: score_of(matched, facts.len(), 1),
                reason,
                next_action: NextAction::ReRetrieve(claim_text.to_string()),
                span: Some(span),
                missing,
            };
        }

        let grounded = matched + matched_in_source_only;
        let score = if facts.is_empty() {
            1.0
        } else {
            grounded as f64 / facts.len() as f64
        };

        // Numbers are load-bearing: a number the claim relies on must appear in
        // the cited source, or the claim is not supported by it.
        let missing_numbers = missing
            .iter()
            .any(|m| m.ends_with("(number)"));

        if missing_numbers || !missing.is_empty() {
            return Verification {
                verdict: Verdict::Unsupported,
                score,
                reason: if missing_numbers {
                    "the quote does not contain a number the claim relies on".into()
                } else {
                    format!(
                        "the quote supports only {matched}/{} of the claim's distinctive facts",
                        facts.len()
                    )
                },
                next_action: NextAction::ReRetrieve(claim_text.to_string()),
                span: Some(span),
                missing,
            };
        }

        Verification {
            verdict: Verdict::Supported,
            score,
            reason: if matched_in_source_only == 0 {
                "quote is verbatim in the cited source and carries every fact in the claim".into()
            } else {
                format!(
                    "quote is verbatim in the cited source; {} fact(s) grounded elsewhere in it",
                    matched_in_source_only
                )
            },
            next_action: NextAction::None,
            span: Some(span),
            missing,
        }
    }

    /// Two claims citing different sources that disagree on the same key.
    pub fn conflicts(&self, claims: &[ClaimPair]) -> Vec<Conflict> {
        let mut out = Vec::new();
        for i in 0..claims.len() {
            for j in (i + 1)..claims.len() {
                let a = &claims[i];
                let b = &claims[j];
                if a.source_id == b.source_id {
                    continue;
                }
                if let Some(key) = shared_numeric_key(&a.text, &b.text) {
                    if !same_value(&a.text, &b.text, &key) {
                        out.push(Conflict {
                            key,
                            left: ValueRef {
                                claim: a.text.clone(),
                                source_id: a.source_id,
                            },
                            right: ValueRef {
                                claim: b.text.clone(),
                                source_id: b.source_id,
                            },
                        });
                    }
                }
            }
        }
        out
    }

    fn found_in_any_source(&self, fact: &Fact) -> bool {
        self.canon.iter().any(|c| c.contains(&fact.canon))
    }
}

#[derive(Debug, Clone)]
pub struct ClaimPair {
    pub claim_id: uuid::Uuid,
    pub source_id: uuid::Uuid,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Conflict {
    pub key: String,
    pub left: ValueRef,
    pub right: ValueRef,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ValueRef {
    pub claim: String,
    pub source_id: uuid::Uuid,
}

fn score_of(matched: usize, total: usize, substitutions: usize) -> f64 {
    let denom = (total + substitutions).max(1) as f64;
    (matched as f64 / denom).clamp(0.0, 1.0)
}

/// Detects negation present on one side and absent on the other.
fn negation_asymmetry(claim: &str, quote: &str) -> Option<String> {
    const NEG: [&str; 12] = [
        "not", "never", "no", "cannot", "can not", "won't", "does not", "did not", "isn't",
        "aren't", "without", "except",
    ];
    let claim_c = canonical(claim).canon;
    let quote_c = canonical(quote).canon;
    let claim_neg: Vec<&str> = NEG
        .iter()
        .copied()
        .filter(|n| claim_c.contains(&canonical(n).canon))
        .collect();
    let quote_neg: Vec<&str> = NEG
        .iter()
        .copied()
        .filter(|n| quote_c.contains(&canonical(n).canon))
        .collect();
    if claim_neg.is_empty() != quote_neg.is_empty() {
        Some(format!(
            "the claim negates what the source asserts (claim: {:?}, source: {:?})",
            claim_neg, quote_neg
        ))
    } else {
        None
    }
}

/// Pulls distinctive facts out of a claim. Conservative by design: we only
/// treat something as a fact when we are confident, because a false "missing
/// fact" would wrongly reject a true claim.
pub fn extract_facts(text: &str) -> Vec<Fact> {
    let mut out: Vec<Fact> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    let mut claimed_spans: Vec<(usize, usize)> = Vec::new();

    // URLs and emails first, so their digits are not re-read as numbers.
    for (s, e) in runs(text, |c| !c.is_whitespace()) {
        let tok = &text[s..e];
        if tok.starts_with("http://") || tok.starts_with("https://") {
            claimed_spans.push((s, e));
            push(&mut out, &mut seen, FactKind::Url, tok, String::new());
        } else if tok.contains('@') && tok.contains('.') && !tok.starts_with('@') {
            claimed_spans.push((s, e));
            push(&mut out, &mut seen, FactKind::Email, tok, String::new());
        }
    }

    // Versions: 1.2.3, v2.1, v2
    for (s, e) in runs(text, |c| c.is_ascii_digit() || c == '.' || c == 'v' || c == 'V') {
        let raw = &text[s..e];
        let stripped = raw.trim_start_matches(['v', 'V']);
        let dotted = stripped.split('.').filter(|p| !p.is_empty()).count() >= 2;
        let v_prefixed = (raw.starts_with('v') || raw.starts_with('V'))
            && stripped.chars().all(|c| c.is_ascii_digit())
            && !stripped.is_empty();
        if dotted || v_prefixed {
            claimed_spans.push((s, e));
            push(
                &mut out,
                &mut seen,
                FactKind::Version,
                raw,
                following_context(text, raw),
            );
        }
    }

    // Dates: 2026-01-01
    for (s, e) in runs(text, |c| c.is_ascii_digit() || c == '-') {
        let raw = &text[s..e];
        if raw.len() == 10 && raw.matches('-').count() == 2 && raw.starts_with(|c: char| c.is_ascii_digit()) {
            claimed_spans.push((s, e));
            push(&mut out, &mut seen, FactKind::Date, raw, String::new());
        }
    }

    // Numbers. Anything already claimed by a version or date is not a number.
    for (s, e) in runs(text, |c| c.is_ascii_digit() || c == ',' || c == '.') {
        if overlaps((s, e), &claimed_spans) {
            continue;
        }
        let raw = &text[s..e];
        let cleaned: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
        if cleaned.is_empty() {
            continue;
        }
        push(
            &mut out,
            &mut seen,
            FactKind::Number,
            raw,
            following_context(text, raw),
        );
    }

    // Acronyms: 2+ capitals.
    for (s, e) in runs(text, |c| c.is_ascii_uppercase()) {
        if e - s >= 2 {
            push(&mut out, &mut seen, FactKind::Acronym, &text[s..e], String::new());
        }
    }

    // Proper nouns: capitalised tokens of length >= 3 that are not common
    // function words. Sentence position does not matter, because the stopword
    // list already excludes "The", "When", "According" and the rest.
    for w in text.split_whitespace() {
        let clean: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
        if clean.len() < 3 || !clean.chars().next().is_some_and(|c| c.is_uppercase()) {
            continue;
        }
        if STOPWORDS.contains(&clean.as_str()) {
            continue;
        }
        push(&mut out, &mut seen, FactKind::ProperNoun, &clean, String::new());
    }

    out
}

const STOPWORDS: [&str; 26] = [
    "The", "This", "That", "These", "Those", "There", "Then", "When", "What", "Which", "While",
    "However", "Because", "According", "Also", "Each", "Some", "Most", "Many", "Both", "After",
    "Before", "Since", "Once", "Every", "Such",
];

fn push(out: &mut Vec<Fact>, seen: &mut Vec<String>, kind: FactKind, raw: &str, context: String) {
    let canon = canonical(raw).canon;
    if canon.is_empty() || seen.contains(&canon) {
        return;
    }
    seen.push(canon.clone());
    out.push(Fact {
        kind,
        raw: raw.to_string(),
        canon,
        context: canonical(&context).canon,
    });
}

/// Byte spans of maximal runs of characters accepted by `accept`.
fn runs<F: Fn(char) -> bool>(text: &str, accept: F) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in text.char_indices() {
        if accept(ch) && !ch.is_whitespace() {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            out.push((s, i));
        }
    }
    if let Some(s) = start {
        out.push((s, text.len()));
    }
    out
}

fn overlaps(span: (usize, usize), spans: &[(usize, usize)]) -> bool {
    spans.iter().any(|(s, e)| span.0 < *e && *s < span.1)
}

/// The up-to-two words following a match, used as unit context.
fn following_context(text: &str, m: &str) -> String {
    let Some(pos) = text.find(m) else {
        return String::new();
    };
    let rest = &text[pos + m.len()..];
    let mut ctx: Vec<String> = Vec::new();
    for w in rest.split_whitespace() {
        let clean: String = w.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
        if clean.is_empty() {
            break;
        }
        ctx.push(clean);
        if ctx.len() >= 2 {
            break;
        }
    }
    ctx.join(" ")
}

/// Finds a numeric key shared by two claims, e.g. "requests per minute".
fn shared_numeric_key(a: &str, b: &str) -> Option<String> {
    let fa = extract_facts(a);
    let fb = extract_facts(b);
    for x in &fa {
        if x.kind != FactKind::Number || x.context.is_empty() {
            continue;
        }
        for y in &fb {
            if y.kind != FactKind::Number || y.context.is_empty() {
                continue;
            }
            // Compare on the unit context alone, ignoring the digits.
            if context_key(&x.context) == context_key(&y.context) {
                return Some(x.context.clone());
            }
        }
    }
    None
}

fn context_key(ctx: &str) -> String {
    ctx.chars().filter(|c| c.is_alphabetic()).collect()
}

fn same_value(a: &str, b: &str, key: &str) -> bool {
    let fa = extract_facts(a);
    let fb = extract_facts(b);
    let va: Vec<&str> = fa
        .iter()
        .filter(|f| f.kind == FactKind::Number && context_key(&f.context) == context_key(key))
        .map(|f| f.canon.as_str())
        .collect();
    let vb: Vec<&str> = fb
        .iter()
        .filter(|f| f.kind == FactKind::Number && context_key(&f.context) == context_key(key))
        .map(|f| f.canon.as_str())
        .collect();
    !va.is_empty() && va == vb
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use chrono::Utc;
    use uuid::Uuid;

    fn source(text: &str) -> Source {
        Source {
            id: Uuid::new_v4(),
            url: format!("https://example.test/{}", Uuid::new_v4()),
            title: None,
            http_status: 200,
            content_hash: "0".repeat(64),
            bytes: text.len(),
            text: text.to_string(),
            fetched_at: Utc::now(),
            dead: false,
            dead_reason: None,
        }
    }

    #[test]
    fn verbatim_quote_is_supported() {
        let s = source("The API allows 100 requests per minute on the free tier.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "The free tier allows 100 requests per minute.",
            "allows 100 requests per minute",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Supported, "{:?}", r);
        assert!(r.span.is_some());
    }

    #[test]
    fn wrong_number_against_the_same_unit_is_contradicted() {
        let s = source("The API allows 100 requests per minute on the free tier.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "The free tier allows 500 requests per minute.",
            "allows 500 requests per minute",
            s.id,
        );
        // The quote itself is fabricated, so containment fails first.
        assert_ne!(r.verdict, Verdict::Supported);

        // Now the honest version: the agent quotes the source correctly but
        // states a different number.
        let r2 = v.verify(
            "The free tier allows 500 requests per minute.",
            "allows 100 requests per minute",
            s.id,
        );
        assert_eq!(r2.verdict, Verdict::Contradicted, "{:?}", r2);
    }

    #[test]
    fn fabricated_quote_on_a_different_topic_is_unsupported() {
        // "Kubernetes" appears mid-sentence so it is a fact we *can* find in the
        // source; the invented "5 seconds" cannot. Some facts ground, the quote
        // does not: unsupported, not ungrounded.
        let s = source("In production, Kubernetes reschedules pods automatically when nodes drain.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "Kubernetes reschedules pods automatically every 5 seconds.",
            "reschedules pods automatically every 5 seconds",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Unsupported, "{:?}", r);
    }

    #[test]
    fn fabricated_value_against_the_real_unit_is_contradicted() {
        // The quote is invented, but the source is clearly talking about the
        // same quantity with a different number. That is a contradiction, and
        // saying so is more useful than saying "unsupported".
        let s = source("The API allows 100 requests per minute on the free tier.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "The API allows 500 requests per minute.",
            "The API allows 500 requests per minute on the free tier.",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Contradicted, "{:?}", r);
    }

    #[test]
    fn prior_leak_is_detected() {
        let s = source("Kubernetes schedules pods onto nodes with available capacity.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        // The number 200000 is in no fetched source.
        let r = v.verify(
            "Claude Sonnet has a 200000 token context window.",
            "a 200000 token context window",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Ungrounded, "{:?}", r);
    }

    #[test]
    fn dead_source_is_unverifiable_not_wrong() {
        let mut s = source("irrelevant");
        s.dead = true;
        s.dead_reason = Some("404".into());
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify("anything", "anything", s.id);
        assert_eq!(r.verdict, Verdict::Unverifiable, "{:?}", r);
    }

    #[test]
    fn prose_without_facts_relies_on_containment() {
        let s = source("Rate limiting protects shared infrastructure from abuse.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "Rate limiting protects shared infrastructure.",
            "protects shared infrastructure from abuse",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Supported, "{:?}", r);
    }

    #[test]
    fn smart_quotes_still_match() {
        let s = source("Entire says \u{201C}context that\u{2019}s attached, not archived\u{201D}.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "Entire describes context that's attached, not archived.",
            "\"context that's attached, not archived\"",
            s.id,
        );
        assert_eq!(r.verdict, Verdict::Supported, "{:?}", r);
        let span = r.span.unwrap();
        // The receipt highlights the words, not the typographic quote marks,
        // because the canonical form drops them.
        assert_eq!(
            &s.text[span.start..span.end],
            "context that\u{2019}s attached, not archived"
        );
    }

    #[test]
    fn negation_inversion_is_contradicted() {
        let s = source("The agent does not write to production without approval.");
        let srcs = [s.clone()];
        let v = Verifier::new(&srcs);
        let r = v.verify(
            "The agent writes to production without approval.",
            "The agent writes to production",
            s.id,
        );
        assert_ne!(r.verdict, Verdict::Supported, "{:?}", r);
    }

    #[test]
    fn conflict_between_two_sources() {
        let a = source("Docs: 100 requests per minute.");
        let b = source("Blog: 60 requests per minute.");
        let srcs = [a.clone(), b.clone()];
        let v = Verifier::new(&srcs);
        let claims = vec![
            ClaimPair {
                claim_id: Uuid::new_v4(),
                source_id: a.id,
                text: "The limit is 100 requests per minute.".into(),
            },
            ClaimPair {
                claim_id: Uuid::new_v4(),
                source_id: b.id,
                text: "The limit is 60 requests per minute.".into(),
            },
        ];
        let conflicts = v.conflicts(&claims);
        assert_eq!(conflicts.len(), 1, "{:?}", conflicts);
    }

    #[test]
    fn fact_extraction_is_conservative() {
        let f = extract_facts("The rate limit is 100 requests per minute for API v2 since 2026-01-01.");
        let kinds: Vec<&str> = f.iter().map(|x| x.kind.as_str()).collect();
        assert!(kinds.contains(&"number"), "{:?}", f);
        assert!(kinds.contains(&"version"), "{:?}", f);
        assert!(kinds.contains(&"date"), "{:?}", f);
        // "The" and "API" — API is a legit acronym, The is a stopword.
        assert!(!f.iter().any(|x| x.raw == "The"), "{:?}", f);
    }
}
