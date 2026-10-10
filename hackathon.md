# hackathon.md — neatHack build log

> Running evidence log for **Verbatim**. Maintained per the neatHack build-log
> skill: dated entries, planned vs completed work, tests that actually ran, and
> failures that remain. No secrets, no private traces.

- **Project:** Verbatim — a research agent where every sentence carries a receipt.
  If we didn't fetch it, we didn't say it.
- **Problem:** agents self-certify their own success. 3–13% of deep-research
  citation URLs never existed (arXiv 2604.03173); best factual F1 under clean-room
  eval is 0.337 (arXiv 2606.11337).
- **Team:** solo — PhiBao (kiter2509@gmail.com)
- **Event:** neatHack, Oct 10–12 2026, one track ("an agent that works, literally").
  Submission: Oct 12, 19:00–23:55 IST, quote-tweet of the official neatlogs post.
- **Stack:** Rust (Axum, rusqlite, Bedrock Converse) · Next.js + TypeScript ·
  neatlogs HTTP trace injection · Entire checkpoints · cfo.ai business model.

## Sponsor usage

| Tool | Role in the project | Evidence |
|---|---|---|
| **neatlogs** | One nested trace per run. Every verification check is a `GUARDRAIL` span with `passed`/`score`, so a failed verification is an error span in the dashboard. Naive and verified runs of the same question are diffed side by side. | trace ids recorded per run; dashboard project `neathack` |
| **Entire** | Enabled from the first commit. The agent commits `answer.md` + `claims.json`, so `entire why answer.md:34` reveals the session behind any sentence. | checkpoint link + ID, recorded below |
| **cfo.ai** | Unit economics for the agent: price per verified report, inference cost per report, gross margin, Base/Growth scenarios, runway. | shared scenario link + screenshots |

## Build log

### 2026-10-10

**14:20 IST — foundation (completed)**
- Rust workspace created: `crates/engine` (Axum service + agent core) and
  `crates/cli` (`verbatim`, the agent-facing CLI).
- Dependency spike on `aws-sdk-bedrockruntime` resolved; the LLM sits behind an
  `Llm` trait so a provider swap is a one-file change.

**Verifier v1 (completed, 26 tests passing)**
`cargo test -p engine --lib` → 26 passed, 0 failed.

- `text.rs` — canonical text form with a per-character byte map back to the
  original, so receipts highlight the real passage. Handles smart quotes, NBSP,
  HTML entities, soft-hyphenation, line-break hyphenation, combining marks and
  punctuation-as-separator (`req/min` ≡ `req per minute`).
- `verify.rs` — five deterministic checks, **no LLM in any of them**:
  1. source available
  2. quote containment (byte-exact after canonicalisation)
  3. entity grounding against the cited source
  4. polarity / value substitution
  5. prior leak — the claim's facts appear in *no* fetched source
- Verdicts: `supported`, `contradicted`, `unsupported`, `conflict`,
  `ungrounded`, `unverifiable`. Every verdict carries a `next_action`, so the
  repairer is a control loop rather than a retry.

**Known failures and how they were fixed (recorded honestly)**
- First cut put the prior-leak check *before* quote containment, so a claim whose
  number appeared nowhere was reported as `ungrounded` even when the source was
  clearly talking about the same quantity. Reordered: containment first, then
  diagnose *why* it failed.
- Offsets were reported as `raw.len()` when a match ran to the end of the source,
  which highlighted trailing punctuation. Fixed by mapping per-character byte
  ranges instead of a single index map.
- Entity grounding was checked against the quoted sentence only, which rejected
  legitimate claims whose subject sits outside the excerpt. Now checked against
  the cited source.

**In progress**
- Retrieval (search + SSRF-guarded fetch, with a pinned corpus for determinism).
- The agent loop: planner, claimer, repairer, compiler.
- neatlogs emitter wiring.
- Next.js reading surface.

## Limitations (current)
- No retrieval or LLM calls yet; the verifier is exercised by unit tests only.
- Corpus mode is the default until live search is wired.

## Next steps
1. Retrieval + seeded corpus.
2. Agent loop with budget enforcement and checkpoint/resume.
3. Frontend reading surface and receipts margin.
4. Demo, video, cfo.ai model, submission.
