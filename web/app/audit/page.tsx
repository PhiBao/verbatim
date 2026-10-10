"use client";

import { useState } from "react";

type AuditClaim = {
  text: string;
  verdict: string;
  source_url: string | null;
  span: { start: number; end: number } | null;
  missing: string[];
  facts: string[];
};

type AuditReport = {
  claims: AuditClaim[];
  stats: Record<string, number>;
  sources: { url: string; title: string | null; sha256: string; bytes: number }[];
  note?: string;
  method?: string;
};

/**
 * Audit an answer that came from somewhere else.
 *
 * This is the ten-second demo: paste a Perplexity or ChatGPT answer with its
 * sources, and get the same per-claim ledger. Verbatim is not competing with
 * those tools; it is checking them.
 */
export default function AuditPage() {
  const [answer, setAnswer] = useState(
    "The Anthropic API free tier allows 100 requests per minute. Claude Sonnet has a 200,000 token context window. Rate limits reset every 60 seconds."
  );
  const [sources, setSources] = useState(
    "https://docs.anthropic.com/en/api/rate-limits\nhttps://www.anthropic.com/pricing"
  );
  const [report, setReport] = useState<AuditReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setReport(null);
    try {
      const res = await fetch("/api/audit", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          answer,
          sources: sources
            .split("\n")
            .map((s) => s.trim())
            .filter(Boolean),
        }),
      });
      const body = await res.json();
      if (!res.ok) throw new Error(body.detail ?? body.error ?? "audit failed");
      setReport(body);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  }

  const stats = report?.stats ?? {};
  const checked = Number(stats.claims_checked ?? 0);
  const verified = Number(stats.verified ?? 0);

  return (
    <main className="shell">
      <header className="masthead">
        <a className="wordmark" href="/">
          <span className="mark" aria-hidden="true">
            &ldquo;
          </span>
          Verbatim
        </a>
        <span className="tagline">Audit an answer that came from somewhere else.</span>
        <nav>
          <a href="/">Ask a question</a>
        </nav>
      </header>

      <section className="ask">
        <h1>Check someone else&rsquo;s work.</h1>
        <p className="lede">
          Paste an answer with the sources it cites. Every distinctive fact in it must
          appear in a source we actually read, or it is flagged. A fact that appears in
          none was invented or recalled &mdash; we say which.
        </p>
        <form className="ask-form" onSubmit={submit}>
          <label htmlFor="answer">Answer</label>
          <textarea
            id="answer"
            rows={6}
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
          />
          <label htmlFor="sources">Sources, one URL per line</label>
          <textarea
            id="sources"
            rows={4}
            value={sources}
            onChange={(e) => setSources(e.target.value)}
            style={{ fontFamily: "var(--mono)", fontSize: "0.8125rem" }}
          />
          <div className="ask-row">
            <button type="submit" disabled={busy}>
              {busy ? "Reading sources" : "Audit"}
            </button>
          </div>
        </form>
        {error && (
          <p className="footnote" style={{ color: "var(--contradicted)" }}>
            {error}
          </p>
        )}
      </section>

      {report && (
        <div className="document">
          <article className="answer">
            <h2>Audit result</h2>
            <section className="ledger">
              <h3>Grounding</h3>
              <dl>
                <dt>sentences checked</dt>
                <dd>{checked}</dd>
                <dt>fully grounded</dt>
                <dd>
                  {verified}
                  {checked > 0 ? ` (${Math.round((verified / checked) * 100)}%)` : ""}
                </dd>
                <dt>invented or recalled</dt>
                <dd>{Number(stats.ungrounded ?? 0)}</dd>
                <dt>partly grounded</dt>
                <dd>{Number(stats.unsupported ?? 0)}</dd>
              </dl>
              {report.method && (
                <p className="footnote" style={{ marginTop: "1rem" }}>
                  {report.method}
                </p>
              )}
            </section>

            <section className="unverified" style={{ marginTop: "2.5rem" }}>
              <h3>Per claim</h3>
              <ul>
                {report.claims.map((c, i) => (
                  <li key={i}>
                    <span className="text">{c.text}</span>
                    <span className="why">
                      {c.verdict}
                      {c.missing.length > 0
                        ? ` — no source mentions: ${c.missing.join(", ")}`
                        : " — every fact appears in a source we read"}
                    </span>
                    {c.source_url && (
                      <span className="why">
                        {c.source_url}
                        {c.span ? ` · bytes ${c.span.start}–${c.span.end}` : ""}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </section>

            <section className="sources" style={{ marginTop: "2.5rem" }}>
              <h3>Sources read</h3>
              <ul>
                {report.sources.map((s) => (
                  <li key={s.url}>
                    {s.url} — {s.bytes} bytes · sha256 {s.sha256.slice(0, 12)}
                  </li>
                ))}
              </ul>
            </section>
          </article>
          <aside className="evidence" aria-label="Findings" />
        </div>
      )}
    </main>
  );
}
