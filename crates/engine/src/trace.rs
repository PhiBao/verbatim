//! neatlogs trace emission.
//!
//! One nested JSON trace per run, posted to `POST /v1/trace`. No SDK exists for
//! Rust, so this is the documented HTTP-injection path — which is also the
//! reason the backend can be Rust at all.
//!
//! Span taxonomy chosen deliberately so the dashboard tells the product's
//! story without narration:
//!
//! | span | what it shows |
//! |---|---|
//! | `WORKFLOW` root | the run, one per question |
//! | `AGENT` | planner, claimer, repairer, compiler |
//! | `RETRIEVER` | a search, with its query and documents |
//! | `TOOL` | a fetch, named for the tool |
//! | `LLM` | a model call, with model and token counts |
//! | **`GUARDRAIL`** | **every verification check, with `passed` and `score`** |
//!
//! The GUARDRAIL choice matters: `passed: false` marks the span as an error in
//! neatlogs, so a failed verification is red in the dashboard for free. That is
//! the whole argument of the product, visible in someone else's tool.

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A span under construction. Children nest, so the tree is the payload.
#[derive(Debug, Clone, Default)]
pub struct Span {
    pub name: String,
    pub kind: Option<String>,
    pub input: Option<Value>,
    pub output: Option<Value>,
    pub model: Option<String>,
    pub tokens: Option<(u64, u64)>,
    pub query: Option<Value>,
    pub documents: Option<Vec<String>>,
    pub tool_name: Option<String>,
    /// GUARDRAIL result. `Some(false)` makes this an error span.
    pub passed: Option<bool>,
    pub score: Option<f64>,
    pub status: Option<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub attributes: serde_json::Map<String, Value>,
    pub children: Vec<Span>,
}

impl Span {
    pub fn new(name: impl Into<String>, kind: &str) -> Self {
        Span {
            name: name.into(),
            kind: Some(kind.into()),
            ..Default::default()
        }
    }

    pub fn agent(name: impl Into<String>) -> Self {
        Span::new(name, "AGENT")
    }
    pub fn llm(name: impl Into<String>) -> Self {
        Span::new(name, "LLM")
    }
    pub fn tool(name: impl Into<String>) -> Self {
        Span::new(name, "TOOL")
    }
    pub fn retriever(name: impl Into<String>) -> Self {
        Span::new(name, "RETRIEVER")
    }
    pub fn guardrail(name: impl Into<String>) -> Self {
        Span::new(name, "GUARDRAIL")
    }
    pub fn workflow(name: impl Into<String>) -> Self {
        Span::new(name, "WORKFLOW")
    }

    pub fn with_input(mut self, v: impl Into<Value>) -> Self {
        self.input = Some(v.into());
        self
    }
    pub fn with_output(mut self, v: impl Into<Value>) -> Self {
        self.output = Some(v.into());
        self
    }
    pub fn with_error(mut self, e: impl Into<String>) -> Self {
        let e = e.into();
        self.status = Some("ERROR".into());
        self.error = Some(e);
        self
    }
    pub fn with_guardrail(mut self, passed: bool, score: f64) -> Self {
        self.passed = Some(passed);
        self.score = Some(score);
        if !passed {
            self.status = Some("ERROR".into());
        }
        self
    }
    pub fn with_attr(mut self, k: &str, v: impl Into<Value>) -> Self {
        self.attributes.insert(k.into(), v.into());
        self
    }
    pub fn with_tokens(mut self, prompt: u64, completion: u64) -> Self {
        self.tokens = Some((prompt, completion));
        self
    }
    pub fn with_tool_name(mut self, t: impl Into<String>) -> Self {
        self.tool_name = Some(t.into());
        self
    }
    pub fn with_query(mut self, q: impl Into<Value>) -> Self {
        self.query = Some(q.into());
        self
    }
    pub fn with_documents(mut self, d: Vec<String>) -> Self {
        self.documents = Some(d);
        self
    }
    pub fn with_model(mut self, m: impl Into<String>) -> Self {
        self.model = Some(m.into());
        self
    }
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = ms;
        self
    }

    pub fn push(&mut self, child: Span) {
        self.children.push(child);
    }

    fn to_json(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("name".into(), json!(self.name));
        if let Some(k) = &self.kind {
            m.insert("kind".into(), json!(k));
        }
        if let Some(v) = &self.input {
            m.insert("input".into(), v.clone());
        }
        if let Some(v) = &self.output {
            m.insert("output".into(), v.clone());
        }
        if let Some(v) = &self.model {
            m.insert("model".into(), json!(v));
        }
        if let Some((p, c)) = self.tokens {
            m.insert(
                "tokens".into(),
                json!({ "prompt": p, "completion": c, "total": p + c }),
            );
        }
        if let Some(q) = &self.query {
            m.insert("query".into(), q.clone());
        }
        if let Some(d) = &self.documents {
            m.insert(
                "documents".into(),
                json!(d.iter().map(|s| json!(s)).collect::<Vec<_>>()),
            );
        }
        if let Some(t) = &self.tool_name {
            m.insert("tool_name".into(), json!(t));
        }
        if let Some(p) = self.passed {
            m.insert("passed".into(), json!(p));
        }
        if let Some(s) = self.score {
            m.insert("score".into(), json!(s));
        }
        if let Some(s) = &self.status {
            m.insert("status".into(), json!(s));
        }
        if let Some(e) = &self.error {
            m.insert("error".into(), json!(e));
        }
        if self.duration_ms > 0 {
            m.insert("duration_ms".into(), json!(self.duration_ms));
        }
        if !self.attributes.is_empty() {
            m.insert("attributes".into(), Value::Object(self.attributes.clone()));
        }
        if !self.children.is_empty() {
            m.insert(
                "children".into(),
                Value::Array(self.children.iter().map(Span::to_json).collect()),
            );
        }
        Value::Object(m)
    }
}

/// Emits traces to neatlogs. Never fails a run: telemetry that can break the
/// product is not telemetry.
#[derive(Clone)]
pub struct Tracer {
    client: reqwest::Client,
    endpoint: String,
    write_key: Option<String>,
    project: String,
    enabled: bool,
    last: Arc<Mutex<Option<String>>>,
}

impl Tracer {
    pub fn from_env() -> Self {
        let endpoint = std::env::var("NEATLOGS_ENDPOINT")
            .unwrap_or_else(|_| "https://ingest.neatlogs.com".into());
        let write_key = std::env::var("NEATLOGS_WRITE_KEY")
            .or_else(|_| std::env::var("NEATLOGS_API_KEY"))
            .ok()
            .filter(|k| !k.is_empty());
        let project = std::env::var("NEATLOGS_PROJECT").unwrap_or_else(|_| "neathack".into());
        let enabled = write_key.is_some();
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                // neatlogs' ingest rejects requests with no User-Agent with a
                // bare nginx 403, which reads like an auth failure and is not.
                // reqwest sends no User-Agent unless told to.
                .user_agent(concat!("verbatim/", env!("CARGO_PKG_VERSION")))
                .build()
                .unwrap_or_default(),
            endpoint,
            write_key,
            project,
            enabled,
            last: Arc::new(Mutex::new(None)),
        }
    }

    pub fn disabled() -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: String::new(),
            write_key: None,
            project: String::new(),
            enabled: false,
            last: Arc::new(Mutex::new(None)),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// The ingest endpoint rejects bodies over roughly 8 KB with a bare nginx
    /// 403, which reads like an auth failure and is not. Measured by bisecting a
    /// real run's payload. A trace that would exceed it is split.
    const MAX_PAYLOAD_BYTES: usize = 7_500;

    /// Posts one trace. `session` groups the traces of one question together,
    /// so the naive and verified runs of the same question sit side by side.
    pub async fn emit(&self, name: &str, session: &str, root: Span) -> Vec<String> {
        if !self.enabled {
            return vec![];
        }
        let children: Vec<Span> = root.children;
        let chunks = chunk_children(children, Self::MAX_PAYLOAD_BYTES);
        let mut ids = Vec::new();

        for (i, chunk) in chunks.into_iter().enumerate() {
            // A split trace keeps the session id, so the dashboard still groups
            // every part of one run together.
            let trace_name = if i == 0 {
                name.to_string()
            } else {
                format!("{name}/part{i}")
            };
            let payload = json!({
                "name": trace_name,
                "project": self.project,
                "attributes": {
                    "neatlogs.session.id": session,
                    "neatlogs.workflow.name": name,
                },
                "input": root.input.clone().unwrap_or(json!({})),
                "children": chunk.iter().map(Span::to_json).collect::<Vec<_>>(),
            });
            if let Some(id) = self.send(&payload).await {
                ids.push(id);
            }
        }
        ids
    }

    /// Sends one already-built payload.
    async fn send(&self, payload: &Value) -> Option<String> {
        let mut req = self
            .client
            .post(format!("{}/v1/trace", self.endpoint.trim_end_matches('/')))
            .json(payload);
        if let Some(k) = &self.write_key {
            req = req.bearer_auth(k);
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    let body: Value = resp.json().await.unwrap_or(json!({}));
                    let id = body
                        .get("trace_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let spans = body.get("spans").and_then(|v| v.as_u64()).unwrap_or(0);
                    tracing::info!(trace_id = ?id, spans = spans, "neatlogs trace ingested");
                    *self.last.lock().await = id.clone();
                    id
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    // Keep the rejected payload so it can be replayed and
                    // diagnosed; without it a 403 is guesswork.
                    let _ = std::fs::write(
                        "/tmp/opencode/last-trace-payload.json",
                        payload.to_string(),
                    );
                    tracing::warn!(
                        status = %status,
                        body = %body,
                        spans = root_span_count(&payload),
                        bytes = %payload.to_string().len(),
                        "neatlogs rejected trace"
                    );
                    None
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "neatlogs ingest failed");
                None
            }
        }
    }

    /// Most recent trace id, for the receipts page.
    pub async fn last_trace_id(&self) -> Option<String> {
        self.last.lock().await.clone()
    }
}

/// Splits a span tree into groups whose serialised size stays under `limit`.
///
/// A long research run produces one span per search, fetch and verification, and
/// the whole tree for one run measured 10.5 KB against an ~8 KB ceiling. Rather
/// than dropping spans, they are split across traces that share a session id.
fn chunk_children(children: Vec<Span>, limit: usize) -> Vec<Vec<Span>> {
    let mut out: Vec<Vec<Span>> = vec![vec![]];
    let mut used = 0usize;
    for child in children {
        let size = serde_json::to_string(&child.to_json()).map(|s| s.len()).unwrap_or(0);
        if !out.last().unwrap().is_empty() && used + size > limit {
            out.push(vec![]);
            used = 0;
        }
        used += size;
        out.last_mut().unwrap().push(child);
    }
    out.retain(|c| !c.is_empty());
    if out.is_empty() {
        out.push(vec![]);
    }
    out
}

/// Number of spans in a payload, for the rejection log.
fn root_span_count(payload: &Value) -> usize {
    payload
        .get("children")
        .and_then(|c| c.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trace_over_the_ingest_ceiling_is_split_not_dropped() {
        // A real run produced 10.5 KB in one trace and the endpoint answered
        // 403. Bisecting a live payload put the ceiling between 7.6 KB and
        // 9.7 KB, so anything larger must be split across traces that share a
        // session id.
        let mut big = Span::workflow("run");
        for i in 0..40 {
            big.push(
                Span::guardrail(format!("verify_{i}"))
                    .with_input(json!({ "claim": "x".repeat(300) })),
            );
        }
        let chunks = chunk_children(big.children, 7_500);
        assert!(chunks.len() > 1, "a large tree must be split");
        for c in &chunks {
            let bytes: usize = c
                .iter()
                .map(|s| serde_json::to_string(&s.to_json()).map(|v| v.len()).unwrap_or(0))
                .sum();
            assert!(bytes <= 7_500, "chunk of {bytes} bytes still exceeds the ceiling");
        }
    }

    #[test]
    fn small_traces_are_not_split() {
        let mut root = Span::workflow("run");
        root.push(Span::agent("planner"));
        let chunks = chunk_children(root.children, 7_500);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn guardrail_failure_becomes_an_error_span() {
        let g = Span::guardrail("verify").with_guardrail(false, 0.2);
        let j = g.to_json();
        assert_eq!(j["passed"], json!(false));
        assert_eq!(j["status"], json!("ERROR"));
        assert_eq!(j["score"], json!(0.2));
    }

    #[test]
    fn children_nest() {
        let mut root = Span::workflow("run");
        root.push(Span::llm("plan").with_tokens(10, 5));
        let j = root.to_json();
        assert_eq!(j["children"][0]["kind"], json!("LLM"));
        assert_eq!(j["children"][0]["tokens"]["total"], json!(15));
    }
}
