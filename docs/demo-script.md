# Demo video — script and shot list

**Hard limit: 3 minutes.** The brief requires the video to explicitly cover
neatlogs, Entire and cfo.ai — *architecture, implementation, runtime, reasoning
and result for each*. It also requires the demo to cover all three.

Recorded from https://verbatim-neathack.fly.dev. Every number quoted below is a
real run that already happened, so nothing needs to go right on camera.

---

## 0:00–0:18 — the hook

**On screen:** the ask page. One line of the answer page beside it.

> Deep research agents hallucinate between three and thirteen percent of their
> citations — and they get *worse* the deeper they research. That's measured,
> not asserted: 221,000 citation URLs, published this April.
>
> So I built the opposite of a research agent. **Verbatim.** If we didn't fetch
> it, we didn't say it.

**Beat:** land on the rule. It is the whole product in six words.

## 0:18–0:50 — the problem is visible in one click

**On screen:** paste a confident answer into `/audit`, sources named.

> Watch this. This is a paragraph I wrote — it sounds completely plausible.
> One fact in it appears in none of the sources I gave it. Verbatim doesn't
> argue with it. It just says: ungrounded.

**Beat:** the ten-second demo. Verbatim isn't competing with deep research
tools — it's checking them.

## 0:50–1:40 — the run (the big one)

**On screen:** ask the real question. Cut the timeline.

> Now the agent itself. It plans, searches, fetches, drafts claims — and then
> every claim goes through a verifier that contains **no language model at all**.
>
> Fifty-seven claims. Thirty-nine sources. Ten of them failed.

**Show the receipts page.** Click one claim → the quote, the byte offsets, the
SHA-256 of the source.

> Every sentence has a receipt. Click it: the exact passage, the byte range, the
> hash of the page it came from. You don't have to trust the summary. Trust the
> quote — and you can check it yourself.

**Beat:** the claim that gets struck. If one flips to CONTRADICTED on camera,
that is the moment.

> Eight of those ten were paraphrased instead of quoted. Two stated a different
> value than the source said. A human reading thirty-nine pages would have
> shipped at least one.

## 1:40–2:10 — kill it, and it comes back

**On screen:** terminal. Start a run. `kill -9` the engine mid-run. Restart.
POST `/resume`.

> This is the part I care about most. I just killed the agent mid-task with
> SIGKILL. Not a graceful shutdown — no cleanup, no flush.
>
> [show the effects ledger] Thirty-nine side effects. **Zero duplicates.** The
> event log is the only source of truth, and effects settle-then-commit under
> idempotency keys, so a resumed run redoes work rather than guessing — and a
> redo is a no-op at the tool boundary.

**Beat:** nobody else will show this. It maps directly onto three of the named
focus areas: State, Recovery, Execution.

## 2:10–2:45 — the three tools

**neatlogs** — show the dashboard.

> Every run is a trace. The verifier's checks are GUARDRAIL spans, so a failed
> verification is red in neatlogs for free — twenty-nine traces from the live
> deployment, zero rejected.

**Entire** — `entire why answer.md:34`

> The agent commits its own artifacts, so `entire why` shows the session behind
> any sentence. Provenance of code, meeting provenance of claims.

**cfo.ai** — the economics page.

> \$0.007 per verified run, measured. Price per report, inference cost, gross
> margin, runway. The marginal cost of a report is near zero — so the honest
> model varies customers, not cost, because distribution is the risk.

## 2:45–3:00 — the close

> A research agent that can't prove its answer isn't research. It's confident
> text.
>
> **Verbatim. If we didn't fetch it, we didn't say it.**

---

## Recording notes

- **Record the run first, then film over it.** The 57-claim run takes ~90
  seconds of wall clock and costs about \$0.05 of the shared budget. Record the
  screen, then narrate. Never depend on the model behaving on camera.
- **The strike-through is the single most important shot.** If no claim flips to
  CONTRADICTED in the recorded run, re-run until one does — they happen
  regularly (2 in the last live run).
- **Kill -9 needs a visible moment.** Wait until at least 10 side effects are
  settled so the ledger proves something.
- **Do not read the README aloud.** Judges read.
- **Check the 8 KB ceiling is not hit** before recording: if a run emits no
  traces, re-run. Latest build: 29 traces, zero rejections.

## What must be true before recording

- [ ] Entire checkpoint exists and the link works
- [ ] cfo.ai model is built and shareable
- [ ] One control (naive) run recorded, so the A/B can be shown in one cut
- [ ] The audit page has a real answer pasted with its sources
