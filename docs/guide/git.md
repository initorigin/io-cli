# Git

A gate says the work holds up. This says what becomes of it.

The agent has had seven git built-ins on every workspace run since long before
this interface existed: `git_status`, `git_diff`, `git_log`, `git_add`,
`git_commit`, `git_branch` and `git_worktree`. They are io-harness's, and each is
a fixed argv that can reach no other subcommand — there is no `push`, no `remote`
and no `reset` among them. What was missing until 0.25.0 was any of it reaching
you.

**The branch the working tree is on is on the status line.** It is read out of
`.git/HEAD` as text, because git writes it there in a format that has not changed
in the lifetime of the tool and this program starts no process to ask — so it
costs a file read, and it follows the agent when the agent switches branch. A
detached head is drawn as a short object id rather than as nothing, and a
directory that is not a repository draws no field at all: `io` runs in plenty of
them and must not get worse there.

**A commit the agent makes is committed into the scrollback** — the branch it
landed on and the message the model wrote. The
diff is not drawn a second time. It is already on screen immediately above, from
the step that wrote it, and drawing it twice would cost you the reason the block
is there at all.

**`/commit` hands this turn's work to the agent, and the agent writes the
message.** io-cli runs no git and composes no subject line: the command sends a
prompt asking it to review what changed with those tools, stage what belongs to
this turn, and say what the change does and why. That is a billed turn against a
real model, and it is why the row says *ask the agent* rather than promising a
deterministic act.

**`[run.commit_identity]` decides who the commit is authored as**, and io-cli
reads that value rather than picking one. io-harness hands the name and email
from that section to git on the commit invocation itself, and the section always
resolves to something, so a repository with no identity of its own is told which
default io-harness will use. You are told before the turn is spent, because the
author of a commit is the one thing about it that cannot be corrected afterwards
without rewriting history.

### The refusal this repairs — and the half of it that is now fixed upstream

**Through io-harness 0.69.0, all seven tools were refused before they ran, for
most operators, and nobody was ever asked.** io-harness's git spawn checked the
`exec` policy itself and accepted only an outright allow. Every other gated act
turned an *ask* into a question on your screen and waited for it; this one
returned a refusal instead, so `ask` behaved exactly as `deny` did — and `ask
before writes`, the posture the wizard recommends, sets `exec` to ask. io-cli
0.25.0 found that and filed it as io-harness#214.

**io-harness 0.70.0 closed it, and 0.29.0 pins 0.70.0.** An asking posture now
raises an ordinary approval: you get the question, and git runs if you say so.
If you run the recommended posture, none of the rest of this section applies to
you any more.

What is left is `read only`, where `exec` is a **deny** and there is still no
question for you to answer. There io-cli names the refusal and offers one rule:
`exec` allowed for `git`, one binary, for this session. `/commit` asks that
*before* it spends the turn, because a commit the policy was always going to
refuse still costs a real completion to discover. The rule goes through the same
remembered layer as anything else you allow for a session, so it is exactly as
strong as those and no stronger — and it is offered **only** where the deny came
from the posture's own default rather than from a rule somebody wrote, because a
later layer can add capability but can never take back a denial. Offering it
against a deny rule would be advice that can never be taken. One binary name is
also the narrowest grant that works: an `exec` pattern has no notion of a
subcommand, so `git` says *this program may be spawned* and nothing about any
other.

**Under a posture that denies rather than asks, no rule is offered.** A rule is
matched before a default, so the same allowance would work under `read only` too,
and a keystroke that quietly defeats the one posture whose name is a promise is
not a convenience. What you are told there is to change posture — a decision, and
not a shortcut.

### A checkout of its own

Every agent in a tree shares one working directory, so two children editing the
same file are one overwriting the other. `worktree = true` on an `[[agent]]` entry
gives that child its own checkout instead:

```toml
[[agent]]
name = "reviewer"
worktree = true
```

io-harness roots it under `.worktrees/` in your repository, on a new branch
created before the child's first step. **The branch is not named after the
entry** — it is the entry's name, the parent run, the step and a digest of the
child's goal, so `reviewer` becomes something like `reviewer-12-3-a1b2c3d4`.
That is what makes two children of one entry, spawned in the same step, land in
two checkouts rather than one; and since nothing here removes either, it is also
the shape to look for when you go and find the work afterwards. If the worktree
cannot be
made — no git, not a repository, the boundary refusing that path — the spawn
fails with the reason and **no child starts**, rather than quietly sharing the
parent's tree and reintroducing the collision the switch exists to remove.
`/fleet` marks the rows whose roster entry asked for one. See [The
fleet](#the-fleet).

That mark is a property of the roster entry and not a directory. io-harness
records a child's actual worktree path and hands it back to nobody, so a path
drawn on that row could only be reconstructed — and a reconstruction is an
address that is wrong the moment either side changes, which matters here because
you would `cd` into it.

**What none of this does.** Nothing removes a worktree and nothing deletes a
branch: that is yours to do, because removing one throws away the work the child
was spawned to produce. io-cli does not open a pull request, and the seven tools
reach no remote at all. The work ends as commits on a branch in your own
checkout, and what happens to it after that is a decision this program does not
make for you.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)
