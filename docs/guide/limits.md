# What this release is not

The configuration file has reached your terminal since 0.14.0: every section of
it bounds a session turn as it already bounded `io exec`, `/status` commits the
whole picture into the scrollback, and the ceilings in force are on the status
line beside what has been drawn against them.

**`[[hook]]` and capability bundles are applied from 0.20.0**, each with the
surface the omission was waiting on: `/plugin` for what a bundle brought and what
was dropped, and io-harness's own refusal sentence for a hook a project file may
not declare. **`[prices]` is read from 0.22.0**, which is the release that reads
the provider-call rows it prices. **A contract carries a verification criterion
from 0.24.0**, which was the last of these to be named here and was named with its
reason: it needed a surface of its own, and `/gates` is that surface. There is
still no `[verify]` section, because there is none in io-harness's schema to
apply — what a session carries comes from `[app.io-cli.gates]`, which is io-cli's
own. See [Verification gates](#verification-gates). **`[run.commit_identity]` is
*read* from 0.25.0**: it has reached a turn's contract since 0.14.0 like every
other section, and until there was a commit to author it was a value nothing in
this interface had cause to look at. See [Git](#git).

That leaves one key still unapplied, and it has a reason too: `run.templates` is
the thirteenth `[run]` key, reachable only through its own accessor. It is not a
silent omission — this is where it is named.

**A price is never invented, and a missing one is never a zero.** io-cli compiles
no rates in and estimates nothing, so an install that has connected no provider
reports tokens and no currency, and a total containing a model the table does not
price is reported as a floor. See [What it costs](#what-it-costs).

**`/import` does not bring a permission boundary across, and never will.** Another
tool spells a permission as a command line; io-harness matches a binary name. The
nearest honest translation is wider than what you granted, so io reports the
allowlist it found and writes no rule at all. Setting the boundary here is
`Shift+Tab`, `/config`, or the `io-permissions` skill — three surfaces where you
can see what you are granting. See [Bringing your setup
across](#bringing-your-setup-across).

**Git stops at your own checkout.** The seven built-ins the agent has are fixed
argvs with no remote among them, so nothing here pushes, fetches or opens a pull
request, and 0.25.0 adds no surface that does. Nothing removes a worktree or
deletes a branch either — both throw away work, and both are yours to decide. And
io-cli starts no git process of its own: the branch on the status line is a read
of `.git/HEAD`, and `/commit` is a prompt. What lifts the tools when your posture
refuses them is a permission rule, not a code path that runs git behind the
policy. See [Git](#git).

**Sixel is still absent**, because encoding it means palette quantisation and
another dependency, for terminals that either speak one of the two protocols
already here or draw half blocks correctly. The Kitty path covers PNG rather than
all four wire formats,
because Kitty's own transfer format is PNG and the only base64 this program has
is the one io-harness already computed — a screenshot is a PNG everywhere that
takes one.

**A text-only model plus an image is a failed run, and it fails at the wire.**
Whether a provider takes image input is asked before an attachment is accepted,
but that is a question about the *provider* — with OpenRouter in front of four
hundred models, the answer is yes while the particular model you have chosen may
still be text-only. What you get then is the provider's own refusal, mid-run:

```
error: provider error (Request, HTTP 404): {"error":{"message":"No endpoints
found that support image input","code":404}}
```

The step and its tokens are already spent when it arrives. This also reaches you
without attaching anything, because enabling images gave the agent `view_image`
and the agent may decide to use it on a model that cannot see — and io-cli cannot
take a tool out of io-harness's own workspace tool set. Checking the model rather
than the provider would mean reading the live catalogue on every attach, and it
would still not close the door the agent opens. **If you work with images, choose
a model that accepts them.**

**The twelve document tools cannot be taken out of that tool set either**, and
six of them write. A model that reaches for `docx_write` in a session where you
never meant a document to be written is stopped by the write gate rather than by
the absence of the tool — which is what the gate is for, and why the writers are
named one by one in [Documents](#documents) rather than counted.

**An image the agent was *given* rather than asked for is not shown.** A picture
returned by an MCP tool, and a browser screenshot, both become images inside
io-harness — but through private plumbing and with no event of any kind, so
nothing reaches this program to draw.

**A skill is listed by name and never pasted.** A template is expanded by io-cli
into prompt text, so nothing but this program is involved; a skill is read by the
*model*, through a tool, under the run's own policy. Choosing one from the
palette puts `use the <name> skill: ` in your prompt and stops there. io-cli
parses no skill file: the five it ships from 0.19.0 are bodies it carries and
writes to disk, and after that they are read the same way yours are — by the
model, through the tool, under the policy.

**`io exec` runs one goal and stops, and a run that pauses is still not answered
by a machine.** An agent that asks a question about what you meant, or proposes a
plan, ends the run at exit `4` with the pause persisted in the store. That is
io-harness's behaviour and it is the right one — a machine answering a question
about intent on your behalf sends the agent down a path nobody chose — so `io
exec` parks the run and says which pause it is parked on. Answering it is a
person's job, and from 0.23.0 there is a door for that: `io resume` from a
script, `/resume` from a session. Approvals are the one pause that cannot happen
in a headless run, because they are declined rather than deferred.

There are no `--max-steps`, `--timeout` or `--max-tokens` flags either: `[run]`
in the configuration file expresses all three, and a CI job's limits belong with
the project rather than in every invocation.

**A rewind does not check whether you edited a file yourself since the turn.** It
puts each file back to the state before that turn first wrote it, and it does not
compare that against what is on disk now — so a hand edit made afterwards is
overwritten. `io` cannot detect this, because the snapshot it restores from is not
readable from outside io-harness; what it does instead is tell you, in the prompt,
before the second keystroke. This is what `git checkout -- <path>` does too, and
it is said here rather than left to be discovered. Making it preventable is a
change to io-harness, not to this interface.

**A rewind undoes one turn**, the last one. **`/resume` lists every session the
walk found** — the twenty-row cap is gone, because it existed only to keep a list
short that nobody could filter. One bound is left, on how many runs the walk will
look at, and the list still says so when it has cut rather than quietly showing
you a subset.

Two more things are absent for reasons worth stating rather than hiding. **A diff
cannot be expanded beyond
the context the harness stored** — three lines either side, which is what
`diff -u` has always carried; more than that is not in the trace, and reading it
off disk would be reading a version of the file that no longer exists. And
**there is no split view**: this renderer commits into the terminal's own
scrollback at its real width, a two-column comparison doubles the horizontal
budget for every line, and word-level emphasis inside a unified diff already
answers the question split view answers.

One ceiling worth knowing about: a hunk is a fragment of a file, and each of its
lines is highlighted from a clean parse. A block comment or a multi-line string
that was opened *above* the hunk is not known here, so those lines read as code.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
