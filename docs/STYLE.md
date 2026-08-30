# The register this crate's prose is written in

Adopted from io-harness's `docs/STYLE.md` and extended, because io-cli has a prose surface a
library does not: **text the product prints at a person mid-task**. This describes the README,
the guide pages, `docs/CONTRACT.md`, the CHANGELOG, the rustdoc, the test names and every line
the binary puts on a terminal, as they already are.

It is a register, not a lint. No test enforces it, deliberately: a checker for prose this small
would reject good sentences more often than it caught bad ones.

## Say what is true of the thing being described

The one rule the rest follow from. A `/help` line must be true of the command it labels; a doc
comment on `contract::session` must describe what `contract::session` does today. This repository
has shipped README claims that were already false for three releases, a comment describing a
behaviour that had been deleted, and a doc that argued a configuration key could not exist while
the pinned dependency already honoured it. Each was found by a documentation pass, not by a test.

## Present tense, and no diary

State what the product does. Do not narrate what it used to do, except where a reader would
otherwise draw the wrong conclusion from what they already believe — then name the release that
changed it, in one clause, and move on:

> Containment decides fan-out and nothing else (0.17.0).

not

> In 0.10.0 the contract rode the contained turn, which meant capabilities and fan-out were one
> switch, and then in 0.17.0 we separated them, so today…

History belongs in `CHANGELOG.md`. A version number inside a sentence is a citation, not a story.

## Name the reason, once

Every non-obvious decision carries its reason where the decision is, and nowhere else. The reason
is the part that survives: an unexplained rule gets deleted by whoever finds it inconvenient, and
a rule explained twice gets corrected once.

> `GIT_TERMINAL_PROMPT=0`, because git does not prompt on stdin — it opens `/dev/tty` directly,
> and a credential prompt on this renderer damages the inline viewport permanently.

## Prefer the concrete failure over the abstract quality

"A picker owns the keyboard between `composer.set` and the choice, so the replacement was always
the note's own text" says more than "the replace path had a bug". Where a limit exists, name what
it costs and what still holds:

> An allowlist is read, shown, and translated into nothing, permanently — `Act::Exec` matches a
> binary name and nothing else, so the nearest faithful import of `bun install` is a blanket allow
> on `bun`, which is wider than the operator granted. Not a deferral; an impossibility.

## A claim is asserted or it is hedged

If a test asserts it, state it flatly. If nothing asserts it, say what is actually known and do
not reach for "should", "generally" or "typically" to cover the gap. What a model does with a
prompt is not a claim this crate can make, and saying so is better than implying otherwise.

## What the product prints is read by someone who is mid-task and possibly stuck

The rules above are about being right. These are about a person reading one line in a terminal
while a run is in flight.

**A refusal names the act, the target, the rule and the layer.** "Refused" alone tells an
operator nothing they can act on, and a refusal that reads like a visit is worse than no line at
all.

**A line an operator must still be able to read is committed to the scrollback, not put in the
footer.** `App::say` is a one-slot notice that the next keystroke destroys; using it for a
multi-line report has twice told someone a restore happened that had not. `record`, always.

**Never report success over a no-op.** A surface that writes nothing and says it wrote is worse
than one that fails, because nothing sends the operator back to check.

**Say how to open what you just closed.** Every HTTP MCP server is refused by default, because
`Policy::default().defaults.net` is `Deny`. A report that states the refusal without naming the
rule that lifts it reads as a defect in the product.

## No first person, no marketing, no adjectives doing an argument's work

The product is the subject: "io refuses", "a contained turn stops for a plan". "Powerful",
"robust", "seamless" and "simply" are removed on sight — each is a claim with no assertion behind
it, and "simply" is usually attached to the part that is not.

## Test names are sentences about behaviour

`f6_a_modified_file_comes_back_as_it_was`, not `test_undo_2`. A failing test's name is the first
line of the bug report, and it is read by someone who has not opened the file. The `f`/`n`/`o`
prefixes point back at the release contract's numbered acceptance criteria.

## Em dashes carry the aside, and lists carry the enumeration

Prose runs to one idea per sentence with an em dash for the qualification. When there are more
than three parallel items, they become a list. A paragraph enumerating five things is a list that
has not been written yet.
