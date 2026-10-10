//! The agent loop.
//!
//! `plan → retrieve → claim → verify → repair → compile`, with a durable event
//! log underneath so a `kill -9` mid-run resumes without repeating side effects.
//!
//! The step that makes this different from a research-agent tutorial is
//! `verify`: it takes no model at all. `repair` is driven by the verdict's
//! `next_action`, so it is a control loop rather than a retry, and `compile`
//! structurally cannot emit a claim whose verdict is not `SUPPORTED`.

use crate::config::Config;
use crate::llm::{args_digest, Completion, Llm};
use crate::model::{Artifact, Claim, Mode, NextAction, Source, Stats, Verdict};
use crate::retrieval::{FetchedDoc, Retrieval, RetrievalMode};
use crate::store::{Event, Store};
use crate::text::canonical;
use crate::trace::{Span, Tracer};
use crate::verify::{ClaimPair, Verifier};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// A planned sub-question.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubQuestion {
    pub id: String,
    pub question: String,
}

/// The plan the planner produced.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    pub sub_questions: Vec<SubQuestion>,
}

pub struct Agent {
    pub cfg: Config,
    pub store: Store,
    pub tracer: Tracer,
    pub llm: Arc<dyn Llm>,
    pub retrieval: Mutex<Retrieval>,
    /// Set by the API layer so the run can be cancelled.
    cancelled: std::sync::atomic::AtomicBool,
}

/// One audited sentence from an external answer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditClaim {
    pub text: String,
    pub verdict: Verdict,
    pub source_url: Option<String>,
    /// Where the first matching fact sits in that source's original bytes.
    pub span: Option<crate::text::Span>,
    /// Facts found in no source we read.
    pub missing: Vec<String>,
    pub facts: Vec<String>,
}

/// The result of auditing someone else's answer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AuditReport {
    pub claims: Vec<AuditClaim>,
    pub stats: Stats,
}

/// What a finished run looks like.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunResult {
    pub run_id: Uuid,
    pub session: String,
    pub artifact: Artifact,
    pub mode: Mode,
    pub trace_ids: Vec<String>,
    /// Total spend for the run, in cents.
    pub cost_cents: f64,
    pub duration_ms: u64,
}

impl Agent {
    pub fn new(cfg: Config, llm: Arc<dyn Llm>) -> Result<Self> {
        let store = Store::open(&cfg.db_path)?;
        let corpus_dir = cfg.corpus_dir.clone();
        let retrieval_mode = cfg.retrieval_mode.clone();
        Ok(Self {
            cfg,
            store,
            tracer: Tracer::from_env(),
            llm,
            retrieval: Mutex::new(Retrieval::new(mode_of(&retrieval_mode), &corpus_dir)),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// An agent wired for tests: in-memory store, no network, no LLM.
    pub fn for_test(llm: Arc<dyn Llm>, retrieval: Retrieval) -> Result<Self> {
        let cfg = Config {
            bind: "127.0.0.1:0".into(),
            db_path: ":memory:".into(),
            neatlogs_write_key: None,
            neatlogs_project: "test".into(),
            neatlogs_endpoint: "http://127.0.0.1:1".into(),
            bedrock_model_id: "test".into(),
            aws_region: "us-east-1".into(),
            litellm_base_url: None,
            litellm_api_key: None,
            litellm_model: "test".into(),
            search_provider: "none".into(),
            search_api_key: None,
            retrieval_mode: "corpus".into(),
            corpus_dir: "./corpus".into(),
            max_run_seconds: 60,
            max_run_cents: 10.0,
            log_format: "text".into(),
        };
        Ok(Self {
            store: Store::in_memory()?,
            tracer: Tracer::disabled(),
            llm,
            retrieval: Mutex::new(retrieval),
            cancelled: std::sync::atomic::AtomicBool::new(false),
            cfg,
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn reset_cancel(&self) {
        self.cancelled
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn budget_cents(&self) -> f64 {
        self.cfg.max_run_cents
    }

    pub fn budget_seconds(&self) -> u64 {
        self.cfg.max_run_seconds
    }

    pub fn retrieval_mode(&self) -> RetrievalMode {
        self.retrieval_mode_of()
    }

    fn retrieval_mode_of(&self) -> RetrievalMode {
        match self.cfg.retrieval_mode.as_str() {
            "live" => RetrievalMode::Live,
            _ => RetrievalMode::Corpus,
        }
    }

    // ---------------------------------------------------------------- runs

    /// Starts a run. Returns once the run is durable, before any work happens.
    pub async fn start(&self, question: &str, mode: Mode) -> Result<Uuid> {
        let run_id = Uuid::new_v4();
        self.store.append(
            run_id,
            "run.started",
            json!({
                "question": question,
                "mode": mode.as_str(),
                "budget_cents": self.cfg.max_run_cents,
                "budget_seconds": self.cfg.max_run_seconds,
                "retrieval": self.retrieval_mode_of_str(),
                "model": self.llm.model_id(),
            }),
        )?;
        Ok(run_id)
    }

    fn retrieval_mode_of_str(&self) -> &'static str {
        match self.retrieval_mode_of() {
            RetrievalMode::Live => "live",
            RetrievalMode::Corpus => "corpus",
        }
    }

    /// Runs a question to completion.
    pub async fn run(&self, question: &str, mode: Mode) -> Result<RunResult> {
        let run_id = self.start(question, mode).await?;
        self.execute(run_id).await
    }

    /// Executes (or resumes) a run to completion.
    pub async fn execute(&self, run_id: Uuid) -> Result<RunResult> {
        let started = std::time::Instant::now();
        let events = self.store.events(run_id)?;
        let question = events
            .iter()
            .find(|e| e.kind == "run.started")
            .and_then(|e| e.body.get("question").cloned())
            .and_then(|v| v.as_str().map(str::to_string))
            .context("run has no question")?;
        let mode = mode_of_events(&events);

        let mut root = Span::workflow("verbatim.run").with_input(json!({ "question": question }));
        let mut cost = 0.0f64;

        // Plan, unless a previous attempt already did.
        let plan = match self.compiled_plan(&events)? {
            Some(p) => p,
            None => {
                let p = self.plan(run_id, &question, mode).await?;
                self.store
                    .append(run_id, "step.plan.completed", json!({ "plan": p }))?;
                root.push(
                    Span::agent("planner")
                        .with_output(json!({
                            "sub_questions": p.sub_questions.iter().map(|s| s.question.clone()).collect::<Vec<_>>()
                        }))
                        .with_attr("neatlogs.agent.role", "Decomposes the question into sub-questions"),
                );
                p
            }
        };

        // Already compiled? Then this is a resume that found finished work.
        if let Some(artifact) = self.compiled_artifact(&events)? {
            let trace_ids = self.store.trace_ids(run_id)?;
            return Ok(RunResult {
                run_id,
                session: session_of(run_id),
                artifact,
                mode,
                trace_ids,
                cost_cents: 0.0,
                duration_ms: started.elapsed().as_millis() as u64,
            });
        }

        // Research every sub-question, then compile.
        let mut all_claims: Vec<Claim> = Vec::new();
        let mut sources = self.store.load_sources(run_id)?;
        let already_done: std::collections::HashSet<String> = self.done_sub_questions(&events);

        for sub in &plan.sub_questions {
            if self.is_cancelled() {
                break;
            }
            if already_done.contains(&sub.id) {
                continue;
            }
            let (claims, spent) = self
                .research(run_id, mode, sub, &mut sources, &mut root)
                .await?;
            cost += spent;
            all_claims.extend(claims);
            if cost > self.cfg.max_run_cents || started.elapsed().as_secs() > self.cfg.max_run_seconds {
                self.store.append(
                    run_id,
                    "run.budget_exhausted",
                    json!({ "cost_cents": cost, "elapsed_s": started.elapsed().as_secs() }),
                )?;
                break;
            }
        }

        let artifact = self
            .compile(run_id, &question, mode, all_claims, sources, cost)
            .await?;

        let session = session_of(run_id);
        root.duration_ms = started.elapsed().as_millis() as u64;
        for t in self.tracer.emit("verbatim.run", &session, root).await {
            let _ = self.store.record_trace(run_id, &t, &session);
        }

        Ok(RunResult {
            run_id,
            session,
            artifact,
            mode,
            trace_ids: self.store.trace_ids(run_id)?,
            cost_cents: cost,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    // -------------------------------------------------------------- planner

    async fn plan(&self, run_id: Uuid, question: &str, mode: Mode) -> Result<Plan> {
        if mode == Mode::Naive {
            // The control gets one undivided pass: no planning advantage.
            return Ok(Plan {
                sub_questions: vec![SubQuestion {
                    id: "q0".into(),
                    question: question.to_string(),
                }],
            });
        }
        let system = PLAN_SYSTEM;
        let user = format!(
            "Question: {question}\n\nReturn JSON only: {{\"sub_questions\": [{{\"id\": \"q1\", \"question\": \"...\"}}, ...]}}"
        );
        let c = self
            .llm
            .converse(system, &user, 2000)
            .await
            .unwrap_or_else(|_| Completion {
                text: fallback_plan_json(question),
                ..Default::default()
            });
        let parsed = crate::llm::extract_json(&c.text)
            .and_then(|v| serde_json::from_value::<Plan>(v).ok())
            .unwrap_or_else(|| Plan {
                sub_questions: vec![SubQuestion {
                    id: "q0".into(),
                    question: question.to_string(),
                }],
            });
        let plan = sanitize_plan(parsed, question);
        self.store.append(
            run_id,
            "llm.plan",
            json!({
                "input": user,
                "output": c.text,
                "tokens": { "prompt": c.input_tokens, "completion": c.output_tokens },
            }),
        )?;
        Ok(plan)
    }

    // ------------------------------------------------------------- research

    async fn research(
        &self,
        run_id: Uuid,
        mode: Mode,
        sub: &SubQuestion,
        sources: &mut Vec<Source>,
        root: &mut Span,
    ) -> Result<(Vec<Claim>, f64)> {
        let mut cost = 0.0;
        let sub_span = Span::agent("research").with_input(json!({ "sub_question": sub.question }));
        let mut sub_span = sub_span;

        // 1. Retrieve.
        let urls = self.retrieve(run_id, &sub.question, sources, &mut sub_span).await?;

        // 2. Draft claims from the retrieved passages only.
        let drafted = self
            .draft_claims(run_id, &sub.question, sources)
            .await?;
        cost += drafted.1;

        // 3. Verify — no LLM in this step.
        let verifier = Verifier::new(sources);
        let mut verified: Vec<Claim> = Vec::new();
        for d in &drafted.0 {
            let v = verifier.verify(&d.text, &d.quote, d.source_id);
            let claim = Claim {
                id: Uuid::new_v4(),
                text: d.text.clone(),
                source_id: d.source_id,
                quote: d.quote.clone(),
                verdict: v.verdict,
                score: v.score,
                reason: v.reason.clone(),
                next_action: v.next_action.clone(),
                span: v.span,
                corroborators: 1,
            };
            sub_span.push(
                Span::guardrail("verify_claim")
                    .with_input(json!({
                        "claim": d.text,
                        "source": source_url(sources, d.source_id),
                        "quote": d.quote,
                    }))
                    .with_guardrail(v.verdict.is_good(), v.score)
                    .with_output(json!({
                        "verdict": v.verdict.as_str(),
                        "reason": v.reason,
                        "missing": v.missing,
                        "next_action": v.next_action,
                        "span": v.span,
                    })),
            );
            verified.push(claim);
        }

        // 4. Repair — driven by each verdict's next_action, bounded.
        if mode == Mode::Verified {
            let repaired = self
                .repair(run_id, &sub.question, &verified, sources, &mut sub_span)
                .await?;
            cost += repaired.1;
            verified = repaired.0;
        }

        self.store.append(
            run_id,
            "step.claims.drafted",
            json!({
                "sub_question_id": sub.id,
                "sub_question": sub.question,
                "urls": urls,
                "claims": verified.iter().map(claim_json).collect::<Vec<_>>(),
            }),
        )?;
        self.store.append(
            run_id,
            "step.sub_question.completed",
            json!({ "sub_question_id": sub.id }),
        )?;
        root.push(sub_span);
        Ok((verified, cost))
    }

    /// Searches and fetches. Every fetch goes through the effects ledger, so a
    /// resumed run repeats none of them.
    async fn retrieve(
        &self,
        run_id: Uuid,
        question: &str,
        sources: &mut Vec<Source>,
        span: &mut Span,
    ) -> Result<Vec<String>> {
        let r = self.retrieval.lock().await;

        let hits = if self.retrieval_mode_of() == RetrievalMode::Corpus {
            r.search_corpus(question, 5)
        } else {
            self.search_live(question, 5).await.unwrap_or_default()
        };

        let mut urls: Vec<String> = Vec::new();
        for h in &hits {
            span.push(
                Span::retriever("search")
                    .with_query(json!(question))
                    .with_documents(vec![h.clone()]),
            );
            urls.push(h.clone());
        }

        for url in &urls {
            // The idempotency check: if a previous attempt already settled this
            // fetch, reuse the stored source instead of repeating it.
            let key = format!("fetch:{}", canonical(url).canon);
            let digest = args_digest("fetch", &json!({ "url": url }));
            if self.store.effect_result_ref(&key)?.is_some() {
                continue;
            }

            match r.fetch(url).await {
                Ok(doc) => {
                    self.store.settle_effect(
                        run_id,
                        &key,
                        "fetch",
                        &digest,
                        Some(&doc.content_hash),
                    )?;
                    let src = doc.into_source();
                    self.store.save_source(run_id, &src)?;
                    span.push(
                        Span::tool("fetch_url")
                            .with_tool_name("fetch_url")
                            .with_input(json!({ "url": url }))
                            .with_output(json!({
                                "bytes": src.bytes,
                                "sha256": src.content_hash,
                                "title": src.title,
                            })),
                    );
                    sources.push(src);
                }
                Err(e) => {
                    // A source we cannot read is not a source that says "no".
                    let src = FetchedDoc::dead(url.clone(), e.to_string(), 0);
                    self.store.save_source(run_id, &src)?;
                    span.push(
                        Span::tool("fetch_url")
                            .with_tool_name("fetch_url")
                            .with_input(json!({ "url": url }))
                            .with_error(e.to_string()),
                    );
                    sources.push(src);
                }
            }
        }
        Ok(urls)
    }

    /// Live web search. Kept behind its own method so a provider swap or an
    /// outage degrades to "no hits" rather than failing the run.
    async fn search_live(&self, question: &str, limit: usize) -> Result<Vec<String>> {
        let key = self
            .cfg
            .search_api_key
            .clone()
            .context("no search API key configured")?;
        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .bearer_auth(&key)
            .query(&[("q", question), ("count", &limit.to_string())])
            .send()
            .await?
            .error_for_status()?;
        let body: Value = resp.json().await?;
        let urls = body
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|r| r.get("url").and_then(|u| u.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Ok(urls)
    }

    /// Drafts claims. The model sees only the retrieved passages, and every
    /// claim must carry a quote — which is what makes it checkable.
    async fn draft_claims(
        &self,
        run_id: Uuid,
        question: &str,
        sources: &[Source],
    ) -> Result<(Vec<Draft>, f64)> {
        let usable: Vec<&Source> = sources.iter().filter(|s| !s.dead && !s.text.is_empty()).collect();
        if usable.is_empty() {
            return Ok((vec![], 0.0));
        }
        let passages = usable
            .iter()
            .map(|s| format!("SOURCE {} \n{}", s.url, excerpt(&s.text, 4000)))
            .collect::<Vec<_>>()
            .join("\n\n");

        let user = format!(
            "SUB-QUESTION: {question}\n\nSOURCES (use only these):\n{passages}\n\nReturn JSON only."
        );

        let c = self.llm.converse(CLAIM_SYSTEM, &user, 4000).await?;
        let cost = c.cost_cents(self.llm.model_id());
        self.store.append(
            run_id,
            "llm.claim",
            json!({
                "input": user,
                "output": c.text,
                "tokens": { "prompt": c.input_tokens, "completion": c.output_tokens },
                "duration_ms": c.duration_ms,
            }),
        )?;

        let mut drafts = Vec::new();
        if let Some(v) = crate::llm::extract_json(&c.text) {
            if let Some(items) = v.get("claims").and_then(|c| c.as_array()) {
                for item in items {
                    let text = item
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let quote = item
                        .get("quote")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    let url = item
                        .get("source")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if text.is_empty() || quote.is_empty() || url.is_empty() {
                        continue;
                    }
                    if let Some(src) = sources.iter().find(|s| same_url(&s.url, &url)) {
                        drafts.push(Draft {
                            text,
                            source_id: src.id,
                            quote,
                        });
                    }
                }
            }
        }
        Ok((drafts, cost))
    }

    /// Repair. One bounded round, driven by what each verdict says to do next.
    ///
    /// The failed claim stays in the ledger with its verdict — the trace should
    /// show the failure, not hide it — and repair adds newly verified claims
    /// beside it.
    async fn repair(
        &self,
        run_id: Uuid,
        question: &str,
        claims: &[Claim],
        sources: &mut Vec<Source>,
        span: &mut Span,
    ) -> Result<(Vec<Claim>, f64)> {
        let mut cost = 0.0;
        let failed: Vec<&Claim> = claims.iter().filter(|c| !c.verdict.is_good()).collect();
        if failed.is_empty() {
            return Ok((claims.to_vec(), 0.0));
        }

        span.push(
            Span::agent("repairer")
                .with_input(json!({
                    "failed": failed.len(),
                    "verdicts": failed.iter().map(|c| c.verdict.as_str()).collect::<Vec<_>>(),
                }))
                .with_output(json!({ "round": 1 }))
                .with_attr("neatlogs.agent.role", "Turns each verdict into a targeted retrieval"),
        );

        // A fresh retrieval aimed at what the failed claims were about.
        let retry_queries: Vec<String> = failed
            .iter()
            .take(3)
            .map(|c| match &c.next_action {
                NextAction::ReRetrieve(q) if !q.trim().is_empty() => q.clone(),
                NextAction::DeclareUnverifiable(_) => format!("{question} {}", c.text),
                _ => format!("{question} {}", c.text),
            })
            .collect();

        let before = sources.len();
        for q in &retry_queries {
            let mut probe = Span::retriever("search");
            let _ = self.retrieve(run_id, q, sources, &mut probe).await;
            span.push(probe);
        }
        let new_sources = sources.len() - before;

        let (drafts, spent) = self.draft_claims(run_id, question, sources).await?;
        cost += spent;

        let verifier = Verifier::new(sources);
        let mut out = claims.to_vec();
        let mut repaired = 0usize;
        for d in &drafts {
            let v = verifier.verify(&d.text, &d.quote, d.source_id);
            span.push(
                Span::guardrail("verify_claim")
                    .with_input(json!({ "claim": d.text, "quote": d.quote }))
                    .with_guardrail(v.verdict.is_good(), v.score)
                    .with_output(json!({
                        "verdict": v.verdict.as_str(),
                        "reason": v.reason,
                        "round": "repair",
                    })),
            );
            if v.verdict.is_good() {
                repaired += 1;
            }
            out.push(Claim {
                id: Uuid::new_v4(),
                text: d.text.clone(),
                source_id: d.source_id,
                quote: d.quote.clone(),
                verdict: v.verdict,
                score: v.score,
                reason: v.reason.clone(),
                next_action: v.next_action.clone(),
                span: v.span,
                corroborators: 1,
            });
        }

        span.push(
            Span::agent("repairer")
                .with_output(json!({
                    "new_sources": new_sources,
                    "new_claims": drafts.len(),
                    "now_verified": repaired,
                })),
        );
        Ok((out, cost))
    }

    /// Builds the answer. **This is where the product's conscience lives**: the
    /// compiler structurally cannot emit a claim whose verdict is not
    /// `SUPPORTED`.
    async fn compile(
        &self,
        run_id: Uuid,
        question: &str,
        mode: Mode,
        claims: Vec<Claim>,
        sources: Vec<Source>,
        cost: f64,
    ) -> Result<Artifact> {
        let artifact = compile_artifact(question, mode, claims, sources, cost);
        self.store
            .append(run_id, "run.compiled", json!({ "artifact": artifact }))?;
        Ok(artifact)
    }

    // ------------------------------------------------------------ projections

    /// Fetches a single URL into a `Source`, for the audit endpoint.
    pub async fn fetch_one(&self, url: &str) -> Result<Source> {
        let r = self.retrieval.lock().await;
        let doc = r.fetch(url).await?;
        Ok(doc.into_source())
    }

    /// Audits an answer that came from somewhere else.
    ///
    /// This is the wedge that makes the technology legible in ten seconds. An
    /// arbitrary answer carries no quotes, so the strongest deterministic check
    /// available is **prior leak**: every distinctive fact in the answer must
    /// appear in a source we actually read. A fact that appears in none was
    /// invented or recalled, and we say so.
    ///
    /// We say exactly this in the README. We do not claim to check support, only
    /// grounding — the claim-level verifier needs a quote, and an external answer
    /// has none.
    pub async fn audit(&self, answer: &str, sources: &[Source]) -> Result<AuditReport> {
        let verifier = Verifier::new(sources);
        let sentences: Vec<&str> = answer
            .split(['.', '\n'])
            .map(|s| s.trim())
            .filter(|s| s.split_whitespace().count() >= 4)
            .collect();

        let mut claims = Vec::new();
        let mut stats = Stats {
            sources_fetched: sources.iter().filter(|s| !s.dead).count() as u32,
            sources_dead: sources.iter().filter(|s| s.dead).count() as u32,
            ..Default::default()
        };

        for sentence in sentences {
            let facts = crate::verify::extract_facts(sentence);
            if facts.is_empty() {
                continue;
            }
            stats.claims_checked += 1;

            // Which source carries the most of this sentence's facts, and where
            // is the most *distinctive* one, so the receipt highlights something
            // meaningful rather than a two-letter acronym.
            let mut best: Option<(usize, usize, Option<crate::text::Span>)> = None;
            for (i, s) in sources.iter().enumerate() {
                if s.dead {
                    continue;
                }
                let c = canonical(&s.text);
                let mut hits = 0usize;
                let mut best_span: Option<crate::text::Span> = None;
                let mut best_distinctive = 0usize;
                for f in &facts {
                    if let Some(sp) = c.find(&f.canon) {
                        hits += 1;
                        if f.raw.len() > best_distinctive {
                            best_distinctive = f.raw.len();
                            best_span = Some(sp);
                        }
                    }
                }
                if hits == 0 {
                    continue;
                }
                if best.map(|(h, _, _)| hits > h).unwrap_or(true) {
                    best = Some((hits, i, best_span));
                }
            }

            let missing: Vec<String> = facts
                .iter()
                .filter(|f| !sources.iter().any(|s| !s.dead && canonical(&s.text).contains(&f.canon)))
                .map(|f| f.raw.clone())
                .collect();

            let verdict = if missing.is_empty() {
                stats.verified += 1;
                Verdict::Supported
            } else if missing.len() == facts.len() {
                stats.ungrounded += 1;
                Verdict::Ungrounded
            } else {
                stats.unsupported += 1;
                Verdict::Unsupported
            };

            claims.push(AuditClaim {
                text: sentence.to_string(),
                verdict,
                source_url: best.map(|(_, i, _)| sources[i].url.clone()),
                span: best.and_then(|(_, _, sp)| sp),
                missing,
                facts: facts.iter().map(|f| f.raw.clone()).collect(),
            });
        }

        let _ = verifier;
        Ok(AuditReport { claims, stats })
    }

    /// What the pinned corpus contains, for the UI and for honesty.
    ///
    /// Async because it takes the retrieval lock: `blocking_lock` panics inside
    /// a tokio runtime, which is exactly how this bug was found.
    pub async fn corpus_summary(&self) -> serde_json::Value {
        let r = self.retrieval.lock().await;
        serde_json::json!({
            "mode": self.retrieval_mode_of_str(),
            "documents": r.corpus_len(),
            "snapshotted_at": r.corpus_snapshot_time(),
            "corpus_dir": r.corpus_dir().display().to_string(),
        })
    }

    fn compiled_plan(&self, events: &[Event]) -> Result<Option<Plan>> {
        Ok(events
            .iter()
            .find(|e| e.kind == "step.plan.completed")
            .and_then(|e| e.body.get("plan").cloned())
            .and_then(|v| serde_json::from_value::<Plan>(v).ok()))
    }

    fn compiled_artifact(&self, events: &[Event]) -> Result<Option<Artifact>> {
        Ok(events
            .iter()
            .find(|e| e.kind == "run.compiled")
            .and_then(|e| e.body.get("artifact").cloned())
            .and_then(|v| serde_json::from_value::<Artifact>(v).ok()))
    }

    fn done_sub_questions(&self, events: &[Event]) -> std::collections::HashSet<String> {
        events
            .iter()
            .filter(|e| e.kind == "step.sub_question.completed")
            .filter_map(|e| {
                e.body
                    .get("sub_question_id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect()
    }
}

/// Builds the finished artifact from verified claims.
///
/// In `naive` mode the compiler emits every drafted claim regardless of verdict,
/// which is what a research agent does today — it believes its own draft. In
/// `verified` mode it emits only `SUPPORTED` claims and names everything else in
/// an explicit "we could not verify" section.
///
/// The two modes are otherwise identical: same retrieval, same model, same
/// budget. The only difference is whether the compiler is allowed to ship an
/// unverified claim. That is the whole experiment.
fn compile_artifact(
    question: &str,
    mode: Mode,
    claims: Vec<Claim>,
    sources: Vec<Source>,
    cost_cents: f64,
) -> Artifact {
    let _ = cost_cents;
    // Overlapping sub-questions retrieve the same page and re-derive the same
    // fact, so collapsing happens here — across every sub-question, which is the
    // only place all the claims are in one list.
    let claims = dedupe_claims(claims);

    // Two sources can both be quoted correctly and still disagree. That is a
    // conflict, and it gets resolved with a recorded rule rather than by the
    // compiler picking whichever claim it happened to see first.
    let (claims, conflicts) = resolve_conflicts(claims, &sources, question);

    let mut stats = Stats {
        sources_fetched: sources.iter().filter(|s| !s.dead).count() as u32,
        sources_dead: sources.iter().filter(|s| s.dead).count() as u32,
        conflicts: conflicts.len() as u32,
        ..Default::default()
    };

    let mut publishable: Vec<Claim> = Vec::new();
    let mut unverified: Vec<Claim> = Vec::new();

    for c in claims {
        stats.claims_checked += 1;
        match c.verdict {
            Verdict::Supported => stats.verified += 1,
            Verdict::Contradicted => stats.contradicted += 1,
            Verdict::Unsupported => stats.unsupported += 1,
            Verdict::Conflict => stats.conflicts += 1,
            Verdict::Ungrounded => stats.ungrounded += 1,
            Verdict::Unverifiable => stats.unverifiable += 1,
        }
        if c.verdict.is_good() || mode == Mode::Naive {
            publishable.push(c);
        } else {
            unverified.push(c);
        }
    }

    let answer_md = render_answer(
        question,
        mode,
        &publishable,
        &unverified,
        &stats,
        cost_cents,
        &conflicts,
    );

    Artifact {
        question: question.to_string(),
        answer_md,
        claims: publishable,
        sources,
        unverified,
        stats,
        conflicts,
    }
}

/// Renders the answer. Claims become sentences, so the reader can see exactly
/// which source each one rests on.
fn render_answer(
    question: &str,
    mode: Mode,
    claims: &[Claim],
    unverified: &[Claim],
    stats: &Stats,
    cost_cents: f64,
    conflicts: &[Conflict],
) -> String {
    let mut md = String::new();
    md.push_str(&format!("## {question}\n\n"));

    if claims.is_empty() {
        md.push_str(
            "No claim in this run was backed by a source we could read. Rather than write an answer anyway, here is what we could not establish.\n\n",
        );
    } else {
        if mode == Mode::Naive {
            md.push_str(
                "> **Control run.** Every claim below was drafted by the model and shipped without a verification gate, including the ones the verifier rejected.\n\n",
            );
        }
        for c in claims {
            md.push_str(&format!("{} [^{}]\n\n", c.text, short(&c.id)));
        }
    }

    if !unverified.is_empty() {
        md.push_str("### We could not verify\n\n");
        md.push_str("These claims were drafted but did not pass verification, so they are not in the answer above.\n\n");
        for c in unverified {
            md.push_str(&format!(
                "- **{}** — {}\n",
                c.text,
                c.reason
            ));
        }
        md.push('\n');
    }

    if !conflicts.is_empty() {
        md.push_str("### Sources disagree\n\n");
        for c in conflicts {
            md.push_str(&format!(
                "**{}**\n\n| | value | source |\n|---|---|---|\n",
                c.key
            ));
            md.push_str(&format!(
                "| left | {} | {} |\n",
                c.left.value.as_deref().unwrap_or("—"),
                host_of(&c.left.url)
            ));
            md.push_str(&format!(
                "| right | {} | {} |\n\n",
                c.right.value.as_deref().unwrap_or("—"),
                host_of(&c.right.url)
            ));
            md.push_str(&format!("Resolved by: _{}_\n\n", c.rule));
        }
    }

    md.push_str("### Verification\n\n");
    md.push_str(&format!(
        "- claims checked: **{}**\n- verified: **{}**\n- contradicted: {} · unsupported: {} · ungrounded: {} · conflicts: {} · unverifiable: {}\n- sources read: {} ({} unavailable)\n- verified share: **{:.0}%**\n- run cost: **${:.3}**\n",
        stats.claims_checked,
        stats.verified,
        stats.contradicted,
        stats.unsupported,
        stats.ungrounded,
        stats.conflicts,
        stats.unverifiable,
        stats.sources_fetched,
        stats.sources_dead,
        stats.verified_fraction() * 100.0,
        cost_cents / 100.0,
    ));
    md
}

/// First 8 characters of an id, for footnote-style citation labels.
fn short(id: &Uuid) -> String {
    id.to_string().chars().take(8).collect()
}

/// A bounded excerpt of a source, so a long page cannot blow up a prompt.
fn excerpt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

fn same_url(a: &str, b: &str) -> bool {
    match (reqwest::Url::parse(a), reqwest::Url::parse(b)) {
        (Ok(x), Ok(y)) => {
            let norm = |u: &reqwest::Url| {
                format!(
                    "{}://{}{}",
                    u.scheme(),
                    u.host_str().unwrap_or(""),
                    u.path()
                )
                .trim_end_matches('/')
                .to_string()
            };
            norm(&x) == norm(&y)
        }
        _ => a == b,
    }
}

const CLAIM_SYSTEM: &str = r#"You are the claimer for a research agent. You may use ONLY the sources
given to you. You have no other knowledge available.

For each distinct factual assertion the sources support about the sub-question,
emit one claim. Each claim must carry:
- "text": one self-contained factual sentence
- "source": the exact URL of the source it came from
- "quote": the exact words from that source that support it, copied verbatim

Rules:
- The quote MUST be copied character-for-character from the source. Never
  paraphrase, never merge two passages, never invent.
- Never state a fact that is not in the sources, even if you are certain it is
  true. If the sources do not establish something, do not claim it.
- Prefer specific, checkable numbers over vague statements.
- One claim per source sentence.
- Output JSON only: {"claims": [{"text": "...", "source": "...", "quote": "..."}]}"#;

/// A claim as drafted by the model, before verification.
#[derive(Debug, Clone)]
pub struct Draft {
    pub text: String,
    pub source_id: Uuid,
    pub quote: String,
}

fn claim_json(c: &Claim) -> Value {
    json!({
        "id": c.id,
        "text": c.text,
        "source_id": c.source_id,
        "quote": c.quote,
        "verdict": c.verdict.as_str(),
        "score": c.score,
        "reason": c.reason,
        "span": c.span,
        "corroborators": c.corroborators,
    })
}

/// Collapses claims that rest on the same evidence.
///
/// Overlapping sub-questions retrieve the same page, and the claimer will
/// re-derive the same fact for each — the token-bucket sentence came back four
/// times in one run, worded four different ways. Substring comparison is too
/// weak for that; the reliable signal is that both claims quote **overlapping
/// spans of the same source**. Same evidence, same fact.

/// A disagreement between two sources about the same quantity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Conflict {
    pub key: String,
    pub left: Side,
    pub right: Side,
    /// The rule that decided, recorded so the decision is auditable.
    pub rule: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Side {
    pub url: String,
    pub claim_id: uuid::Uuid,
    pub value: Option<String>,
    pub quote: String,
}

/// Finds pairs of verified claims that quote different sources disagreeing on
/// the same quantity, and resolves them with a recorded rule.
///
/// The rule is deliberately simple and stateable in one sentence: **the source
/// whose host carries the entity the claim is about wins.** A vendor's own docs
/// beat a third-party blog about that vendor. When neither side is
/// authoritative the conflict is left unresolved and reported, rather than
/// silently decided.
fn resolve_conflicts(
    claims: Vec<Claim>,
    sources: &[Source],
    question: &str,
) -> (Vec<Claim>, Vec<Conflict>) {
    let verifier = Verifier::new(sources);
    let pairs: Vec<ClaimPair> = claims
        .iter()
        .map(|c| ClaimPair {
            claim_id: c.id,
            source_id: c.source_id,
            text: c.text.clone(),
        })
        .collect();
    let detected = verifier.conflicts(&pairs);
    if detected.is_empty() {
        return (claims, vec![]);
    }

    let mut out: Vec<Conflict> = Vec::new();
    let mut losers: Vec<uuid::Uuid> = Vec::new();

    for d in detected {
        let left = claims.iter().find(|c| c.text == d.left.claim);
        let right = claims.iter().find(|c| c.text == d.right.claim);
        let (Some(l), Some(r)) = (left, right) else {
            continue;
        };
        let lurl = source_url(sources, l.source_id);
        let rurl = source_url(sources, r.source_id);

        let value_of = |c: &Claim| -> Option<String> {
            crate::verify::extract_facts(&c.text)
                .into_iter()
                .find(|f| f.kind == crate::verify::FactKind::Number)
                .map(|f| f.raw)
        };

        let l_auth = is_authoritative(&lurl, &l.text, question);
        let r_auth = is_authoritative(&rurl, &r.text, question);
        let (rule, winner_id) = match (l_auth, r_auth) {
            (true, false) => (
                format!("{} is the primary source for this subject", host_of(&lurl)),
                l.id,
            ),
            (false, true) => (
                format!("{} is the primary source for this subject", host_of(&rurl)),
                r.id,
            ),
            _ => (
                "neither source is primary, so the disagreement is reported unresolved".into(),
                uuid::Uuid::nil(),
            ),
        };

        out.push(Conflict {
            key: d.key.clone(),
            left: Side {
                url: lurl.clone(),
                claim_id: l.id,
                value: value_of(l),
                quote: l.quote.clone(),
            },
            right: Side {
                url: rurl.clone(),
                claim_id: r.id,
                value: value_of(r),
                quote: r.quote.clone(),
            },
            rule,
        });

        if winner_id.is_nil() {
            // Unresolved: neither side may ship, because we cannot say which.
            losers.push(l.id);
            losers.push(r.id);
        } else {
            // The claim that lost on authority is the one that gets dropped.
            losers.push(if winner_id == l.id { r.id } else { l.id });
        }
    }

    let kept: Vec<Claim> = claims
        .into_iter()
        .filter(|c| !losers.contains(&c.id))
        .collect();
    (kept, out)
}

fn is_authoritative(url: &str, claim: &str, question: &str) -> bool {
    let host = host_of(url).to_lowercase();
    if host.is_empty() {
        return false;
    }
    // A source is authoritative when its host carries the entity the run is
    // about. The claim alone is not enough — a claim about "the API" names no
    // vendor — so the question that started the run supplies the subject too.
    let entities: Vec<String> = crate::verify::extract_facts(question)
        .into_iter()
        .chain(crate::verify::extract_facts(claim))
        .filter(|f| {
            matches!(
                f.kind,
                crate::verify::FactKind::ProperNoun | crate::verify::FactKind::Acronym
            )
        })
        .map(|f| f.raw.to_lowercase())
        .filter(|e| e.len() >= 4 && !GENERIC_ENTITIES.contains(&e.as_str()))
        .collect();
    entities.iter().any(|e| host.contains(e))
}

const GENERIC_ENTITIES: [&str; 8] = ["api", "http", "https", "docs", "blog", "www", "com", "test"];

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| url.to_string())
}

fn dedupe_claims(claims: Vec<Claim>) -> Vec<Claim> {
    let mut kept: Vec<Claim> = Vec::new();
    for c in claims {
        let duplicate = kept.iter().any(|k| {
            if k.source_id != c.source_id {
                return false;
            }
            match (k.span, c.span) {
                // Same source, overlapping evidence: the same fact restated.
                (Some(a), Some(b)) => a.start < b.end && b.start < a.end,
                // No verified span, so fall back to wording overlap.
                _ => similar(&k.text, &c.text),
            }
        });
        if !duplicate {
            kept.push(c);
        }
    }
    kept
}

/// Word-overlap similarity on content words, used only when a claim has no
/// verified span to compare. A claim supported by a real quote is judged on its
/// evidence, not its phrasing.
fn similar(a: &str, b: &str) -> bool {
    let words = |t: &str| -> std::collections::BTreeSet<String> {
        canonical(t)
            .canon
            .split(' ')
            .filter(|w| w.len() > 3 && !GENERIC.contains(w))
            .map(str::to_string)
            .collect()
    };
    let wa = words(a);
    let wb = words(b);
    let inter = wa.intersection(&wb).count();
    let union = wa.union(&wb).count();
    union > 0 && (inter as f64 / union as f64) >= 0.6
}

const GENERIC: [&str; 12] = [
    "that", "this", "with", "from", "which", "their", "there", "when", "will", "they", "been",
    "also",
];

fn source_url(sources: &[Source], id: Uuid) -> String {
    sources
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.url.clone())
        .unwrap_or_default()
}

fn mode_of(s: &str) -> RetrievalMode {
    match s {
        "live" => RetrievalMode::Live,
        _ => RetrievalMode::Corpus,
    }
}

fn mode_of_events(events: &[Event]) -> Mode {
    match events
        .iter()
        .find(|e| e.kind == "run.started")
        .and_then(|e| e.body.get("mode").cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .as_deref()
    {
        Some("naive") => Mode::Naive,
        _ => Mode::Verified,
    }
}

fn session_of(run_id: Uuid) -> String {
    format!("verbatim-{run_id}")
}

/// Drops empty and duplicate sub-questions, and guarantees at least one.
fn sanitize_plan(plan: Plan, question: &str) -> Plan {
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<SubQuestion> = Vec::new();
    for (i, mut s) in plan.sub_questions.into_iter().enumerate() {
        let q = s.question.trim().to_string();
        if q.is_empty() {
            continue;
        }
        let key = q.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        s.question = q;
        if s.id.trim().is_empty() {
            s.id = format!("q{i}");
        }
        out.push(s);
        if out.len() >= 6 {
            break;
        }
    }
    if out.is_empty() {
        out.push(SubQuestion {
            id: "q0".into(),
            question: question.to_string(),
        });
    }
    Plan { sub_questions: out }
}

fn fallback_plan_json(question: &str) -> String {
    json!({
        "sub_questions": [
            { "id": "q1", "question": format!("What is the direct answer to: {question}") },
            { "id": "q2", "question": format!("What evidence or sources support an answer to: {question}") },
        ]
    })
    .to_string()
}

const PLAN_SYSTEM: &str = r#"You are the planner for a research agent.

Break the user's question into the smallest set of independent sub-questions
that, answered together, fully answer the original. Each sub-question must be
answerable from public web sources.

Rules:
- At most 5 sub-questions.
- No sub-question may restate another.
- Cover the question's scope, not a generic background survey.
- Output JSON only, no prose.

Output shape:
{"sub_questions": [{"id": "q1", "question": "..."}, {"id": "q2", "question": "..."}]}"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::ReplayLlm;
    use crate::retrieval::CorpusDoc;

    fn test_agent() -> Agent {
        let docs = vec![CorpusDoc {
            url: "https://a.test/one".into(),
            title: Some("One".into()),
            http_status: 200,
            text: "The API allows 100 requests per minute on the free tier.".into(),
            snapshotted_at: "2026-10-10T00:00:00Z".into(),
        }];
        let r = Retrieval::with_corpus(docs, "/tmp/opencode/verbatim-test-corpus");
        Agent::for_test(Arc::new(ReplayLlm::new(vec![])), r).unwrap()
    }

    #[tokio::test]
    async fn a_run_with_no_sources_compiles_an_empty_honest_artifact() {
        let a = test_agent();
        let res = a.run("What is the rate limit?", Mode::Verified).await.unwrap();
        assert_eq!(res.artifact.stats.claims_checked, 0);
        assert_eq!(res.artifact.claims.len(), 0);
        // The headline metric is honest even when it is zero.
        assert_eq!(res.artifact.stats.verified_fraction(), 0.0);
    }

    #[tokio::test]
    async fn events_are_recorded_for_a_run() {
        let a = test_agent();
        let res = a.run("Anything?", Mode::Verified).await.unwrap();
        let evs = a.store.events(res.run_id).unwrap();
        assert!(evs.iter().any(|e| e.kind == "run.started"));
        assert!(evs.iter().any(|e| e.kind == "step.plan.completed"));
        assert!(evs.iter().any(|e| e.kind == "run.compiled"));
    }

    #[tokio::test]
    async fn resume_of_a_finished_run_returns_the_same_artifact() {
        let a = test_agent();
        let first = a.run("Question?", Mode::Verified).await.unwrap();
        let second = a.execute(first.run_id).await.unwrap();
        assert_eq!(
            first.artifact.claims.len(),
            second.artifact.claims.len(),
            "a resumed finished run must not redo work"
        );
    }

    #[test]
    fn plan_is_sanitized() {
        let p = sanitize_plan(
            Plan {
                sub_questions: vec![
                    SubQuestion { id: "q1".into(), question: "  ".into() },
                    SubQuestion { id: "".into(), question: "Same".into() },
                    SubQuestion { id: "q3".into(), question: "same".into() },
                    SubQuestion { id: "q4".into(), question: "Other".into() },
                ],
            },
            "fallback",
        );
        assert_eq!(p.sub_questions.len(), 2);
        assert_eq!(p.sub_questions[0].id, "q1");
    }

    #[test]
    fn extract_json_handles_fences_and_prose() {
        assert!(crate::llm::extract_json("```json\n{\"a\":1}\n```").is_some());
        assert!(crate::llm::extract_json("sure: {\"a\":1} done").is_some());
        assert!(crate::llm::extract_json("nope").is_none());
    }
}
