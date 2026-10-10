//! HTTP API.
//!
//! Thin on purpose: the engine owns the logic, this owns transport and the SSE
//! stream that lets the UI show a run as it happens.

use crate::agent::{Agent, RunResult};
use crate::config::Config;
use crate::model::{Artifact, Mode, Source, Stats};
use crate::retrieval::RetrievalMode;
use crate::trace::Tracer;
use axum::extract::{Path, State};
use axum::http::{header, Method, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Application state shared by every handler.
#[derive(Clone)]
pub struct AppState {
    pub agent: Arc<Agent>,
    pub tracer: Arc<Tracer>,
    /// Fan-out for run progress events.
    pub tx: broadcast::Sender<String>,
}

#[derive(Deserialize)]
pub struct RunRequest {
    pub question: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    20
}

#[derive(Deserialize)]
pub struct AuditRequest {
    pub answer: String,
    pub sources: Vec<String>,
}

pub fn router(state: AppState) -> Router {
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/capabilities", get(capabilities))
        .route("/api/runs", post(create_run))
        .route("/api/runs/{id}", get(get_run))
        .route("/api/runs/{id}/events", get(run_events))
        .route("/api/runs/{id}/resume", post(resume_run))
        .route("/api/runs/{id}/cancel", post(cancel_run))
        .route("/api/audit", post(audit_answer))
        .route("/api/corpus", get(corpus_info))
        .layer(cors)
        .with_state(state)
}

async fn healthz(State(s): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": crate::VERSION,
        "events": s.agent.store.event_count().unwrap_or(0),
        "neatlogs": s.tracer.is_enabled(),
        "retrieval": match s.agent.retrieval_mode() {
            RetrievalMode::Live => "live",
            RetrievalMode::Corpus => "corpus",
        },
    }))
}

async fn capabilities(State(s): State<AppState>) -> impl IntoResponse {
    Json(s.agent.cfg.capabilities())
}

async fn create_run(
    State(s): State<AppState>,
    Json(req): Json<RunRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let question = req.question.trim().to_string();
    if question.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "question is required".into()));
    }
    if question.len() > 2000 {
        return Err((StatusCode::BAD_REQUEST, "question is too long".into()));
    }
    let mode = match req.mode.as_deref() {
        Some("naive") => Mode::Naive,
        _ => Mode::Verified,
    };

    // Runs are spawned: a long research task must not hold the request open.
    let run_id = s
        .agent
        .start(&question, mode)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let agent = s.agent.clone();
    let tx = s.tx.clone();
    tokio::spawn(async move {
        let _ = tx.send(progress(run_id, "started", json_null()));
        match agent.execute(run_id).await {
            Ok(res) => {
                let _ = tx.send(progress(run_id, "compiled", run_payload(&res)));
            }
            Err(e) => {
                let _ = tx.send(progress(
                    run_id,
                    "failed",
                    serde_json::json!({ "error": e.to_string() }),
                ));
            }
        }
    });

    Ok(Json(serde_json::json!({
        "run_id": run_id,
        "mode": mode.as_str(),
        "status": "running",
        "session": format!("verbatim-{run_id}"),
    })))
}

async fn get_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let run_id = Uuid::parse_str(&id).map_err(|_| (StatusCode::BAD_REQUEST, "bad id".into()))?;
    let events = s
        .agent
        .store
        .events(run_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    if events.is_empty() {
        return Err((StatusCode::NOT_FOUND, "no such run".into()));
    }
    let question = events
        .iter()
        .find(|e| e.kind == "run.started")
        .and_then(|e| e.body.get("question").and_then(|q| q.as_str()))
        .unwrap_or("")
        .to_string();
    let mode = events
        .iter()
        .find(|e| e.kind == "run.started")
        .and_then(|e| e.body.get("mode").and_then(|m| m.as_str()))
        .unwrap_or("verified")
        .to_string();

    let artifact = events
        .iter()
        .find(|e| e.kind == "run.compiled")
        .and_then(|e| e.body.get("artifact").cloned())
        .and_then(|v| serde_json::from_value::<Artifact>(v).ok());

    let sources = s.agent.store.load_sources(run_id).unwrap_or_default();
    let effects = s.agent.store.effects(run_id).unwrap_or_default();
    let trace_ids = s.agent.store.trace_ids(run_id).unwrap_or_default();

    Ok(Json(serde_json::json!({
        "run_id": run_id,
        "question": question,
        "mode": mode,
        "artifact": artifact,
        "steps": steps_of(&events),
        "sources": sources.iter().map(source_json).collect::<Vec<_>>(),
        "effects": effects,
        "trace_ids": trace_ids,
        "stats": artifact.as_ref().map(|a| a.stats),
    })))
}

/// Server-sent events for one run. The UI follows this to show progress live.
async fn run_events(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, String)> {
    Uuid::parse_str(&id).map_err(|_| (StatusCode::BAD_REQUEST, "bad id".into()))?;
    let mut rx = s.tx.subscribe();
    let stream = async_stream::stream! {
        while let Ok(msg) = rx.recv().await {
            if msg.contains(&id) {
                yield Ok(Event::default().data(msg));
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(15)),
    ))
}

async fn resume_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let run_id = Uuid::parse_str(&id).map_err(|_| (StatusCode::BAD_REQUEST, "bad id".into()))?;
    let agent = s.agent.clone();
    let res = agent
        .execute(run_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({
        "run_id": run_id,
        "resumed": true,
        "effects": s.agent.store.effects(run_id).map(|e| e.len()).unwrap_or(0),
        "claims": res.artifact.claims.len(),
    })))
}

async fn cancel_run(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let _ = Uuid::parse_str(&id).map_err(|_| (StatusCode::BAD_REQUEST, "bad id".into()))?;
    s.agent.cancel();
    Ok(Json(serde_json::json!({ "cancelled": true })))
}

/// Audits an answer that came from somewhere else.
///
/// This is the wedge that makes the technology legible in ten seconds: paste a
/// Perplexity or ChatGPT answer with its sources and get the same per-claim
/// ledger. Verbatim is not competing with those tools; it is checking them.
async fn audit_answer(
    State(s): State<AppState>,
    Json(req): Json<AuditRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if req.sources.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "at least one source url".into()));
    }
    let agent = s.agent.clone();
    let mut fetched: Vec<Source> = Vec::new();
    for url in req.sources.iter().take(10) {
        match agent.fetch_one(url).await {
            Ok(src) => fetched.push(src),
            Err(_) => continue,
        }
    }
    if fetched.is_empty() {
        return Err((
            StatusCode::BAD_GATEWAY,
            "could not read any of those sources".into(),
        ));
    }

    let report = agent
        .audit(&req.answer, &fetched)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "claims": report.claims,
        "stats": report.stats,
        "sources": fetched.iter().map(|s| serde_json::json!({
            "url": s.url, "title": s.title, "sha256": s.content_hash, "bytes": s.bytes,
        })).collect::<Vec<_>>(),
        "note": "Every verdict below is deterministic: quote containment, entity grounding, polarity and prior-leak. No model scored this answer.",
    })))
}

async fn corpus_info(State(s): State<AppState>) -> impl IntoResponse {
    Json(s.agent.corpus_summary().await)
}

// ------------------------------------------------------------------ helpers

fn json_null() -> serde_json::Value {
    serde_json::Value::Null
}

fn progress(run_id: Uuid, kind: &str, extra: serde_json::Value) -> String {
    serde_json::json!({
        "run_id": run_id,
        "kind": kind,
        "at": chrono::Utc::now().to_rfc3339(),
        "data": extra,
    })
    .to_string()
}

fn run_payload(res: &RunResult) -> serde_json::Value {
    serde_json::json!({
        "run_id": res.run_id,
        "session": res.session,
        "mode": res.mode.as_str(),
        "stats": res.artifact.stats,
        "trace_ids": res.trace_ids,
        "cost_cents": res.cost_cents,
        "duration_ms": res.duration_ms,
    })
}

fn source_json(s: &Source) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "url": s.url,
        "title": s.title,
        "http_status": s.http_status,
        "bytes": s.bytes,
        "sha256": s.content_hash,
        "dead": s.dead,
        "dead_reason": s.dead_reason,
        "fetched_at": s.fetched_at,
    })
}

/// Projects the event log into the timeline the UI renders.
fn steps_of(events: &[crate::store::Event]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for e in events {
        let label = match e.kind.as_str() {
            "run.started" => "Run started",
            "step.plan.completed" => "Planned",
            "llm.plan" => "Planner",
            "llm.claim" => "Drafted claims",
            "step.claims.drafted" => "Verified claims",
            "step.sub_question.completed" => "Sub-question complete",
            "run.budget_exhausted" => "Budget reached",
            "run.compiled" => "Compiled",
            other => other,
        };
        out.push(serde_json::json!({
            "seq": e.seq,
            "kind": e.kind,
            "label": label,
            "at": e.at,
            "body": e.body,
        }));
    }
    out
}

/// Formats a stats block for the CLI and the README.
pub fn format_stats(s: &Stats) -> String {
    format!(
        "{} claims checked · {} verified ({:.0}%) · {} contradicted · {} unsupported · {} ungrounded · {} conflicts · {} unverifiable",
        s.claims_checked,
        s.verified,
        s.verified_fraction() * 100.0,
        s.contradicted,
        s.unsupported,
        s.ungrounded,
        s.conflicts,
        s.unverifiable,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_project_the_event_log() {
        let store = crate::store::Store::in_memory().unwrap();
        let run = Uuid::new_v4();
        store
            .append(run, "run.started", serde_json::json!({"question": "q"}))
            .unwrap();
        store
            .append(run, "run.compiled", serde_json::json!({"artifact": {}}))
            .unwrap();
        let evs = store.events(run).unwrap();
        let steps = steps_of(&evs);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["label"], "Run started");
        assert_eq!(steps[1]["label"], "Compiled");
    }

    #[test]
    fn stats_format_is_honest_at_zero() {
        let s = Stats::default();
        assert!(format_stats(&s).contains("0 claims checked"));
    }
}
