# Pricing methodology

Tokscope shows an **estimated API-equivalent cost**. It answers: “What would
these recorded requests have cost at the providers' public, standard API token
rates?” It is not subscription spend, an invoice, or the value of a Codex Pro
or Claude Max plan.

## Accounting contract

Tokscope normalizes every request into additive, non-overlapping token buckets:

```text
total = input + cache read + cache write + output + reasoning
```

The columns intentionally expose every term in that equation.

Provider logs do not all use the same convention:

- Codex reports `input_tokens` inclusive of cached input. Tokscope subtracts
  `cached_input_tokens` from Input and records it as Cache read.
- Codex reports `reasoning_output_tokens` as a subset of `output_tokens`.
  Tokscope subtracts reasoning from Output and records it separately. Output and
  Reasoning are then added and billed exactly once at the output rate.
- Claude reports Input, Cache read, Cache write, and Output as distinct buckets,
  so Tokscope retains them independently.

This normalization is load-bearing. Treating Codex's raw Output and Reasoning
as sibling buckets inflates both Total and cost.

## Request-level pricing

Pricing is calculated per canonical request before requests are aggregated into
minutes, days, months, models, or providers. This matters for tiered pricing.
For example, OpenAI's long-context threshold applies to each request, not to a
month or to all requests sharing a minute.

For standard GPT-5.5 and GPT-5.6 Sol requests, the audited rates on 2026-08-19
were:

| Token class | Up to 272K prompt tokens | Above 272K prompt tokens |
|---|---:|---:|
| Uncached input | $5.00 / 1M | $10.00 / 1M |
| Cached input | $0.50 / 1M | $1.00 / 1M |
| Output, including reasoning | $30.00 / 1M | $45.00 / 1M |

When a request crosses 272K total prompt tokens, the higher rates apply to the
whole request. Cache-write pricing is also retained where the selected model's
catalog entry provides it. Claude input, output, cache-read, and cache-write
rates are resolved independently from Anthropic's public pricing.

Tokscope preserves a compact request-level pricing basis in its durable history
so historical usage can be repriced from the active catalog without pretending
an aggregate bucket was one large request. Provider-reported costs, when
available, remain authoritative; estimated rows are never added on top of them.

The usage table's `Avg. $ / 1M` is:

```text
estimated API cost / total additive tokens * 1,000,000
```

It is a blended cost across input, caching, output, reasoning, models, and
providers—not a model's advertised output rate.

## Historical validation snapshot

During the 2026-08-19 pricing audit, replaying every archived request with the
standard/global catalog produced:

| Month | API-equivalent cost |
|---|---:|
| May 2026 | $3,184.611350 |
| June 2026 | $1,430.460346 |
| July 2026 | $16,464.093300 |
| August 2026 at the captured screenshot | $17,041.94 |

July reconciled to $11,875.442593 of OpenAI usage and $4,588.650707 of
Anthropic usage. August was still live, so its value increased after the
capture; it is not a fixed regression fixture.

The same audit explained a discrepancy with Tokscale 4.13.0: that release kept
Codex's raw Output and also added its nested Reasoning value, billing reasoning
twice. Subtracting the duplicate reasoning charge reconciled the fixed months.
Tokscale's `main` branch contained the same exclusive-output correction by
2026-08-19, but the published 4.13.0 package did not. Treat this as a dated
interoperability note rather than an assertion about future Tokscale releases.

## Limits of the estimate

Local transcripts provide token accounting, model, provider, and sometimes an
authoritative reported cost. They generally cannot prove optional commercial
modifiers such as Batch, Priority/Fast processing, regional data residency,
contract discounts, credits, or separately billed server-side tools. Tokscope
therefore labels the result **Est. API cost** and assumes ordinary global API
pricing unless a provider-reported cost is present.

Primary references:

- [OpenAI GPT-5.6 Sol](https://developers.openai.com/api/docs/models/gpt-5.6-sol)
- [OpenAI GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5)
- [Anthropic pricing](https://platform.claude.com/docs/en/about-claude/pricing)
