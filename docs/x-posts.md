# X posts

Three build-in-public posts and one submission post. "Building in public" is
**15% of the score** and we currently have zero, so post the first one before you
do anything else.

Two rules that matter:

- **Post the submission early in the window.** neatlogs and cfo.ai will engage
  with *the first 50 quote tweets*. The window is Mon 12 Oct, 19:00–23:55 IST.
- **Do not claim anything that is not in the repo.** Every number below is a real
  measurement. Judges can check them.

---

## Post 1 — the thesis (post this now)

> Everyone is building agents that do more. Almost nobody is building agents you
> can *check*.
>
> Published this year: 3–13% of citation URLs from deep research agents never
> existed. 221,000 citations audited. And deep research agents hallucinate *more*
> than plain search, not less.
>
> So I'm building the opposite. Verbatim — a research agent where every sentence
> carries a receipt.
>
> The verifier contains **no language model**. It re-reads the cited source and
> checks the quoted evidence is actually in it, byte for byte. Claims that fail
> get struck. Claims that can't be checked get labelled, not asserted.
>
> One rule: if we didn't fetch it, we didn't say it.
>
> building for #neatHack 🧵

Why this works: it leads with a measured number, states the inversion in one
line, and the rule is quotable.

---

## Post 2 — a bug the observability found (post after Post 1)

This one is the most credible thing you can post, because it is specific and it
admits a failure.

> Spent four hours on a bug that logged itself as an auth error.
>
> Our agent posts one trace per run to neatlogs. Every run: `403 Forbidden` with
> an nginx HTML body. Looks like a bad key. Wasn't.
>
> So I bisected a real payload against the live endpoint:
>
> 7,672 bytes → 200 OK
> 9,693 bytes → 403
>
> The ingest ceiling is ~8 KB, and a full run's span tree was 10.5 KB. A naive
> implementation logs "auth failed" and ships with **no traces at all** — and
> never notices.
>
> Fix: split an oversized trace across several, sharing a session id. Then write
> a test that pins the ceiling so it can't come back.
>
> The lesson isn't "watch your payload size". It's that an error message can be
> confidently wrong, and the only way through is bisecting the real thing.

---

## Post 3 — the verifier catching a real claim (post with the video)

> Ran my own agent on a question where sources disagree.
>
> A third-party blog says the Anthropic free tier allows 500 requests per minute.
> Anthropic's own docs say 60. **Both quotes were verbatim**, so the quote check
> passed on both.
>
> The agent caught the disagreement, resolved it with a recorded rule — the source
> whose host carries the entity the run is about wins — dropped the losing claim,
> and *printed the disagreement in the answer* instead of silently picking one.
>
> That's the difference between a citation and evidence.
>
> Full demo, 2m42s 👇

---

## Post 4 — the submission

This is the one that counts. It must be a **quote post of the official neatlogs
submission post**, and it must contain: team, name, project, public repo, and
demo link.

> Quote-tweeting as our #neatHack submission.
>
> **Verbatim — research you can check.**
>
> A research agent where every sentence carries a receipt. The verifier takes no
> language model: it re-fetches the cited source and confirms the quoted evidence
> is really in it. If we didn't fetch it, we didn't say it.
>
> On one live run: 62 claims checked, 44 verified, **18 rejected**, 31 sources.
>
> The three tools, each doing real work:
>
> **neatlogs** — one nested trace per run. Every verification check is a
> GUARDRAIL span, so a failed check is red in the dashboard for free. 8 traces
> this run, 0 rejected. It's also how I found an 8 KB ingest ceiling that was
> silently dropping traces.
>
> **Entire** — the agent commits its own artifacts, so `entire checkpoint list`
> ties every commit to the session that produced it. 2 checkpoints.
>
> **cfo.ai** — 83% gross margin, because a verified report costs $0.007 to
> produce. Break-even at 301 customers. Unit economics aren't the risk;
> distribution is.
>
> Rust + Axum · 43 tests · 29 live sources
>
> Demo (2m42s): <VIDEO LINK>
> Try it: https://verbatim-neathack.fly.dev
> Code: https://github.com/PhiBao/verbatim
> Checkpoint: https://entire.io/gh/PhiBao/verbatim/commit/6cc6e36

---

## Facts you can safely cite

| Fact | Source |
|---|---|
| 3–13% of citation URLs hallucinated | arXiv 2604.03173 — 221,000 URLs |
| 62 claims / 44 verified / 18 rejected / 31 sources | live run `02078eb3`, on the deployed app |
| $0.007 per verified run | measured on the LiteLLM gateway |
| 83.4% gross margin, 301 break-even customers | cfo.ai model |
| 43 tests passing | `cargo test --workspace` |
| 8 KB ingest ceiling | bisected: 7,672 → 200, 9,693 → 403 |
| resume repeats nothing | 9 effects, 9 distinct idempotency keys, 0 duplicates |
| 2 Entire checkpoints | `entire checkpoint list` |

## Things not to say

- **Don't say "trustless", "autonomous", or "AI-powered".** The product is about
  evidence; those words are the opposite.
- **Don't say it never hallucinates.** It catches unsupported claims. Some slip
  past — that's what the "we could not verify" section is for.
- **Don't claim live web search is always on.** The deployed app uses live Tavily
  retrieval and falls back to a pinned corpus, and says so in the trace.
- **Don't post the Crustdata, Tavily, LiteLLM or neatlogs keys.** Check any
  screenshot before posting it.

## Before you post the submission

- [ ] Upload the video somewhere public (YouTube unlisted is fine) and put the
      link in the post
- [ ] Check the repo is public and the README renders
- [ ] Check `https://verbatim-neathack.fly.dev` loads and you can start a run
- [ ] Confirm the Entire checkpoint link opens
- [ ] Quote the **official** neatlogs submission post, not their announcement
