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
//!
//! # Two ways in, one overlay (0.23.0)
//!
//! A plan can also be decided long after the run that proposed it stopped, as a
//! `plans` row read back off the store. [`Review::resumed`] opens on one, and
//! everything from there — the keys, the drawn steps, the footer — is the same
//! code as the live path. The one difference is where the verdict goes:
//! [`Review::resolve`] returns it rather than sending it, for the caller to
//! deliver with `resume_with_plan_decision_observed`.

use io_harness::{PendingPlan, Plan, PlanGate, PlanReview, PlanVerdict};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::{mpsc, oneshot};

use crate::app::COMPOSER_ROWS;
use crate::composer::{Composer, Reply};
use crate::intent::Destination;
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

/// The overlay a plan is decided through, live or resumed.
pub struct Review {
    plan: Plan,
    verdict: Destination<Option<PlanVerdict>>,
    composer: Composer,
}

impl Review {
    /// Open on a plan, with an empty prompt: a correction pre-filled with
    /// anything is a correction the operator did not write.
    pub fn new(proposed: Proposed) -> Self {
        Self {
            plan: proposed.plan,
            verdict: Destination::Turn(proposed.verdict),
            composer: Composer::new(),
        }
    }

    /// Open on a plan a run already paused on, read back off the store.
    ///
    /// Only `plan` is taken. The row's `verdict` and `decided_by` describe a
    /// decision already made, and a plan that has one is not one to decide again;
    /// keeping that check in the caller keeps this constructor from being the
    /// place a decided plan is quietly re-opened.
    pub fn resumed(pending: &PendingPlan) -> Self {
        Self {
            plan: pending.plan.clone(),
            verdict: Destination::Stored,
            composer: Composer::new(),
        }
    }

    /// The plan on screen.
    pub fn plan(&self) -> &Plan {
        &self.plan
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

    /// Resolve the plan. Consumes the overlay: a plan decided twice is a run
    /// spending its budget on a decision nobody took.
    ///
    /// Returns `None` when the verdict has been delivered to a live turn, and
    /// `Some(verdict)` when this overlay was opened by [`Self::resumed`] — there
    /// was no turn awaiting it, so it comes back here for the caller to deliver
    /// with `io_harness::resume_with_plan_decision_observed`.
    pub fn resolve(self, verdict: Option<PlanVerdict>) -> Option<Option<PlanVerdict>> {
        self.verdict.deliver(verdict)
    }

    /// The steps as steps, and the three ways out.
    ///
    /// Identical for both ways in, footer included, and that is not an oversight:
    /// `Esc` here is a verdict rather than a deferral. It cancels the plan on a
    /// live turn and on a resumed one alike, so "Esc cancels" is true of both —
    /// unlike [`crate::intent`], where declining defers and the two paths defer
    /// to different places.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let header = self.header(theme);
        let footer = self.footer(theme);
        // The composer keeps its row wherever there is one to keep. Below that
        // there is no correction to type and the plan is approved or cancelled by
        // a key, which are the two verdicts a single row can carry.
        let composer_rows = if area.height > 2 { COMPOSER_ROWS } else { 0 };
        let text_rows = area.height.saturating_sub(composer_rows);

        // **The header and the footer are reserved; the steps are what gives
        // way.** Through 0.31.0 every line went into one `Paragraph` sized to
        // `lines.len()`, and a `Paragraph` simply stops painting at the bottom of
        // its area — so the last thing pushed was the first thing lost, and the
        // last thing pushed is the footer. That made the one overlay whose own
        // module doc forbids losing its footer the only overlay that always lost
        // it first, and it lost it before `lines.len()` even reached the height,
        // because these lines wrap.
        let head_rows = crate::rows::wrapped(std::slice::from_ref(&header), area.width);
        let foot_rows = crate::rows::wrapped(std::slice::from_ref(&footer), area.width);
        let room = text_rows
            .saturating_sub(head_rows)
            .saturating_sub(foot_rows);

        let mut lines = vec![header];
        lines.extend(crate::rows::elide(
            self.steps(theme),
            room,
            area.width,
            theme,
        ));
        lines.push(footer);

        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            Rect {
                height: text_rows,
                ..area
            },
        );
        if composer_rows > 0 {
            self.composer.render(
                frame,
                Rect {
                    y: area.y.saturating_add(text_rows),
                    height: composer_rows,
                    ..area
                },
                theme,
            );
        }
    }

    /// What this is, and how much of it there is.
    ///
    /// **`Tone::Accent`, not `Tone::Warning`.** A plan the agent is asking
    /// permission to run is not a warning, and `Tone::Warning`'s word is literally
    /// `warning` — so under `NO_COLOR` every proposal announced itself as one.
    /// `Tone::Refused` is left meaning what its own doc says it means, and the
    /// rest of the vocabulary has to be worth the same.
    fn header(&self, theme: &Theme) -> Line<'static> {
        theme.notice(
            Tone::Accent,
            format!(
                "a plan, before any of it runs {} {} steps",
                theme.glyphs.dash,
                self.plan.steps.len()
            ),
        )
    }

    /// The three ways out. Never elided: an operator who cannot see the keys is an
    /// operator guessing at a surface that is holding a run.
    fn footer(&self, theme: &Theme) -> Line<'static> {
        theme.notice(
            Tone::Muted,
            format!(
                "Enter approves {} type a correction and Enter sends it back {} Esc cancels",
                theme.glyphs.dash, theme.glyphs.dash
            ),
        )
    }

    fn steps(&self, theme: &Theme) -> Vec<Line<'static>> {
        self.plan
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let owner = match &step.agent {
                    Some(agent) => format!(" [{agent}]"),
                    None => String::new(),
                };
                theme.notice(
                    Tone::Normal,
                    format!("{}. {}{owner}", index + 1, step.intent),
                )
            })
            .collect()
    }

    /// Rows this overlay would like the viewport to be: its header, every step as
    /// it will actually wrap, its footer, and the composer.
    ///
    /// A twelve-step plan asks for a twelve-step plan. Whether it gets one is
    /// [`crate::app::App::viewport_wanted`]'s decision, and what it does when it
    /// does not is the elision above.
    pub fn rows_wanted(&self, width: u16, theme: &Theme) -> u16 {
        let mut lines = vec![self.header(theme)];
        lines.extend(self.steps(theme));
        lines.push(self.footer(theme));
        crate::rows::wrapped(&lines, width).saturating_add(COMPOSER_ROWS)
    }
}
