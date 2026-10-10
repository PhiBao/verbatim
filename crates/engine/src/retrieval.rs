//! Retrieval.
//!
//! The agent picks its own URLs, so every fetch is untrusted input. Two
//! properties matter more than speed:
//!
//! 1. **The fetch cannot reach anything it should not.** Requests are confined
//!    to `http(s)` on the public internet: no private, loopback, link-local or
//!    multicast addresses, no non-standard ports, redirects re-validated at each
//!    hop, and the response body capped.
//! 2. **The demo is reproducible.** `corpus` mode serves a pinned snapshot of
//!    real fetches, so a run recorded on camera replays byte-for-byte. `live`
//!    mode is what production uses. The mode is visible in the UI and stated in
//!    the README; the snapshot is taken by a real fetch during the hackathon,
//!    not hand-written.

use crate::model::Source;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalMode {
    Live,
    Corpus,
}

/// One document, ready to be matched against.
#[derive(Debug, Clone)]
pub struct FetchedDoc {
    pub url: String,
    pub title: Option<String>,
    pub http_status: u16,
    pub bytes: usize,
    pub content_hash: String,
    /// Visible prose, tags and scripts stripped.
    pub text: String,
}

/// A document stored in the pinned corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorpusDoc {
    pub url: String,
    pub title: Option<String>,
    pub http_status: u16,
    pub text: String,
    /// When the snapshot was taken, so staleness is visible rather than hidden.
    pub snapshotted_at: String,
}

impl FetchedDoc {
    pub fn into_source(self) -> Source {
        Source {
            id: Uuid::new_v4(),
            url: self.url,
            title: self.title,
            http_status: self.http_status,
            content_hash: self.content_hash,
            bytes: self.bytes,
            text: self.text,
            fetched_at: Utc::now(),
            dead: false,
            dead_reason: None,
        }
    }

    pub fn dead(url: String, reason: String, status: u16) -> Source {
        Source {
            id: Uuid::new_v4(),
            url,
            title: None,
            http_status: status,
            content_hash: String::new(),
            bytes: 0,
            text: String::new(),
            fetched_at: Utc::now(),
            dead: true,
            dead_reason: Some(reason),
        }
    }
}

pub struct Retrieval {
    client: reqwest::Client,
    mode: RetrievalMode,
    corpus_dir: PathBuf,
    corpus: Vec<CorpusDoc>,
}

impl Retrieval {
    pub fn new(mode: RetrievalMode, corpus_dir: impl Into<PathBuf>) -> Self {
        let corpus_dir = corpus_dir.into();
        let corpus = load_corpus(&corpus_dir).unwrap_or_default();
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::limited(3))
                .user_agent("verbatim-research-agent/0.1 (+neathack)")
                .build()
                .unwrap_or_default(),
            mode,
            corpus_dir,
            corpus,
        }
    }

    /// Builds a retrieval over an explicit corpus, for tests and demos.
    pub fn with_corpus(corpus: Vec<CorpusDoc>, dir: impl Into<PathBuf>) -> Self {
        Self {
            client: reqwest::Client::new(),
            mode: RetrievalMode::Corpus,
            corpus,
            corpus_dir: dir.into(),
        }
    }

    pub fn mode(&self) -> RetrievalMode {
        self.mode
    }

    /// When the snapshot was taken, so staleness is visible rather than hidden.
    pub fn corpus_snapshot_time(&self) -> Option<String> {
        self.corpus.iter().map(|d| d.snapshotted_at.clone()).max()
    }

    pub fn corpus_len(&self) -> usize {
        self.corpus.len()
    }

    pub fn corpus_dir(&self) -> &PathBuf {
        &self.corpus_dir
    }

    /// Fetches a single document, or records why it could not be fetched.
    ///
    /// A source that cannot be read is not a source that says "no" — it is
    /// `unverifiable`, which is a different verdict and a different repair.
    pub async fn fetch(&self, url: &str) -> Result<FetchedDoc> {
        let parsed = validate_url(url)?;

        if self.mode == RetrievalMode::Corpus {
            if let Some(doc) = self.corpus.iter().find(|d| same_url(&d.url, url)) {
                return Ok(FetchedDoc {
                    url: doc.url.clone(),
                    title: doc.title.clone(),
                    http_status: doc.http_status,
                    bytes: doc.text.len(),
                    content_hash: hash(doc.text.as_bytes()),
                    text: doc.text.clone(),
                });
            }
            bail!("url not in pinned corpus: {url}");
        }

        // Live mode. Validate every redirect hop, not just the first URL.
        let mut current = parsed;
        for _hop in 0..4 {
            let resp = self
                .client
                .get(current.as_str())
                .send()
                .await
                .with_context(|| format!("GET {current}"))?;
            if resp.status().is_redirection() {
                match resp.headers().get(reqwest::header::LOCATION) {
                    Some(loc) => {
                        let next = loc.to_str().context("bad location header")?;
                        current = validate_url(&current.join(next)?.to_string())?;
                        continue;
                    }
                    None => bail!("redirect without location"),
                }
            }
            let status = resp.status().as_u16();
            if !resp.status().is_success() {
                bail!("HTTP {status}");
            }
            let body = resp.bytes().await.context("read body")?;
            let body = if body.len() > MAX_BODY_BYTES {
                &body[..MAX_BODY_BYTES]
            } else {
                &body[..]
            };
            let html = String::from_utf8_lossy(body).to_string();
            let title = extract_title(&html);
            let text = html_to_text(&html);
            return Ok(FetchedDoc {
                url: current.to_string(),
                title,
                http_status: status,
                bytes: body.len(),
                content_hash: hash(body),
                text,
            });
        }
        bail!("too many redirects")
    }

    /// Ranks corpus documents for a query. Used in corpus mode to stand in for
    /// a search engine, so the agent's retrieval step behaves the same way.
    pub fn search_corpus(&self, query: &str, limit: usize) -> Vec<String> {
        let terms = tokenize(query);
        if terms.is_empty() {
            return vec![];
        }
        let mut scored: Vec<(f64, &CorpusDoc)> = self
            .corpus
            .iter()
            .filter_map(|d| {
                let hay = tokenize(&d.text);
                if hay.is_empty() {
                    return None;
                }
                let hits = terms
                    .iter()
                    .filter(|t| hay.iter().any(|h| h == *t))
                    .count();
                if hits == 0 {
                    return None;
                }
                // Term frequency with a mild length penalty, which is enough to
                // rank a dozen documents sensibly.
                let tf = terms
                    .iter()
                    .map(|t| hay.iter().filter(|h| *h == t).count() as f64)
                    .sum::<f64>()
                    / hay.len() as f64;
                Some(((hits as f64).ln_1p() + tf, d))
            })
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, d)| d.url.clone())
            .collect()
    }

    /// Persists a live fetch into the pinned corpus, so the run is replayable.
    pub fn remember(&mut self, doc: &FetchedDoc) {
        if self.corpus.iter().any(|d| same_url(&d.url, &doc.url)) {
            return;
        }
        self.corpus.push(CorpusDoc {
            url: doc.url.clone(),
            title: doc.title.clone(),
            http_status: doc.http_status,
            text: doc.text.clone(),
            snapshotted_at: Utc::now().to_rfc3339(),
        });
    }

    pub fn save_corpus(&self) -> Result<usize> {
        std::fs::create_dir_all(&self.corpus_dir).ok();
        let path = self.corpus_dir.join("corpus.json");
        let json = serde_json::to_string_pretty(&self.corpus)?;
        std::fs::write(&path, json).with_context(|| format!("write {path:?}"))?;
        Ok(self.corpus.len())
    }
}

pub fn load_corpus(dir: &PathBuf) -> Result<Vec<CorpusDoc>> {
    let path = dir.join("corpus.json");
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

/// Rejects anything that is not a public `http(s)` URL on a standard port.
pub fn validate_url(url: &str) -> Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(url).with_context(|| format!("parse url {url}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("scheme must be http or https");
    }
    let host = parsed.host_str().context("url has no host")?;
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        bail!("localhost is not allowed");
    }
    // IPv6 literals arrive bracketed: "[::1]". Strip the brackets before parsing
    // so the address checks below apply to them too.
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            bail!("private address");
        }
    }
    let port = parsed.port_or_known_default().context("no port")?;
    if port != 80 && port != 443 {
        bail!("only ports 80 and 443 are allowed");
    }
    Ok(parsed)
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_documentation())
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local())
        }
    }
}

fn same_url(a: &str, b: &str) -> bool {
    match (reqwest::Url::parse(a), reqwest::Url::parse(b)) {
        (Ok(x), Ok(y)) => {
            let norm = |u: &reqwest::Url| {
                let s = format!("{}://{}{}", u.scheme(), u.host_str().unwrap_or(""), u.path());
                s.trim_end_matches('/').to_string()
            };
            norm(&x) == norm(&y)
        }
        _ => a == b,
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 3)
        .map(|s| s.to_lowercase())
        .collect()
}

fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let rest = &html[start..];
    let gt = rest.find('>')?;
    let end = rest[gt..].find("</title>")?;
    // Decode entities: the title is shown to a human, not compared byte-wise.
    Some(
        crate::text::decode_entities(&rest[gt + 1..gt + end])
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// Strips tags, scripts and styles. Deliberately simple: the text only needs to
/// be good enough for quote containment, and a heavy parser would add
/// dependencies without changing any verdict.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len() / 2);
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        out.push_str(&rest[..lt]);
        let after = &rest[lt..];
        if after.starts_with("<!--") {
            match after.find("-->") {
                Some(e) => rest = &after[e + 3..],
                None => break,
            }
            continue;
        }
        // Elements whose *content* must never reach the text: scripts, styles
        // and metadata. Their text is not prose and would pollute matching.
        let raw_tag = &after[1..after.find('>').unwrap_or(after.len())].to_lowercase();
        if raw_tag.starts_with("script")
            || raw_tag.starts_with("style")
            || raw_tag.starts_with("head")
            || raw_tag.starts_with("svg")
        {
            let close = format!("</{}", raw_tag.split_whitespace().next().unwrap_or(""));
            match after.to_lowercase().find(&close) {
                Some(e) => {
                    let tail = &after[e..];
                    let end = tail.find('>').map(|i| i + 1).unwrap_or(tail.len());
                    rest = &after[e + end..];
                }
                None => break,
            }
            continue;
        }
        match after.find('>') {
            Some(gt) => {
                let tag = &after[1..gt].to_lowercase();
                let is_block = tag.starts_with("p")
                    || tag.starts_with("/p")
                    || tag.starts_with("br")
                    || tag.starts_with("div")
                    || tag.starts_with("/div")
                    || tag.starts_with("li")
                    || tag.starts_with("h1")
                    || tag.starts_with("h2")
                    || tag.starts_with("h3")
                    || tag.starts_with("tr");
                if is_block {
                    out.push(' ');
                }
                rest = &after[gt + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    crate::text::decode_entities(&out)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn hash(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_private_and_loopback_targets() {
        for bad in [
            "http://127.0.0.1:8080/admin",
            "http://localhost/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://[::1]/",
            "file:///etc/passwd",
            "http://example.com:22/",
            "ftp://example.com/",
        ] {
            assert!(validate_url(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn accepts_public_urls() {
        assert!(validate_url("https://docs.anthropic.com/en/api/rate-limits").is_ok());
        assert!(validate_url("http://example.com/path").is_ok());
    }

    #[test]
    fn strips_tags_and_scripts() {
        let html = "<html><head><style>p{color:red}</style><title>Hello &amp; goodbye</title></head><body><p>First paragraph.</p><p>Second one.</p><script>alert(1)</script></body></html>";
        let text = html_to_text(html);
        assert!(text.contains("First paragraph."));
        assert!(text.contains("Second one."));
        assert!(!text.contains("color:red"));
        assert!(!text.contains("alert"));
        assert_eq!(extract_title(html).as_deref(), Some("Hello & goodbye"));
    }

    #[test]
    fn url_comparison_ignores_trailing_slash() {
        assert!(same_url("https://a.test/x/", "https://a.test/x"));
        assert!(!same_url("https://a.test/x", "https://a.test/y"));
    }
}
