# What it costs

**`/cost` commits what has been spent, and `/stats` commits how the runs have
gone.** Two commands rather than one, because they are two questions — what the
work cost, and whether it worked — and a single screen carrying thirteen sections
is one nobody reads to the end. `/usage` opens `/cost`.

`/cost` has four sections: **this run**, **this session**, **by model** and **by
day**, each with the calls, the money, and the token split under it. Every figure
is a row io-harness has been writing into `runs.db` since its 0.18.0 and this
interface has never read — the model, the prompt, completion, cache-read,
cache-write and reasoning split, the latency and the time to first token, one row
per provider call.

**Nothing is estimated.** There is no sampling, no extrapolation from a partial
window, and no model of what a token "usually" costs. Three rules follow from
that and each is on the page rather than in this file:

- **Cache reads and cache writes are the breakdown of the prompt they are part
  of, never an addition to it.** They are already inside `prompt_tokens`, so the
  page shows the prompt total and the parts under it — read, written, fresh — and
  the parts sum to the total rather than past it. Reasoning is the same
  arrangement under completion: a vendor that reports it separately still bills it
  as output.
- **A call the provider reported no usage for is *unknown*, never free.**
  io-harness stores that as SQL `NULL` and reads it back as `None` for exactly
  this reason; summing it as zero would report a turn that cost something as a
  turn that cost nothing. The count of them is a line on the page.
- **A model with no rate in the table makes the total a *floor*, and the page
  says so** — in that word, with the number of calls it applies to, and on the
  row itself in the grouped sections so a reader scanning for the largest figure
  can see which of them is incomplete without reading past the list.

`/stats` is the other half: runs by outcome, runs by day, the first-try counts,
gate failures by phase, recovery, the slowest calls and the time to first token
over the last 200 runs, and what the store holds on disk. **First-try is
io-harness's own definition** — finished *and* successful *and* carrying no gate
failure — and it arrives as three counts rather than as a rate, because the
denominator is a choice. Where a share is shown, the row names what it is a share
*of*. The sandbox gate's phases and the review, command and contains gates use
two vocabularies that do not overlap, so they are two lists and are never merged
into one. Nothing on the page compacts anything: `Store::compact` is a full
`VACUUM` that needs free disk roughly equal to the file, so the free figure is
reported and the reclaiming is not offered here.

### Where a price comes from

**io-cli compiles no prices in.** A rate baked into a binary is a promise the
binary cannot keep: providers move prices without announcing it, a release cadence
is not a pricing cadence, and an operator reading a confident wrong number is
worse off than one reading no number at all. So the table ships empty, and **an
install that has connected nothing sees tokens and no currency** — which is the
honest answer, not a bug.

It is filled from **the model catalogue your connected provider already serves** —
the same fetch `io setup` has made since 0.1.0 to offer you a list of models,
whose prices it read and threw away on the same row. The rates land in `[prices]`,
which is io-harness's own section: `as_of` for the date, and one line per model
under `[prices.models]`.

**Refreshing is a row one descent below `prices.as_of` on `/config`, and it shows
every rate that would move before it writes anything** — what each was and what it would become — and you can
decline the lot. That is not courtesy. The file records what a rate *is* and never
where it came from, so io-cli cannot tell a correction you made by hand from a
value an older catalogue served; it does not guess, it shows you. A refetch that
comes back empty, or far shorter than the table it would replace, is **refused**
and the old table kept: a truncated response that replaced a full table with a
handful of rows would turn most of your spending into "unpriced" and shrink your
reported bill with nothing failing anywhere. A first fill has nothing to compare
against and is never refused. Rows for models the catalogue no longer serves are
left alone, because io-harness prices a call by the model name on it and an old
row is what prices an old run correctly.

**Whose price it is gets said, on every surface that draws money.** OpenAI and
Anthropic publish no prices on any endpoint — their model endpoints
carry capabilities and limits and no cost field — so for those two the rates
necessarily come from the reference catalogue rather than from the vendor. The
page names which and on what date. On OpenRouter the two coincide, because the
reference catalogue is OpenRouter's own. `[app.io-cli.prices] source_url` points
at a different catalogue, which is how an operator on a self-hosted or
`compatible` endpoint gets prices at all.

**The unit is `$`.** io-harness prices in micro-units and names no currency,
because it is not the layer that knows one. io-cli is: every catalogue it can read
quotes USD and every provider it can connect to bills in it. Four decimals below a
unit and two above, in integer arithmetic — a turn costs a fraction of a cent and
a month costs dollars, and one precision cannot show both.

### What your extensions cost, per request

`/cost` is what a run spent. `/context` is what it is *carrying* — and since
0.37.0 it says where that came from rather than giving you one number for the
lot. The tool catalogue is split by what put each tool on the wire: a row per MCP
server, named with the bundle that contributed it where a bundle did, plus a
`skill catalogue` row for the block io-harness writes into the system prompt. That
last one is new; before 0.37.0 a session's skills were counted inside the system
block, so they appeared to cost nothing.

`/plugin` and `/mcp` put the same figure on the row where you switch the thing on,
drawn from the same snapshot, so the two surfaces cannot tell you different
things about one server.

**Not everything a bundle contributes costs anything, and the difference is
large.** Of the seven kinds of thing a bundle can contribute, three reach the
wire and four do not:

| Contribution | Cost per request |
| --- | --- |
| MCP servers | every tool's name, description and JSON schema, every request |
| Skills | one line each — name and description. A body is read on demand by `read_skill` and only when the model asks |
| Agents | a roster in the system block |
| Hooks | nothing |
| Templates | nothing — io-cli expands them into your prompt |
| A declared binary | nothing |
| Policy layers | nothing |

So a bundle that ships only hooks is free forever, and one that ships an MCP
server with forty tools is not. That is the decision `/plugin`'s cost column
exists to inform.

Two honest gaps in the number. An **agent roster** is not counted against the
bundle that contributed it: io-harness composes it into the system block with no
marker io-cli can locate, and a number invented for it would be worse than none.
And a server that has not been on a wire yet is drawn as unmeasured rather than as
zero — the two mean different things, and only one of them is something you could
act on.

**Withholding a tool does not reduce any of this.** See
[What this release is not](limits.md) — the catalogue sent is the same either way,
by design. The lever these numbers inform is turning a bundle or a server off.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
