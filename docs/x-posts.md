# X posts

**Two posts.** One now, one at submission. "Building in public" is **15% of the
score** and we currently have none of it, so post #1 today even if it is late.

- **Post 1 — now.** The init post. Attach `docs/diagrams/run.png`.
- **Post 2 — tomorrow, in the submission window.** The quote-tweet.

Facts to cite, and things not to say, are at the bottom. Read those first — one
wrong claim on a public repo is worse than no post at all.

---

# POST 1 — the init post

**Attach:** `docs/diagrams/run.png`

> Starting #neatHack with a rule instead of a feature list:
>
> **If we didn't fetch it, we didn't say it.**
>
> Everyone is building agents that do more. Almost nobody is building agents you
> can *check*. Published this April, across 221,000 citation URLs: 3–13% of deep
> research agent citations point at pages that never existed — and the deeper
> they research, the worse it gets.
>
> So I'm building the opposite. **Verbatim** — a research agent where every
> sentence carries a receipt.
>
> The diagram is the whole design. The one step in that pipeline with **no
> language model in it** is the one that decides what may ship:
>
> → it re-fetches the cited source
> → checks the quoted evidence is really in it, byte for byte
> → strikes the claim if it isn't
> → and labels anything it can't check instead of asserting it
>
> Claims that fail go back to retrieval and get re-researched. Claims that pass
> get a receipt: the quote, the byte range, the SHA-256 of the page.
>
> The `cfo.ai` box has no arrow on purpose. Nothing in a run flows to it. It
> prices the business, and drawing a line would imply otherwise.
>
> Rust + Axum · verifier has 43 tests · neatlogs for traces, Entire for
> commit ↔ session.
>
> Shipping tomorrow. 🧵

**Why this ordering works:** the diagram carries the architecture so the text can
carry the argument. The line about the missing arrow is the detail that signals
someone actually thought about the drawing rather than generating it.

---

# POST 2 — the submission

**Must be a quote post of the official neatlogs submission post**, and must
contain: team, name, project, public repo, demo link.

**Post it early in the window.** neatlogs and cfo.ai engage with *the first 50
quote tweets*. The window is Mon 12 Oct, 19:00–23:55 IST.

**Attach:** the video, or `docs/diagrams/run.png` plus the video link.

> Quote-tweeting this as my #neatHack submission.
>
> **Verbatim — research you can check.**
>
> A research agent where every sentence carries a receipt. The verifier takes no
> language model: it re-fetches the cited source and confirms the quoted evidence
> is really in it. If we didn't fetch it, we didn't say it.
>
> One live run, unaided, against the open web:
>
> **62 claims checked · 44 verified · 18 rejected**
> **31 sources read · 4 contradicted outright**
>
> Every rejection is the product working. Fourteen claims were paraphrased rather
> than quoted; four stated a different number than the source did. A human
> reading the same 31 pages would have shipped at least one.
>
> The three tools, each doing real work rather than decorating:
>
> **neatlogs** — one nested trace per run. Every verification check is a
> GUARDRAIL span, so a failed check is red in the dashboard for free. It is also
> how I found an 8 KB ingest ceiling that was silently dropping traces: 7,672
> bytes → 200 OK, 9,693 → 403.
>
> **Entire** — the agent commits its own artifacts, so `entire checkpoint list`
> ties every commit to the session that produced it. Two checkpoints, refs
> pushed.
>
> **cfo.ai** — 83.4% gross margin, break-even at 301 paying customers, because a
> verified report costs $0.007 to produce and ships inside a subscription. The
> sheet labels its own inputs: cost per report measured, Enterprise price
> estimated, cash and opex planning assumptions. Unit economics are not the risk.
> Distribution is.
>
> Also killed the engine mid-run with SIGKILL and resumed: 9 side effects
> settled, 9 distinct idempotency keys, 0 repeated.
>
> Rust + Axum · 43 tests · MIT
>
> ▶ Demo, 2m42s: <VIDEO LINK>
> ⌘ Try it: https://verbatim-neathack.fly.dev
> ◇ Code: https://github.com/PhiBao/verbatim
> ▣ Checkpoint: <ENTIRE CHECKPOINT LINK>

---

## Facts you can safely cite

| Claim | Where it comes from |
|---|---|
| 3–13% of citation URLs hallucinated | arXiv 2604.03173 — 221,000 URLs |
| 62 / 44 / 18, 31 sources | live run `02078eb3` on the deployed app |
| $0.007 per verified run | measured on the LiteLLM gateway |
| 83.4% margin, 301 break-even | the cfo.ai sheet |
| 43 tests | `cargo test --workspace` |
| 8 KB ingest ceiling | bisected: 7,672 → 200, 9,693 → 403 |
| resume repeats nothing | 9 effects, 9 distinct keys, 0 duplicates |
| 2 checkpoints | `entire checkpoint list` |

## Do not say

- **Not "trustless", "autonomous", "AI-powered".** The product is about evidence.
  Those words are the opposite of the claim.
- **Not "it never hallucinates".** It catches unsupported claims. Some get past —
  that is what the "we could not verify" section exists for.
- **Not "always live search".** The deployed app uses live Tavily and falls back
  to a pinned corpus, and says so in the trace.
- **No keys in any screenshot.** Crustdata, Tavily, LiteLLM and the neatlogs key.
  Check before posting.

## Before posting #2

- [ ] Video uploaded somewhere public, link in hand
- [ ] Repo public; README renders with the diagram
- [ ] `https://verbatim-neathack.fly.dev` loads and a run can be started
- [ ] Checkpoint link opens
- [ ] The official neatlogs submission post is quoted, not their announcement
- [ ] Verify the claim counts still match the run you link to
