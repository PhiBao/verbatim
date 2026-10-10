"use client";

import { useCallback, useEffect, useRef, useState } from "react";

/** One verified claim, exactly as the engine reports it. */
type Claim = {
  id: string;
  text: string;
  source_id: string;
  quote: string;
  verdict: Verdict;
  score: number;
  reason: string;
  next_action: unknown;
  span: { start: number; end: number } | null;
  corroborators: number;
};

type Verdict =
  | "supported"
  | "contradicted"
  | "unsupported"
  | "conflict"
  | "ungrounded"
  | "unverifiable";

type Source = {
  id: string;
  url: string;
  title: string | null;
  http_status: number;
  bytes: number;
  sha256: string;
  dead: boolean;
  dead_reason: string | null;
};

type Stats = {
  claims_checked: number;
  verified: number;
  contradicted: number;
  unsupported: number;
  ungrounded: number;
  conflicts: number;
  unverifiable: number;
  sources_fetched: number;
  sources_dead: number;
};

type Artifact = {
  question: string;
  answer_md: string;
  claims: Claim[];
  sources: Source[];
  unverified: Claim[];
  stats: Stats;
};

type RunState = {
  runId: string | null;
  status: "idle" | "running" | "done" | "failed";
  error: string | null;
  mode: "verified" | "naive";
};

const VERDICT_LABEL: Record<Verdict, string> = {
  supported: "verified",
  contradicted: "contradicted",
  unsupported: "unsupported",
  conflict: "conflict",
  ungrounded: "ungrounded",
  unverifiable: "unverifiable",
};

export default function Home() {
  const [question, setQuestion] = useState(
    "What rate limits does the Anthropic Messages API enforce, and how are they measured?"
  );
  const [mode, setMode] = useState<"verified" | "naive">("verified");
  const [run, setRun] = useState<RunState>({
    runId: null,
    status: "idle",
    error: null,
    mode: "verified",
  });
  const [artifact, setArtifact] = useState<Artifact | null>(null);
  const [struck, setStruck] = useState<string | null>(null);
  const poll = useRef<number | null>(null);

  const load = useCallback(async (runId: string) => {
    try {
      const res = await fetch(`/api/runs/${runId}`);
      if (!res.ok) return;
      const body = await res.json();
      if (body.artifact) {
        setArtifact(body.artifact);
        setRun((r) => ({ ...r, status: "done" }));
        if (poll.current) window.clearInterval(poll.current);
        poll.current = null;
      }
    } catch {
      /* a transient fetch failure must not break the reading surface */
    }
  }, []);

  const submit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!question.trim()) return;
      setArtifact(null);
      setStruck(null);
      setRun({ runId: null, status: "running", error: null, mode });
      try {
        const res = await fetch("/api/runs", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ question, mode }),
        });
        const body = await res.json();
        if (!res.ok) throw new Error(body.detail ?? body.error ?? "run failed");
        setRun({ runId: body.run_id, status: "running", error: null, mode });
        poll.current = window.setInterval(() => load(body.run_id), 1500);
      } catch (err) {
        setRun({
          runId: null,
          status: "failed",
          error: err instanceof Error ? err.message : String(err),
          mode,
        });
      }
    },
    [question, mode, load]
  );

  useEffect(() => () => {
    if (poll.current) window.clearInterval(poll.current);
  }, []);

  // The one moment that matters: a claim that flips to CONTRADICTED.
  useEffect(() => {
    if (!artifact) return;
    const bad = artifact.unverified.find((c) => c.verdict === "contradicted");
    if (bad) {
      setStruck(bad.id);
      const t = window.setTimeout(() => setStruck(null), 1200);
      return () => window.clearTimeout(t);
    }
  }, [artifact]);

  const stats = artifact?.stats;
  const pct = stats && stats.claims_checked > 0
    ? Math.round((stats.verified / stats.claims_checked) * 100)
    : 0;

  return (
    <main className="shell">
      <header className="masthead">
        <a className="wordmark" href="/">
          <span className="mark" aria-hidden="true">
            &ldquo;
          </span>
          Verbatim
        </a>
        <span className="tagline">If we didn&rsquo;t fetch it, we didn&rsquo;t say it.</span>
        <nav>
          <a href="/audit">Audit an answer</a>
          <a href="https://github.com/PhiBao/verbatim">Source</a>
        </nav>
      </header>

      {!artifact && (
        <section className="ask">
          <h1>
            Research you can check.
            <br />
            Every sentence carries a receipt.
          </h1>
          <p className="lede">
            A research agent whose claims are verified by a program that did not write
            them. If a fact appears in no source we fetched, it cannot ship &mdash;
            even when it happens to be true.
          </p>
          <form className="ask-form" onSubmit={submit}>
            <label className="sr-only" htmlFor="q">
              Question
            </label>
            <textarea
              id="q"
              value={question}
              onChange={(e) => setQuestion(e.target.value)}
              rows={3}
            />
            <div className="ask-row">
              <button type="submit" disabled={run.status === "running"}>
                {run.status === "running" ? "Researching" : "Research"}
              </button>
              <div
                className="mode-toggle"
                role="group"
                aria-label="Run mode"
              >
                <button
                  type="button"
                  aria-pressed={mode === "verified"}
                  onClick={() => setMode("verified")}
                >
                  verified
                </button>
                <button
                  type="button"
                  aria-pressed={mode === "naive"}
                  onClick={() => setMode("naive")
                  }
                >
                  control
                </button>
              </div>
              <span className="footnote">
                Control run: same retrieval, same model, no verification gate.
              </span>
            </div>
          </form>
          {run.status === "failed" && (
            <p className="footnote" style={{ color: "var(--contradicted)" }}>
              {run.error}
            </p>
          )}
        </section>
      )}

      {run.status === "running" && (
        <section className="ask">
          <div className="runbar">
            <div className="row">
              <span>
                <span className="spinner" aria-hidden="true" /> running
              </span>
              <span className="val">{mode}</span>
            </div>
            <div className="row">
              <span>plan &rarr; retrieve &rarr; claim &rarr; verify &rarr; compile</span>
              <span className="val">deterministic verifier</span>
            </div>
          </div>
          <p className="footnote">
            The verifier takes no model. It re-reads the cited source and checks the
            quote is actually in it, byte for byte after normalisation.
          </p>
        </section>
      )}

      {artifact && (
        <div className="document">
          <article className="answer">
            <h2>{artifact.question}</h2>

            {artifact.claims.length === 0 ? (
              <p className="empty-note">
                No claim in this run was backed by a source we could read. Rather than
                write an answer anyway, here is what we could not establish.
              </p>
            ) : (
              artifact.claims.map((c, i) => (
                <p
                  key={c.id}
                  className={`claim${struck === c.id ? " is-struck" : ""}`}
                  data-verdict={c.verdict}
                >
                  <span className="text">{c.text}</span>{" "}
                  <a className="badge" href={`#receipt-${c.id}`}>
                    [{short(c.id)}]
                  </a>
                  <sup style={{ display: "none" }}>{i + 1}</sup>
                </p>
              ))
            )}

            {artifact.unverified.length > 0 && (
              <section className="unverified">
                <h3>We could not verify</h3>
                <ul>
                  {artifact.unverified.map((c) => (
                    <li key={c.id}>
                      {c.text}
                      <span className="why">
                        {VERDICT_LABEL[c.verdict]} &mdash; {c.reason}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            )}

            <section className="ledger">
              <h3>Verification</h3>
              <dl>
                <dt>claims checked</dt>
                <dd>{stats?.claims_checked ?? 0}</dd>
                <dt>verified</dt>
                <dd>
                  {stats?.verified ?? 0}
                  {stats && stats.claims_checked > 0 ? ` (${pct}%)` : ""}
                </dd>
                <dt>contradicted</dt>
                <dd>{stats?.contradicted ?? 0}</dd>
                <dt>unsupported</dt>
                <dd>{stats?.unsupported ?? 0}</dd>
                <dt>ungrounded</dt>
                <dd>{stats?.ungrounded ?? 0}</dd>
                <dt>unverifiable</dt>
                <dd>{stats?.unverifiable ?? 0}</dd>
                <dt>sources read</dt>
                <dd>
                  {stats?.sources_fetched ?? 0}
                  {stats && stats.sources_dead > 0
                    ? ` (${stats.sources_dead} unavailable)`
                    : ""}
                </dd>
              </dl>
              <button
                className="ghost copy-btn"
                onClick={() => navigator.clipboard.writeText(window.location.href)}
              >
                Copy receipt link
              </button>
            </section>

            <section className="sources">
              <h3>Sources read</h3>
              <ul>
                {artifact.sources.map((s) => (
                  <li key={s.id} className={s.dead ? "dead" : ""}>
                    {s.url} {s.dead ? `— ${s.dead_reason ?? "unavailable"}` : `— ${s.bytes} bytes`}
                  </li>
                ))}
              </ul>
            </section>

            <p style={{ marginTop: "2.5rem" }}>
              <button className="ghost" onClick={() => setArtifact(null)}>
                Ask another question
              </button>
            </p>
          </article>

          <aside className="evidence" aria-label="Evidence">
            {artifact.claims.map((c) => {
              const src = artifact.sources.find((s) => s.id === c.source_id);
              return (
                <div
                  key={c.id}
                  id={`receipt-${c.id}`}
                  className="receipt"
                  data-verdict={c.verdict}
                >
                  <div className="verdict">{VERDICT_LABEL[c.verdict]}</div>
                  <blockquote>{c.quote}</blockquote>
                  <div className="src">{src?.url ?? "unknown source"}</div>
                  {c.span && (
                    <div className="src">
                      bytes {c.span.start}&ndash;{c.span.end} &middot; sha256{" "}
                      {src?.sha256.slice(0, 12)}
                    </div>
                  )}
                  <div className="src">score {c.score.toFixed(2)}</div>
                </div>
              );
            })}
          </aside>
        </div>
      )}
    </main>
  );
}

function short(id: string): string {
  return id.slice(0, 8);
}
