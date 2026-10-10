# cfo.ai — step by step

You need cfo.ai for the submission: *"Use cfo.ai to plan your agent's pricing,
costs, revenue and runway. Include the resulting business plan in your
submission."*

You already signed up with **kiter2509@gmail.com**, so this is just: build the
model, then share it.

**Time: about 15 minutes.** No finance knowledge needed — you paste one message
and cfo.ai builds the model.

---

## Step 1 — Open it and log in

Go to **https://app.cfo.ai**

Log in with the same method you signed up with. If you used the email route, it
sends a 6-digit code — check your inbox.

You'll land in a chat called **Welcome**.

> **Ignore the "researching your company" message.** cfo.ai looks up whatever
> domain your email is from. Yours is gmail, so that research will be useless.
> The next step tells it to ignore that.

## Step 2 — The starting card

There's a card next to Ari's greeting with two questions. Answer them like this:

| Question | What to pick |
|---|---|
| "Where would you like to start?" | **A different business** |
| "Do you want to use your real numbers?" | **Just estimate my numbers for now** |

Then click **Get started**.

> If the card isn't showing, or you've already clicked past it, don't worry —
> skip to Step 3 and paste the message. It works either way.

## Step 3 — Paste this message

Copy the whole block below and paste it into the chat with Ari. Send it.

Note the **$0.007** figure — that is measured from a real run of the agent on
the hackathon's model gateway, not a guess. It's the strongest number in the
whole submission, so try not to let it get rounded away.

```
Build me a financial model for a SaaS business called Verbatim. Ignore the
company research from my email domain — that is not this business.

What Verbatim is: an AI research agent that verifies its own output. Every
sentence it produces is checked against the source it came from, and anything
it cannot verify is labelled instead of asserted. It sells monthly
subscriptions.

Pricing (four tiers):
- Free      $0    10 verified reports/month, 1 seat
- Solo      $49   100 reports/month, 1 seat
- Team      $199  500 reports/month, 5 seats
- Enterprise custom, unlimited reports, SSO

Unit economics — please model these as inputs, not hardcoded:
- Average model cost per verified report: $0.02
  (this is measured at $0.007 in reality; I am rounding up for headroom)
- Infrastructure: about $35/month per 500-report customer
- Blended ARPU at a mix of 60% Solo / 30% Team / 10% Enterprise

Please build:

1. A monthly unit economics table showing: customers, revenue, reports
   delivered, inference cost, infrastructure cost, gross profit, and gross
   margin. Start from 250 paying customers.

2. Three scenarios that vary the customer count only, holding price and cost
   per report constant:
   - Downside: 60 customers at month 12
   - Base: 250 customers at month 12
   - Upside: 900 customers at month 12
   Show gross margin for each.

3. A runway view: assume $500,000 of cash and $60,000/month of operating
   costs. Show how many months of runway each scenario gives, and roughly how
   many paying customers are needed to break even.

4. A shareable Page summarising unit economics and runway, so I can send a
   link to other people.

Keep the assumptions visible and labelled as assumptions, so a reader can see
which numbers are estimates and which are measured.
```

## Step 4 — Wait, then check it

Ari builds this in pieces — it may take a couple of minutes and create more than
one Page.

When it's done, glance at the numbers:

- **Gross margin should be roughly 85–90%.** If it's much lower, something got
  double-counted. If it's ~99%, a cost line is missing.
- **Inference cost should be a small line**, a few thousand dollars at 250
  customers. That is the point: marginal cost per report is near zero, so the
  real risk is distribution, not unit economics.

If something looks wrong, just say so in plain English — e.g. *"the inference
cost looks too high, model cost per report is $0.02"*. You do not need to know
finance to spot a wrong number.

## Step 5 — Share it and send me the link

CFO.ai calls these **Pages**. To share one:

1. Open the Page you want (the unit economics / runway one)
2. Find **Share** — usually top-right of the Page
3. Choose the **Scenario** you want the link to show (pick **Base**)
4. Click **Create link**
5. Copy it

Then **paste the link into this chat**. I'll put it in the README and the
submission.

> Only Pages can be shared — Scenarios and formulas can't. So if Share isn't
> available, you're probably looking at the model rather than a Page. Ask Ari:
> *"make me a shareable Page with the unit economics and runway"*.

## Step 6 — Screenshot it too

Take 2–3 screenshots of the model (the unit economics table, the scenario
comparison, the runway view). Save them as:

```
docs/cfo-economics.png
docs/cfo-scenarios.png
docs/cfo-runway.png
```

The demo video needs to show cfo.ai, and screenshots are easier to cut in than
a live screen-share.

---

## If something goes wrong

| Problem | What to do |
|---|---|
| It researched a random company | Say: *"ignore the company research, this is a fictional business I am describing"* |
| It asks for real data | Say: *"use the assumptions I gave you, I have no accounting data"* |
| The card never appeared | Just paste the Step 3 message — the card is optional |
| Numbers look wrong | Describe it in plain English. Ari will fix it. |
| No Share button | Ask for a Page explicitly |
| You're stuck on billing | You already started the trial. Settings → Billing shows the status. |

## What "done" looks like

- [ ] A model exists in cfo.ai with the four pricing tiers
- [ ] Unit economics show ~85–90% gross margin
- [ ] Three scenarios (60 / 250 / 900 customers)
- [ ] A runway figure with a break-even customer count
- [ ] A **shareable Page link**, pasted into this chat

Once that's done, the cfo.ai third of the submission is complete.
