# Verbatim

**If we didn't fetch it, we didn't say it.**

An AI research agent where every sentence carries a receipt. Each claim is
checked by a deterministic verifier — not the model that wrote it — that
re-fetches the cited source and confirms the quoted evidence is actually there.
Claims that fail are struck and re-researched. Claims that cannot be verified
are named as unverified instead of being asserted.

Built for [neatHack](https://neatlogs.com/hackathon) · Oct 10–12 2026.

---

## The problem, in one paragraph

Deep research agents hallucinate their citations at a higher rate the deeper
they research. Rao, Wong and Callison-Burch (arXiv 2604.03173) measured **3–13%
of citation URLs that likely never existed** across 53k and 168k citations, and
found deep research agents *worse* than plain search-augmented LLMs. SciConBench
(arXiv 2606.11337) puts the best deep-research agent at a factual F1 of **0.337**
under clean-room evaluation. Both papers reach the same conclusion: an agent that
scores its own work is not measuring its own work.

## The idea

Verification is the load-bearing primitive, not a feature. So Verbatim inverts the
usual pipeline:

```
plan → retrieve → claim → VERIFY → repair → compile
                                   ↑
                         no LLM in this step
```

The verifier runs five deterministic checks. The one that matters most is
**prior leak**: if a claim's distinctive facts appear in *no* fetched source, it
came from the model's memory rather than from this run — and it cannot ship, even
if it happens to be true.

---

## Status

Early. See `hackathon.md` for the running build log and evidence.
