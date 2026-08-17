//! Turning io-harness run events into lines.
//!
//! Two rules shape this module.
//!
//! **Nothing is dropped.** `EventKind` is `#[non_exhaustive]` and has fifty
//! variants today, so a wildcard arm is not a shortcut here, it is required by
//! the type. What matters is that the wildcard *renders* — an event this release
//! has no design for arrives as a muted single line naming its kind, rather than
//! disappearing. An unrendered event is a bug this design should surface.
//!
//! **Streaming text is not committed a token at a time.** Tokens accumulate in a
//! live buffer that the viewport draws, and the whole passage is committed to
//! scrollback once when it is finished. Committing per token would put a line in
//! the terminal's scrollback for every few characters the model produced.

use io_harness::{EventKind, RunEvent};
use ratatui::text::{Line, Span};

use crate::status::SEPARATOR;
use crate::theme::{Theme, Tone};

/// Accumulates streaming text and turns events into committed lines.
pub struct Events {
    theme: Theme,
    /// Model text received since the last commit.
    live: String,
}

impl Events {
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            live: String::new(),
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// The unfinished tail of what is streaming: everything since the last
    /// newline. Drawn in the viewport, where it can be replaced as it grows; a
    /// committed line cannot be.
    pub fn live(&self) -> &str {
        &self.live
    }

    /// Commit whatever has streamed so far, if anything.
    pub fn flush(&mut self) -> Vec<Line<'static>> {
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

    /// What this event commits to scrollback. Empty means "nothing yet" — which
    /// for a token is the correct answer, not a dropped event.
    pub fn event(&mut self, event: &RunEvent) -> Vec<Line<'static>> {
        let theme = self.theme;
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
                    Span::styled("› ", theme.style(Tone::Accent)),
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
                // A step's own narration commits after whatever streamed before
                // it, so the transcript reads in the order it happened.
                let mut lines = self.flush();

                // What was decided, what it ran, what came back — then the
                // metadata. 0.1.0 put the step number and the token count in the
                // middle of that sentence, which made a transcript something to
                // parse rather than to skim. Content before metadata is the rule
                // the rest of the interface already follows.
                let mut spans = vec![Span::styled(decision.clone(), theme.style(Tone::Normal))];
                if !tool_call.is_empty() {
                    spans.push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
                    spans.push(Span::styled(tool_call.clone(), theme.style(Tone::Accent)));
                }
                // Always said, in both directions. A result that appears only
                // sometimes is a column a reader cannot skim down, and `changed`
                // is the one thing this event reports about what came back.
                spans.push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
                spans.push(Span::styled(
                    if *changed {
                        "changed files"
                    } else {
                        "no change"
                    },
                    theme.style(if *changed { Tone::Success } else { Tone::Muted }),
                ));
                spans.push(Span::styled(
                    format!("{SEPARATOR}{tokens} tok{SEPARATOR}step {}", event.step),
                    theme.style(Tone::Muted),
                ));
                lines.push(Line::from(spans));
                lines
            }
            EventKind::ToolCall { name, target } => {
                // One line per call. The full output goes to the run's durable
                // trace rather than to the screen; uncollapsed tool output is what
                // makes a transcript unreadable.
                let target = if target.is_empty() {
                    String::new()
                } else {
                    format!(" {target}")
                };
                vec![Line::from(vec![
                    Span::styled("  ⋅ ", theme.style(Tone::Muted)),
                    Span::styled(name.clone(), theme.style(Tone::Accent)),
                    Span::styled(target, theme.style(Tone::Muted)),
                ])]
            }
            EventKind::Refused {
                act,
                target,
                rule,
                layer,
            } => {
                // Act, target, rule, layer — in that order, and the last two are
                // the facts no other terminal agent can print, because no other
                // core records them. Asserted by position rather than by presence:
                // a `contains` assertion is just as green when the sentence is
                // inside out.
                let mut text = format!("{act} {target}");
                match (rule, layer) {
                    (Some(rule), Some(layer)) => {
                        text.push_str(&format!("{SEPARATOR}rule {rule}{SEPARATOR}layer {layer}"));
                    }
                    (Some(rule), None) => text.push_str(&format!("{SEPARATOR}rule {rule}")),
                    // Said, not left blank. In io-harness a missing rule means the
                    // policy's own default for that act decided — the *least*
                    // vouched-for kind of action rather than the most — so silence
                    // here would read as the opposite of what happened.
                    (None, _) => text.push_str(&format!(
                        "{SEPARATOR}no rule named it: the tier default decided"
                    )),
                }
                let mut lines = self.flush();
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
                let mut lines = self.flush();
                lines
                    .push(theme.notice(Tone::Warning, format!("{act} {target} — waiting for you")));
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
                let mut lines = self.flush();
                lines.push(theme.notice(
                    if decision == "deny" {
                        Tone::Refused
                    } else {
                        Tone::Muted
                    },
                    format!("{act} {target}{SEPARATOR}{decision}"),
                ));
                lines
            }
            EventKind::Finished {
                outcome,
                steps,
                tokens,
            } => {
                let mut lines = self.flush();
                let tone = outcome_tone(outcome);
                lines.push(theme.notice(tone, format!("{outcome} · {steps} steps · {tokens} tok")));
                if let Some(help) = outcome_help(outcome) {
                    lines.push(Line::from(Span::styled(
                        format!("  {help}"),
                        theme.style(Tone::Muted),
                    )));
                }
                lines.push(Line::from(""));
                lines
            }
            // The other forty-three kinds. Not styled in this release, and not
            // discarded either: each arrives as one muted line naming itself, so a
            // release that starts emitting something new is visible rather than
            // silent.
            other => {
                vec![Line::from(vec![
                    Span::styled("  · ", theme.style(Tone::Muted)),
                    Span::styled(kind_name(other), theme.style(Tone::Muted)),
                ])]
            }
        }
    }
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
