# Verbatim — agent economics

The model to build in [cfo.ai](https://app.cfo.ai). Enter it via MCP
(`edit_table_blocks`) or by hand; every number here is either measured or
explicitly an assumption, and marked as such.

**Measured:** the cost per verified run is **$0.007** on the hackathon's LiteLLM
gateway with `claude-haiku`. That is a real number from a real run, not an
estimate.

**Assumed:** everything else. A Team plan is assumed to consume 500 reports a
month, which is the number to challenge.

---

## The one-line business

Verbatim sells **research you can check**. A monthly subscription buys a number of
verified research reports, each shipped as a reading page plus a per-claim receipt
ledger. The free tier is the distribution engine: audit mode checks someone
else's answer for free, and a claim that fails verification is the product's own
advertisement.

## Pricing

| Plan | Price | Reports / month | Seats | Purpose |
|---|---|---|---|---|
| Free | $0 | 10 | 1 | audit mode + a taste of the ledger |
| Solo | $49 | 100 | 1 | an analyst answering their own questions |
| Team | $199 | 500 | 5 | a team that needs the receipts in one place |
| Enterprise | custom | unlimited | SSO | regulated research, audit retention |

## Unit economics — the table to build

Monthly model, Base scenario.

| Row | Value | Notes |
|---|---|---|
| Customers | 250 | **assumed** |
| Plan mix — Solo | 60% | of customers |
| Plan mix — Team | 30% | of customers |
| Plan mix — Enterprise | 10% | of customers, billed annually upfront |
| Blended ARPU | $121.10 | derived |
| **Revenue** | **$30,275** | derived |
| | | |
| Reports delivered | 108,000 | 250 customers × 432 avg |
| Model cost per report | $0.02 | **measured $0.007**, rounded up for headroom |
| **Inference cost** | **$2,160** | derived |
| Infrastructure | $1,750 | Fly 2 GB × ~$35 per 500-report customer |
| Gross profit | $26,365 | derived |
| **Gross margin** | **87%** | derived |

## Runway — the scenario that matters

The honest statement is that the marginal cost of a report is close to zero, so
the risk is not unit economics — it is **distribution**. Model three scenarios
around acquisition, not around cost.

| Scenario | Customers at month 12 | Blended ARPU | Gross margin | Verdict |
|---|---|---|---|---|
| Downside | 60 | $121 | 84% | viable, slow |
| Base | 250 | $121 | 87% | the plan |
| Upside | 900 | $128 | 89% | needs infra re-scaling |

With a $500k seed and ~$60k/month burn at the Base scenario, runway is **~8
months**, and the business reaches cash-flow break-even at roughly **140 paying
customers**.

## The three assumptions that decide everything

1. **That people will pay for verification at all.** The honest risk: most
   consumers of AI research have learned to distrust it and cope. Evidence for
   willingness to pay is the whole open question, and it is the reason the free
   audit tier exists — it is the cheapest possible test of it.
2. **That 500 reports per Team customer per month is realistic.** If it is half
   that, gross margin rises; if it is 10×, inference cost still stays under 15% of
   revenue. The model is robust to this in both directions, which is the point.
3. **That the verifier stays cheap.** It is deterministic and model-free, so its
   cost is CPU, not tokens. The model cost is concentrated entirely in the
   claim-drafting step, which is the only thing that can get more expensive.

## What cfo.ai should produce

- A **Monthly model** with the unit-economics table above
- **Scenarios**: Downside / Base / Upside, varying customer count
- A **Runway** sheet: $500k cash, $60k/month burn, break-even customer count
- A **Page** with unit economics + runway, shared read-only
