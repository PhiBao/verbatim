//! The LLM seam.
//!
//! One trait, one implementation (AWS Bedrock via the Converse API). Bedrock is
//! both a hackathon sponsor-adjacent choice and the cheapest credible path to a
//! frontier model; the seam exists so that swapping providers is a one-file
//! change, and so tests can run without any network at all.

use anyhow::{Context, Result};
use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message, SystemContentBlock};
use aws_smithy_types::Blob;

/// A completed model call.
#[derive(Debug, Clone, Default)]
pub struct Completion {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Milliseconds the call took.
    pub duration_ms: u64,
}

impl Completion {
    /// Rough cost in USD cents, using Bedrock's published Claude Sonnet rates.
    pub fn cost_cents(&self, model: &str) -> f64 {
        let (in_per_m, out_per_m) = if model.contains("haiku") {
            (0.80, 4.0)
        } else if model.contains("opus") {
            (15.0, 75.0)
        } else {
            (3.0, 15.0)
        };
        (self.input_tokens as f64 * in_per_m + self.output_tokens as f64 * out_per_m) / 10_000.0
    }
}

#[async_trait::async_trait]
pub trait Llm: Send + Sync {
    async fn converse(&self, system: &str, user: &str, max_tokens: u32) -> Result<Completion>;
    fn model_id(&self) -> &str;
}

/// Bedrock-backed implementation.
pub struct BedrockLlm {
    client: aws_sdk_bedrockruntime::Client,
    model_id: String,
}

impl BedrockLlm {
    pub async fn new(region: &str, model_id: &str) -> Self {
        let sdk = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let conf = aws_sdk_bedrockruntime::config::Builder::from(&sdk)
            .region(aws_config::Region::new(region.to_string()))
            .build();
        Self {
            client: aws_sdk_bedrockruntime::Client::from_conf(conf),
            model_id: model_id.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl Llm for BedrockLlm {
    async fn converse(&self, system: &str, user: &str, max_tokens: u32) -> Result<Completion> {
        let started = std::time::Instant::now();

        let out = self
            .client
            .converse()
            .model_id(self.model_id.clone())
            .system(SystemContentBlock::Text(system.to_string()))
            .messages(
                Message::builder()
                    .role(ConversationRole::User)
                    .content(ContentBlock::Text(user.to_string()))
                    .build()
                    .context("build message")?,
            )
            .inference_config(
                aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                    .max_tokens(max_tokens as i32)
                    .temperature(0.0)
                    .build(),
            )
            .send()
            .await
            .context("bedrock converse failed")?;

        let mut text = String::new();
        if let Some(block) = out.output() {
            if let aws_sdk_bedrockruntime::types::ConverseOutput::Message(m) = block {
                for c in m.content() {
                    if let ContentBlock::Text(t) = c {
                        text.push_str(t);
                    }
                }
            }
        }

        let usage = out.usage();
        let (input_tokens, output_tokens) = usage
            .map(|u| (u.input_tokens() as u64, u.output_tokens() as u64))
            .unwrap_or((0, 0));
        Ok(Completion {
            text,
            input_tokens,
            output_tokens,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

/// An OpenAI-compatible chat-completions client.
///
/// The hackathon provides a LiteLLM gateway with a **$20 shared budget per key
/// over 30 days**, which makes cost a first-class constraint rather than an
/// afterthought. That budget is enforced by the provider, but the agent also
/// enforces its own ceiling so a run cannot silently eat the team's allowance.
///
/// Pricing here is the worst case we assume for budgeting, because the gateway
/// does not report per-request cost.
pub struct OpenAiCompatLlm {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    model_id: String,
    /// Assumed $ per million input tokens, for budget accounting.
    price_in_per_m: f64,
    /// Assumed $ per million output tokens, for budget accounting.
    price_out_per_m: f64,
}

impl OpenAiCompatLlm {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        let model_id = model_id.into();
        let (price_in_per_m, price_out_per_m) = price_for(&model_id);
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .unwrap_or_default(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            model_id,
            price_in_per_m,
            price_out_per_m,
        }
    }

    /// The smallest model that can do the job, given a $20 shared allowance.
    pub fn cheapest_model() -> &'static str {
        "claude-haiku"
    }
}

/// Assumed pricing per million tokens, used only for the agent's own budget
/// accounting. Conservative (i.e. slightly high), so the ceiling trips early.
fn price_for(model: &str) -> (f64, f64) {
    match model {
        m if m.contains("opus") => (15.0, 75.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("haiku") => (0.80, 4.0),
        m if m.contains("nova-pro") => (0.80, 3.20),
        m if m.contains("nova-lite") => (0.06, 0.24),
        m if m.contains("nova-micro") => (0.035, 0.14),
        _ => (3.0, 15.0),
    }
}

#[async_trait::async_trait]
impl Llm for OpenAiCompatLlm {
    async fn converse(&self, system: &str, user: &str, max_tokens: u32) -> Result<Completion> {
        let started = std::time::Instant::now();
        let body = serde_json::json!({
            "model": self.model_id,
            "max_tokens": max_tokens,
            "temperature": 0,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });

        let resp = self
            .client
            .post(format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("llm request failed")?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            // The gateway enforces the shared budget itself; surface that plainly
            // rather than letting it look like a generic failure.
            if status.as_u16() == 429 || status.as_u16() == 402 {
                anyhow::bail!(
                    "model gateway rejected the request ({status}): the shared $20 budget is \
                     likely exhausted. {}",
                    first_line(&text)
                );
            }
            anyhow::bail!("llm returned {status}: {}", first_line(&text));
        }

        let v: serde_json::Value = serde_json::from_str(&text).context("parse llm response")?;
        let content = v
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        let input_tokens = v.pointer("/usage/prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0);
        let output_tokens = v
            .pointer("/usage/completion_tokens")
            .and_then(|t| t.as_u64())
            .unwrap_or(0);

        Ok(Completion {
            text: content,
            input_tokens,
            output_tokens,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn model_id(&self) -> &str {
        &self.model_id
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(200).collect()
}

/// A recording implementation used by tests and by the corpus demo path. It
/// replays pre-written responses so the demo is deterministic without pretending
/// a model was called.
#[derive(Default)]
pub struct ReplayLlm {
    pub responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    pub model: String,
}

impl ReplayLlm {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses.into_iter().collect()),
            model: "replay".into(),
        }
    }
}

#[async_trait::async_trait]
impl Llm for ReplayLlm {
    async fn converse(&self, _system: &str, _user: &str, _max_tokens: u32) -> Result<Completion> {
        let mut q = self.responses.lock().unwrap();
        let text = q.pop_front().unwrap_or_else(|| "{}".to_string());
        Ok(Completion {
            text,
            input_tokens: 0,
            output_tokens: 0,
            duration_ms: 0,
        })
    }
    fn model_id(&self) -> &str {
        &self.model
    }
}

/// Pulls the first JSON object out of a model response, tolerating prose around
/// it and ```` ```json ```` fences. Models are not reliable JSON emitters, so the
/// agent must never trust one.
pub fn extract_json(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Some(v);
    }
    // Strip a code fence.
    let unfenced = trimmed
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(unfenced) {
        return Some(v);
    }
    // Walk to the first balanced object.
    let start = trimmed.find('{')?;
    let bytes = trimmed.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate().skip(start) {
        if esc {
            esc = false;
            continue;
        }
        match b {
            b'\\' if in_str => esc = true,
            b'"' => in_str = !in_str,
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&trimmed[start..=i]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

/// Computes a short, stable digest for an effect's arguments, used as the
/// idempotency key.
pub fn args_digest(tool: &str, args: &serde_json::Value) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(tool.as_bytes());
    h.update(b"\x1f");
    h.update(serde_json::to_vec(args).unwrap_or_default());
    hex::encode(h.finalize())
}

/// Placeholder to keep the Blob import honest until a multimodal path exists.
#[allow(dead_code)]
fn _blob_used(b: Vec<u8>) -> Blob {
    Blob::new(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_from_prose_and_fences() {
        assert_eq!(
            extract_json("Sure! {\"a\": 1} hope that helps").unwrap()["a"],
            json_like(1)
        );
        assert!(extract_json("```json\n{\"a\": 2}\n```").is_some());
        assert!(extract_json("no json here").is_none());
    }

    fn json_like(n: i32) -> serde_json::Value {
        serde_json::json!(n)
    }
}
