//! The agent's question about intent, answered in the session it was asked in.
//!
//! **Not an approval, and the difference decides the surface.** An approval asks
//! whether an act is permitted and is answered with one of three keys; this asks
//! what was *meant*, its answer authorizes nothing, and there is no key that can
//! carry it — the answer is prose the operator types. So the overlay is a
//! [`Composer`] with a question over it rather than a row of choices, and the
//! agent's own `choices` are shown as what they are: offers, which
//! io-harness's own documentation says an answer is not obliged to take.
//!
//! Declining is a real answer. `Responder::answer` returning `None` persists the
//! question and pauses the run, resumable, which is what an operator who does not
//! know the answer needs — and it is what a session with no responder at all does
//! today, so `Esc` here leaves the run exactly where 0.9.0 would have left it.
//!
//! # Two ways in, one overlay (0.23.0)
//!
//! A question can also be met long after the run that asked it stopped, as a
//! `pending_questions` row read back off the store. [`Intent::resumed`] opens on
//! one, and from that point every keystroke, every drawn line and `Esc` itself
//! are the same code as the live path. The one difference is where the answer
//! goes once it is given: a live turn is awaiting it on a channel, a stored row
//! has nobody awaiting anything, so [`Intent::resolve`] hands it back to the
//! caller to deliver with `resume_with_answer_observed`.

use io_harness::{AnswerFuture, PendingQuestion, Question, Responder};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::{mpsc, oneshot};

use crate::composer::{Composer, Reply};
use crate::theme::{Theme, Tone};

/// One question, and the channel its answer goes back down.
#[derive(Debug)]
pub struct Asked {
    /// What the agent wants to know.
    pub question: Question,
    /// `None` declines, which pauses the run rather than answering it wrongly.
    pub answer: oneshot::Sender<Option<String>>,
}

/// The responder handed to a turn's contract.
///
/// Unbounded for the reason [`crate::approval`]'s channel is: the alternatives
/// are blocking the run and dropping a question, and a dropped question is a turn
/// that waits forever. The depth is one in practice — the run is stopped from the
/// moment it asks until the moment it is answered.
#[derive(Debug)]
pub struct Answerer {
    questions: mpsc::UnboundedSender<Asked>,
}

/// An answerer and the receiver the interface drains.
pub fn channel() -> (Answerer, mpsc::UnboundedReceiver<Asked>) {
    let (questions, rx) = mpsc::unbounded_channel();
    (Answerer { questions }, rx)
}

impl Responder for Answerer {
    fn answer<'a>(&'a self, question: &'a Question) -> AnswerFuture<'a> {
        let question = question.clone();
        Box::pin(async move {
            let (answer, reply) = oneshot::channel();
            // A closed channel and an interface that took the question and went
            // away are the same fact — nobody is going to answer — and both mean
            // `None`, which pauses the run with the question persisted rather
            // than failing it.
            let _ = self.questions.send(Asked { question, answer });
            reply.await.unwrap_or(None)
        })
    }
}

/// Where a decision goes once it is made.
///
/// **The two paths resolve differently and the type says so.** A live turn is
/// parked on a `oneshot` and is released by a send; a stored row is a run that
/// already ended, with no receiver anywhere and nothing to release — its answer
/// has to travel back out to the caller, which delivers it through
/// `io_harness::resume_with_answer_observed`. Faking a `oneshot` for that second
/// case would compile and would be a lie: in the live path a channel whose
/// receiver is gone is precisely the shape that means *nobody can answer, pause
/// the run*, so a stored answer sent down one would be indistinguishable from an
/// abandoned question.
///
/// Generic, and shared with [`crate::plan`], because a verdict has the identical
/// pair of destinations. A second copy of this enum is a second place the stored
/// path could quietly grow a channel nobody is holding.
#[derive(Debug)]
pub(crate) enum Destination<T> {
    /// A live turn is awaiting the value on the channel it handed over.
    Turn(oneshot::Sender<T>),
    /// A run that already stopped. Nothing is awaiting; the value is returned.
    Stored,
}

impl<T> Destination<T> {
    /// Deliver, and hand back whatever the caller must now deliver itself.
    ///
    /// `None` means it is already delivered — the turn had it the moment this
    /// returned. `Some(value)` means this overlay was opened on a stored row and
    /// the value has gone nowhere yet.
    pub(crate) fn deliver(self, value: T) -> Option<T> {
        match self {
            Self::Turn(sender) => {
                let _ = sender.send(value);
                None
            }
            Self::Stored => Some(value),
        }
    }

    /// Whether the run behind this overlay has already stopped.
    pub(crate) fn parked(&self) -> bool {
        matches!(self, Self::Stored)
    }
}

/// The overlay a question is asked through, live or resumed.
pub struct Intent {
    question: Question,
    answer: Destination<Option<String>>,
    composer: Composer,
}

impl Intent {
    /// Open on a question. The composer starts empty: an answer pre-filled with
    /// one of the agent's own choices is an answer the operator did not give.
    pub fn new(asked: Asked) -> Self {
        Self {
            question: asked.question,
            answer: Destination::Turn(asked.answer),
            composer: Composer::new(),
        }
    }

    /// Open on a question a run already paused on, read back off the store.
    ///
    /// The row carries the answer and who gave it as well; neither is a question,
    /// so neither reaches the overlay — an answered row is not something to ask
    /// again, and refusing to open on one is the caller's job, which is the only
    /// place that knows whether it is resuming or replaying.
    pub fn resumed(pending: &PendingQuestion) -> Self {
        Self {
            question: Question {
                question: pending.question.clone(),
                context: pending.context.clone(),
                choices: pending.choices.clone(),
            },
            answer: Destination::Stored,
            composer: Composer::new(),
        }
    }

    /// The question on screen, for whatever has to show or assert it.
    pub fn question(&self) -> &Question {
        &self.question
    }

    /// A keystroke while the overlay is up.
    ///
    /// `Some` closes it: `Some(Some(text))` answers, `Some(None)` declines. An
    /// empty prompt submits nothing at all — `Enter` on an empty line is a
    /// mis-key, and answering the agent with an empty string would send it back
    /// to work with no more information than it had.
    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Option<Option<String>> {
        if key.code == crossterm::event::KeyCode::Esc {
            return Some(None);
        }
        match self.composer.key(key) {
            Reply::Submitted(text) if !text.trim().is_empty() => Some(Some(text)),
            _ => None,
        }
    }

    /// Resolve the question. Consumes the overlay, because a question answered
    /// twice is a run that receives an answer nobody typed.
    ///
    /// Returns `None` when the answer has been delivered — a live turn was
    /// awaiting it and now has it. Returns `Some(answer)` when this overlay was
    /// opened by [`Self::resumed`]: there was no turn to send to, so the answer
    /// comes back out here and the caller delivers it with
    /// `io_harness::resume_with_answer_observed`. Dropping that value drops the
    /// operator's answer, which is why it is a return rather than a side effect.
    pub fn resolve(self, answer: Option<String>) -> Option<Option<String>> {
        self.answer.deliver(answer)
    }

    /// The question, its context, the choices offered, and the prompt.
    ///
    /// The whole viewport, like an approval: the run is stopped, so there is
    /// nothing behind this worth half a screen.
    ///
    /// One line of it depends on which way in was taken, and it is the only one:
    /// declining a live question defers it *within* a turn that is still running
    /// and will carry on the moment it is answered, while declining a resumed one
    /// leaves the run parked exactly as it was found — the operator opened it, so
    /// "for later" would be a promise nothing behind the screen is keeping. That
    /// difference is a word chosen here, not a second `render`.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(theme.notice(Tone::Warning, self.question.question.clone()));
        if let Some(context) = &self.question.context {
            lines.push(theme.notice(Tone::Muted, context.clone()));
        }
        for choice in &self.question.choices {
            lines.push(theme.notice(Tone::Muted, format!("{} {choice}", theme.glyphs.bullet)));
        }
        let leaves = if self.answer.parked() {
            "Esc leaves the run parked"
        } else {
            "Esc leaves it for later"
        };
        lines.push(theme.notice(
            Tone::Muted,
            format!(
                "type an answer {} Enter sends it {} {leaves}",
                theme.glyphs.dash, theme.glyphs.dash
            ),
        ));

        let head = u16::try_from(lines.len())
            .unwrap_or(u16::MAX)
            .min(area.height);
        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            Rect {
                height: head,
                ..area
            },
        );
        if area.height > head {
            self.composer.render(
                frame,
                Rect {
                    y: area.y + head,
                    height: area.height - head,
                    ..area
                },
                theme,
            );
        }
    }
}
