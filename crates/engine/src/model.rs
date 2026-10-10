//! Domain types.
//!
//! These are the persisted shapes. The append-only event log is the source of
//! truth; these structs are what the UI renders and what the CLI emits.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How a run produced its answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// The full pipeline: plan, retrieve, claim, verify, repair, compile.
    Verified,
    /// The control: plan, retrieve, write. No verifier. Used for the A/B.
    Naive,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Verified => "verified",
            Mode::Naive => "naive",
        }
    }
}

/// The one thing that decides whether the agent is right has no opinion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The quote is in the cited source and carries the claim's facts.
    Supported,
    /// The source exists and says the opposite, or a different value.
    Contradicted,
    /// The source exists but does not say this.
    Unsupported,
    /// Two sources disagree on the same key.
    Conflict,
    /// The claim's distinctive facts appear in no fetched source: it came from
    /// the model's priors, not from this run.
    Ungrounded,
    /// We could not check it: the cited source is dead, blocked or paywalled.
    /// Not "wrong" — "not checkable here", which is a different thing.
    Unverifiable,
}

impl Verdict {
    pub fn is_good(&self) -> bool {
        matches!(self, Verdict::Supported)
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Supported => "supported",
            Verdict::Contradicted => "contradicted",
            Verdict::Unsupported => "unsupported",
            Verdict::Conflict => "conflict",
            Verdict::Ungrounded => "ungrounded",
            Verdict::Unverifiable => "unverifiable",
        }
    }
}

/// What the repairer should do about a verdict. Machine-actionable by design:
/// a verdict is not an opinion, it is an instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NextAction {
    /// Nothing to do.
    None,
    /// Search again, with this query.
    ReRetrieve(String),
    /// Add the claim to the "we could not verify" section, with this reason.
    DeclareUnverifiable(String),
    /// Pick between two conflicting values using this trust rule.
    ResolveConflict { rule: String },
    /// Drop the claim entirely.
    Drop,
}

/// A source the agent actually fetched.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: Uuid,
    pub url: String,
    pub title: Option<String>,
    pub http_status: u16,
    /// SHA-256 of the bytes we received.
    pub content_hash: String,
    pub bytes: usize,
    /// Normalised canonical text used for matching.
    pub text: String,
    pub fetched_at: DateTime<Utc>,
    /// Dead or blocked: claims citing it are unverifiable, not wrong.
    pub dead: bool,
    /// Why it is dead, when it is.
    pub dead_reason: Option<String>,
}

/// One atomic assertion, and the evidence behind it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: Uuid,
    pub text: String,
    /// The source the agent says this came from.
    pub source_id: Uuid,
    /// The exact passage the agent says supports it.
    pub quote: String,
    pub verdict: Verdict,
    /// 0.0–1.0. Proportion of the claim's distinctive facts found in the quote.
    pub score: f64,
    pub reason: String,
    pub next_action: NextAction,
    /// Where the quote matched in the source, in original bytes.
    pub span: Option<crate::text::Span>,
    /// How many independent retrieval passes produced this same claim.
    pub corroborators: u32,
}

impl Claim {
    pub fn is_publishable(&self) -> bool {
        self.verdict.is_good()
    }
}

/// The finished answer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub question: String,
    pub answer_md: String,
    pub claims: Vec<Claim>,
    pub sources: Vec<Source>,
    pub unverified: Vec<Claim>,
    pub stats: Stats,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Stats {
    pub claims_checked: u32,
    pub verified: u32,
    pub contradicted: u32,
    pub unsupported: u32,
    pub ungrounded: u32,
    pub conflicts: u32,
    pub unverifiable: u32,
    pub sources_fetched: u32,
    pub sources_dead: u32,
}

impl Stats {
    /// The headline number. Honest by construction: it can be zero.
    pub fn verified_fraction(&self) -> f64 {
        if self.claims_checked == 0 {
            0.0
        } else {
            self.verified as f64 / self.claims_checked as f64
        }
    }
}

/// A step in the run timeline, as shown in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub seq: u32,
    pub kind: String,
    pub label: String,
    pub status: String,
    pub duration_ms: u64,
    pub detail: serde_json::Value,
}

pub fn new_id() -> Uuid {
    Uuid::new_v4()
}
