//! Configuration, entirely from the environment.
//!
//! Nothing here is secret by default and nothing here is committed: the real
//! values live in `.env`, which is gitignored.

use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub db_path: String,

    /// neatlogs ingest write key (`nlw_...`). Ingest-only: it can POST traces
    /// but never read data back, so it is safe to hold server-side.
    pub neatlogs_write_key: Option<String>,
    /// Project *name* — required because a write key identifies a user, not a
    /// project, so every trace must carry it at the root.
    pub neatlogs_project: String,
    pub neatlogs_endpoint: String,

    /// Bedrock model id, e.g. `anthropic.claude-sonnet-4-5-20250929-v1:0`.
    pub bedrock_model_id: String,
    pub aws_region: String,

    /// The hackathon's LiteLLM gateway. Cheap, shared, and budget-capped at $20
    /// per key over 30 days — so it is the default when configured.
    pub litellm_base_url: Option<String>,
    pub litellm_api_key: Option<String>,
    pub litellm_model: String,

    /// Search provider: `brave` or `none` (corpus-only).
    pub search_provider: String,
    pub search_api_key: Option<String>,

    /// Where fetched documents come from: `live` or `corpus`.
    pub retrieval_mode: String,
    pub corpus_dir: String,

    /// Wall-clock and spend ceilings, enforced at runtime.
    pub max_run_seconds: u64,
    pub max_run_cents: f64,

    pub log_format: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        // A `.env` in the working directory is loaded if present. This is the
        // only place file contents are read for secrets.
        let _ = dotenvy::dotenv();

        let retrieval_mode = std::env::var("VERBATIM_RETRIEVAL").unwrap_or_else(|_| "corpus".into());
        let search_provider = std::env::var("VERBATIM_SEARCH").unwrap_or_else(|_| "brave".into());
        let search_api_key = std::env::var("BRAVE_SEARCH_API_KEY")
            .or_else(|_| std::env::var("VERBATIM_SEARCH_API_KEY"))
            .ok()
            .filter(|k| !k.is_empty());

        Ok(Self {
            bind: std::env::var("VERBATIM_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into()),
            db_path: std::env::var("VERBATIM_DB").unwrap_or_else(|_| "./var/verbatim.db".into()),
            neatlogs_write_key: std::env::var("NEATLOGS_WRITE_KEY")
                .or_else(|_| std::env::var("NEATLOGS_API_KEY"))
                .ok()
                .filter(|k| !k.is_empty()),
            neatlogs_project: std::env::var("NEATLOGS_PROJECT")
                .unwrap_or_else(|_| "neathack".into()),
            neatlogs_endpoint: std::env::var("NEATLOGS_ENDPOINT")
                .unwrap_or_else(|_| "https://ingest.neatlogs.com".into()),
            bedrock_model_id: std::env::var("BEDROCK_MODEL_ID").unwrap_or_else(|_| {
                "us.anthropic.claude-sonnet-4-5-20250929-v1:0".into()
            }),
            aws_region: std::env::var("AWS_REGION")
                .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
                .unwrap_or_else(|_| "us-east-1".into()),
            litellm_base_url: std::env::var("LITELLM_BASE_URL").ok().filter(|s| !s.is_empty()),
            litellm_api_key: std::env::var("LITELLM_API_KEY").ok().filter(|s| !s.is_empty()),
            litellm_model: std::env::var("LITELLM_MODEL")
                .unwrap_or_else(|_| crate::llm::OpenAiCompatLlm::cheapest_model().into()),
            search_provider,
            search_api_key,
            retrieval_mode,
            corpus_dir: std::env::var("VERBATIM_CORPUS").unwrap_or_else(|_| "./corpus".into()),
            max_run_seconds: env_u64("VERBATIM_MAX_RUN_SECONDS", 600),
            max_run_cents: env_f64("VERBATIM_MAX_RUN_CENTS", 25.0),
            log_format: std::env::var("VERBATIM_LOG_FORMAT").unwrap_or_else(|_| "text".into()),
        })
    }

    /// True when we can actually reach an LLM.
    /// The model the agent will actually call, and where.
    pub fn llm_target(&self) -> String {
        if self.litellm_api_key.is_some() {
            format!("{}{}", self.litellm_base_url.clone().unwrap_or_default(), self.litellm_model)
        } else {
            format!("bedrock:{}", self.bedrock_model_id)
        }
    }

    pub fn has_llm(&self) -> bool {
        self.litellm_api_key.is_some()
            || (std::env::var("AWS_ACCESS_KEY_ID").is_ok()
                && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok())
    }

    pub fn has_neatlogs(&self) -> bool {
        self.neatlogs_write_key.is_some()
    }

    /// Human-readable status for the health endpoint and the demo.
    pub fn capabilities(&self) -> serde_json::Value {
        serde_json::json!({
            "llm": self.has_llm(),
            "neatlogs": self.has_neatlogs(),
            "search": self.search_provider != "none" && self.search_api_key.is_some(),
            "retrieval": self.retrieval_mode,
            "model": self.llm_target(),
        })
    }

    pub fn require_llm(&self) -> anyhow::Result<()> {
        if self.has_llm() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "no AWS credentials in the environment: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY"
            ))
            .context("LLM unavailable")
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
