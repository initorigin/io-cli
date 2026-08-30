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
| `/` | at an empty prompt, open the command palette |
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
`Right` and `Left` on
a `/config` row whose setting is a boolean or a closed set of words change it to
the next value — and the value after that, and back again — writing each one and
redrawing the row from the file's own answer rather than from an account of what
was just done. No picker does anything with a horizontal arrow — they fall
through to a do-nothing arm — and the interception is scoped to this one list, so
it takes no key away from any other surface. The composer still moves its cursor
with them and an approval still moves between its answers; neither has the
keyboard while a picker is up.

**It is the arrows and not the spacebar, and that is a compromise rather than a
preference.** `Space` is the obvious key for a toggle and it is unavailable: a
picker treats every printable character as a fuzzy filter, so the space in a
two-word query is a keystroke the list has already claimed, and binding it would
change a setting in the middle of typing a search for a different one. `Left` and
`Right` are simply the keys the picker does not want. The cost is
discoverability: an arrow does not announce itself the way a spacebar would, and
nothing on the row says to press one. That is a real limit of the compromise, and
stating it beats leaving it to be found. What makes it a small one is that `Enter`
opens the same values as a list and does everything the arrows do and more — so a
key nobody finds costs a keystroke rather than a capability.

A number is deliberately not cycled. Its values are a ladder rather than a pair,
too long to step through without seeing where you are, and a held arrow would
write the file once per key repeat — so `Enter` opens it instead. A key io-cli
does not know the values of says so and asks you to press `Enter`, rather than
absorbing the keystroke and looking broken.

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
