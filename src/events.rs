//! Turning io-harness run events into lines.
//!
//! Three rules shape this module.
//!
//! **Nothing is dropped.** `EventKind` is `#[non_exhaustive]` and has fifty-one
//! variants today, so a wildcard arm is not a shortcut here, it is required by
//! the type. What matters is that the wildcard *renders* — an event this release
//! has no design for arrives as a muted single line naming its kind, rather than
//! disappearing. An unrendered event is a bug this design should surface.
//!
//! **Streaming text is not committed a token at a time.** Tokens accumulate in a
//! live buffer that the viewport draws, and the whole passage is committed to
//! scrollback once when it is finished. Committing per token would put a line in
//! the terminal's scrollback for every few characters the model produced.
//!
//! **A tool call is a cell, not a line.** io-harness announces a call before it
//! runs — `EventKind::ToolCall` carries the tool and its target and nothing about
//! what came back, because nothing has come back yet — and reports the result
//! only in the `Step` that commits afterwards. So a call is held open from its
//! announcement, shown live in the viewport while it runs, and committed to
//! scrollback once, complete with its result and how long it took, when the step
//! lands. 0.2.0 committed the announcement immediately, which is why a
//! transcript said what the agent was about to do and never what happened.

use std::time::Duration;

use io_harness::{EventKind, RunEvent, TodoState, MCP_TOOL_PREFIX, NAMESPACE, TODO_MAX_ITEMS};
use ratatui::text::{Line, Span};

use crate::picker::fit;
use crate::theme::{Theme, Tone};

/// The width one committed plan row is fitted to.
///
/// A constant rather than the terminal's real width, because `event` is a
/// function of an event and a session age and nothing hands it a width. Eighty is
/// the terminal this product is audited at, and a committed line that overruns a
/// narrower one wraps rather than truncating — `tests/narrow.rs` pins that — so
/// being wrong here costs a wrapped row and never a lost fact.
///
/// The reason to fit at all is io-harness's own `TODO_TEXT_CAP`: one item may be
/// two hundred characters, and sixty-four of those wrapping three rows each is
/// not a list an operator reads, it is the transcript buried under one.
const ROW: usize = 80;

/// A tool call that has been announced and not yet closed.
///
/// Held rather than committed because the two facts a reader actually wants —
/// what came back and how long it took — are not on the announcing event at all.
struct Pending {
    name: String,
    target: String,
    /// The session age at which the call was announced. An age handed in by the
    /// driver, never a clock read here: this module has no timer, and N1 is what
    /// keeps it that way.
    opened_at: Duration,
    /// A duration io-harness itself measured, when it reports one.
    ///
    /// `EventKind::Mcp` is the only event in the whole enum that carries a
    /// per-tool duration, so for an MCP tool this is a real measurement of how
    /// long the tool ran. For every other tool it stays `None` and the cell
    /// falls back to io-cli's own observation, which is a different kind of
    /// number and is printed as one.
    measured: Option<Duration>,
}

/// Accumulates streaming text and turns events into committed lines.
pub struct Events {
    theme: Theme,
    /// Model text received since the last commit.
    live: String,
    /// Calls announced and not yet closed, in the order they were announced.
    ///
    /// A `Vec` and never an `Option`: `read_batch` announces every call in a
    /// parallel batch up front and only then runs any of them, so two or more
    /// open calls before a single `Step` is the ordinary case rather than an
    /// edge one. A single slot would be wrong on the first parallel read.
    open: Vec<Pending>,
    /// Whether the permission boundary refused something during the step now in
    /// flight.
    ///
    /// A refusal does not end a step. io-harness turns it into an observation
    /// fed back to the model and the step commits anyway, so this *marks* the
    /// step rather than closing a cell — and it is the only honest result word
    /// available when the step's own decisions cannot be paired to the calls.
    refused_this_step: bool,
}

impl Events {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            live: String::new(),
            open: Vec::new(),
            refused_this_step: false,
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// The one live row the viewport draws: an open tool call if there is one,
    /// otherwise the unfinished tail of what is streaming.
    ///
    /// It is arbitrated rather than concatenated because the row is not free.
    /// No `Token` arrives while a tool is dispatching, so the buffer is frozen —
    /// but it is frozen holding the assistant's last unterminated line, which is
    /// usually not empty. The obvious fix, flushing at `ToolCall` to empty it,
    /// is wrong: committing the tail appends a blank line after it, so every tool
    /// cell in the transcript would arrive with a blank row in front of it.
    /// An open call is the more urgent thing to say, so it wins the row, and the
    /// tail stays live until something legitimately commits it.
    pub fn live(&self) -> String {
        let Some(call) = self.open.last() else {
            return self.live.clone();
        };
        let glyphs = &self.theme.glyphs;
        let mut row = format!("{} {}", glyphs.bullet, call.name);
        if !call.target.is_empty() {
            row.push(' ');
            row.push_str(&call.target);
        }
        row.push(' ');
        row.push_str(glyphs.ellipsis);
        if self.open.len() > 1 {
            row.push_str(&format!(" (+{} more)", self.open.len() - 1));
        }
        row
    }

    /// End the turn: commit whatever streamed, then close every call still open.
    ///
    /// The public entry point, called when a turn finishes or is interrupted. A
    /// `Step` may never arrive — io-harness skips `commit_step` when a sub-agent's
    /// child deferred — so this is the only place that can honestly close a call
    /// that nothing ever reported on.
    ///
    /// Those cells carry no duration. io-cli knows when the call was announced
    /// and nothing at all about when it stopped, and a number printed there would
    /// be a guess wearing a measurement's clothes.
    pub fn flush(&mut self) -> Vec<Line<'static>> {
        let mut lines = self.flush_text();
        let theme = self.theme;
        for call in std::mem::take(&mut self.open) {
            lines.push(cell_line(theme, &call, "unfinished", None));
        }
        self.refused_this_step = false;
        lines
    }

    /// Commit whatever text has streamed so far, if any, leaving open calls open.
    ///
    /// Separate from the public `flush` because most of the callers below are
    /// mid-step — a refusal and an approval request both arrive while the call
    /// they are about is still running — and closing that call as unfinished
    /// there would report the opposite of what happened.
    fn flush_text(&mut self) -> Vec<Line<'static>> {
        if self.live.trim().is_empty() {
            self.live.clear();
            return Vec::new();
        }
        let text = std::mem::take(&mut self.live);
        let mut lines: Vec<Line<'static>> = text
            .lines()
            .map(|line| {
                Line::from(Span::styled(
                    line.to_string(),
                    self.theme.style(Tone::Normal),
                ))
            })
            .collect();
        lines.push(Line::from(""));
        lines
    }

    /// What this event commits to scrollback, given the session age the driver
    /// read the clock for.
    ///
    /// Empty means "nothing yet" — which for a token, and for a tool call that
    /// has not finished, is the correct answer rather than a dropped event.
    ///
    /// `at` is handed in for the same reason `App::tick` takes one: nothing here
    /// may read a clock, so a test can state the interval between two events by
    /// hand and assert on it without anything being timed.
    pub fn event(&mut self, event: &RunEvent, at: Duration) -> Vec<Line<'static>> {
        let theme = self.theme;
        let separator = theme.glyphs.separator;
        let dash = theme.glyphs.dash;
        match &event.kind {
            EventKind::Token { text } => {
                self.live.push_str(text);
                // Every COMPLETE line commits as it arrives; only the unfinished
                // tail stays live. That is what keeps the viewport a fixed few
                // rows while an answer of any length streams: a line that will
                // never change again belongs to the terminal, not to us.
                let mut lines = Vec::new();
                while let Some(newline) = self.live.find('\n') {
                    let finished: String = self.live.drain(..=newline).collect();
                    lines.push(Line::from(Span::styled(
                        finished.trim_end_matches('\n').to_string(),
                        theme.style(Tone::Normal),
                    )));
                }
                lines
            }
            EventKind::Started { goal, provider } => {
                let mut lines = vec![Line::from(vec![
                    Span::styled(theme.glyphs.marker, theme.style(Tone::Accent)),
                    Span::styled(goal.clone(), theme.style(Tone::Normal)),
                ])];
                lines.push(Line::from(Span::styled(
                    format!("  via {provider}"),
                    theme.style(Tone::Muted),
                )));
                lines.push(Line::from(""));
                lines
            }
            EventKind::Step {
                decision,
                tool_call,
                tokens,
                changed,
            } => {
                // Taken before anything else, so that the tail flush below
                // cannot close these as unfinished a line before the step that
                // finished them says otherwise.
                let open = std::mem::take(&mut self.open);
                let refused = std::mem::replace(&mut self.refused_this_step, false);

                // A step's own narration commits after whatever streamed before
                // it, so the transcript reads in the order it happened.
                let mut lines = self.flush_text();

                // Then the calls it ran, in the order they were announced, then
                // the step line: chronological, which is the only order a
                // transcript can be skimmed in.
                //
                // `decision` is `decisions.join("; ")`, one sentence per thing the
                // step did, each written after that thing ran. Pairing them to the
                // calls positionally is right whenever there is one of each — but
                // the count is not guaranteed: the workspace loop pushes extra
                // decisions for spawned children, and both loops push an "awaiting
                // approval" segment. So the pairing is used only when the two
                // counts agree, and never indexed into blind.
                // An empty segment is not a result either, and a step whose
                // decision is blank pairs to a cell with nothing in its result
                // column — so a blank disqualifies the pairing rather than being
                // printed as an answer.
                let parts: Vec<&str> = decision.split("; ").collect();
                let complete = parts.iter().all(|part| !part.trim().is_empty());
                let paired = complete && parts.len() == open.len();
                let verdict = if *changed {
                    "changed files"
                } else {
                    "no change"
                };
                for (index, call) in open.iter().enumerate() {
                    // Never invented. The harness's own sentence when it can be
                    // matched, otherwise the coarsest true thing known about the
                    // step — a refusal marked it, or the step's own verdict.
                    let result = if paired {
                        parts[index]
                    } else if refused {
                        "refused"
                    } else {
                        verdict
                    };
                    lines.push(cell_line(theme, call, result, Some(at)));
                }

                // What was decided, what it ran, what came back — then the
                // metadata. 0.1.0 put the step number and the token count in the
                // middle of that sentence, which made a transcript something to
                // parse rather than to skim. Content before metadata is the rule
                // the rest of the interface already follows.
                let mut spans = vec![Span::styled(decision.clone(), theme.style(Tone::Normal))];
                if !tool_call.is_empty() {
                    spans.push(Span::styled(separator, theme.style(Tone::Muted)));
                    spans.push(Span::styled(
                        tool_names(tool_call),
                        theme.style(Tone::Accent),
                    ));
                }
                // Always said, in both directions. A result that appears only
                // sometimes is a column a reader cannot skim down, and `changed`
                // is the one thing this event reports about what came back.
                spans.push(Span::styled(separator, theme.style(Tone::Muted)));
                spans.push(Span::styled(
                    if *changed {
                        "changed files"
                    } else {
                        "no change"
                    },
                    theme.style(if *changed { Tone::Success } else { Tone::Muted }),
                ));
                spans.push(Span::styled(
                    format!("{separator}{tokens} tok{separator}step {}", event.step),
                    theme.style(Tone::Muted),
                ));
                lines.push(Line::from(spans));
                lines
            }
            EventKind::ToolCall { name, target } => {
                // Nothing is committed here, and that is the point: this event is
                // emitted before the call runs, so a line written now could only
                // say what the agent was about to do. The call is held open, shown
                // by `live()` while it runs, and committed once — with its result
                // and its duration — by the `Step` above.
                //
                // One cell per call either way. The full output goes to the run's
                // durable trace rather than to the screen; uncollapsed tool output
                // is what makes a transcript unreadable.
                self.open.push(Pending {
                    name: name.clone(),
                    target: target.clone(),
                    opened_at: at,
                    measured: None,
                });
                Vec::new()
            }
            // 0.65.0 — a resume found a call that was started and never finished,
            // and refused to drive rather than make it a second time. It is styled
            // rather than left to the catch-all because the muted word
            // `recovery_paused` says nothing an operator can act on, and the two
            // things they need — which tool, and the attempt id a decision has to
            // name — are both carried by the event.
            EventKind::RecoveryPaused { attempt_id, tool } => {
                let mut lines = self.flush_text();
                lines.push(Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Warning)),
                    Span::styled(
                        format!("paused {dash} {tool} was interrupted"),
                        theme.style(Tone::Warning),
                    ),
                ]));
                lines.push(Line::from(Span::styled(
                    format!(
                        "  whether it ran is unknown, so nothing was repeated; attempt {attempt_id}"
                    ),
                    theme.style(Tone::Muted),
                )));
                lines
            }
            EventKind::Refused {
                act,
                target,
                rule,
                layer,
            } => {
                // Marks the step, and closes nothing. A refusal is fed back to the
                // model as an observation and the step commits anyway, so there is
                // no call to close here — and often no open call at all, since a
                // dial, a verification and an MCP server spawn are all refused
                // outside any step. Nothing below indexes into the open list, which
                // is what keeps this arm correct at step zero.
                //
                // The act and the target are deliberately not matched against an
                // open call: `act` is a policy verb such as "write" or literally
                // "tool", and `target` is the policy's resolved path, while a
                // call's target is the raw argument the model wrote. Matching them
                // by string would pair the wrong things confidently.
                self.refused_this_step = true;

                // Act, target, rule, layer — in that order, and the last two are
                // the facts no other terminal agent can print, because no other
                // core records them. Asserted by position rather than by presence:
                // a `contains` assertion is just as green when the sentence is
                // inside out.
                let mut text = format!("{act} {target}");
                match (rule, layer) {
                    (Some(rule), Some(layer)) => {
                        text.push_str(&format!("{separator}rule {rule}{separator}layer {layer}"));
                    }
                    (Some(rule), None) => text.push_str(&format!("{separator}rule {rule}")),
                    // Said, not left blank. In io-harness a missing rule means the
                    // policy's own default for that act decided — the *least*
                    // vouched-for kind of action rather than the most — so silence
                    // here would read as the opposite of what happened.
                    (None, _) => text.push_str(&format!(
                        "{separator}no rule named it: the tier default decided"
                    )),
                }
                let mut lines = self.flush_text();
                lines.push(theme.notice(Tone::Refused, text));
                lines
            }
            EventKind::ApprovalRequested { act, target } => {
                // One line, and deliberately a thin one. The event carries only the
                // act and the target; the rule, the layer and the content a write
                // proposes arrive on the approver seam instead, and the overlay is
                // drawn from those. This is the transcript's note that the run
                // stopped, not the question itself — the question must never be
                // committed, which is what F1 asserts.
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!("{act} {target} {dash} waiting for you"),
                ));
                lines
            }
            EventKind::ApprovalDecided {
                act,
                target,
                decision,
            } => {
                // The harness's own record of what it was told, which is not the
                // same line as io-cli's. They agree because the answer travelled
                // one way; if they ever disagree, this is where it shows.
                let mut lines = self.flush_text();
                lines.push(theme.notice(
                    if decision == "deny" {
                        Tone::Refused
                    } else {
                        Tone::Muted
                    },
                    format!("{act} {target}{separator}{decision}"),
                ));
                lines
            }
            EventKind::Finished {
                outcome,
                steps,
                tokens,
            } => {
                let mut lines = self.flush_text();
                let tone = outcome_tone(outcome);
                lines.push(theme.notice(
                    tone,
                    format!("{outcome}{separator}{steps} steps{separator}{tokens} tok"),
                ));
                if let Some(help) = outcome_help(outcome) {
                    lines.push(Line::from(Span::styled(
                        format!("  {help}"),
                        theme.style(Tone::Muted),
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            EventKind::Mcp {
                server,
                tool,
                millis,
                ..
            } => {
                // The one place io-harness reports how long a tool actually ran.
                // It is harvested onto the open cell rather than printed here, so
                // that an MCP call closes with a measured duration while every
                // other call closes with io-cli's observation of the interval —
                // the difference the `~` prefix exists to show.
                //
                // Matched by rebuilding io-harness's own namespaced tool name,
                // from io-harness's own two constants rather than from a spelling
                // of them typed here, which is what `announce` puts on the
                // `ToolCall`. Newest first, because a batch may hold two calls to
                // the same tool and the later one is the one still running.
                //
                // A connect, a discover or a disconnect carries no tool and no
                // duration, and matches nothing.
                if let (Some(tool), Some(millis)) = (tool, millis) {
                    let name = format!("{MCP_TOOL_PREFIX}{server}{NAMESPACE}{tool}");
                    if let Some(call) = self.open.iter_mut().rev().find(|c| c.name == name) {
                        call.measured = Some(Duration::from_millis(*millis));
                    }
                }
                // Still rendered as itself. Harvesting a number off an event is not
                // the same as having designed a line for it, and an event that
                // vanished from the transcript because something read a field off
                // it is exactly the silence this module refuses.
                vec![Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(kind_name(&event.kind), theme.style(Tone::Muted)),
                ])]
            }
            // Guarded on the items rather than only on the tag, because io-harness
            // accepts a write of none: `parse_todo_items` validates each item it is
            // given and never rejects an empty list, so `{"items": []}` dispatches
            // as a real `TodoWrote`. A header reading `0 of 0 done` over nothing at
            // all is the placeholder F12's sabotage arm names, arriving through the
            // transcript's door instead of the status line's. An empty write falls
            // through to the catch-all below and commits the muted `todo_wrote`
            // word: the event still happened, and this module never drops one.
            EventKind::TodoWrote { items } if !items.is_empty() => {
                // The plan commits after whatever streamed before it, so the prose
                // that led up to it is above rather than below. Not under the
                // `todo_write` cell that announced it: `ToolCall` commits nothing
                // and only holds the call open, and the next `Step` writes the cell
                // — so the cell lands *after* this list, and the transcript reads
                // plan, then the call that wrote it. Reordering that would mean
                // committing an open call early, which is the one thing `live()`
                // and every other tool cell in this module depend on not happening.
                let mut lines = self.flush_text();

                // io-harness's own arithmetic for a done count, and io-harness's
                // own caveat with it: nothing in the core verifies an item, so
                // this is what the agent says about its own work rather than a
                // checked fact, and the header says so in those words.
                let done = items
                    .iter()
                    .filter(|item| item.state == TodoState::Done)
                    .count();
                lines.push(Line::from(Span::styled(
                    format!(
                        "  plan{separator}{done} of {} done, by the agent's own account",
                        items.len(),
                    ),
                    theme.style(Tone::Muted),
                )));

                // A bullet, two spaces of indent and a space after the mark — the
                // same leader a tool cell wears, because both are one row of a
                // list under a heading.
                let bullet_leader = theme.glyphs.bullet.chars().count() + 3;
                for item in items {
                    // io-harness's own three words, from `TodoState::as_str`, and
                    // not a spelling io-cli invented: they are the wire form the
                    // model wrote and the column the store holds. A word rather
                    // than only a tone, because a colour is nothing under
                    // `NO_COLOR`, on a monochrome terminal or to a screen reader.
                    let state = item.state.as_str();
                    let tone = match item.state {
                        TodoState::Done => Tone::Success,
                        TodoState::Active => Tone::Accent,
                        TodoState::Pending => Tone::Muted,
                    };
                    // What is left of the row once the leader, the separator and
                    // the state word have taken theirs. Counted in characters,
                    // never in bytes.
                    let taken = bullet_leader + separator.chars().count() + state.chars().count();
                    let room = ROW.saturating_sub(taken);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {} ", theme.glyphs.bullet),
                            theme.style(Tone::Muted),
                        ),
                        Span::styled(
                            fit(&item.text, room, &theme.glyphs),
                            theme.style(Tone::Normal),
                        ),
                        Span::styled(separator, theme.style(Tone::Muted)),
                        Span::styled(state, theme.style(tone)),
                    ]));
                }

                // The event carries the model's list *before* the store's cap is
                // applied: the dispatcher clones `items` and only then does
                // `Store::write_todos` keep `TODO_MAX_ITEMS` of them, and the
                // dropped count reaches no event at all. So this line is the only
                // place the whole length is knowable, and an operator not told
                // here would read a plan of sixty-four and never learn the agent
                // wrote more.
                if items.len() > TODO_MAX_ITEMS {
                    lines.push(theme.notice(
                        Tone::Warning,
                        format!(
                            "the agent wrote {} items; the run's store keeps the first \
                             {TODO_MAX_ITEMS}, so the last {} are in this transcript and \
                             nowhere else",
                            items.len(),
                            items.len() - TODO_MAX_ITEMS,
                        ),
                    ));
                }
                lines.push(Line::from(""));
                lines
            }
            // The other forty-one kinds. Not styled in this release, and not
            // discarded either: each arrives as one muted line naming itself, so a
            // release that starts emitting something new is visible rather than
            // silent.
            other => {
                vec![Line::from(vec![
                    Span::styled(leader(separator), theme.style(Tone::Muted)),
                    Span::styled(kind_name(other), theme.style(Tone::Muted)),
                ])]
            }
        }
    }
}

/// The muted leader an unstyled event line starts with: two spaces of indent,
/// the separator's own mark, then a space.
///
/// The mark is trimmed out of the separator rather than written again, so there
/// is one of it in the product and not two. An event line and the status line
/// under it are meant to read as one surface, and two spellings of the same mark
/// is how that stops being true.
fn leader(separator: &str) -> String {
    format!("  {} ", separator.trim())
}

/// One committed tool cell: the tool, its target, what came back, how long it
/// took.
///
/// Content before metadata, like every other line in this interface, and every
/// fact in words rather than in colour — the result reads the same under
/// `NO_COLOR` and in a screen reader as it does on a colour terminal.
///
/// `at` is the session age this cell is being closed at, or `None` when the cell
/// is being closed without anything having reported on it.
fn cell_line(theme: Theme, call: &Pending, result: &str, at: Option<Duration>) -> Line<'static> {
    let separator = theme.glyphs.separator;
    let mut spans = vec![
        Span::styled(
            format!("  {} ", theme.glyphs.bullet),
            theme.style(Tone::Muted),
        ),
        Span::styled(call.name.clone(), theme.style(Tone::Accent)),
    ];
    // `!= call.name` because io-harness falls the target back to the tool's own
    // name when the call carries no path, pattern or key — so a `git_diff` with
    // no argument arrived as `git_diff git_diff`, which reads like a stutter.
    // Found in a live run, not in the suite.
    if !call.target.is_empty() && call.target != call.name {
        spans.push(Span::styled(
            format!(" {}", call.target),
            theme.style(Tone::Muted),
        ));
    }
    spans.push(Span::styled(separator, theme.style(Tone::Muted)));
    spans.push(Span::styled(result.to_string(), theme.style(Tone::Normal)));

    // Two different kinds of number, told apart on the line itself. A measured
    // duration is io-harness's own and is printed plainly; anything else is the
    // interval io-cli observed between two events — which includes the model's
    // own turnaround and the queue in front of the tool — and wears a `~` to say
    // that it is an observation rather than how long the tool ran.
    //
    // `at` is `None` when the cell is closed with nothing having reported on it.
    // io-cli does not know a duration there and says none, rather than printing
    // the age of the announcement as though it were one.
    let observed = at.map(|at| format!("~{}", format_millis(at.saturating_sub(call.opened_at))));
    if let Some(duration) = call.measured.map(format_millis).or(observed) {
        spans.push(Span::styled(
            format!("{separator}{duration}"),
            theme.style(Tone::Muted),
        ));
    }
    Line::from(spans)
}

/// `420ms`, `1.4s`, `92.0s`. A tool cell's own duration.
///
/// Separate from [`format_elapsed`](crate::status::format_elapsed), which floors
/// to whole seconds and would print `0s` for every tool call that took less than
/// one — which is most of them. Two formats because they answer two questions:
/// how long the session has been open, and how long one call took.
fn format_millis(duration: Duration) -> String {
    let millis = duration.as_millis();
    if millis < 1_000 {
        return format!("{millis}ms");
    }
    format!("{:.1}s", duration.as_secs_f64())
}

/// How a run's outcome should read.
///
/// The vocabulary is io-harness's, and the distinction that matters here is
/// between `success` and `finished`: `success` means a verification criterion
/// passed, and `finished` means a run with no criterion ended on its own terms.
/// **Every io-cli turn ends `finished`**, because a steerable turn is built on
/// `TaskContract::workspace`, which carries `Verification::None`.
///
/// Found in a live run, not in the suite: treating anything that was not
/// `success` as a warning meant every ordinary, completely successful turn ended
/// the transcript with the word "warning".
pub fn outcome_tone(outcome: &str) -> Tone {
    match outcome {
        // Ended well, with or without a criterion to end against.
        "success" | "finished" => Tone::Success,
        // Somebody or something stopped it deliberately. Not a failure, and not
        // nothing either — the work did not complete.
        "cancelled"
        | "denied"
        | "refused"
        | "plan_rejected"
        | "stalled"
        | "budget_ceiling_reached" => Tone::Warning,
        // Waiting on a human this release has no way of asking. A warning rather
        // than an error: nothing went wrong, the run simply cannot go on from
        // here, and `outcome_help` is what says so on screen.
        "awaiting_answer" | "awaiting_approval" | "awaiting_plan" => Tone::Warning,
        // The run gave up and wants a human. Anything unrecognised lands here too:
        // an outcome this release has never seen is better over- than
        // under-reported.
        _ => Tone::Error,
    }
}

/// A sentence to print under an outcome the operator cannot otherwise act on.
///
/// A turn that ends waiting for a human is a dead end in this release: the
/// approval overlay is 0.2.0 and answering a question is 0.7.0, so there is
/// nothing on screen that can resolve it. Saying only "awaiting_answer" leaves
/// somebody stuck with no next action, which a live first run found by walking
/// straight into it — the agent was denied three times, asked for permission,
/// and the session had no way to give it.
pub fn outcome_help(outcome: &str) -> Option<&'static str> {
    match outcome {
        // Still a dead end, and a different one: a question about *intent*, which
        // io-harness deliberately distinguishes from an approval about permission.
        // Answering one needs a responder on a caller-supplied contract, which is
        // the same entry point that would cost `Ctrl+C`. 0.7.0.
        "awaiting_answer" => Some(
            "the agent asked what you meant, and this release has no way to answer \
             that. Say it in your next prompt.",
        ),
        // A turn that still ends here after 0.2.0 is one whose question was never
        // answered — the overlay was dropped when the turn ended, or the run asked
        // for something this interface does not bind, such as a plan gate.
        "awaiting_approval" | "awaiting_plan" => Some(
            "the run stopped waiting on a decision it never got. Ask again, or \
             press Shift+Tab to choose a posture that does not need one.",
        ),
        "denied" | "refused" => Some(
            "the permission boundary stopped it. The line above names the rule and \
             the layer; press Shift+Tab to change the posture for the next turn.",
        ),
        _ => None,
    }
}

/// The tools a step called, by name.
///
/// `Step.tool_call` is `name:arguments` per call, joined by `" | "`, and the
/// arguments are raw JSON. 0.1.1 put the whole thing on the step line, which was
/// the best available then — a reader had nothing else saying what ran. A live
/// run of 0.3.0 showed what it costs now that tool cells exist: every step
/// printed its arguments as a wall of escaped JSON directly under a cell that had
/// just said the same thing readably.
///
/// So the names are kept and the arguments are dropped. 0.1.1's F5 asks that a
/// step read as decision, then what it ran, then what came back, and it still
/// does — the tool cell above it is where the arguments live now.
fn tool_names(tool_call: &str) -> String {
    tool_call
        .split(" | ")
        .map(|call| call.split_once(':').map_or(call, |(name, _)| name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The snake-case name of a kind, taken from its `Debug` form.
///
/// `EventKind` is `#[non_exhaustive]` with no accessor for its own tag, and its
/// serde tag is only reachable by serializing — which would mean carrying a
/// serializer for a label. `Debug` is derived, its first token is the variant
/// name, and the mapping to snake case is the one `#[serde(rename_all)]` uses.
pub fn kind_name(kind: &EventKind) -> String {
    let debug = format!("{kind:?}");
    let variant: String = debug
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect();
    let mut snake = String::new();
    for (index, character) in variant.char_indices() {
        if character.is_ascii_uppercase() && index > 0 {
            snake.push('_');
        }
        snake.push(character.to_ascii_lowercase());
    }
    snake
}
