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
//!
//! # Several questions, one overlay (0.33.0)
//!
//! io-harness 0.72.0 gave [`Responder`] a defaulted `answer_all`. Its default body
//! loops [`Responder::answer`] once per question, in order — which is precisely
//! what this interface could always do, and precisely the shape that made five
//! questions arrive as five consecutive overlays, each one hiding the four behind
//! it. [`Answerer`] overrides it so a batch crosses the channel as one
//! [`Questions`] and opens one [`Intent`].
//!
//! **The harness commits a batch only when every entry is `Some`.** Answer four of
//! five and the whole batch parks for a human — so the overlay does not deliver
//! anything until every question has been *decided*, and declining is a decision.
//! That is why `Esc` on a batch decides the question on screen rather than closing
//! the overlay: `Esc` has never answered anything, and a batch carrying one `None`
//! parks the run exactly as a single declined question does.
//!
//! **One question, unchanged.** A [`Questions`] of one is a delivery of one, the
//! overlay has no batch chrome on it, and every key does what it did in 0.32.0.
//!
//! **A parked batch resumes as a batch.** The two ways in stay one surface: the
//! store parks a whole `ask_questions` as one row whose own columns are a
//! *rendering* of the ask rather than the ask, so [`Intent::resumed`] takes the
//! questions out of `PendingQuestion::questions` and hands them to the same
//! constructor a live batch goes through. Delivery is still singular, because the
//! row is — see [`Intent::resolve`].
//!
//! # An offer can say more than its label (0.33.0)
//!
//! io-harness 0.72.0 turned a choice from a string into a `Choice`, which may
//! carry a **description** — one sentence saying what taking it means — and a
//! **preview** — a short block showing what taking it would do. Both reached this
//! overlay the moment the harness was pinned, and neither was drawn.
//!
//! They are drawn differently because they are different things. A description is
//! always visible, on a row of its own under the label, because a reader comparing
//! five offers needs all five sentences at once. A preview is a block, and five
//! blocks at once is a wall nobody reads — so it unfolds under the marker, one at
//! a time, and folds when the marker moves. The list builder carries the argument
//! for both, and `quoted` the one for why the block is marked the way the rest of
//! this product marks quoted words.
//!
//! **The harness bounds a preview before it sends one; this overlay does not
//! restate those bounds as its own.** What arrives is drawn. What is bounded here
//! is the *drawing*: the block asks for the rows it wraps to, and the viewport
//! clamps that to what the terminal can spare.

use io_harness::{AnswerFuture, AnswersFuture, PendingQuestion, Question, Responder};
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

/// What one delivery carries: a question, or a batch of them asked together.
///
/// **The unit of the channel, so that a batch cannot be torn apart in transit.**
/// One `recv` is one overlay — five questions asked as a batch arrive as one
/// value and are answered on one surface, which is the whole reason
/// [`Responder::answer_all`] is overridden below.
///
/// **Never empty**, and that is a construction-time property rather than a
/// checked one: the only two ways to build one are [`From<Asked>`](Asked), which
/// carries exactly one, and [`Answerer::answer_all`], which returns early on an
/// empty slice rather than sending. The field is private, so no caller outside
/// this module can make a third. [`Intent`] indexes on that.
#[derive(Debug)]
pub struct Questions {
    asked: Vec<Asked>,
}

impl Questions {
    /// How many questions this delivery is carrying. Never zero.
    pub fn len(&self) -> usize {
        self.asked.len()
    }

    /// Never true. Present because [`Self::len`] exists and a length without an
    /// emptiness test is a lint; the honest answer is the invariant above.
    pub fn is_empty(&self) -> bool {
        self.asked.is_empty()
    }
}

impl From<Asked> for Questions {
    fn from(one: Asked) -> Self {
        Self { asked: vec![one] }
    }
}

/// The responder handed to a turn's contract.
///
/// Unbounded for the reason [`crate::approval`]'s channel is: the alternatives
/// are blocking the run and dropping a question, and a dropped question is a turn
/// that waits forever. The depth is one in practice — the run is stopped from the
/// moment it asks until the moment it is answered.
#[derive(Debug)]
pub struct Answerer {
    questions: mpsc::UnboundedSender<Questions>,
}

/// An answerer and the receiver the interface drains.
pub fn channel() -> (Answerer, mpsc::UnboundedReceiver<Questions>) {
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
            let _ = self.questions.send(Asked { question, answer }.into());
            reply.await.unwrap_or(None)
        })
    }

    /// A batch crosses as **one** delivery, and one overlay answers all of it.
    ///
    /// The trait's default body would loop [`Self::answer`], which is what this
    /// interface did before io-harness 0.72.0 existed and is not wrong — it is
    /// merely one question at a time, and the second question is not even sent
    /// until the first has been answered. Overriding it is the only way the
    /// operator ever sees that there are five.
    ///
    /// **A reply channel per question, not one for the batch.** The vector the
    /// harness gets back is built by awaiting them in the order they were asked,
    /// so the ordering is a property of the construction rather than of anything
    /// the overlay remembers to do — and declining stays per question, which is
    /// what the harness's own `Vec<Option<String>>` is for.
    ///
    /// Every failure lands on the same safe answer. A closed channel, an interface
    /// that took the batch and went away, and an overlay dropped mid-answer all
    /// drop the senders, every `await` resolves `None`, and the run parks with the
    /// questions persisted.
    fn answer_all<'a>(&'a self, questions: &'a [Question]) -> AnswersFuture<'a> {
        Box::pin(async move {
            // Nothing to ask. Sending an empty batch would open a modal overlay
            // with no question on it and no key that closes it.
            if questions.is_empty() {
                return Vec::new();
            }
            let mut replies = Vec::with_capacity(questions.len());
            let asked: Vec<Asked> = questions
                .iter()
                .map(|question| {
                    let (answer, reply) = oneshot::channel();
                    replies.push(reply);
                    Asked {
                        question: question.clone(),
                        answer,
                    }
                })
                .collect();
            let _ = self.questions.send(Questions { asked });
            let mut answers = Vec::with_capacity(replies.len());
            for reply in replies {
                answers.push(reply.await.unwrap_or(None));
            }
            answers
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

/// A resumed batch's decisions as the one text its row is answered with.
///
/// **Every answer beside the question it answers**, because a bare list of five
/// sentences is not readable as an answer set and the model would have to re-derive
/// the pairing from position — which is io-harness's own argument for the block it
/// writes when a [`Responder`] answers a batch in the process that asked. This is
/// deliberately the same shape, so the model reads the same thing whether the batch
/// was answered live or a day later through `/resume`. It is spelled out here
/// rather than called because the harness's own `assemble_answers` is private to
/// its run loop; the cost is stated rather than hidden, and it is why the test for
/// this asserts the pairing rather than a literal block.
///
/// **`None` when anything was left unanswered**, and that is the same rule the
/// harness applies to a live batch: a batch is answered wholly or not at all, so a
/// decline anywhere in it means there is no answer to resume with and the run stays
/// parked on the row it was found on. Assembling a text with a hole in it would
/// resolve the row — one compare-and-swap, no second chance — on an ask the
/// operator did not finish.
fn assembled(batch: &[Question], answers: &[Option<String>]) -> Option<String> {
    if answers.len() != batch.len() || answers.iter().any(Option::is_none) {
        return None;
    }
    let mut text = String::new();
    for (at, (question, answer)) in batch.iter().zip(answers).enumerate() {
        let answer = answer.as_deref().unwrap_or_default();
        text.push_str(&format!("{}. {}\n   {answer}\n", at + 1, question.question));
    }
    Some(text.trim_end().to_string())
}

/// What the last row says. The offers are the agent's words; this one is the
/// product's, and it has to read as a peer of them rather than as a way out.
///
/// Public so a test can name the row rather than assert a string it wrote out
/// itself — the row's position is F9's whole claim, and a literal here and there
/// is two spellings of one row.
pub const OWN_WORDS: &str = "answer in your own words";

/// What a description is indented by so it sits under the label it explains.
///
/// Two cells, which is [`crate::glyphs::Glyphs::marker`]'s width in both glyph
/// sets and the width of the blank the picker draws in its place on an unmarked
/// row — so a description starts under its label's first character. On a question
/// that takes several the picker draws a four-cell box before every label and the
/// description stays where it is: it is prose *about* the row above it rather
/// than another row in that column, and the alternative is this file writing out
/// the width of a box that is private to `picker.rs`, which is a second spelling
/// of a number only that file should own.
const EXPLAINED: &str = "  ";

/// The rows the offers occupy, one entry per choice, in the list [`Intent::list`]
/// builds.
///
/// **Not `0..choices.len()`, and that is the whole reason this exists.** A choice
/// carrying a description takes a second row for it, so every offer after it has
/// moved down. Everything this overlay exchanges with the [`Picker`] —
/// [`Outcome::Chosen`], [`Picker::chosen`], [`Picker::set_unfold`],
/// [`Picker::focus`] — is a **row**; everything it exchanges with io-harness is a
/// **choice**; and the two stopped being the same number the moment a description
/// got a line of its own. Translating in one place is what keeps `Enter` on the
/// fourth offer from answering with the third.
///
/// A pure function of the question rather than a field, because the layout is a
/// pure function of the question: a copy held in [`Intent`] is a copy that can
/// disagree with the picker it describes.
fn offer_rows(question: &Question) -> Vec<usize> {
    let mut rows = Vec::with_capacity(question.choices.len());
    let mut at = 0;
    for choice in &question.choices {
        rows.push(at);
        at += 1 + usize::from(explanation(choice).is_some());
    }
    rows
}

/// The description an offer carries, or `None` when it carries nothing worth a
/// row.
///
/// Blank is `None`. A `Some("")` — or a description of nothing but spaces —
/// would otherwise buy an empty row under the label, which reads as a gap the
/// list has for no reason.
fn explanation(choice: &io_harness::Choice) -> Option<&str> {
    choice
        .description
        .as_deref()
        .map(str::trim)
        .filter(|description| !description.is_empty())
}

/// A preview as the block that is drawn for it.
///
/// **The quoting vocabulary is the one this product already draws for quoted
/// words**: [`crate::markdown`] renders a model's `> ` blockquote as
/// `theme.glyphs.rule` followed by a space, and this is the same prefix on the
/// same [`Tone::Muted`]. The roadmap wanted the diff renderer's gutter instead;
/// that gutter is not reusable — `diff.rs`'s `GUTTER`, `number`, `blank_gutter`,
/// `unchanged`, `changed` and `spans` are all private and every one of them is
/// shaped around an [`io_harness::Edit`]'s hunks, which a preview is not. Of the
/// two quoting idioms that do exist, the blockquote is the one that *means*
/// "these are somebody else's words", which is exactly what a preview is; the
/// approval overlay's two-space indent means "this is file content" and carries
/// no mark at all, so an indented block with nothing in front of it is
/// indistinguishable from a wrapped row of the list above it. io-harness's own
/// reference responder prefixes with `| `, which is a third spelling this product
/// does not otherwise use — and adopting it would mean either a new
/// [`crate::glyphs::Glyphs`] field or a character chosen here rather than by the
/// glyph set, which is how a terminal that cannot draw something ends up drawing
/// a box.
///
/// It therefore **differs between the glyph sets** — `─ ` in Unicode and `- ` in
/// ASCII — exactly as the markdown blockquote already does, and both are one cell
/// plus a space so the block occupies the same column either way.
///
/// **Only the first row of a wrapped line carries the prefix.** The block is
/// drawn by a wrapping `Paragraph`, which is what makes [`crate::rows::wrapped`]
/// the honest measurement of its height; re-marking a continuation row would mean
/// wrapping the text here, which is a second wrapper to disagree with ratatui's.
/// Stated rather than hidden, because it is visible on a narrow terminal.
///
/// **What arrives is drawn.** io-harness bounds a preview before it is sent; this
/// does not restate those limits as a promise of its own — a number copied here
/// is a number that goes on being asserted after the harness changes it. What is
/// bounded here is the *drawing*: the block asks for the rows it wraps to and
/// [`crate::app::App::viewport_wanted`] clamps that to what the terminal spares,
/// and the picker elides against whatever it is given.
fn quoted(preview: &str, theme: &Theme) -> Vec<Line<'static>> {
    preview
        .lines()
        .map(|line| theme.notice(Tone::Muted, format!("{} {line}", theme.glyphs.rule)))
        .collect()
}

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
///
/// # A batch is answered in place, and reviewed as it is answered (0.33.0)
///
/// **The contract's open question was whether a batch needs a separate review
/// surface before it is sent. It does not, and this is the answer that ships.**
///
/// The overlay shows one question of the batch at a time — its question line, its
/// context, its offers, its free-text row: the same surface a single question has,
/// so nothing about answering one question changes because four others exist.
/// Deciding it moves to the next question that has not been decided; deciding the
/// last one delivers the whole batch. `PgUp` and `PgDn` walk the batch in either
/// direction at any point before that, and a question already decided re-opens
/// with its own answer back in the composer, so an "edit" is the operator retyping
/// or re-sending their own words rather than a second kind of screen with a second
/// set of keys.
///
/// A separate review pane was rejected for three reasons, in order of weight.
/// **It would be a second surface for the same act** — the answers are already all
/// on the screen the operator typed them into, one page-key apart, and a list of
/// them elsewhere is a second rendering of state that can disagree with the first.
/// **It would need a submit key that answers nothing**, and the one thing this
/// overlay has always been careful about is a reflexive `Enter` meaning something
/// nobody read — F9's marker rule exists for exactly that. **And it would make the
/// simple case pay for the rare one**: a batch of one is overwhelmingly the common
/// delivery, and a review step it must pass through is a keystroke added to every
/// question the agent has ever asked.
///
/// What the operator is owed instead is knowing where they are and that nothing has
/// been sent yet, and that is two lines of the head: which question of how many,
/// and what this one was already decided as if they have come back to it.
///
/// **Nothing is delivered until every question is decided**, because io-harness
/// commits a batch only when every entry is `Some` — four answers out of five park
/// the run just as thoroughly as none. Declining is a decision, so `Esc` decides
/// the question on screen and moves on rather than closing the overlay: the run
/// still parks, which is the only thing `Esc` has ever promised.
pub struct Intent {
    /// Every question in this delivery, in the order the agent asked them.
    ///
    /// Never empty — see [`Questions`], which can only be built carrying at least
    /// one — which is what lets [`Self::current`] index rather than answer
    /// `Option`.
    batch: Vec<Question>,
    /// Where each question's answer goes, one per entry of `batch`.
    ///
    /// A destination *per question* rather than one for the batch, because a live
    /// batch is N turns' worth of channels: the harness handed out one reply
    /// channel per question and expects each of them to resolve.
    answers: Vec<Destination<Option<String>>>,
    /// What has been decided for each question, `None` until it has been.
    ///
    /// The outer `Option` is *decided at all*; the inner one is io-harness's own
    /// meaning, where `None` is "nobody here can answer this". Collapsing the two
    /// would make a declined question indistinguishable from an unvisited one, and
    /// the overlay would deliver a batch the operator never finished.
    decided: Vec<Option<Option<String>>>,
    /// What is in the composer for each question, kept across a move.
    ///
    /// Half a typed answer is the operator's work, and a page key that threw it
    /// away would be the surface losing something nobody could get back. For a
    /// decided question this holds the decision, which is what re-opening one puts
    /// back in the composer.
    drafts: Vec<String>,
    /// Which question is on screen.
    at: usize,
    composer: Composer,
    /// The offers and the free-text row of the question on screen, as one list.
    ///
    /// Rebuilt on every move rather than kept per question: the picker's state
    /// that matters here is the marker, and the marker's position is fixed by F9
    /// on arrival anyway. The cost is stated rather than hidden — the marks on a
    /// `multiple` question are not restored when it is re-opened, and the decision
    /// they produced is in the composer instead, as the prose it always was.
    offers: Picker,
    /// The tallest unfold `offers` has actually been **told** about.
    ///
    /// **A preview's height is a function of the width it is drawn at, and the
    /// width is not known until something draws.** [`Self::render`] measures every
    /// preview through [`crate::rows::wrapped`] and hands each one to
    /// [`Picker::set_unfold`] before the picker reserves anything — but the driver
    /// reads the demand *before* it draws (`paint_picker` calls
    /// [`crate::app::App::viewport_wanted`] and only then `draw`), so on the first
    /// frame of a question the picker has been told nothing and its reservation is
    /// the composer's row alone.
    ///
    /// [`Self::rows_wanted`] therefore measures the previews itself and adds
    /// whatever the picker is not already reserving — and this is the number it
    /// subtracts, so the same total comes out before and after a frame. Without it
    /// the demand either lags a frame behind the surface (an overlay that opens
    /// too short and grows under the operator's hands) or double-counts the block
    /// once render has run.
    reserved: u16,
}

impl Intent {
    /// Open on a delivery, live. The composer starts empty: an answer pre-filled
    /// with one of the agent's own choices is an answer the operator did not give.
    ///
    /// Takes anything a delivery can be made from, so a single [`Asked`] still
    /// opens an overlay by itself and the driver's select arm hands over whatever
    /// the channel gave it.
    pub fn new(questions: impl Into<Questions>) -> Self {
        let Questions { asked } = questions.into();
        let mut batch = Vec::with_capacity(asked.len());
        let mut answers = Vec::with_capacity(asked.len());
        for one in asked {
            batch.push(one.question);
            answers.push(Destination::Turn(one.answer));
        }
        Self::opened(batch, answers)
    }

    /// The **one** way an overlay is built, whichever way in was taken.
    ///
    /// The two constructors differ in exactly one thing — where each answer goes —
    /// and nothing else about a question overlay may depend on that. Before 0.33.0
    /// [`Self::resumed`] assembled its own struct literal beside this one, and the
    /// copy is precisely how a resumed batch came to be drawn as a single question
    /// with no offers on it: the stored path built its own `Question` instead of
    /// taking the ones the agent asked, and [`Self::list`] never saw them. One
    /// construction, so a surface the live path has is a surface the stored path
    /// has.
    ///
    /// `batch` is never empty — [`Questions`] cannot be, and [`Self::resumed`]
    /// falls back to the row's own columns when the store holds no batch — which is
    /// what lets this index it.
    fn opened(batch: Vec<Question>, answers: Vec<Destination<Option<String>>>) -> Self {
        let offers = Self::list(&batch[0]);
        Self {
            decided: vec![None; batch.len()],
            drafts: vec![String::new(); batch.len()],
            at: 0,
            batch,
            answers,
            composer: Composer::new(),
            offers,
            reserved: crate::app::COMPOSER_ROWS,
        }
    }

    /// The question on screen.
    ///
    /// Indexes rather than answering `Option` because [`Questions`] cannot be
    /// empty and [`Self::at`] is only ever set from a position in `batch`.
    fn current(&self) -> &Question {
        &self.batch[self.at]
    }

    /// The offers, then the row that is not one.
    ///
    /// The free-text row is **last and is a peer**, not a footer: it is chosen the
    /// way every other row is chosen, and io-harness's own documentation says an
    /// answer is not obliged to be one of `choices`, so the surface should not
    /// imply otherwise. Its index is the **last row's**, which is what the unfold
    /// is keyed on and what [`Intent::key`] focuses when the operator starts
    /// typing. Not `choices.len()` since 0.33.0 — a described offer takes a row
    /// for its description, so the two numbers part company the first time an
    /// agent explains one of its own offers; see [`offer_rows`].
    ///
    /// **A description is a row of its own, and a heading row.** The agent's
    /// sentence about an offer is not a second thing to choose, so it must not be
    /// selectable, must not take the marker on the way past, and must not be
    /// answerable — which is exactly what [`Row::heading`] already is, and the
    /// reason this needs no new picker mechanic. Folding it into the row's
    /// `detail` instead would put it on the same line, where the picker fits it
    /// into whatever the label leaves and drops it altogether on a narrow
    /// terminal: the first thing cut would be the explanation of the choice being
    /// made, which is the opposite of what it is for.
    ///
    /// **A description is always visible and a preview is not**, and the
    /// difference is what each of them is. A description is one sentence saying
    /// what taking the offer *means*, and a reader comparing five offers needs all
    /// five of those at once. A preview is a block showing what taking it would
    /// *do*; five of those on screen together is a wall nobody reads, so it
    /// unfolds under the marker instead, one at a time, which is the mechanic
    /// [`Picker::set_unfold`] exists for.
    ///
    /// **The cost, stated rather than hidden:** the picker opens a block directly
    /// beneath the row that owns it, so on an offer carrying *both* a description
    /// and a preview the open preview sits between the label and its description
    /// rather than after it. Nothing is lost and nothing overlaps — the
    /// description is still on the screen, one block lower, and only while that
    /// offer holds the marker. Putting it back in its place would mean the picker
    /// knowing that some rows belong to the row above them, which is a second
    /// grouping mechanic bought for one arrangement of two optional fields.
    ///
    /// **The spacebar is spent only on a question that says it takes several.**
    /// [`Picker::accepting_several`] costs a picker its space key, and a question
    /// answered with one choice — every question written before io-harness 0.72.0,
    /// and most written since — is answered in prose with spaces in it. Opting in
    /// unconditionally would take the spacebar from every two-word answer in the
    /// product to buy a mark nobody can make.
    fn list(question: &Question) -> Picker {
        let mut rows: Vec<Row> = Vec::with_capacity(question.choices.len() + 1);
        for choice in &question.choices {
            rows.push(Row::new(choice.label.clone()));
            if let Some(description) = explanation(choice) {
                rows.push(Row::heading(format!("{EXPLAINED}{description}")));
            }
        }
        rows.push(Row::new(OWN_WORDS));
        let free = rows.len() - 1;
        let mut offers = Picker::new(
            if question.multiple {
                "space marks several, or write an answer"
            } else {
                "choose an answer, or write one"
            },
            rows,
        );
        if question.multiple {
            offers = offers.accepting_several();
        }
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
    ///
    /// # A parked batch is a batch, and the row's own columns are not it (0.33.0)
    ///
    /// **`questions` is read first, and it is the only place a parked batch
    /// survives.** io-harness 0.72.0 parks a whole `ask_questions` as *one*
    /// `pending_questions` row, and that row's columns are a rendering rather than
    /// the ask: `question` is the batch as numbered prose (`"1. …\n2. …"`),
    /// `context` is the **first** question's context, and `choices` is empty
    /// because the synthesised row question has none. Building from those three
    /// gave a resumed batch one accent line holding embedded newlines — which
    /// ratatui does not break a `Line` on, so every question ran together into one
    /// wrapped paragraph — question one's context presented as everyone's, a picker
    /// with no offers at all, and `multiple` defaulted to `false`, which is no
    /// marks and no [`Question::answer_of`]. Everything the operator needed was in
    /// `questions` and nothing read it.
    ///
    /// It is reachable through this release's own flow rather than at an edge: the
    /// batch overlay's `Esc` decides a question as unanswered and moves on, the
    /// harness commits a batch only when every entry is `Some`, so the run parks —
    /// and `/resume` opens it here. `io exec` reaches it on the first ask, having
    /// no [`Responder`] at all.
    ///
    /// **Empty `questions` is the singular ask and stays exactly as it was**, which
    /// is not a fallback so much as the other half of the store's shape:
    /// `Store::put_question` writes no `questions` value, and a row written by
    /// 0.71.0 has no such column to write. Those rows are built from `question`,
    /// `context` and `choices` as they always were.
    pub fn resumed(pending: &PendingQuestion) -> Self {
        let batch = if pending.questions.is_empty() {
            let mut question =
                Question::new(pending.question.clone()).with_choices(pending.choices.clone());
            if let Some(context) = pending.context.clone() {
                question = question.with_context(context);
            }
            vec![question]
        } else {
            pending.questions.clone()
        };
        // One destination per question, as the live path has, so `record` and
        // `key` cannot tell the two apart. What differs is delivery: the row is
        // one row however many questions it parked, and `resolve` says so.
        let answers = batch.iter().map(|_| Destination::Stored).collect();
        Self::opened(batch, answers)
    }

    /// The question on screen, for whatever has to show or assert it.
    pub fn question(&self) -> &Question {
        self.current()
    }

    /// The offers and the free-text row of the question on screen.
    ///
    /// Read by the tests, which assert F9 — that the free-text row is last and
    /// holds the marker — **by index**. A screen assertion cannot see which row is
    /// focused, and the marker's position is the whole of that decision: an
    /// overlay opening on the agent's first offer turns a reflexive `Enter` into
    /// agreement with a suggestion nobody read.
    pub fn offers(&self) -> &Picker {
        &self.offers
    }

    /// A keystroke while the overlay is up.
    ///
    /// `Some` closes it, and carries **one entry per question of the delivery, in
    /// the order the agent asked them** — `Some(text)` answers, `None` declines.
    /// A single question is a vector of one, which is every delivery this overlay
    /// saw before 0.33.0.
    ///
    /// `None` means the overlay stays up, which now covers one more case than it
    /// did: a question of a batch that was just decided while others have not
    /// been. Nothing is delivered until every one of them has been, because
    /// io-harness commits a batch only when every entry is `Some` and a half
    /// answered batch parks the run with no more information than an empty one.
    ///
    /// An empty prompt submits nothing at all — `Enter` on an empty line is a
    /// mis-key, and answering the agent with an empty string would send it back
    /// to work with no more information than it had.
    pub fn key(&mut self, key: crossterm::event::KeyEvent) -> Option<Vec<Option<String>>> {
        use crossterm::event::KeyCode;

        // **The page keys walk the batch, and only when there is a batch.** On a
        // single question they fall through to the composer exactly as they did
        // before, which is the 0.32.0 surface and is not being reopened.
        if self.batch.len() > 1 && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown) {
            let last = self.batch.len() - 1;
            let to = if key.code == KeyCode::PageUp {
                self.at.saturating_sub(1)
            } else {
                self.at.saturating_add(1).min(last)
            };
            self.go(to);
            return None;
        }
        let decision = self.decision(key)?;
        self.record(decision)
    }

    /// Store what was decided for the question on screen, then move on — or
    /// deliver, when there is nothing left to move to.
    ///
    /// **The search wraps**, because the operator can have paged backwards: after
    /// deciding question two of five with four still open, the next thing to ask is
    /// three, and after deciding five it is whatever was left behind.
    fn record(&mut self, decision: Option<String>) -> Option<Vec<Option<String>>> {
        if let Some(text) = &decision {
            // Held as this question's draft as well as its decision, so re-opening
            // it puts the operator's own words back rather than an empty line.
            self.drafts[self.at] = text.clone();
        }
        self.decided[self.at] = Some(decision);
        let count = self.batch.len();
        let next = (1..=count)
            .map(|step| (self.at + step) % count)
            .find(|at| self.decided[*at].is_none());
        match next {
            Some(next) => {
                self.go(next);
                None
            }
            // Every question decided. The outer `Option` is `Some` on every entry
            // by construction here, so flattening cannot invent an answer.
            None => Some(
                self.decided
                    .iter()
                    .map(|decided| decided.clone().flatten())
                    .collect(),
            ),
        }
    }

    /// Put a different question of the batch on screen.
    ///
    /// The draft is saved only for a question that has **not** been decided:
    /// `Composer::key` clears the composer as it submits, so a decided question's
    /// composer is empty and saving it would wipe the decision that was just
    /// written into the same slot.
    fn go(&mut self, to: usize) {
        if to == self.at {
            return;
        }
        if self.decided[self.at].is_none() {
            self.drafts[self.at] = self.composer.text();
        }
        self.at = to;
        self.offers = Self::list(&self.batch[to]);
        // A fresh list has been told about the composer's row and nothing else,
        // and the next question's previews are a different set of blocks at a
        // width nothing has measured yet.
        self.reserved = crate::app::COMPOSER_ROWS;
        self.composer = Composer::new();
        self.composer.set(&self.drafts[to]);
    }

    /// The row that takes prose, which is the last one.
    ///
    /// Read off the picker rather than computed from `choices.len()`: a described
    /// offer takes a row for its description, so the count of choices stopped
    /// being the count of rows before it in 0.33.0. [`offer_rows`] says the rest.
    fn free_row(&self) -> usize {
        self.offers.rows().len().saturating_sub(1)
    }

    /// Whether the marker is on the free-text row, which is where the composer is
    /// unfolded.
    ///
    /// **Not [`Picker::unfolded_now`], and the difference is new in 0.33.0.** Until
    /// an offer could carry a preview the composer's row was the only row that
    /// unfolded anything, so "something is open" and "the operator is writing"
    /// were the same question. They are not: an offer with a preview unfolds too,
    /// and routing `Enter` by the old test would have sent it to the composer —
    /// so `Enter` on an offer whose preview is open would have answered with
    /// whatever was typed for a *different* row, or with nothing at all.
    fn writing(&self) -> bool {
        self.offers.selection() == Some(self.free_row())
    }

    /// What this keystroke decides about the question on screen, if anything.
    ///
    /// `Some(Some(text))` answers it, `Some(None)` declines it, `None` is every
    /// key that moves, types or does nothing. This is the whole of the 0.32.0
    /// surface: one question, one list, one composer.
    fn decision(&mut self, key: crossterm::event::KeyEvent) -> Option<Option<String>> {
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

        // **The spacebar marks, on a question that takes several and while the
        // marker is on an offer.** It sits above the printable arm rather than
        // inside it, so on a question that takes one answer a space is a space and
        // reaches the composer exactly as every other character does — which is
        // what makes `multiple` invisible to every question written before it
        // existed. With the composer unfolded under the marker the space is prose
        // again: the free-text row is not an offer and cannot be marked.
        if key.code == KeyCode::Char(' ')
            && self.current().multiple
            && !self.writing()
            && !modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            && !modifiers.contains(crossterm::event::KeyModifiers::ALT)
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
            let free = self.free_row();
            self.offers.focus(free);
            return self.typed(key);
        }

        // `Enter` and `Tab` take what is under the marker. On the free-text row
        // that is whatever has been typed into it; on an offer it is the offer —
        // **including an offer whose preview is unfolded**, which is why the test
        // is which row holds the marker and no longer "is anything unfolded".
        if self.writing() {
            return self.typed(key);
        }
        match self.offers.key(key) {
            Outcome::Chosen(index) => self.taken(index),
            // **`Cancelled` cannot arrive, and saying so is the point.** The only
            // key a `Picker` cancels on is `Ctrl+C`, and `App::key` takes that
            // before this overlay ever sees it — its own comment settles the
            // question "does Ctrl+C decline, or interrupt?" with *it interrupts*.
            // An arm answering `Some(None)` here would be a second, contradictory
            // answer to that question, in a different file.
            Outcome::Cancelled | Outcome::Idle => None,
        }
    }

    /// What `Enter` on the offers answers with.
    ///
    /// The offer verbatim. Not the row's label re-read off the screen and not a
    /// fitted copy of it: the string the agent sent is the string it gets back,
    /// which is the whole reason `Outcome::Chosen` indexes the caller's own
    /// unfiltered rows.
    ///
    /// **A `multiple` question is spelled by [`Question::answer_of`] and never
    /// here.** The harness owns that joiner precisely so two interfaces answering
    /// the same question produce the same text; a `", "` written out in io-cli
    /// would be a second spelling that agrees today and drifts the first time the
    /// harness changes its mind.
    ///
    /// Read through [`Picker::chosen`], which is "the marks, or the row under the
    /// marker when nothing is marked" — so `Enter` on a plural question with
    /// nothing marked still sends the offer the operator is looking at, rather
    /// than the empty string `answer_of` would make of an empty list. An empty
    /// answer is not an answer; it is information the agent did not have and
    /// would now believe.
    fn taken(&self, row: usize) -> Option<Option<String>> {
        let question = self.current();
        // **A row, translated to a choice, and never used as one.** Through
        // 0.32.0 the two were the same number; a described offer takes a row for
        // its description, so from 0.33.0 indexing `choices` with a row answers
        // the agent with a *different* offer than the one under the marker — and
        // silently, because both are real offers of the same question.
        let rows = offer_rows(question);
        let choice = |row: usize| {
            rows.iter()
                .position(|at| *at == row)
                .and_then(|choice| question.choices.get(choice))
        };
        if question.multiple {
            let labels: Vec<String> = self
                .offers
                .chosen()
                .into_iter()
                .filter_map(choice)
                .map(|choice| choice.label.clone())
                .collect();
            return (!labels.is_empty()).then(|| Some(Question::answer_of(labels)));
        }
        choice(row).map(|choice| Some(choice.label.clone()))
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

    /// Resolve every question of the delivery. Consumes the overlay, because a
    /// question answered twice is a run that receives an answer nobody typed.
    ///
    /// `answers` is positional: entry `n` goes to question `n`, which is the order
    /// [`Self::key`] hands them back in and the order io-harness expects. An
    /// `answers` shorter than the batch leaves the rest of the reply channels to
    /// drop, and a dropped channel is `None` — the run parks with those questions
    /// persisted, which is the safe direction and the only one available.
    ///
    /// Returns `None` when everything has been delivered — a live turn was
    /// awaiting each answer and now has it. Returns `Some(answer)` when this
    /// overlay was opened by [`Self::resumed`]: there was no turn to send to, so
    /// the answer comes back out here and the caller delivers it with
    /// `io_harness::resume_with_answer_observed`. Dropping that value drops the
    /// operator's answer, which is why it is a return rather than a side effect.
    ///
    /// **A resumed batch is still one answer, because it is still one row.** The
    /// store parks a whole `ask_questions` under a single `question_id` and
    /// `answer_question` is a single compare-and-swap, so there is exactly one text
    /// to hand back however many questions the operator just worked through —
    /// which is why this is a single `Option` and not a vector, and why there is no
    /// loop resuming the run once per question. The text pairs every answer with
    /// the question it answers, and a batch with anything left unanswered has no
    /// text at all: it comes back `Some(None)` and the run stays parked.
    pub fn resolve(self, answers: Vec<Option<String>>) -> Option<Option<String>> {
        // A stored batch: many decisions, one row, one text. Live batches fall
        // through — each of their questions has a channel of its own that the
        // harness is awaiting, and joining those would answer none of them.
        if self.batch.len() > 1 && self.answers.iter().all(Destination::parked) {
            return Some(assembled(&self.batch, &answers));
        }
        let mut undelivered = None;
        for (destination, answer) in self.answers.into_iter().zip(answers) {
            if let Some(kept) = destination.deliver(answer) {
                undelivered = Some(kept);
            }
        }
        undelivered
    }

    /// The question, its context, the choices offered, and the prompt.
    ///
    /// The whole viewport, like an approval: the run is stopped, so there is
    /// nothing behind this worth half a screen.
    ///
    /// One line of it depends on which way in was taken: declining a live question
    /// defers it *within* a turn that is still running and will carry on the moment
    /// it is answered, while declining a resumed one leaves the run parked exactly
    /// as it was found — the operator opened it, so "for later" would be a promise
    /// nothing behind the screen is keeping. That difference is a word chosen here,
    /// not a second `render`.
    ///
    /// A batch adds two more lines and takes that word for a third: where in the
    /// batch this question is, what it was already decided as if the operator has
    /// come back to it, and the page keys. On a single question none of them are
    /// drawn, so the surface is 0.32.0's to the row.
    ///
    /// The list carries a described offer's sentence on a row of its own, always,
    /// and the row under the marker opens one block beneath itself: the composer
    /// on the free-text row, and that offer's preview on an offer that has one.
    /// Every preview is measured here, at this width, before the picker reserves
    /// anything — the reservation is blank rows, and a block measured after them
    /// would be drawn over the list.
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
        // **Measured, then reserved, then drawn — in that order, on the same
        // width.** The picker reserves an unfold out of the rows it was given
        // *while it renders*, so a height that arrives after `Picker::render` is a
        // height that reserved nothing this frame: the block would be drawn over
        // the list rather than into rows kept blank for it. The width is
        // `below.width`, which is `area.width`, which is the width the block's own
        // rectangle gets — so the number measured and the number drawn against are
        // the same number rather than two that agree today.
        let previews = self.previews(below.width, theme);
        self.reserved = previews
            .iter()
            .map(|(_, height)| *height)
            .max()
            .unwrap_or(0)
            .max(crate::app::COMPOSER_ROWS);
        for (row, height) in previews {
            self.offers.set_unfold(row, height);
        }
        self.offers.render(frame, below, theme);
        // One block is open at a time and the picker says which — it is the row
        // under the marker, and there is only ever one of those. Whose block it is
        // decides what goes in it: the free-text row's is the composer, an offer's
        // is its preview. An offer with neither reserves nothing, so `opened` is
        // `None` and nothing is drawn under it at all.
        let block = self.open_preview(theme);
        if let Some(open) = self.offers.opened() {
            match block {
                Some(lines) => frame.render_widget(
                    Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
                    open,
                ),
                None => self.composer.render(frame, open, theme),
            }
        }
    }

    /// Every offer that carries a preview: the row it sits on, and the rows its
    /// quoted block wraps to at `width`.
    ///
    /// **Wrapped, never counted.** A preview is drawn by a `Paragraph` with
    /// wrapping on, so its height is a function of the width and `lines.len()` is
    /// not it — the measurement that has already cost this product two defects,
    /// both of them one surface painting over rows another had been promised.
    /// [`crate::rows::wrapped`] asks ratatui's own wrapper, so the reservation and
    /// the drawing cannot disagree.
    fn previews(&self, width: u16, theme: &Theme) -> Vec<(usize, u16)> {
        let question = self.current();
        offer_rows(question)
            .into_iter()
            .zip(&question.choices)
            .filter_map(|(row, choice)| {
                let lines = quoted(choice.preview.as_deref()?, theme);
                (!lines.is_empty()).then(|| (row, crate::rows::wrapped(&lines, width)))
            })
            .collect()
    }

    /// The block belonging to the row under the marker, when that row is an offer
    /// with a preview.
    ///
    /// `None` on the free-text row — where the block is the composer — and on an
    /// offer with nothing to show, which reserves no block at all.
    fn open_preview(&self, theme: &Theme) -> Option<Vec<Line<'static>>> {
        let question = self.current();
        let row = self.offers.selection()?;
        let at = offer_rows(question)
            .iter()
            .position(|offer| *offer == row)?;
        let lines = quoted(question.choices.get(at)?.preview.as_deref()?, theme);
        (!lines.is_empty()).then_some(lines)
    }

    /// The question, its context, and the line naming the keys.
    ///
    /// One line of it depends on which way in was taken: declining a live question
    /// defers it *within* a turn that is still running and will carry on the moment
    /// it is answered, while declining a resumed one leaves the run parked exactly
    /// as it was found — the operator opened it, so "for later" would be a promise
    /// nothing behind the screen is keeping. That difference is a word chosen here,
    /// not a second `render`.
    ///
    /// A batch adds two more lines and takes that word for a third: where in the
    /// batch this question is, what it was already decided as if the operator has
    /// come back to it, and the page keys. On a single question none of them are
    /// drawn, so the surface is 0.32.0's to the row.
    fn head(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        let dash = theme.glyphs.dash;
        // **Where in the batch, above the question rather than below it**, and
        // only when there is a batch: on the single question that is nearly every
        // delivery this line says nothing and the surface is 0.32.0's exactly.
        if self.batch.len() > 1 {
            lines.push(theme.notice(
                Tone::Muted,
                format!(
                    "question {} of {} {dash} nothing is sent until every one is decided",
                    self.at + 1,
                    self.batch.len()
                ),
            ));
            // Coming back to a question that was already decided says so, because
            // the composer holding the operator's own words back is otherwise
            // indistinguishable from a draft they abandoned.
            if let Some(decided) = &self.decided[self.at] {
                lines.push(theme.notice(
                    Tone::Muted,
                    match decided {
                        Some(text) => format!("already answered {dash} {text}"),
                        None => format!("already left unanswered {dash} answering replaces that"),
                    },
                ));
            }
        }
        // **`Tone::Accent`, not `Tone::Warning`.** `Tone::Warning`'s word is
        // literally `warning`, so every question this agent asked was prefixed
        // with it — and `Tone::Refused`'s own doc keeps the vocabulary honest:
        // these tones mean something, and a question is not a warning. It is the
        // product's own colour because it is the product asking for the operator's
        // attention, which is exactly what the prompt marker uses it for.
        lines.push(theme.notice(Tone::Accent, self.current().question.clone()));
        if let Some(context) = &self.current().context {
            lines.push(theme.notice(Tone::Muted, context.clone()));
        }
        // On a batch `Esc` decides *this* question and moves on, so it cannot be
        // described as leaving the overlay. The run still parks — a batch carrying
        // one `None` is not committed — which is what `Esc` has always promised
        // and the reason the word survives the change.
        let leaves = if self.batch.len() > 1 {
            "Esc leaves this one unanswered"
        } else if self.answers.iter().any(Destination::parked) {
            "Esc leaves the run parked"
        } else {
            "Esc leaves it for later"
        };
        lines.push(theme.notice(
            Tone::Muted,
            format!("Enter sends the marked row {dash} Tab too {dash} {leaves}"),
        ));
        if self.batch.len() > 1 {
            lines.push(theme.notice(
                Tone::Muted,
                format!(
                    "PgUp and PgDn move between the questions {dash} an answered one \
                     re-opens to be changed"
                ),
            ));
        }
        lines
    }

    /// Rows this overlay would like the viewport to be.
    ///
    /// Its head as it will actually wrap, every offer, a row for each offer the
    /// agent explained, the free-text row, the tallest block any row unfolds —
    /// the composer, or a preview measured at `width` — and the picker's own head
    /// row. A request:
    /// [`crate::app::App::viewport_wanted`] clamps it to what the terminal can
    /// spare, and [`Self::render`] degrades against whatever it is given.
    pub fn rows_wanted(&self, width: u16, theme: &Theme) -> u16 {
        // **The tallest preview, never the focused one**, which is the property
        // `Picker::rows_wanted` was rebuilt around in 0.33.0 and this must not
        // undo: a demand that followed the marker would change by the difference
        // between a one-row preview and a twelve-row one on every arrow key, and
        // the driver re-places the viewport whenever the demand changes — a
        // terminal tear-down and a cursor query per keystroke, on a surface that
        // is open while a turn is in flight.
        //
        // Added on top of what the picker already reserves rather than instead of
        // it, and net of `reserved`, because the picker cannot measure this
        // itself: the width is not known until something draws, and the driver
        // reads the demand before it draws — see the `reserved` field. After a frame
        // the picker has been told and this term is zero; before one it is the
        // whole block, and the total is the same number either way — which is what
        // stops the overlay opening a block too short and growing under the
        // operator's hands.
        let tallest = self
            .previews(width, theme)
            .into_iter()
            .map(|(_, height)| height)
            .max()
            .unwrap_or(0);
        crate::rows::wrapped(&self.head(theme), width)
            .saturating_add(self.offers.rows_wanted())
            .saturating_add(tallest.saturating_sub(self.reserved))
    }
}
