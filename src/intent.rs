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

use io_harness::{AnswerFuture, Question, Responder};
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

/// The overlay a question is asked through.
pub struct Intent {
    asked: Asked,
    composer: Composer,
}

impl Intent {
    /// Open on a question. The composer starts empty: an answer pre-filled with
    /// one of the agent's own choices is an answer the operator did not give.
    pub fn new(asked: Asked) -> Self {
        Self {
            asked,
            composer: Composer::new(),
        }
    }

    /// The question on screen, for whatever has to show or assert it.
    pub fn question(&self) -> &Question {
        &self.asked.question
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

    /// Send the answer back to the run. Consumes the overlay, because a question
    /// answered twice is a run that receives an answer nobody typed.
    pub fn resolve(self, answer: Option<String>) {
        let _ = self.asked.answer.send(answer);
    }

    /// The question, its context, the choices offered, and the prompt.
    ///
    /// The whole viewport, like an approval: the run is stopped, so there is
    /// nothing behind this worth half a screen.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(theme.notice(Tone::Warning, self.asked.question.question.clone()));
        if let Some(context) = &self.asked.question.context {
            lines.push(theme.notice(Tone::Muted, context.clone()));
        }
        for choice in &self.asked.question.choices {
            lines.push(theme.notice(
                Tone::Muted,
                format!("{} {choice}", theme.glyphs.bullet),
            ));
        }
        lines.push(theme.notice(
            Tone::Muted,
            format!(
                "type an answer {} Enter sends it {} Esc leaves it for later",
                theme.glyphs.dash, theme.glyphs.dash
            ),
        ));

        let head = u16::try_from(lines.len()).unwrap_or(u16::MAX).min(area.height);
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
