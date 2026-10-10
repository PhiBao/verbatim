//! The `verbatim` CLI — the lever.
//!
//! The verifier is only worth something if it can be run by someone other than
//! the agent that produced the claim. So the checks live behind a small CLI,
//! designed for an agent or a human to drive:
//!
//! - **deep modules**: `verify` takes a claim and a source and nothing else
//! - **`--dry-run`** on anything with a side effect
//! - **subcommands disclose gradually**: `verbatim verify --help` explains itself
//! - **errors say what to do next**, not just what went wrong
//! - **JSON output by default**, because that is what a caller can use

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use engine::agent::Agent;
use engine::config::Config;
use engine::llm::{BedrockLlm, Llm, ReplayLlm};
use engine::retrieval::{Retrieval, RetrievalMode};
use engine::verify::Verifier;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "verbatim",
    version,
    about = "If we didn't fetch it, we didn't say it.",
    long_about = "A research agent where every sentence carries a receipt.\n\n\
                  `verbatim verify` checks one claim against one source with no model \
                  involved, so you can run the same check the agent runs and get the \
                  same answer.",
    after_help = "Examples:\n  \
                  verbatim verify --claim \"The limit is 100 rpm\" --quote \"100 requests per minute\" --source https://example.test/limits\n  \
                  verbatim audit --answer \"...\" --source https://a.test/x\n  \
                  verbatim run \"What are the Anthropic rate limits?\" --json"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,

    /// Emit JSON instead of human-readable text. Default for `verify` and `audit`.
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check one claim against one source. Deterministic: no model is called.
    Verify {
        /// The claim, as a self-contained factual sentence.
        #[arg(long)]
        claim: String,
        /// The exact passage the claim says supports it.
        #[arg(long)]
        quote: String,
        /// The source URL the quote is attributed to.
        #[arg(long)]
        source: String,
        /// Read the source from the pinned corpus instead of the network.
        #[arg(long)]
        corpus: bool,
    },

    /// Audit an answer that came from somewhere else. Works on any answer with
    /// sources you can name — including another agent's.
    Audit {
        /// The answer text to audit.
        #[arg(long)]
        answer: String,
        /// Source URLs to check it against. Repeatable.
        #[arg(long, num_args = 1..)]
        source: Vec<String>,
    },

    /// Run a full research task end to end.
    Run {
        question: String,
        /// `verified` (default) or `naive` — the control run.
        #[arg(long, default_value = "verified")]
        mode: String,
        /// Show what would happen without calling the model or fetching.
        #[arg(long)]
        dry_run: bool,
    },

    /// Inspect the durable run ledger.
    Runs {
        #[command(subcommand)]
        cmd: RunsCmd,
    },

    /// Summarise the pinned corpus.
    Corpus,
}

#[derive(Subcommand)]
enum RunsCmd {
    /// List runs recorded in the event log.
    List,
    /// Replay one run's event log.
    Show { run_id: String },
    /// Resume a run that was interrupted.
    Resume { run_id: String },
    /// Show the side effects a run has settled. Zero repeats means the resume
    /// repeated nothing.
    Effects { run_id: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::from_env()?;

    match cli.cmd {
        Cmd::Verify {
            claim,
            quote,
            source,
            corpus,
        } => verify(&cfg, &claim, &quote, &source, corpus).await,

        Cmd::Audit { answer, source } => audit(&cfg, &answer, &source).await,

        Cmd::Run {
            question,
            mode,
            dry_run,
        } => run(&cfg, &question, &mode, dry_run).await,

        Cmd::Runs { cmd } => runs(&cfg, cmd).await,

        Cmd::Corpus => corpus(&cfg).await,
    }
}

async fn verify(
    cfg: &Config,
    claim: &str,
    quote: &str,
    source_url: &str,
    use_corpus: bool,
) -> Result<()> {
    if claim.trim().is_empty() {
        bail!("--claim is empty. Pass the claim as a self-contained sentence, e.g. --claim \"the free tier allows 100 requests per minute\"");
    }
    if quote.trim().is_empty() {
        bail!("--quote is empty. Pass the exact words from the source that support the claim. Without a quote there is nothing to check.");
    }

    let mode = if use_corpus || cfg.retrieval_mode == "corpus" {
        RetrievalMode::Corpus
    } else {
        RetrievalMode::Live
    };
    let r = Retrieval::new(mode, &cfg.corpus_dir, cfg.tavily_key.clone());
    let doc = r
        .fetch(source_url)
        .await
        .with_context(|| format!("could not read {source_url}. If it is not on the public web, add it to the pinned corpus with `engine snapshot`."))?;

    let src = doc.into_source();
    let srcs = [src.clone()];
    let v = Verifier::new(&srcs);
    let result = v.verify(claim, quote, src.id);

    let out = serde_json::json!({
        "claim": claim,
        "quote": quote,
        "source": src.url,
        "source_sha256": src.content_hash,
        "verdict": result.verdict.as_str(),
        "score": result.score,
        "reason": result.reason,
        "missing": result.missing,
        "next_action": result.next_action,
        "span": result.span,
    });

    if cli_json() {
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("verdict : {}", result.verdict.as_str());
        println!("score   : {:.2}", result.score);
        println!("reason  : {}", result.reason);
        if !result.missing.is_empty() {
            println!("missing : {}", result.missing.join(", "));
        }
        if let Some(sp) = result.span {
            let raw = &src.text;
            // The span indexes the raw fetched bytes; map it onto the text view.
            let shown = substring_at(raw, sp.start, sp.end);
            println!("matched : {:?}", shown);
        }
        println!(
            "next    : {}",
            match &result.next_action {
                engine::model::NextAction::None => "nothing to do".into(),
                engine::model::NextAction::ReRetrieve(q) => format!("search again for: {q}"),
                engine::model::NextAction::DeclareUnverifiable(r) => {
                    format!("declare unverifiable: {r}")
                }
                engine::model::NextAction::ResolveConflict { rule } => {
                    format!("resolve conflict using: {rule}")
                }
                engine::model::NextAction::Drop => "drop this claim".into(),
            }
        );
    }
    Ok(())
}

/// A span indexes the raw fetched document; show the corresponding slice.
fn substring_at(text: &str, start: usize, end: usize) -> String {
    if start <= end && end <= text.len() {
        text[start..end].to_string()
    } else {
        text.chars().take(120).collect()
    }
}

async fn audit(cfg: &Config, answer: &str, sources: &[String]) -> Result<()> {
    if sources.is_empty() {
        bail!("--source is required. Pass at least one URL to check the answer against.");
    }
    let cfg2 = cfg.clone();
    let llm = Arc::new(ReplayLlm::new(vec![]));
    let r = Retrieval::new(RetrievalMode::Corpus, &cfg2.corpus_dir, cfg2.tavily_key.clone());
    let agent = Agent::for_test(llm, r)?;

    let mut fetched = Vec::new();
    for url in sources.iter().take(10) {
        match agent.fetch_one(url).await {
            Ok(s) => fetched.push(s),
            Err(e) => {
                eprintln!("warning: could not read {url}: {e}");
            }
        }
    }
    if fetched.is_empty() {
        bail!("none of the sources could be read. Check the URLs, or add them to the pinned corpus.");
    }

    let report = agent.audit(answer, &fetched).await?;
    let out = serde_json::json!({
        "claims": report.claims,
        "stats": report.stats,
        "sources": fetched.iter().map(|s| serde_json::json!({
            "url": s.url, "title": s.title, "sha256": s.content_hash, "bytes": s.bytes,
        })).collect::<Vec<_>>(),
        "method": "deterministic prior-leak audit. Grounding only: with no quotes to check, we verify that each sentence's distinctive facts appear in a source we actually read.",
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

async fn run(cfg: &Config, question: &str, mode: &str, dry_run: bool) -> Result<()> {
    if question.trim().is_empty() {
        bail!("a question is required, e.g. verbatim run \"what are the rate limits?\"");
    }
    let verified = mode != "naive";

    if dry_run {
        // Everything a caller needs to decide whether to spend money, without
        // spending any.
        let plan = serde_json::json!({
            "dry_run": true,
            "question": question,
            "mode": if verified { "verified" } else { "naive" },
            "model": cfg.bedrock_model_id,
            "budget_cents": cfg.max_run_cents,
            "budget_seconds": cfg.max_run_seconds,
            "retrieval": cfg.retrieval_mode,
            "search": cfg.search_provider,
            "steps": ["plan", "retrieve", "claim", "verify", if verified { "repair" } else { "compile" }, "compile"],
            "note": "no model was called and no page was fetched",
        });
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    let llm: Arc<dyn Llm> = if cfg.has_llm() {
        Arc::new(BedrockLlm::new(&cfg.aws_region, &cfg.bedrock_model_id).await)
    } else {
        bail!("no AWS credentials in the environment. Set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, or use --dry-run to see the plan.");
    };

    let agent = Agent::new(cfg.clone(), llm)?;
    let res = agent
        .run(question, if verified { engine::model::Mode::Verified } else { engine::model::Mode::Naive })
        .await?;

    let out = serde_json::json!({
        "run_id": res.run_id,
        "session": res.session,
        "mode": res.mode.as_str(),
        "stats": res.artifact.stats,
        "cost_cents": res.cost_cents,
        "duration_ms": res.duration_ms,
        "answer_md": res.artifact.answer_md,
        "claims": res.artifact.claims.iter().map(|c| serde_json::json!({
            "text": c.text, "verdict": c.verdict.as_str(), "score": c.score,
            "source": res.artifact.sources.iter().find(|s| s.id == c.source_id).map(|s| s.url.clone()),
            "span": c.span,
        })).collect::<Vec<_>>(),
        "unverified": res.artifact.unverified.iter().map(|c| serde_json::json!({
            "text": c.text, "verdict": c.verdict.as_str(), "reason": c.reason,
        })).collect::<Vec<_>>(),
        "trace_ids": res.trace_ids,
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

async fn runs(cfg: &Config, cmd: RunsCmd) -> Result<()> {
    let store = engine::store::Store::open(&cfg.db_path)
        .with_context(|| format!("open {}", cfg.db_path))?;

    match cmd {
        RunsCmd::List => {
            let runs = store.run_summaries()?;
            if runs.is_empty() {
                println!("no runs recorded yet. Start one with `verbatim run \"question\"`.");
                return Ok(());
            }
            for r in runs {
                println!(
                    "{}  {:<8} {} claims, {} verified  {}",
                    r.run_id,
                    r.mode,
                    r.claims,
                    r.verified,
                    r.question
                );
            }
        }
        RunsCmd::Show { run_id } => {
            let id = uuid::Uuid::parse_str(&run_id)
                .with_context(|| format!("'{run_id}' is not a run id. Run `verbatim runs list` to see them."))?;
            for e in store.events(id)? {
                println!("{:>4}  {}  {}", e.seq, e.kind, e.body);
            }
        }
        RunsCmd::Resume { run_id } => {
            let id = uuid::Uuid::parse_str(&run_id).with_context(|| format!("bad run id {run_id}"))?;
            let llm = Arc::new(ReplayLlm::new(vec![]));
            let agent = Agent::for_test(llm, Retrieval::new(RetrievalMode::Corpus, &cfg.corpus_dir, cfg.tavily_key.clone()))?;
            let _ = id;
            println!(
                "resume replays the ledger and continues from the last settled step.\n\nRun the engine with `cargo run -p engine` and POST /api/runs/<id>/resume, or use `verbatim runs show {id}` to replay what has already happened."
            );
        }
        RunsCmd::Effects { run_id } => {
            let id = uuid::Uuid::parse_str(&run_id).with_context(|| format!("bad run id {run_id}"))?;
            let effects = store.effects(id)?;
            if effects.is_empty() {
                println!("0 settled side effects — this run settled no side effects.");
                return Ok(());
            }
            println!("{} settled side effects", effects.len());
            for e in &effects {
                println!(
                    "  {}  {}  {}",
                    e["tool"].as_str().unwrap_or(""),
                    e["args_hash"].as_str().unwrap_or(""),
                    e["at"].as_str().unwrap_or("")
                );
            }
        }
    }
    Ok(())
}

async fn corpus(cfg: &Config) -> Result<()> {
    let r = Retrieval::new(RetrievalMode::Corpus, &cfg.corpus_dir, cfg.tavily_key.clone());
    let docs = r.corpus_len();
    if docs == 0 {
        println!("the pinned corpus is empty. Build it with `engine snapshot`.");
        return Ok(());
    }
    println!("{docs} documents in the pinned corpus");
    if let Some(t) = r.corpus_snapshot_time() {
        println!("most recent snapshot: {t}");
    }
    Ok(())
}

fn cli_json() -> bool {
    // `verbatim verify` and `verbatim audit` default to JSON because that is the
    // form a caller can use; humans get the readable form unless they ask.
    std::env::args().any(|a| a == "--json")
        || std::env::args().nth(1).as_deref() == Some("verify")
        || std::env::args().nth(1).as_deref() == Some("audit")
}

