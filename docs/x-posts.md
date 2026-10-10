# X posts

**Two threads.** You do not need Premium: every tweet below is under 280
characters, and the counts are verified against X's rule that a URL costs 23
characters no matter how long it is.

- **Thread 1 — post now.** Attach `docs/diagrams/run.png` to the first tweet.
- **Thread 2 — post tomorrow** in the submission window.

Build-in-public is **15% of the score** and we have none of it. Post thread 1
today even if it is late.

---

## Thread 1 — the init post

Attach the diagram to tweet 1/5. Thread 2 below is the one that counts.

**1/5 — attach the diagram to this tweet** · 190 chars

```text
Starting #neatHack with a rule instead of a feature list:

If we didn't fetch it, we didn't say it.

Everyone is building agents that do more. Almost nobody is building agents you can check.
```

**2/5** · 175 chars

```text
Published this April, across 221,000 citation URLs: 3-13% of deep research agent citations point at pages that never existed.

And the deeper they research, the worse it gets.
```

**3/5** · 212 chars

```text
So I'm building the opposite. Verbatim — a research agent where every sentence carries a receipt.

The diagram is the whole design. The one step with no language model in it is the one that decides what may ship.
```

**4/5** · 193 chars

```text
The verifier:

→ re-fetches the cited source
→ checks the quoted evidence is really in it, byte for byte
→ strikes the claim if it isn't
→ labels anything it can't check instead of asserting it
```

**5/5** · 195 chars

```text
Claims that pass get a receipt: the quote, the byte range, the SHA-256 of the page.

Rust + Axum. The verifier has 43 tests. neatlogs for traces, Entire for commit-to-session.

Shipping tomorrow.
```


---

## Thread 2 — the submission

Quote the **official neatlogs submission post**. Post it early in the window — neatlogs and cfo.ai engage with the first 50 quote tweets. Window: Mon 12 Oct, 19:00-23:55 IST.

**1/8 — quote the official neatlogs submission post** · 255 chars

```text
My #neatHack submission: Verbatim — research you can check.

Every sentence carries a receipt. The verifier takes no language model: it re-fetches the cited source and confirms the quoted evidence is really in it.

If we didn't fetch it, we didn't say it.
```

**2/8** · 220 chars

```text
One live run, against the open web:

62 claims checked
44 verified
18 rejected
31 sources read

Every rejection is the product working. 14 were paraphrased rather than quoted. 4 stated a different number than the source.
```

**3/8** · 173 chars

```text
A human reading the same 31 pages would have shipped at least one of them.

That's the argument. Seeing isn't fixing, and a citation you didn't check is a guess with a link.
```

**4/8** · 238 chars

```text
neatlogs — one nested trace per run. Every verification check is a GUARDRAIL span, so a failed check is red in the dashboard for free.

It's also how I found an 8 KB ingest ceiling silently dropping traces: 7,672 bytes → 200, 9,693 → 403.
```

**5/8** · 206 chars

```text
Entire — the agent commits its own artifacts, so 'entire checkpoint list' ties every commit to the session that produced it.

2 checkpoints, refs pushed. The reasoning behind a change stays with the change.
```

**6/8** · 265 chars

```text
cfo.ai — 83.4% gross margin, break-even at 301 paying customers, because a verified report costs $0.007 to produce.

The sheet labels its own inputs: cost measured, Enterprise price estimated, cash and opex assumed.

Unit economics aren't the risk. Distribution is.
```

**7/8** · 220 chars

```text
Also killed the engine mid-run with SIGKILL and resumed it.

9 side effects settled. 9 distinct idempotency keys. 0 repeated.

The event log is the only source of truth, so a resumed run redoes work rather than guessing.
```

**8/8** · 142 chars

```text
Rust + Axum, 43 tests, MIT.

Demo, 2m42s: <VIDEO LINK>
Try it: https://verbatim-neathack.fly.dev
Code: https://github.com/PhiBao/verbatim
Checkpoint: <ENTIRE LINK>
```


---

## Facts you can safely cite

| Claim | Where it comes from |
|---|---|
| 3-13% of citation URLs hallucinated | arXiv 2604.03173 — 221,000 URLs |
| 62 / 44 / 18, 31 sources | live run `02078eb3` on the deployed app |
| $0.007 per verified run | measured on the LiteLLM gateway |
| 83.4% margin, 301 break-even | the cfo.ai sheet |
| 43 tests | `cargo test --workspace` |
| 8 KB ingest ceiling | bisected: 7,672 → 200, 9,693 → 403 |
| resume repeats nothing | 9 effects, 9 distinct keys, 0 duplicates |
| 2 checkpoints | `entire checkpoint list` |

## Do not say

- **Not "trustless", "autonomous", "AI-powered".** The product is about evidence;
  those words are the opposite of the claim.
- **Not "it never hallucinates".** It catches unsupported claims. Some get past —
  that is what the "we could not verify" section exists for.
- **Not "always live search".** The deployed app uses live Tavily and falls back
  to a pinned corpus, and says so in the trace.
- **No keys in any screenshot.** Crustdata, Tavily, LiteLLM, neatlogs.

## Before posting thread 2

- [ ] Video uploaded somewhere public, link swapped in for `<VIDEO LINK>`
- [ ] Checkpoint link swapped in for `<ENTIRE LINK>`
- [ ] Repo public; README renders with the diagram
- [ ] https://verbatim-neathack.fly.dev loads and a run can be started
- [ ] Quoting the official neatlogs submission post, not their announcement
- [ ] Claim counts still match the run you link to
