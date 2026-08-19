//! The plan the agent proposed, decided before any of it runs.
//!
//! **Registering a gate is what turns the planning phase on**, and while it is on
//! io-harness denies every write and every exec under a `plan-gate` layer — so at
//! the moment this overlay is up the workspace has not been touched and cannot
//! be. That is what makes cancelling cheap: it is not an undo, it is a decision
//! taken before there is anything to undo.
//!
//! Three verdicts, which are the three io-harness offers. `Enter` on an empty
//! prompt approves, text and `Enter` sends it back with that correction, and
//! `Esc` cancels — so the destructive answer is the one key that never means
//! anything else, and the correction is prose rather than a menu, because "not
//! like that, like this" is the whole value of a gate.

use io_harness::{Plan, PlanGate, PlanReview, PlanVerdict};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::{mpsc, oneshot};

use crate::composer::{Composer, Reply};
use crate::theme::{Theme, Tone};

/// One proposed plan, and the channel its verdict goes back down.
#[derive(Debug)]
pub struct Proposed {
    /// The steps, in the order the agent intends them.
    pub plan: Plan,
    /// `None` declines to decide, which pauses the run with the plan persisted.
    pub verdict: oneshot::Sender<Option<PlanVerdict>>,
}

/// The plan gate handed to a turn's contract.
#[derive(Debug)]
pub struct Gate {
    plans: mpsc::UnboundedSender<Proposed>,
}

/// A gate and the receiver the interface drains.
pub fn channel() -> (Gate, mpsc::UnboundedReceiver<Proposed>) {
    let (plans, rx) = mpsc::unbounded_channel();
    (Gate { plans }, rx)
}

impl PlanGate for Gate {
    fn review<'a>(&'a self, plan: &'a Plan) -> PlanReview<'a> {
        let plan = plan.clone();
        Box::pin(async move {
            let (verdict, reply) = oneshot::channel();
            // An interface that is gone and one that took the plan and went away
            // are the same fact, and both mean `None` — the run pauses with the
            // plan persisted rather than acting on a decision nobody made.
            let _ = self.plans.send(Proposed { plan, verdict });
            reply.await.unwrap_or(None)
        })
    }
}

/// The overlay a plan is decided through.
pub struct Review {
    proposed: Proposed,
    composer: Composer,
}

impl Review {
    /// Open on a plan, with an empty prompt: a correction pre-filled with
    /// anything is a correction the operator did not write.
    pub fn new(proposed: Proposed) -> Self {
        Self {
            proposed,
            composer: Composer::new(),
        }
    }

    /// The plan on screen.
    pub fn plan(&self) -> &Plan {
        &self.proposed.plan
    }

    /// A keystroke while the overlay is up. `Some` closes it with that verdict.
    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Option<PlanVerdict> {
        if key.code == crossterm::event::KeyCode::Esc {
            return Some(PlanVerdict::Cancel);
        }
        // **The approval is read here rather than off the composer's `Reply`,
        // because the composer will not submit an empty prompt at all** — that
        // refusal is right for a prompt, which must never send the model an empty
        // turn, and wrong here, where an empty prompt is the whole answer. There
        // is nothing to say about a plan you agree with, and a key meaning "yes,
        // and I have nothing to add" is one nobody would find.
        if key.code == crossterm::event::KeyCode::Enter
            && key.modifiers.is_empty()
            && self.composer.is_empty()
        {
            return Some(PlanVerdict::Approve);
        }
        match self.composer.key(key) {
            Reply::Submitted(text) if text.trim().is_empty() => None,
            Reply::Submitted(text) => Some(PlanVerdict::revise(text)),
            Reply::Idle => None,
        }
    }

    /// Send the verdict back to the run.
    pub fn resolve(self, verdict: Option<PlanVerdict>) {
        let _ = self.proposed.verdict.send(verdict);
    }

    /// The steps as steps, and the three ways out.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let mut lines: Vec<Line<'static>> = vec![theme.notice(
            Tone::Warning,
            format!(
                "a plan, before any of it runs {} {} steps",
                theme.glyphs.dash,
                self.proposed.plan.steps.len()
            ),
        )];
        for (index, step) in self.proposed.plan.steps.iter().enumerate() {
            let owner = match &step.agent {
                Some(agent) => format!(" [{agent}]"),
                None => String::new(),
            };
            lines.push(theme.notice(
                Tone::Normal,
                format!("{}. {}{owner}", index + 1, step.intent),
            ));
        }
        lines.push(theme.notice(
            Tone::Muted,
            format!(
                "Enter approves {} type a correction and Enter sends it back {} Esc cancels",
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
