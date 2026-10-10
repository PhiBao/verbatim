# cfo.ai — what to type

Paste the block at the bottom into **Ari** at [app.cfo.ai](https://app.cfo.ai).
One paste builds the whole model. Nothing here is invented: the $0.007 per report
is measured from a real run of the agent, and everything else is labelled as an
assumption.

---

## The one thing to get right first

During onboarding Ari shows a card with two questions. Answer them like this:

| Question | Choose | Why |
|---|---|---|
| Where would you like to start? | **A different business** | Verbatim is not the company behind `kiter2509@gmail.com`. If you pick "My business", Ari researches that email domain and models the wrong thing. |
| Do you want to use your real numbers? | **Just estimate my numbers for now** | There is no real revenue yet. Estimates plus stated assumptions is the honest starting point, and it is what the submission asks for. |

If you have already finished onboarding and the card is gone, just paste the
prompt — Ari will build the model from the description.

---

## What to paste

Copy everything in the box:

```text
I'm planning Verbatim, an AI research agent that produces research reports where
every claim is verified against the source it cites. A separate deterministic
checker — no language model — re-reads the cited source and confirms the quoted
evidence is really in it. Claims that fail are dropped and claims that can't be
checked are flagged, so the product's selling point is that you can check its work.

Help me model the business. Treat all of this as the starting assumptions:

PRICING (monthly subscription)
- Free: $0, 10 reports/month, 1 seat — the distribution engine
- Solo: $49/month, 100 reports, 1 seat
- Team: $199/month, 500 reports, 5 seats
- Enterprise: custom, billed annually

BASE CASE, month 12
- 250 paying customers
- Mix: 60% Solo, 30% Team, 10% Enterprise
- Reports delivered: 108,000 per month

COSTS
- Model inference: measured at $0.007 per report, but assume $0.02 for headroom
- Infrastructure: about $35 per month per 500-report customer
- Starting cash: $500,000
- Monthly operating burn: $60,000

WHAT I NEED
1. A monthly model through month 12 showing customers, revenue, inference cost,
   infrastructure cost, gross profit and gross margin.
2. Three scenarios — Downside with 60 customers, Base with 250, Upside with 900 —
   and what happens to gross margin in each.
3. Runway: how many months the cash lasts in each scenario, and roughly how many
   paying customers we need to break even.

Then build me a Page I can share that shows the unit economics and the runway
side by side, and give me the share link.
```

---

## After it finishes

1. Read the numbers. If Ari invented revenue or a cost we did not give it, tell it
   to use our assumption instead.
2. Open the Page it built.
3. Click **Share** → create the link → copy it.
4. Send me the link.

That link goes in the README and the demo video. The submission requires the
resulting business plan, so the link *is* the deliverable — not a screenshot of it.
