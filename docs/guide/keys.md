# Keys

<!-- keys:start -->

| Key | Does |
| --- | --- |
| `Enter` | send the prompt |
| `Shift+Enter` | new line — or `Alt+Enter`, `Ctrl+J`, or end the line with \ |
| paste again | the same block again: shows it, then collapses it back |
| `Up / Down` | walk prompt history |
| `Ctrl+C` | stop the turn; again to stop it now; twice at an empty prompt, exit |
| `Ctrl+D` | exit, on an empty prompt |
| `Shift+Tab` | cycle the permission posture, from the next turn |
| `Ctrl+L` | clear the viewport, never the scrollback |
| `Esc Esc` | at an empty prompt, undo the last turn — its files and all |
| `Ctrl+T` | put the whole conversation back into the scrollback |
| `Ctrl+F` | show the fleet: the children this turn has spawned |
| `y / a / n` | answer an approval: allow once, allow this session, deny |
| `Esc` | stop the running turn, or close a picker without choosing |
| `Tab` | in any list, take the row under the marker; `Shift+Tab` steps back |
| `/` | open the command palette — at the prompt or while a turn runs |
| `@` | after a space, complete a path from the workspace |
| `!` | run the rest of the line in your shell; the agent never sees it |

<!-- keys:end -->

**`Shift+Enter` works where the terminal reports it.** `io` negotiates the Kitty
keyboard protocol on terminals that advertise it, asking for one flag —
`DISAMBIGUATE_ESCAPE_CODES` — because without it a terminal sends the same byte
for `Enter` and for `Shift+Enter` and the newline binding is unreachable. What is
pushed is popped again on every path out of the process, a panic included. The
trailing-backslash fallback still works everywhere, and on a terminal that does
not advertise the protocol nothing is written at all.

**And from 0.13.0 `io` tells you which one that is.** The table above is the
shipped naming, and a README is read on a machine other than the one it
describes — so `/help` and the wizard's closing screen name the key *this*
terminal can report. On one that cannot report `Shift+Enter` they name
`Alt+Enter` and the trailing backslash, and say the key is unreportable here
rather than leaving you to press it and watch a half-written prompt go to the
model. Nothing about the composer changed: `Shift+Enter`, `Alt+Enter`, `Ctrl+J`
and a trailing `\` all still work wherever the terminal can distinguish them.

**Four more keys exist only while something is queued**, and they are deliberately
not in the table above: that table is what the session binds all the time, and
these are borrowed by the queue surface for as long as it is open and handed back
the moment it shuts. While a prompt is waiting behind a running turn, the arrows
mark a queued line instead of walking prompt history, `Shift`+the arrows move the
marked line up and down the queue, `Enter` on an empty prompt takes the marked
line back into the composer to edit — `Enter` again puts it back where it was —
and `Esc` abandons an edit in progress, or closes the surface and gives all four
keys back. Every other key still falls through to the composer, because typing is
how the next line joins the queue. It is the same trade the fleet view makes with
the same two arrows, and `/steer` is what sends the queue into the turn rather
than waiting for it.

**Two more are borrowed by `/config` from 0.28.0**, and they are left out of the
table above for that same reason: it is what the session binds all the time, and
these are held for as long as one list is open and handed back when it shuts.
`Right` and `Left` on a `/config` row open that setting's values as a list, with
the marker already on the value in force, so you can see where you are before you
move. No picker does anything with a horizontal arrow — they fall through to a
do-nothing arm — and the interception is scoped to this one list, so it takes no
key away from any other surface. The composer still moves its cursor with them and
an approval still moves between its answers; neither has the keyboard while a
picker is up.

**Until 0.33.0 those two arrows wrote the file on the keystroke.** On a boolean or
a closed set of words they stepped to the next value, wrote it into a scope file
and redrew the row — one press, one write, no confirmation. It was the only
unconfirmed write in this product reachable from a bare arrow key, and it is why
`/config` could not be opened while a turn was running. The arrows now do what
`Enter` does; `Enter` on a value is the confirmation, exactly as it is on every
other managed surface, and `Up`/`Down` move inside the values while `Left`/`Right`
do nothing there. Nothing about the arrows is a shortcut past a decision any more,
which is the whole of the change: the affordance was worth keeping and the write
was not.

**A third is borrowed by a question that takes several answers, and it is the
spacebar.** Where the agent asks something that accepts more than one choice, the
offers are drawn with a box in front of each — `[ ]` and `[x]` in both glyph sets,
because a tick is not a character every terminal has — and `Space` marks and
unmarks the offer under the marker. `Enter` sends the marked set; with nothing
marked it sends the offer you are looking at, so a reflexive `Enter` still answers
with something the agent asked rather than with nothing.

It is borrowed and not bound, and the distinction is load-bearing. Every other
list in this product treats a printable character as a filter, and half the labels
it filters over have spaces in them — `Any OpenAI-compatible endpoint`, a
session's own workspace row — so a spacebar taken from every picker at once would
be a filter that cannot spell the rows it is filtering. Only a list that was told
it accepts several spends the key, and on that list a space typed into the
free-text row is prose again, because that row is not an offer and cannot be
marked.

A marked offer stays marked while you narrow the list, including when the narrowing
hides it. An operator marking five rows out of four hundred narrows the list to
find each one, and a filter that un-marked as it went would throw away the marks
made under the last query. The cost is stated rather than hidden: a marked row can
be off the screen while it is marked.

**`PgUp` and `PgDn` are borrowed when the agent asks several things at once.**
They walk the batch, one question on the screen at a time; see
[While it works](the-session.md) for what that surface does.

### Moving a key

The keys the session itself owns can be rebound in `[app.io-cli.keys]`, by action
name:

```toml
[app.io-cli.keys]
clear = "ctrl+k"
rewind = "ctrl+r ctrl+r"
```

| Action | Default |
| --- | --- |
| `exit` | `Ctrl+D` |
| `posture` | `Shift+Tab` |
| `clear` | `Ctrl+L` |
| `transcript` | `Ctrl+T` |
| `rewind` | `Esc Esc` |
| `fleet` | `Ctrl+F` |

A binding is a chord, or two chords separated by a space. Modifiers are `ctrl`,
`alt` and `shift`, joined to the key with `+`, in any order and any case; a key is
a single character, a named key — `esc`, `enter`, `tab`, `backtab`, `space`, the
four arrows, `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, `insert`
— or `f1` through `f12`. Because `+` is the join, `+` itself cannot be bound; that
is a real limit of the syntax and stating it beats a rule that quietly works for
`plus` and not for the character anyone would type. This spelling is public
contract from 0.6.0 on: it is the one VS Code, Zed and helix already write.

The rest of the table is not rebindable, because those keys belong to whatever
owns the keyboard while it is up — the composer, an approval, a picker — and an
approval's `y`, `a` and `n` are the *words* of the answer rather than shortcuts
for it.

**`Ctrl+C` is fixed, and it is the only one that is.** It interrupts a running
turn and leaves `io`, so a configuration file able to take it away is one able to
lock you inside a running agent. Both spellings of that mistake are refused out
loud with the reason: naming `interrupt`, and putting any other action onto
`ctrl+c`.

Nothing about a bad line is fatal and nothing is silent. A value that cannot be
read leaves its action on the default and names the key it kept; a name that is no
action of ours says which names there are; and every notice is committed into the
scrollback as the session starts, rather than left to be discovered by pressing
something. `/help` renders the table as the session *actually behaves* rather than
the defaults that shipped, and marks `Ctrl+C` as fixed.

---

[README](../../README.md) · [All guides](../CAPABILITIES.md) · [What you may depend on](../CONTRACT.md)

## While a turn is running

**Eleven commands report while the agent works**, and they are `/status`,
`/context`, `/cost`, `/stats`, `/help`, `/theme`, `/copy`, `/expand`, `/fleet`,
`/image` and `/config`.

The palette opens mid-turn too, and path completion with it — the `/` and the `@`
in the table above. `/compact` and `/steer` reach the running turn through their
own arms, as they always have, and are not part of this set.

Until 0.32.0 every slash but those last two was refused with *not while a turn is
running — Ctrl+C interrupts it first*, so reading a status page meant stopping
your own work to do it. None of these was a missing capability; each was one the
product had and withheld.

**Everything else keeps that refusal, and the rule is what a command does rather
than how harmless it looks.** A command that reassigns the session or the
provider, writes the store or a configuration file, or submits a turn of its own
is refused: `/exit`, `/setup`, `/model`, `/resume`, `/fork`, `/commit`,
`/remember`, `/memory`, `/skills`, `/mcp`, `/provider`, `/plugin`, `/import`,
`/profile`, `/effort`, `/contain`, `/undo`, `/plan`, `/clear`, `/store`,
`/export`, `/gates`, and a `!` line.

**`/config` joined the first list in 0.33.0, and only in its bare form.** Through
0.32.0 it was refused in every form, because the bare list carried a row that
re-read your provider's catalogue, wrote the prices into a scope file and
reassigned the configuration the running turn was holding — and because a
horizontal arrow on a row wrote the file where it stood. Neither is there now: the
refresh moved one descent below `prices.as_of`, the arrows open the values instead
of writing one, and what is left is a list of what every setting is and which file
decided it.

So the split is no longer between a safe spelling of a command and a dangerous
one. `/config <key>` and `/config <key> <value>` descend toward a write and keep
the refusal, which is the first time the mid-turn rule has read past a command's
first word. A trailing space is still the bare form.
