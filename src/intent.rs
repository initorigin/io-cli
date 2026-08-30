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
use crate::picker::{Outcome, Picker, Row};
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

/// What the last row says. The offers are the agent's words; this one is the
/// product's, and it has to read as a peer of them rather than as a way out.
const OWN_WORDS: &str = "answer in your own words";

/// The overlay a question is asked through, live or resumed.
///
/// # One list, no modes (0.32.0)
///
/// The offers and the free-text answer are rows of the same [`Picker`], not two
/// panes with a key that moves between them. `Enter` on an offer sends it
/// verbatim; taking the marker to the last row unfolds the [`Composer`] directly
/// beneath it, and `Enter` there sends what was typed. One marker, one list, one
/// interaction covering both ways to answer a question — and a question with no
/// choices at all is that same list with one row, already unfolded, which is the
/// surface this overlay had before.
///
/// The unfold is a general `Picker` mechanic and this is its only caller. That is
/// deliberate: 0.31.0 shipped an entire pipeline reachable from nothing but its own
/// tests, and a mode built before anything needs it is the same defect wearing a
/// different hat.
pub struct Intent {
    question: Question,
    answer: Destination<Option<String>>,
    composer: Composer,
    /// The offers and the free-text row, as one list.
    offers: Picker,
}

impl Intent {
    /// Open on a question. The composer starts empty: an answer pre-filled with
    /// one of the agent's own choices is an answer the operator did not give.
    pub fn new(asked: Asked) -> Self {
        let offers = Self::list(&asked.question);
        Self {
            question: asked.question,
            answer: Destination::Turn(asked.answer),
            composer: Composer::new(),
            offers,
        }
    }

    /// The offers, then the row that is not one.
    ///
    /// The free-text row is **last and is a peer**, not a footer: it is chosen the
    /// way every other row is chosen, and io-harness's own documentation says an
    /// answer is not obliged to be one of `choices`, so the surface should not
    /// imply otherwise. Its index is `choices.len()`, which is what the unfold is
    /// keyed on and what [`Intent::key`] focuses when the operator starts typing.
    fn list(question: &Question) -> Picker {
        let mut rows: Vec<Row> = question
            .choices
            .iter()
            .map(|choice| Row::new(choice.clone()))
            .collect();
        rows.push(Row::new(OWN_WORDS));
        let free = rows.len() - 1;
        let mut offers = Picker::new("choose an answer, or write one", rows);
        offers.set_unfold(free, crate::app::COMPOSER_ROWS);
        // **The marker opens on the free-text row, never on the agent's first
        // offer**, and that is a safety property rather than a preference. An
        // overlay that opens with an offer marked turns a reflexive `Enter` — the
        // key an operator has just pressed to submit the prompt that started this
        // turn — into silent agreement with a suggestion they have not read. The
        // product already refuses to pre-fill the composer with a choice for the
        // same reason: an answer the operator did not give.
        //
        // It also keeps the surface this overlay has always had. Opening here
        // means an untouched overlay is a composer with a question above it,
        // `Enter` on it is still the mis-key it has always been, and a question
        // with no choices at all is unchanged in every respect.
        offers.focus(free);
        offers
    }

    /// Open on a question a run already paused on, read back off the store.
    ///
    /// The row carries the answer and who gave it as well; neither is a question,
    /// so neither reaches the overlay — an answered row is not something to ask
    /// again, and refusing to open on one is the caller's job, which is the only
    /// place that knows whether it is resuming or replaying.
    pub fn resumed(pending: &PendingQuestion) -> Self {
        let question = Question {
            question: pending.question.clone(),
            context: pending.context.clone(),
            choices: pending.choices.clone(),
        };
        let offers = Self::list(&question);
        Self {
            question,
            answer: Destination::Stored,
            composer: Composer::new(),
            offers,
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
        use crossterm::event::KeyCode;

        if key.code == KeyCode::Esc {
            return Some(None);
        }
        // **Movement always belongs to the list**, or the composer would be a room
        // with no door out.
        let modifiers = key.modifiers;
        if matches!(
            key.code,
            KeyCode::Up | KeyCode::Down | KeyCode::Home | KeyCode::End | KeyCode::BackTab
        ) || (key.code == KeyCode::Tab
            && modifiers.contains(crossterm::event::KeyModifiers::SHIFT))
        {
            self.offers.key(key);
            return None;
        }

        // **Typing is always the answer, and the marker follows it.** This is the
        // one place the surface differs from every other `Picker` in the product,
        // and it is deliberate: a picker's printable keys filter its rows, which is
        // right for four hundred models and wrong for five offers. Here the
        // expensive act is answering, not finding — so a character moves the
        // marker to the row that takes prose and goes into the composer, and an
        // operator who simply starts typing gets what they meant with nothing to
        // press first. It also keeps the surface this overlay has always had:
        // before 0.32.0 every keystroke went to a composer, and a question with no
        // choices still behaves exactly that way.
        if matches!(key.code, KeyCode::Char(_))
            && !modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            && !modifiers.contains(crossterm::event::KeyModifiers::ALT)
        {
            self.offers.focus(self.question.choices.len());
            return self.typed(key);
        }

        // `Enter` and `Tab` take what is under the marker. On the free-text row
        // that is whatever has been typed into it; on an offer it is the offer.
        if self.offers.unfolded_now() {
            return self.typed(key);
        }
        match self.offers.key(key) {
            // The offer verbatim. Not the row's label re-read off the screen and
            // not a fitted copy of it: the string the agent sent is the string it
            // gets back, which is the whole reason `Outcome::Chosen` indexes the
            // caller's own unfiltered rows.
            Outcome::Chosen(index) => self
                .question
                .choices
                .get(index)
                .map(|choice| Some(choice.clone())),
            // **`Cancelled` cannot arrive, and saying so is the point.** The only
            // key a `Picker` cancels on is `Ctrl+C`, and `App::key` takes that
            // before this overlay ever sees it — its own comment settles the
            // question "does Ctrl+C decline, or interrupt?" with *it interrupts*.
            // An arm answering `Some(None)` here would be a second, contradictory
            // answer to that question, in a different file.
            Outcome::Cancelled | Outcome::Idle => None,
        }
    }

    /// A keystroke that belongs to the composer.
    ///
    /// An empty prompt submits nothing at all — `Enter` on an empty line is a
    /// mis-key, and answering the agent with an empty string would send it back to
    /// work knowing no more than it did.
    fn typed(&mut self, key: crossterm::event::KeyEvent) -> Option<Option<String>> {
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
    pub fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let lines = self.head(theme);
        // **Measured, not counted.** Through 0.31.0 this was `lines.len()` while
        // the paragraph below wraps, so a question or a context line longer than
        // the terminal is wide took more rows than the count admitted — the offers
        // fell off the bottom, and the composer was then drawn on top of rows the
        // paragraph had already painted. `rows::wrapped` asks ratatui's own
        // wrapper, so the two cannot disagree.
        let head = crate::rows::wrapped(&lines, area.width).min(area.height);
        frame.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            Rect {
                height: head,
                ..area
            },
        );

        // **The offers and the composer share the rest, and the list gives the
        // composer its rows rather than competing for them.** There is no
        // `if area.height > head` here: with the viewport sized to what this
        // overlay asked for there is room, and on a terminal too small for that
        // the picker elides its own rows with a count rather than silently
        // dropping the one thing the operator can type into.
        let Some(rest) = area.height.checked_sub(head).filter(|rest| *rest > 0) else {
            return;
        };
        let below = Rect {
            y: area.y.saturating_add(head),
            height: rest,
            ..area
        };
        self.offers.render(frame, below, theme);
        if let Some(open) = self.offers.opened() {
            self.composer.render(frame, open, theme);
        }
    }

    /// The question, its context, and the line naming the keys.
    ///
    /// One line of it depends on which way in was taken, and it is the only one:
    /// declining a live question defers it *within* a turn that is still running
    /// and will carry on the moment it is answered, while declining a resumed one
    /// leaves the run parked exactly as it was found — the operator opened it, so
    /// "for later" would be a promise nothing behind the screen is keeping. That
    /// difference is a word chosen here, not a second `render`.
    fn head(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        // **`Tone::Accent`, not `Tone::Warning`.** `Tone::Warning`'s word is
        // literally `warning`, so every question this agent asked was prefixed
        // with it — and `Tone::Refused`'s own doc keeps the vocabulary honest:
        // these tones mean something, and a question is not a warning. It is the
        // product's own colour because it is the product asking for the operator's
        // attention, which is exactly what the prompt marker uses it for.
        lines.push(theme.notice(Tone::Accent, self.question.question.clone()));
        if let Some(context) = &self.question.context {
            lines.push(theme.notice(Tone::Muted, context.clone()));
        }
        let leaves = if self.answer.parked() {
            "Esc leaves the run parked"
        } else {
            "Esc leaves it for later"
        };
        lines.push(theme.notice(
            Tone::Muted,
            format!(
                "Enter sends the marked row {} Tab too {} {leaves}",
                theme.glyphs.dash, theme.glyphs.dash
            ),
        ));
        lines
    }

    /// Rows this overlay would like the viewport to be.
    ///
    /// Its head as it will actually wrap, every offer, the free-text row, the
    /// composer unfolded beneath it, and the picker's own head row. A request:
    /// [`crate::app::App::viewport_wanted`] clamps it to what the terminal can
    /// spare, and [`Self::render`] degrades against whatever it is given.
    pub fn rows_wanted(&self, width: u16, theme: &Theme) -> u16 {
        crate::rows::wrapped(&self.head(theme), width).saturating_add(self.offers.rows_wanted())
    }
}
