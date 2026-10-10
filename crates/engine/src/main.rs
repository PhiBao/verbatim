//! The engine binary: an HTTP API plus a corpus snapshot tool.

use anyhow::Context;
use engine::agent::Agent;
use engine::api::{router, AppState};
use engine::config::Config;
use engine::retrieval::Retrieval;
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    init_tracing(&cfg);

    tracing::info!(
        version = engine::VERSION,
        db = %cfg.db_path,
        retrieval = %cfg.retrieval_mode,
        model = %cfg.bedrock_model_id,
        neatlogs = cfg.has_neatlogs(),
        "verbatim engine starting"
    );

    if cfg.has_neatlogs() {
        tracing::info!("neatlogs trace emission enabled");
    } else {
        tracing::warn!(
            "NEATLOGS_WRITE_KEY is not set: runs will work but no traces will be emitted"
        );
    }

    match std::env::args().nth(1).as_deref() {
        Some("snapshot") => snapshot(&cfg).await,
        _ => serve(&cfg).await,
    }
}

fn init_tracing(cfg: &Config) {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if cfg.log_format == "json" {
        fmt().with_env_filter(filter).json().init();
    } else {
        fmt().with_env_filter(filter).with_target(false).init();
    }
}

async fn serve(cfg: &Config) -> anyhow::Result<()> {
    let llm = build_llm(cfg).await;
    let agent = Arc::new(Agent::new(cfg.clone(), llm)?);
    let (tx, _rx) = broadcast::channel::<String>(256);

    let state = AppState {
        agent: agent.clone(),
        tracer: Arc::new(engine::trace::Tracer::from_env()),
        tx,
    };

    let app = router(state);
    let listener = tokio::net::TcpListener::bind(&cfg.bind)
        .await
        .with_context(|| format!("bind {}", cfg.bind))?;
    tracing::info!(addr = %cfg.bind, "listening");
    axum::serve(listener, app).await.context("serve")?;
    Ok(())
}

/// Fetches a list of URLs into the pinned corpus.
///
/// This is how the corpus is built: by a real fetch, during the hackathon, from
/// a real script. Nothing in it is hand-written, and the snapshot time is stored
/// with every document so staleness is visible.
async fn snapshot(cfg: &Config) -> anyhow::Result<()> {
    let list = std::env::var("VERBATIM_SNAPSHOT_URLS")
        .context("set VERBATIM_SNAPSHOT_URLS to a comma-separated list of URLs")?;
    let urls: Vec<String> = list
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let mut r = Retrieval::new(
        engine::retrieval::RetrievalMode::Live,
        &cfg.corpus_dir,
        cfg.tavily_key.clone(),
    );
    let mut ok = 0usize;
    for url in &urls {
        print!("{url} ... ");
        match r.fetch(url).await {
            Ok(doc) => {
                println!(
                    "{} bytes, sha256 {}",
                    doc.bytes,
                    &doc.content_hash[..12]
                );
                r.remember(&doc);
                ok += 1;
            }
            Err(e) => println!("FAILED: {e}"),
        }
    }
    let n = r.save_corpus()?;
    tracing::info!(stored = n, fetched = ok, "corpus snapshot written");
    println!("wrote {} documents to {}", n, cfg.corpus_dir);
    Ok(())
}

async fn build_llm(cfg: &Config) -> Arc<dyn engine::llm::Llm> {
    // The LiteLLM gateway wins when configured: it is cheap, it is what the
    // hackathon provides, and its $20 shared budget is exactly the kind of
    // constraint this project is about.
    if let (Some(base), Some(key)) = (cfg.litellm_base_url.clone(), cfg.litellm_api_key.clone()) {
        tracing::info!(
            model = %cfg.litellm_model,
            gateway = %base,
            "using the hackathon LiteLLM gateway (shared $20 budget)"
        );
        return Arc::new(engine::llm::OpenAiCompatLlm::new(base, key, cfg.litellm_model.clone()));
    }

    if cfg.has_llm() {
        tracing::info!(model = %cfg.bedrock_model_id, "using AWS Bedrock");
        return Arc::new(engine::llm::BedrockLlm::new(&cfg.aws_region, &cfg.bedrock_model_id).await);
    }

    tracing::warn!(
        "no model credentials: the agent will run with a replay LLM, which only replays \
         canned responses. Set LITELLM_API_KEY or AWS credentials to run for real."
    );
    Arc::new(engine::llm::ReplayLlm::new(vec![]))
}
