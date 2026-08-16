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

    /// The text streamed but not yet committed. Drawn in the viewport, where it
    /// can be replaced as it grows; a committed line cannot be.
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
                Vec::new()
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
                let mut detail = format!("step {}", event.step);
                if !tool_call.is_empty() {
                    detail.push_str(&format!(" · {tool_call}"));
                }
                detail.push_str(&format!(" · {tokens} tok"));
                if *changed {
                    detail.push_str(" · changed files");
                }
                lines.push(Line::from(vec![
                    Span::styled(decision.clone(), theme.style(Tone::Muted)),
                    Span::styled(format!("  {detail}"), theme.style(Tone::Muted)),
                ]));
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
                // A plain notice in this release. The surface that names the rule
                // and the layer properly is 0.2.0's; what must not happen now is
                // pretending to a surface that does not exist, or losing the two
                // facts no other core records.
                let mut text = format!("{act} {target}");
                if let Some(rule) = rule {
                    text.push_str(&format!(" — rule {rule}"));
                }
                if let Some(layer) = layer {
                    text.push_str(&format!(", layer {layer}"));
                }
                let mut lines = self.flush();
                lines.push(theme.notice(Tone::Refused, text));
                lines
            }
            EventKind::ApprovalRequested { act, target } => {
                let mut lines = self.flush();
                lines.push(theme.notice(
                    Tone::Warning,
                    format!("{act} {target} needs approval; this release answers from the configured posture"),
                ));
                lines
            }
            EventKind::Finished {
                outcome,
                steps,
                tokens,
            } => {
                let mut lines = self.flush();
                let tone = if outcome == "success" {
                    Tone::Success
                } else {
                    Tone::Warning
                };
                lines.push(theme.notice(tone, format!("{outcome} · {steps} steps · {tokens} tok")));
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
