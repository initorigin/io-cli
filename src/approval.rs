//! The two-way seam between io-harness and the operator.
//!
//! [`crate::bridge`] is the other seam and the easy one: an observer is handed an
//! event and hands it on, and nothing waits for the interface. This one is the
//! opposite shape. `Approver::decide_in_context` runs **on the agent's own task**
//! and the run stays paused until the future it returns resolves, so this module
//! is the only place in the product where the interface can stop the agent.
//!
//! Two consequences shape everything below.
//!
//! **A question that is never answered must deny.** Both ends can vanish — the
//! whole interface (the mpsc closes) or one question it took and abandoned (the
//! oneshot closes) — and both mean the same thing to a run that cannot proceed
//! without an answer. A blocked turn is worse than a refused one: a refusal
//! reaches the model as an observation it can adapt to, and a block reaches
//! nobody. F4 asserts it on a closed channel rather than on a timeout, because a
//! deadlock asserted with a clock is a test that passes on a fast machine.
//!
//! **The rule and the layer only exist here.** `EventKind::ApprovalRequested`
//! carries the act and the target and nothing else; the glob that put the action
//! in the grey tier, the layer that glob came from, and the content a write would
//! leave behind arrive as the [`Request`] and [`ApprovalContext`] handed to this
//! trait. So the approval overlay is drawn from what this module forwards, and
//! never from the event stream.

use crossterm::event::{KeyCode, KeyEvent};
use io_harness::approve::DecisionFuture;
use io_harness::{Act, ApprovalContext, Approver, Decision, Effect, Request, Rule};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::{mpsc, oneshot};

use crate::status::SEPARATOR;
use crate::theme::{Theme, Tone};

/// What the model is told when nobody answered.
///
/// It reaches the model as an observation rather than as an error, which is the
/// point: a run told this can do something else, and a run left waiting cannot.
pub const UNANSWERED: &str = "nobody was there to approve it";

/// One question, on its way to the interface, with the way back inside it.
///
/// Answering consumes it. There is no way to hold an `Ask` and answer it twice,
/// and dropping one is a denial rather than a leak — which is the behaviour F4
/// asks for, expressed as a type rather than as a rule somebody has to remember.
pub struct Ask {
    request: Request,
    context: ApprovalContext,
    answer: oneshot::Sender<Decision>,
}

impl Ask {
    /// Answer it. The run resumes on whatever this says.
    pub fn answer(self, decision: Decision) {
        // A send error means the run ended while the question was on screen — an
        // interrupt, or a ceiling reached elsewhere. There is nobody left to tell,
        // and that is not a failure of the interface.
        let _ = self.answer.send(decision);
    }

    /// What kind of action is being asked about.
    pub fn act(&self) -> Act {
        self.request.act
    }

    /// The path, or the binary name for an exec.
    pub fn target(&self) -> &str {
        &self.request.target
    }

    /// What a write would leave behind, whole. The harness hands an approver the
    /// resulting file rather than a patch, so anything diff-shaped is this
    /// product's to compute — and is 0.3.0's, not this release's.
    pub fn content(&self) -> Option<&str> {
        self.request.content.as_deref()
    }

    /// The glob that put this action in the grey tier, or `None` when the tier
    /// default did.
    ///
    /// `None` is not "no reason". io-harness's own documentation is explicit that
    /// an unnamed action in the grey tier is the *least* vouched-for kind, so a
    /// surface that renders `None` as blank tells the reader the opposite of what
    /// happened. F8 asserts both cases.
    pub fn rule(&self) -> Option<&str> {
        self.context.rule.as_deref()
    }

    /// The policy layer the deciding rule came from, or `None` for the tier
    /// default. Layers are named after whoever wrote them, so this is the field
    /// that sends a reader to the right configuration file.
    pub fn layer(&self) -> Option<&str> {
        self.context.layer.as_deref()
    }

    /// The run's goal, in the words the operator typed.
    pub fn goal(&self) -> &str {
        &self.context.goal
    }
}

/// The approver handed to `Session::turn_steered`.
pub struct Asker {
    asks: mpsc::UnboundedSender<Ask>,
}

/// An asker and the receiver the interface drains.
///
/// Unbounded for the same reason [`crate::bridge`]'s channel is: the alternatives
/// are blocking the run and dropping a question, and a dropped question is a turn
/// that waits forever. In practice the depth is one — the run is paused from the
/// moment it asks until the moment it is answered, so a second question cannot
/// arrive from the same run while the first is outstanding.
pub fn channel() -> (Asker, mpsc::UnboundedReceiver<Ask>) {
    let (asks, rx) = mpsc::unbounded_channel();
    (Asker { asks }, rx)
}

impl Asker {
    async fn ask(&self, request: Request, context: ApprovalContext) -> Decision {
        let (answer, reply) = oneshot::channel();
        let ask = Ask {
            request,
            context,
            answer,
        };
        // One path for both ways this can fail, deliberately. A failed `send`
        // returns the `Ask` and drops it, which closes the oneshot inside it — so
        // "the interface is gone" and "the interface took the question and went
        // away" arrive here as the same closed channel. An early return for the
        // first was written, sabotaged, and found to fail no test at all: it was a
        // second spelling of this line.
        let _ = self.asks.send(ask);
        reply.await.unwrap_or_else(|_| Decision::deny(UNANSWERED))
    }
}

impl Approver for Asker {
    fn decide<'a>(&'a self, request: &'a Request) -> DecisionFuture<'a> {
        // The harness calls `decide_in_context`; this exists because the trait
        // requires it, and it must still ask rather than answer on its own — an
        // approver with two behaviours is one that ships the wrong one.
        let request = request.clone();
        Box::pin(async move { self.ask(request, ApprovalContext::default()).await })
    }

    fn decide_in_context<'a>(
        &'a self,
        request: &'a Request,
        context: &'a ApprovalContext,
    ) -> DecisionFuture<'a> {
        let request = request.clone();
        let context = context.clone();
        Box::pin(async move { self.ask(request, context).await })
    }
}

/// What the operator can say. Three, deliberately.
///
/// io-harness offers two more — `Decision::Defer`, which stops the run and
/// persists the request for a decision later, and `Decision::Approve { modified }`,
/// which rewrites the action. Deferring is only useful alongside a resume this
/// product does not have until 0.4.0, and rewriting is an editor inside an
/// overlay. Both are left unbound rather than half-built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// This action, this once.
    Once,
    /// This action and every later one like it, for the rest of the session.
    Session,
    /// No, with a reason the model can adapt to.
    Deny,
}

impl Answer {
    /// In the order they are offered, least committal first. A reader moving
    /// rightwards is giving away more, which is the direction a permission
    /// surface should read in.
    pub const ALL: [Answer; 3] = [Answer::Once, Answer::Session, Answer::Deny];

    /// The key that chooses it directly.
    pub fn key(self) -> char {
        match self {
            Self::Once => 'y',
            Self::Session => 'a',
            Self::Deny => 'n',
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Once => "allow once",
            Self::Session => "allow this session",
            Self::Deny => "deny",
        }
    }

    /// The word that goes in the transcript, after the decision is made.
    pub fn spoken(self) -> &'static str {
        match self {
            Self::Once => "allowed once",
            Self::Session => "allowed for this session",
            Self::Deny => "denied",
        }
    }
}

/// What the model is told when the operator says no.
pub const REFUSED_BY_OPERATOR: &str = "the operator denied it";

/// An open question, and the answer being chosen.
///
/// It owns the [`Ask`], so the run stays paused for exactly as long as this
/// exists. Dropping one without answering is a denial — see [`Ask`] — which is
/// what makes an interrupt, a panic or an unwound resize safe here.
pub struct Approval {
    ask: Ask,
    chosen: usize,
}

impl Approval {
    pub fn new(ask: Ask) -> Self {
        // Opens on `Once`: the least committal answer, and never on a remembered
        // allow. A surface that opens on the widest answer is one where `Enter`
        // by reflex gives away the most.
        Self { ask, chosen: 0 }
    }

    /// Which answer is highlighted.
    pub fn chosen(&self) -> Answer {
        Answer::ALL[self.chosen]
    }

    pub fn ask(&self) -> &Ask {
        &self.ask
    }

    /// A keystroke. `Some` means the operator answered and this overlay is over.
    ///
    /// Two ways in, on purpose: a letter for the reader who knows the key, and
    /// arrows with `Enter` for the one who does not. A key that only works when
    /// you already know it is not an interface.
    pub fn key(&mut self, key: KeyEvent) -> Option<Answer> {
        match key.code {
            KeyCode::Left => {
                self.chosen = self.chosen.saturating_sub(1);
                None
            }
            KeyCode::Right => {
                if self.chosen + 1 < Answer::ALL.len() {
                    self.chosen += 1;
                }
                None
            }
            KeyCode::Enter => Some(self.chosen()),
            KeyCode::Char(c) => Answer::ALL
                .into_iter()
                .find(|answer| answer.key() == c.to_ascii_lowercase()),
            _ => None,
        }
    }

    /// Answer it, and let the run go on.
    pub fn answer(self, answer: Answer) {
        let decision = match answer {
            Answer::Once => Decision::approve(),
            Answer::Session => Decision::Approve {
                modified: None,
                // The harness applies this for the rest of the *run*, which is one
                // turn. Carrying it into the next turn is the caller's job and is
                // what F5 asserts; see `remembered`.
                remember: vec![self.remembered()],
            },
            Answer::Deny => Decision::deny(REFUSED_BY_OPERATOR),
        };
        self.ask.answer(decision);
    }

    /// The rule *allow this session* means.
    ///
    /// The narrowest thing a person means when they say yes to the same question
    /// twice: this act, on this target. Not the act alone, which would allow every
    /// write; and the pattern is the target as written, because a bare filename is
    /// matched against a basename too and would allow the same name anywhere in
    /// the tree.
    pub fn remembered(&self) -> Rule {
        Rule {
            act: self.ask.act(),
            effect: Effect::Allow,
            pattern: self.ask.target().to_string(),
        }
    }

    /// Draw it. Three parts, in the order a reader needs them: what is being
    /// asked, what it would do, and how to answer.
    ///
    /// The whole thing is one viewport's worth of rows — four, in a session — so
    /// the content is what flexes. It is the only part that can be arbitrarily
    /// long and the only part a reader can ask for more of by other means.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let width = area.width as usize;
        let mut lines = vec![self.question(width, theme)];

        // Everything between the question and the answers, if there is any room.
        let room = area.height.saturating_sub(2) as usize;
        if room > 0 {
            lines.extend(self.preview(room, width, theme));
        }
        if area.height >= 2 {
            lines.push(self.answers(theme));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// `write src/main.rs · rule src/*.rs · layer app`.
    ///
    /// Act, then target, then the rule, then the layer — content before metadata,
    /// the same order the rest of the interface reads in, and the order F2 asserts
    /// by position rather than by presence.
    fn question(&self, width: usize, theme: &Theme) -> Line<'static> {
        let mut spans = vec![
            Span::styled(
                act_word(self.ask.act()).to_string(),
                theme.style(Tone::Warning),
            ),
            Span::styled(" ", theme.style(Tone::Normal)),
            Span::styled(
                crate::picker::fit(self.ask.target(), width.saturating_sub(8)),
                theme.style(Tone::Normal),
            ),
        ];
        spans.push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
        match (self.ask.rule(), self.ask.layer()) {
            (Some(rule), Some(layer)) => spans.push(Span::styled(
                format!("rule {rule}{SEPARATOR}layer {layer}"),
                theme.style(Tone::Muted),
            )),
            (Some(rule), None) => spans.push(Span::styled(
                format!("rule {rule}"),
                theme.style(Tone::Muted),
            )),
            // Said plainly rather than left blank. In io-harness a missing rule
            // means the tier default decided — the least vouched-for kind of
            // action, not the most — so an empty space here would read as the
            // opposite of what happened.
            (None, _) => spans.push(Span::styled(
                "no rule named it: the tier default decided",
                theme.style(Tone::Muted),
            )),
        }
        Line::from(spans)
    }

    /// The content a write would leave behind, in the rows that are left.
    ///
    /// It says what it cut. A reader who thinks they have seen a whole file and
    /// has seen a third of it is worse off than one who was told.
    fn preview(&self, room: usize, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let Some(content) = self.ask.content() else {
            return Vec::new();
        };
        let total = content.lines().count();
        let shown = if total > room { room - 1 } else { total };
        let mut lines: Vec<Line<'static>> = content
            .lines()
            .take(shown)
            .map(|line| {
                Line::from(Span::styled(
                    format!("  {}", crate::picker::fit(line, width.saturating_sub(2))),
                    theme.style(Tone::Muted),
                ))
            })
            .collect();
        if total > shown {
            lines.push(Line::from(Span::styled(
                format!("  ⋯ {} more lines", total - shown),
                theme.style(Tone::Muted),
            )));
        }
        lines
    }

    /// `› allow once · allow this session · deny`, on one row.
    ///
    /// One row rather than a list, because the viewport is four rows and the facts
    /// above have first claim on them. The marker is the same `›` every other
    /// selection surface uses, so the answer that `Enter` would take is marked by
    /// more than a colour.
    fn answers(&self, theme: &Theme) -> Line<'static> {
        let mut spans = Vec::new();
        for (index, answer) in Answer::ALL.into_iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
            }
            let chosen = index == self.chosen;
            spans.push(Span::styled(
                if chosen { "› " } else { "  " }.to_string(),
                theme.style(Tone::Accent),
            ));
            spans.push(Span::styled(
                format!("{} {}", answer.key(), answer.label()),
                theme.style(if chosen { Tone::Accent } else { Tone::Muted }),
            ));
        }
        Line::from(spans)
    }
}

/// The word for an act. io-harness spells these in its own serde tags and in
/// `EventKind::Refused`; this is the same vocabulary so a refusal and a request
/// read as the same product.
pub fn act_word(act: Act) -> &'static str {
    match act {
        Act::Read => "read",
        Act::Write => "write",
        Act::Exec => "run",
        Act::Net => "reach",
    }
}
