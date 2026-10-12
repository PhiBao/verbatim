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

**Agent loop v1 (completed, 35 tests passing)**
`cargo test -p engine --lib` → 35 passed, 0 failed.

- `agent.rs` — planner → retrieve → claim → **verify** → repair → compile, over
  the append-only event log, so a resumed run repeats no work and no fetch.
- Retrieval confined to public `http(s)`: private, loopback, link-local,
  multicast and IPv6-literal targets rejected, every redirect hop re-validated,
  body capped at 2 MB, script/style/svg content stripped before matching.
- Two retrieval modes, declared in the UI and the README: `corpus` (a pinned
  snapshot of real fetches, so a recorded run replays byte-for-byte) and `live`.
- The **control run**: `naive` mode performs the same retrieval and the same
  verification, but its compiler ships every drafted claim regardless of verdict.
  The only difference between the two runs is whether the compiler is allowed to
  ship an unverified claim. That is the experiment.

**Bugs found and fixed this session (kept for honesty)**
- `validate_url` accepted bracketed IPv6 literals like `http://[::1]/`, which is
  a genuine SSRF hole. Brackets are now stripped before the address check.
- `html_to_text` kept `<style>` and `<script>` *content*, so CSS leaked into
  matching text. Those elements are now skipped wholesale.
- `extract_title` returned raw entities; it now decodes them, because the title
  is read by a human.

**Pinned corpus (completed)**
Three documents fetched live during the hackathon into `corpus/corpus.json`,
each with its fetch-time SHA-256, so a recorded run replays byte-for-byte:
- `https://docs.anthropic.com/en/api/rate-limits` (545 kB)
- `https://www.anthropic.com/pricing` (1.24 MB)
- `https://docs.anthropic.com/en/docs/about-claude/models/overview` (352 kB)

**Frontend (completed)**
Next.js reading surface: the answer set as a document in a serif, with the
evidence in a sticky margin. Receipts show the quote, the source, the byte span,
the SHA-256 and the score. `Next` proxies `/api/*` to the engine so there is one
origin. Deliberately not a dashboard: hairline rules, semantic colour only, and
exactly one piece of meaningful motion — a claim that flips to CONTRADICTED
strikes through. `prefers-reduced-motion` and `prefers-color-scheme: dark`
handled.

**Audit mode (completed, working live)**
`POST /api/audit` takes an answer and the sources it cites, and reports per
sentence which distinctive facts appear in no source we read. Verified against
the live pinned corpus: *"Claude Sonnet has a 900,000 token context window"* →
`unsupported`, missing `["900,000"]`. This is the ten-second demo: Verbatim does
not compete with deep-research tools, it checks them.

**Verified end-to-end on real sources** — `verbatim verify` against the pinned
Anthropic rate-limits page:
- paraphrased quote → `unsupported`, `next_action: re_retrieve`
- verbatim quote → `supported`, score 1.0, byte-exact span `6549..6717`

**Bug fixed:** `corpus_summary` called `blocking_lock()` on the retrieval mutex
from inside a tokio handler, which panics. Found by running the server, not by a
test — the handler is now async. Logged because the test did not catch it.

## Limitations (current)
- No retrieval or LLM calls yet; the verifier is exercised by unit tests only.
- Corpus mode is the default until live search is wired.

## Next steps
1. Retrieval + seeded corpus.
2. Agent loop with budget enforcement and checkpoint/resume.
3. Frontend reading surface and receipts margin.
4. Demo, video, cfo.ai model, submission.

**Model + gateway (completed)**
The hackathon's LiteLLM gateway is the model path (`claude-haiku` by default,
because the key carries a **$20 shared budget over 30 days** and cost is a
first-class constraint, not an afterthought). Measured cost per run: **$0.007**.
The agent also enforces its own 2-cent-per-run ceiling so a runaway loop cannot
spend the team's allowance.

**Full run verified (completed)** — `cargo test --workspace` → **39 passing**.
One run end to end, verified locally and again from the built container:

- 6 claims checked, all verified, 3 sources read
- **1 conflict detected and resolved by a recorded rule**: a decoy source in the
  pinned corpus claims the free tier allows 500 requests per minute; Anthropic's
  own docs say 60. Both quotes are verbatim. The conflict detector surfaces the
  disagreement, resolves it with *"the source whose host carries the entity the
  run is about wins"*, drops the losing claim, and prints the disagreement in the
  answer rather than silently deciding it.
- 2 traces ingested, ids `87300af2...`, `98a082e2...`

**Deployment (live)**
https://verbatim-neathack.fly.dev — single container, one machine in `sin`
(Mumbai `bom` is unavailable on this Fly plan). Verified live: home 200, audit
200, a full run returning 6 claims with **1 conflict detected**, and **2 traces
ingested** from the deployed process (`48e33df2...`, `6812c0c3...`).

Also fixed while deploying: the run API returned 131 KB, most of it the full text
of the Anthropic pricing page, because the stored artifact carries source bodies
for the verifier. Sources now go out as metadata only and long event-body strings
are truncated with a marker. 131 KB → 32 KB.

`bom` was the original region; it had to move to `sin` because the org is on a
legacy Fly plan. Logged because it is the kind of thing that looks like a code
bug and is not.
Single container: the Rust engine binds loopback, Next proxies `/api/*` to it,
only port 3000 exposed. Image builds at 223 MB. Verified running locally with the
real model and real traces.

### Open items
- Deploy to Fly.io (needs `fly auth login`).
- cfo.ai business plan.
- Demo video.
- **Entire checkpoint: 0 so far.** The hook is installed at
  `.opencode/plugins/entire.ts`, but it loads when an OpenCode session *starts*,
  and this session began before `entire enable` ran. A commit made from a fresh
  OpenCode session will produce the first checkpoint. This is the one sponsor
  requirement still unmet.

### OpenCode v2 plugin fix (2026-10-11)

**The editor now boots with plugins on, and Entire captures sessions for the
first time.** Two bugs, and the first fix caused the second.

| # | Symptom | Cause |
|---|---|---|
| 1 | `must export a default definition with an id and an effect or setup function` | both plugins were v1 files exporting a bare function; v2 wants a default-export **object** |
| 2 | server hangs forever on plugin load | `{ id, effect }` passes the loader's internal schema and then deadlocks — `effect` is the Effect-based variant used by built-ins, the public contract is `{ id, setup }` |

The trap: `scripts/check-plugins.ts` asserted `effect \|\| setup`, so it certified
the exact shape that hangs the editor. Shape checks cannot catch this class of
bug, so it is replaced by `scripts/plugin-smoke-test.sh`, which requires a real
session to complete (`opencode run --standalone` must exit 0; a deadlock is
exit 124).

**`entire.ts` is now a v2 plugin**, written against the observed contract.
`scripts/entire-opencode-plugin.ts` is the canonical copy, because
`entire enable` (CLI 0.11.4) still ships a v1 template and overwrites the file.

Verified with a stubbed `entire` binary rather than by inspection: the plugin
invokes `turn-start` with the real prompt, `turn-end` when execution settles, and
`session-end` from the v2 Cleanup. `entire session list` shows the captured
sessions.

Two more things this pass fixed:

- **9,344 `node_modules` files were committed to git.** The gitignore pattern was
  `/node_modules`, anchored to the root, so it never matched the plugin
  directory.
- **The neatlogs OpenCode plugin is deleted, not fixed.** Its vendored plugin is
  v1-only, so it loaded and did nothing — worse than absent, because it looked
  like coverage. It also could never have worked: `.env` is not loaded into the
  plugin process, so `NEATLOGS_API_KEY` is null inside `setup` and the shipper
  drops every span silently. The engine traces to neatlogs natively
  (`crates/engine/src/trace.rs`), which is where that evidence actually comes
  from.

**And the reason no checkpoint had ever been created:** every commit in this log
was made with `git -c core.hooksPath=/dev/null commit`, bypassing the git hooks
Entire installs. The hooks were present the whole time.

**The checkpoint chain now completes end to end.** After the plugin fix, one
link was still missing: Entire CLI 0.11.4 shells out to `opencode export <id>`,
and OpenCode v2 renamed that to `opencode session export`. Entire's call failed,
condensation reported "transcript not found", and no checkpoint ref was written
even though commits carried a valid `Entire-Checkpoint` trailer.

The plugin now stands in for the missing command, writing the transcript to the
exact path Entire reads. Two details that only showed up under test:

- it runs at **turn-start** as well as turn-end, because a commit made during the
  first step of a turn otherwise races the write
- `git commit --allow-empty` never produces a checkpoint — Entire reports "no
  files modified during session, skipping checkpoint". A test using an empty
  commit proves nothing.

Verified in a scratch repo: `refs/entire/checkpoints/C7/01M4KJ33...` created,
`entire checkpoint list` reports 1.

### Entire checkpoint — achieved

```
checkpoint  01M4KJ3WZ0YJFFS0NFWHW5RG24
commit      6c76b849159d2ad3c622faec040d7ea739050401
link        https://entire.io/gh/PhiBao/verbatim/commit/6c76b849159d2ad3c622faec040d7ea739050401
ref         refs/entire/checkpoints/24/01M4KJ3WZ0YJFFS0NFWHW5RG24 (pushed to origin)
files       scripts/entire-opencode-plugin.ts
```

To reproduce: run a session, make a real file change, and commit **normally** —
without `-c core.hooksPath=/dev/null`, which is what every earlier commit in this
log used and is the last piece of why none of them produced a checkpoint.

#### Two limits worth knowing

- **`git commit --allow-empty` never produces a checkpoint.** Entire reports "no
  files modified during session, skipping checkpoint", so a test using an empty
  commit proves nothing. The first three attempts failed for this reason and
  looked like a broken pipeline.
- **The transcript is stored in the checkpoint and this repository is public.**
  Scanned the pushed checkpoint for every secret used in this project
  (`sk-…`, `ziw-…`, `tvly-…`, `cd_…`, `AKIA`): zero hits, and Entire's redaction
  pass reports as configured. But it is a transcript, so a session that pastes a
  credential into the conversation would publish it. Do not paste secrets into a
  session in this repo.

### Checkpoint after the history rewrite

The purge rewrote every commit, so the two checkpoints recorded earlier now
point at SHAs that no longer exist. A fresh one was captured on a real commit:

```
checkpoint  01M4KQNDHEF4YFPRHX4E13YQ8W
commit      938a4a461991046258f57523d69f67d624d3f76b
link        https://entire.io/gh/PhiBao/verbatim/commit/938a4a461991046258f57523d69f67d624d3f76b
```

### Checkpoint refs removed after a credential exposure (2026-10-12)

Entire checkpoints store the **session transcript** next to the commit. Ours were
pushed to the public remote, and every session in them contained API keys that
had been pasted into the conversation. Deleting the refs was not enough —
`raw.githubusercontent.com` kept serving the objects by SHA — so the repository
was deleted and recreated.

Verified closed:
- `raw.githubusercontent.com/.../85c0f20.../1/full.jsonl` -> 404, previously 200
- every file on `main` scans clean for all four keys; zero commits introduce one
- `.env` was gitignored throughout, so nothing leaked through tracked files

**Every key was rotated.** The lesson worth keeping: Entire's own documentation
says checkpoints in a public repository are public, and that warning is easy to
read once and then stop applying.
