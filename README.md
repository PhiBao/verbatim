# Verbatim

### If we didn't fetch it, we didn't say it.

A research agent where **every sentence carries a receipt**. Each claim is checked
by a deterministic verifier that did not write it: the verifier re-reads the cited
source and confirms the quoted evidence is actually in it. Claims that fail are
struck and re-researched. Claims that cannot be verified are named as unverified
instead of being asserted.

Built for [neatHack](https://neatlogs.com/hackathon) · Oct 10–12 2026 · solo.

---

## The problem, in one paragraph

Agents now self-certify. The evidence that this is broken is unambiguous and
recent:

- **Rao, Wong & Callison-Burch** (arXiv 2604.03173) measured **3–13% of citation
  URLs that likely never existed** across 53,090 and 168,021 citations, and found
  deep research agents hallucinate at *higher* rates than plain search-augmented
  LLMs — the deeper they research, the worse it gets.
- **SciConBench** (arXiv 2606.11337) puts the best deep-research agent at a
  factual **F1 of 0.337** under clean-room evaluation, and finds consumer agents
  producing contradictory conclusions even with ground truth available.
- **OpenAI** retired SWE-bench Verified in February 2026 after finding **59.4% of
  audited tasks had tests that reject correct solutions**, and that every frontier
  model could reproduce the gold patch verbatim.

Every one of those failures is a *verification* failure. An agent that grades its
own work is not measuring its own work.

## The idea

Verbatim inverts the pipeline. The step that decides whether the agent is right
contains **no language model at all**:

```
plan → retrieve → claim → VERIFY → repair → compile
                              ↑
                    five deterministic checks,
                    no LLM in any of them
```

The check that has no equivalent anywhere else is **prior leak**: if a claim's
distinctive facts appear in *no* source we actually fetched, it came from the
model's memory rather than from this run — and it cannot ship, **even when it
happens to be true**. Truth is not the standard. Evidence is.

### The five checks

| # | Check | Catches | Verdict |
|---|---|---|---|
| 1 | **Source available** — the cited source was fetched and is not dead | unreadable links | `unverifiable` |
| 2 | **Quote containment** — the quoted passage is in the cited source, byte-exact after canonicalisation | **fabricated quotes** | `unsupported` |
| 3 | **Entity grounding** — every number, version, date, acronym and proper noun in the claim is in the cited source | paraphrase drift | `unsupported` |
| 4 | **Polarity / value substitution** — the source isn't saying the opposite, or the same quantity with a different number | confident inversion | `contradicted` |
| 5 | **Prior leak** — the claim's facts appear in **no** fetched source | asserting from memory | `ungrounded` |

Canonicalisation handles what a raw substring test cannot: smart quotes, NBSP,
HTML entities, soft and line-break hyphenation, Unicode composition, and
punctuation as a separator — so `req/min`, `requests per minute` and
`requests-per-minute` all compare equal. The canonical form carries a
per-character byte map back to the original, so a match is reported as real byte
offsets into the text a human is reading. A receipt that highlights the wrong
passage is worse than no receipt.

**Every verdict carries a `next_action`.** A verdict is not an opinion, it is an
instruction: `re_retrieve` with a suggested query, `declare_unverifiable` with a
reason, `resolve_conflict` with a trust rule, or `drop`. Repair is a control loop,
not a retry.

### The compiler is the product's conscience

`compile_artifact` **structurally cannot** emit a claim whose verdict is not
`SUPPORTED`. Everything else lands in a named *"We could not verify"* section
with its reason. The headline metric is honest by construction — it can be zero.

---

## The control run, and why it is the real demo

`naive` mode performs the **same retrieval and the same verification**, but its
compiler ships every drafted claim regardless of verdict — which is what a
research agent does today. It has the information and ignores it.

The only difference between the two runs is **whether the compiler is allowed to
ship an unverified claim.** That is the whole experiment, and it is the honest
version of the A/B: same model, same sources, same budget, one changed decision.

A run on a real conflict:

> **How many requests per minute does the Anthropic free tier allow, and does the
> rate limit reset every minute?**
>
> Sources disagree: a third-party blog says **500**; Anthropic's own docs say
> **60**. Both quotes are verbatim, so the quote check passes on both. The
> conflict detector catches the disagreement, resolves it with a recorded rule —
> *the source whose host carries the entity the run is about wins, so
> `docs.anthropic.com` beats a blog about Anthropic* — drops the losing claim, and
> **prints the disagreement in the answer** rather than silently deciding it.

---

## Architecture

```
                    ┌──────────────────────────────────────────┐
   ask / audit ───▶ │  Next.js reading surface                 │
                    │  answer as a document · receipts margin   │
                    └───────────────────┬──────────────────────┘
                                        │ /api/* (single origin)
                    ┌───────────────────▼──────────────────────┐
                    │        Rust · Axum · rusqlite             │
                    │                                           │
                    │  planner ─▶ research loop                 │
                    │     search ─ fetch ─ claim ─ verify ─ repair│
                    │                    │                       │
                    │           ┌────────▼──────────┐            │
                    │           │  VERIFIER (no LLM)│  ★ the core │
                    │           └────────┬──────────┘            │
                    │                    ▼                        │
                    │  answer compiler (refuses unverified)      │
                    │                                           │
                    │  append-only event log │ effects ledger    │
                    │  (the only source of    │ (exactly-once,   │
                    │   truth; UI is a         │  settle-then-    │
                    │   projection)            │  commit)         │
                    └───────────────────┬──────────────────────┘
                                        │
              neatlogs POST /v1/trace ──┤── Entire checkpoints per commit
              (WORKFLOW/AGENT/LLM/     │
               RETRIEVER/TOOL/GUARDRAIL)│── cfo.ai unit economics
```

**Backend: Rust** (Axum, Tokio, rusqlite, reqwest). Why: the verifier is the
product's core claim, and Rust is the right language for a component whose
correctness is the argument. The whole crate has no `unsafe`.

**Frontend: Next.js + TypeScript.** Why: it serves a reading surface, and a
reading surface is a web page.

### Durability

An **append-only event log is the only source of truth**; run state, the timeline
and the artifact are projections of it. Side effects settle-then-commit under
idempotency keys. A hard `kill -9` before an effect settles leaves no record of
it, so a resumed run redoes the work rather than guessing — and a redo is a no-op
at the tool boundary, so the effects ledger proves it repeated nothing.

This is the pattern that makes "state across steps, runs and failures" real rather
than claimed: **model-visible means logged, and user-visible means logged.**

---

## Hackathon relevance

**Track:** *Build an agent that works (literally)* — one track, every project
competes for the same prizes.

**Judging criteria and how this project answers them:**

| Criterion | Weight | How |
|---|---|---|
| The agent | 30% | plan → retrieve → claim → verify → repair → compile, end to end, with recovery driven by verdicts rather than retries |
| Use of neatlogs, Entire, cfo.ai | 25% | each is structurally necessary, below |
| Demo video | 20% | one memorable beat: a claim that gets struck |
| Building in public | 15% | traced build sessions, dated build log, conflict the verifier caught |
| Usefulness & originality | 10% | audit mode checks *other* agents' answers |

### The three partners, each with a real role

**neatlogs** — one nested JSON trace per run posted to `POST /v1/trace`
(documentation explicitly supports Rust via this path; there is no Rust SDK).
Span kinds are chosen so the dashboard tells the product's story without
narration: `WORKFLOW` root → `AGENT` (planner, repairer) → `RETRIEVER` (query +
documents) → `TOOL` (fetch) → `LLM` (model + tokens) → and, crucially,
**`GUARDRAIL` for every verification check with `passed` and `score`**. A failed
verification is natively an error span in neatlogs, so the argument of the product
is visible in someone else's tool. Naive and verified runs share a
`neatlogs.session.id` and are diffed side by side.

**Entire** — enabled from the first commit. The agent commits its artifacts
(`answer.md`, `claims.json`), so **`entire why answer.md:34`** reveals the session
behind any sentence: provenance of code, meeting provenance of claims.

**cfo.ai** — unit economics for the agent: price per verified report, inference
cost per report, gross margin, Base/Growth scenarios, runway.

---

## Running

### Prerequisites (for local development)

- Rust (stable), Node 22+, pnpm
- A model endpoint (the hackathon's LiteLLM gateway, or AWS Bedrock)
- Optional: a neatlogs write key

### Install

```bash
git clone https://github.com/PhiBao/verbatim
cd verbatim
cargo build --release --workspace
cd web && pnpm install && cd ..
```

### Configure

Copy `.env.example` to `.env` and fill it in. `.env` is gitignored and never
committed.

| Variable | Required | Meaning |
|---|---|---|
| `LITELLM_BASE_URL` / `LITELLM_API_KEY` / `LITELLM_MODEL` | one model | the hackathon gateway, or Bedrock |
| `NEATLOGS_API_KEY` | for traces | project key from app.neatlogs.com |
| `NEATLOGS_PROJECT` | for traces | project **name** — a write key identifies a user, not a project |
| `VERBATIM_RETRIEVAL` | default `corpus` | `corpus` = pinned snapshot; `live` = real web retrieval |

### Commands

```bash
# The API and the reading surface
cargo run --release --bin engine          # http://127.0.0.1:8787
cd web && pnpm dev                        # http://localhost:3000, proxies /api to the engine

# Check one claim against one source — no model involved
verbatim verify \
  --claim "The Anthropic Messages API rate limits are measured in requests per minute, input tokens per minute, and output tokens per minute." \
  --quote "The rate limits for the Messages API are measured in requests per minute (RPM), input tokens per minute (ITPM), and output tokens per minute (OTPM) for each model class." \
  --source https://docs.anthropic.com/en/api/rate-limits

# Audit an answer that came from somewhere else
verbatim audit --answer "..." --source https://example.test/a

# See the plan without spending anything
verbatim run "what are the Anthropic rate limits?" --dry-run
```

### Test

```bash
cargo test --workspace      # 39 tests
cargo clippy --workspace    # warnings only
```

The verifier's tests are the ones that matter, because they are the ones that
would let a stranger catch us lying. They include the cases a naive
implementation gets wrong: a quote with one number changed, a quote with smart
quotes, a soft hyphenation, a dead source, and a claim whose facts appear in no
fetched source.

---

## Demo

**Live:** https://verbatim-neathack.fly.dev

A real run against the live web, unaided:

| | |
|---|---|
| claims checked | **57** |
| verified | **47** |
| contradicted | **2** |
| unsupported | **8** |
| sources read | **39** |
| runs | verified and control, same question |
| traces ingested | **29**, zero rejected |
| side effects | **39**, zero duplicated on resume |

The 10 rejected claims are the product working. Eight were paraphrased rather
than quoted; two stated a different value than the source. A human reading the
same 39 pages would almost certainly have shipped at least one of them.

1. **Ask** a question where sources disagree.
2. **Watch** the timeline: plan → searches → fetches → claims → verdicts.
3. **The beat:** a confident claim flips to `CONTRADICTED` and strikes through.
4. **The receipts page:** click any sentence → the quote, the byte offsets, the
   SHA-256 of the source, and the score.
5. **Audit mode:** paste someone else's answer with its sources and get the same
   ledger. Verbatim does not compete with deep-research tools; it checks them.

### Reproducibility

The demo runs against a **pinned corpus** — a snapshot of real pages fetched
during the hackathon by `engine snapshot`, stored with their SHA-256. A recorded
run replays byte-for-byte and cannot break because a website changed. `live` mode
is what production uses. Both are labelled in the UI and stated here.

---

## Verification — what is real, what is not

**Real, live, and externally verifiable:**
- The verifier. Five deterministic checks, no model. `verbatim verify` runs the
  same code path the agent runs, so anyone can reproduce any verdict from a
  terminal.
- Traces. 29 real traces ingested from the deployed process, including
  `GUARDRAIL` spans for every failed check. Visible in the public neatlogs
  project.
- The 57-claim run above, reproducible against live Tavily retrieval.
- The control run. Same retrieval, same model, same budget; only the compile rule
  differs.
- Cost accounting. Measured per run against the gateway's own usage.
- Conflict detection and resolution, with the deciding rule recorded.

**Deterministic by design, and labelled as such:**
- The pinned corpus. Real fetches, snapshotted during the hackathon.

**Not built, and not claimed:**
- Live retrieval depends on Tavily and the LiteLLM gateway. When live search
  returns nothing usable, the run falls back to the pinned corpus and says so in
  the trace, rather than failing or silently shrinking.
- Audit mode checks **grounding**, not **support**: an arbitrary answer carries no
  quotes, so the strongest deterministic check available is whether each
  sentence's facts appear in a source we read. We do not claim to verify support
  without a quote to check.
- No multi-agent crew. A single agent plus an independent verifier is the thesis;
  multi-agent has documented failure modes and would dilute it.

---

## Security

- **SSRF.** The agent chooses its own URLs, so every fetch is untrusted. Requests
  are confined to public `http(s)` on ports 80/443: private, loopback, link-local,
  multicast, unspecified and **bracketed IPv6 literals** are rejected, every
  redirect hop is re-validated, the body is capped at 2 MB, and script/style/svg
  content is stripped before matching.
- **Prompt injection.** Fetched content is untrusted and reaches the model. It is
  never treated as instructions, and a claim is only as good as the deterministic
  quote check against the raw bytes — so injected text that tries to look like
  evidence is measured against the source it claims to come from.
- **Secrets.** `.env` is gitignored. The neatlogs key is ingest-only (it can POST
  traces, never read them back). No secret ever reaches the repo, and trace
  bodies are written to `/tmp` on rejection, never to a log that ships.
- **Budget.** The model gateway enforces a $20 shared ceiling; the agent also
  enforces its own 2-cent-per-run ceiling so one runaway loop cannot spend the
  team's allowance.
- **Failure states.** A dead source is `unverifiable`, not silently absent. A
  budget breach stops the run and returns a *partial verified* answer with an
  explicit banner. Malformed model output is rejected, not guessed at.

---

## Limitations and what's next

- The naive control run is honest but narrow: it proves the compiler's gate
  matters, not that the retrieval is good.
- Authority resolution is one stateable rule. A richer model would weigh
  recency, corroboration count and primary-vs-secondary explicitly.
- Two-witness corroboration (two blind claimers, one adjudicator) is designed and
  not built.

---

## Build log

`hackathon.md` is the running, dated log with every failure and fix, including
the ones the tests missed: an SSRF hole that accepted bracketed IPv6 literals,
`<style>` content leaking into matching text, a `blocking_lock` panic inside a
tokio handler, and the 8 KB ingest ceiling that was found by bisecting a live
payload.
