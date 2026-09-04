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
use io_harness::{Act, ApprovalContext, Approver, Decision, Edit, Effect, Request, Rule};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tokio::sync::{mpsc, oneshot};

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

    /// What a write would leave behind, whole.
    ///
    /// The harness hands an approver the resulting file rather than a patch, so
    /// the old side has to come from somewhere else — 0.3.0 reads it off disk and
    /// hands both to `Edit::with_hunk`, which is the harness's own renderer. See
    /// this module's own `diff_of`, which is where that reading happens.
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

/// The approver handed to `Session::turn_bounded_steered` and to
/// `Session::turn_contained_bounded_steered`.
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
    /// The write, as a change rather than as a wall of text.
    ///
    /// Computed once, when the overlay opens, because `render` runs per frame and
    /// reading a file per frame is a file read per keystroke. `None` means there
    /// is nothing to diff — the act is not a write, or the target could not be
    /// read for a reason that is not "it does not exist yet" — and the overlay
    /// falls back to showing the proposed content plainly, which is what 0.2.0
    /// did for every write.
    proposed: Option<Edit>,
}

impl Approval {
    /// Open an overlay for a question, resolving a write's target against
    /// `root`.
    ///
    /// The root is required rather than inferred, and a live run is what settled
    /// that. `Request.target` arrives **relative to the workspace** — `notes.txt`,
    /// not `/tmp/x/notes.txt` — so an implementation that only read absolute
    /// paths never found the file and quietly fell back to showing the proposed
    /// content, which is a feature that ships looking like it works. Resolving
    /// against this process's working directory instead would be worse:
    /// `io -C <dir>` sets the workspace without changing it, so a relative name could
    /// match a different file that happens to exist here.
    pub fn new(ask: Ask, root: &std::path::Path) -> Self {
        let proposed = diff_of(&ask, root);
        // Opens on `Once`: the least committal answer, and never on a remembered
        // allow. A surface that opens on the widest answer is one where `Enter`
        // by reflex gives away the most.
        Self {
            ask,
            chosen: 0,
            proposed,
        }
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
        let decision = decision(answer, self.ask.act(), self.ask.target());
        self.ask.answer(decision);
    }

    /// The rule this approval's *allow this session* means.
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
        let mut lines = self.question(width, theme);

        // Everything between the question and the answers, if there is any room.
        let room = (area.height as usize).saturating_sub(lines.len() + 1);
        if room > 0 {
            lines.extend(self.preview(room, width, theme));
        }
        // The cursor goes on the answer `Enter` would take, and it is put there
        // here rather than by `App::render`. The overlay takes the whole viewport
        // — `App::render` returns as soon as this has drawn — so there is no
        // composer on this frame to own the caret, and ratatui hides the cursor
        // on any frame that sets no position. That removes the only focus
        // indicator a screen reader has at the one moment a person is being asked
        // to give a permission away. Owned by the widget that owns the selection,
        // the way `Composer::render` owns its insertion point.
        //
        // The fallback is the first row rather than nothing: at two rows there is
        // no room for the answers line at all, and a frame too cramped to show the
        // choices is exactly when a reader most needs telling where they are.
        let mut cursor = Position {
            x: area.x,
            y: area.y,
        };
        if area.height >= 2 {
            let (answers, column) = self.answers(theme);
            cursor = Position {
                x: (area.x + column).min(area.right().saturating_sub(1)),
                y: (area.y + lines.len() as u16).min(area.bottom().saturating_sub(1)),
            };
            lines.push(answers);
        }
        frame.render_widget(Paragraph::new(lines), area);
        frame.set_cursor_position(cursor);
    }

    /// `warning: write src/main.rs · rule src/*.rs · layer app`.
    ///
    /// Act, then target, then the rule, then the layer — content before metadata,
    /// the same order the rest of the interface reads in, and the order F2 asserts
    /// by position rather than by presence.
    ///
    /// **The act carries its tone's word, and that is 0.6.0's F4** — a different
    /// criterion from the 0.3.0 F4 this module's own documentation names. Until
    /// 0.6.0 this row styled the act `Tone::Warning` through a bare
    /// `Span::styled`, which made it
    /// the one place left in the product where a colour was the sole carrier of a
    /// meaning: with `NO_COLOR` set the row read `write src/main.rs`, and nothing
    /// on it said a decision was being asked for. The overlay around it carried
    /// that, but no word did. Routing it through [`Theme::notice`] — the same
    /// constructor every other toned line in the product is built with, and the
    /// one the transcript already uses to announce this very event — puts
    /// `warning: ` in front of the act, whatever the terminal can render.
    ///
    /// The word **leads** the row rather than trailing it, for the reason the row
    /// below exists: this viewport clips, and the load-bearing fact has to sit
    /// where it cannot be the part that goes. The target is still what gives way,
    /// and its room is now the width less the word as well as the act.
    fn question(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let separator = theme.glyphs.separator;
        let act = act_word(self.ask.act());
        // Measured from the tone rather than written out, so this and `notice`
        // cannot drift: `word` plus the colon and the space it is prefixed with.
        let prefix = Tone::Warning
            .word()
            .map_or(0, |word| word.chars().count() + 2);
        let asked = theme.notice(
            Tone::Warning,
            format!(
                "{act} {}",
                // The target is what gets shortened, because it is the only part
                // that can be arbitrarily long and the only part whose middle a
                // reader can infer. The act cannot be shortened at all.
                crate::picker::fit(
                    self.ask.target(),
                    width.saturating_sub(prefix + act.len() + 1),
                    &theme.glyphs,
                ),
            ),
        );

        // **On its own row, and this cost a test to learn.** Laid out beside the
        // target, the rule and the layer are the first thing off the end of an
        // eighty-column terminal — and ratatui clips a row rather than complaining,
        // so the two facts this release exists to show would vanish silently on the
        // supported terminal size. A row of their own is the only layout where they
        // cannot be the part that goes.
        let why = match (self.ask.rule(), self.ask.layer()) {
            (Some(rule), Some(layer)) => format!("rule {rule}{separator}layer {layer}"),
            (Some(rule), None) => format!("rule {rule}"),
            // Said plainly rather than left blank. In io-harness a missing rule
            // means the tier default decided — the least vouched-for kind of
            // action, not the most — so an empty space here would read as the
            // opposite of what happened.
            (None, _) => "no rule named it: the tier default decided".to_string(),
        };
        vec![
            asked,
            Line::from(Span::styled(
                crate::picker::fit(&why, width, &theme.glyphs),
                theme.style(Tone::Muted),
            )),
        ]
    }

    /// The content a write would leave behind, in the rows that are left.
    ///
    /// It says what it cut. A reader who thinks they have seen a whole file and
    /// has seen a third of it is worse off than one who was told.
    fn preview(&self, room: usize, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        if let Some(edit) = &self.proposed {
            return self.as_diff(edit, room, width, theme);
        }
        let Some(content) = self.ask.content() else {
            return Vec::new();
        };
        let total = content.lines().count();
        let shown = total.min(room);
        // The count rides the last shown line rather than taking a row of its own.
        // At the tightest size the overlay has exactly one row for content, and a
        // whole row spent saying "40 more lines" would show a reader the number of
        // lines they are approving and not one of the lines themselves.
        let cut = total.saturating_sub(shown);
        content
            .lines()
            .take(shown)
            .enumerate()
            .map(|(index, line)| {
                let suffix = if cut > 0 && index + 1 == shown {
                    // Measured, not assumed: the ASCII elision is three cells
                    // where the Unicode one is one, and the room left for the
                    // line itself is the width less whatever this actually took.
                    format!("  {} {cut} more lines", theme.glyphs.elision)
                } else {
                    String::new()
                };
                let room = width.saturating_sub(2 + suffix.chars().count());
                Line::from(Span::styled(
                    format!(
                        "  {}{suffix}",
                        crate::picker::fit(line, room, &theme.glyphs)
                    ),
                    theme.style(Tone::Muted),
                ))
            })
            .collect()
    }

    /// The write as a diff, in the rows there are.
    ///
    /// **The counts come first and the path does not come at all.** The
    /// transcript's version of this cell leads with the path, because there a diff
    /// arrives with nothing above it. Here the question row directly above already
    /// names the target, and repeating it cost a whole assertion: at eighty
    /// columns with a long path, ratatui clipped the row — silently, as it always
    /// does — and what went was `+2 -0`, the one fact worth having when there is
    /// only one row. That is 0.2.0's lesson arriving a second time in the same
    /// overlay, and the answer is the same: put the load-bearing fact where it
    /// cannot be the part that goes.
    ///
    /// At the tightest size that one row is `+3 -1`, which is the difference
    /// between a write that touches three lines and one that rewrites four
    /// hundred — a different decision, not a smaller one.
    fn as_diff(&self, edit: &Edit, room: usize, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let separator = theme.glyphs.separator;
        let mut lines = vec![Line::from(vec![
            Span::styled(format!("  +{}", edit.lines_added), theme.style(Tone::Added)),
            Span::styled(" ".to_string(), theme.style(Tone::Muted)),
            Span::styled(
                format!("-{}", edit.lines_removed),
                theme.style(Tone::Removed),
            ),
            Span::styled(
                match &edit.hunk {
                    Some(_) => format!("{separator}{}", edit.tool),
                    // Absent is a fact and not an empty diff — the counts beside
                    // it are what say the file did change.
                    None => format!("{separator}{}{separator}no diff stored", edit.tool),
                },
                theme.style(Tone::Muted),
            ),
        ])];

        // Past the cell's own header, which the row above replaces, and without
        // the blank line it ends with: an overlay bounded by the viewport cannot
        // spend one on breathing. Every line is fitted, because this viewport
        // clips.
        lines.extend(
            crate::diff::cell(edit, theme, u16::try_from(width).unwrap_or(u16::MAX))
                .into_iter()
                .skip(1)
                .filter(|line| line.width() > 0)
                .map(|line| fit_line(line, width, theme)),
        );

        let total = lines.len();
        let shown = total.min(room);
        let cut = total.saturating_sub(shown);
        lines.truncate(shown);
        // Said, not silently dropped. A reader who thinks they have seen a whole
        // change and has seen its first row is worse off than one who was told.
        if cut > 0 {
            if let Some(last) = lines.pop() {
                // **Room is made for it, rather than it being appended to a row
                // that is already full.** Every line above was fitted to the
                // whole width, so a suffix pushed onto the last one overflows
                // and the terminal clips exactly the words that say something
                // was hidden — which is the one thing on this overlay that must
                // not be lost, and which a taller viewport made visible by
                // changing which line ends up last.
                let suffix = format!("  {} {cut} more lines", theme.glyphs.elision);
                let room = width.saturating_sub(suffix.chars().count());
                let mut fitted = fit_line(last, room, theme);
                fitted
                    .spans
                    .push(Span::styled(suffix, theme.style(Tone::Muted)));
                lines.push(fitted);
            }
        }
        lines
    }

    /// `› allow once · allow this session · deny`, on one row.
    ///
    /// One row rather than a list, because the viewport does not grow and the
    /// facts above have first claim on what it has. The marker is the same `›` every other
    /// selection surface uses, so the answer that `Enter` would take is marked by
    /// more than a colour.
    ///
    /// It hands back **where on the row the chosen answer starts** as well as the
    /// row itself, because the terminal cursor is placed there. Counted in the
    /// same loop that lays the spans out rather than recomputed by the caller: a
    /// second copy of this layout is a second thing to keep in step, and the one
    /// that drifts is the one nothing is drawn from.
    fn answers(&self, theme: &Theme) -> (Line<'static>, u16) {
        let separator = theme.glyphs.separator;
        let mut spans = Vec::new();
        let mut width = 0usize;
        let mut column = 0usize;
        for (index, answer) in Answer::ALL.into_iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(separator, theme.style(Tone::Muted)));
                width += separator.chars().count();
            }
            let chosen = index == self.chosen;
            let marker = if chosen { theme.glyphs.marker } else { "  " };
            spans.push(Span::styled(marker.to_string(), theme.style(Tone::Accent)));
            width += marker.chars().count();
            let text = format!("{} {}", answer.key(), answer.label());
            // Past the marker, which is decoration: a reader following the caret
            // should land on the key and the words, not on the arrow.
            if chosen {
                column = width;
            }
            width += text.chars().count();
            spans.push(Span::styled(
                text,
                theme.style(if chosen { Tone::Accent } else { Tone::Muted }),
            ));
        }
        (Line::from(spans), u16::try_from(column).unwrap_or(u16::MAX))
    }
}

/// The word for an act. io-harness spells these in its own serde tags and in
/// `EventKind::Refused`; this is the same vocabulary so a refusal and a request
/// read as the same product.
/// The change a write would make, as an `io_harness::Edit`.
///
/// **io-cli still computes no diff.** `Edit::with_hunk` is the harness's own
/// renderer — the same one that produced every hunk in the durable trace — so an
/// approval and a transcript show a change in exactly the same words. What io-cli
/// supplies is the *old* side, because the write has not happened yet and the
/// harness has nothing stored for it: the approver is handed the whole resulting
/// file, never a patch.
///
/// **Only the request's own target is read**, and only for a write. This is the
/// one workspace read the interface performs, it is of the file the operator is
/// being asked about, and it exists so they can see what changes rather than what
/// the file will end up containing.
///
/// The three outcomes are deliberately different:
/// - the file is not there → an empty old side, so a new file reads as all
///   addition, which is what it is;
/// - the file is there → a real diff against it;
/// - the file is there and cannot be read, or is not UTF-8 → `None`, and the
///   overlay shows the proposed content plainly. Diffing against an empty old
///   side here would show every existing line as new, which is a lie about the
///   size of the change and exactly the wrong direction to be wrong in.
fn diff_of(ask: &Ask, root: &std::path::Path) -> Option<Edit> {
    if ask.act() != Act::Write {
        return None;
    }
    let after = ask.content()?;
    let target = ask.target();

    // `join` with an absolute target returns the target, so one line covers both
    // shapes — and the relative one is what a real run actually sends.
    let path = root.join(target);

    let before = match std::fs::read_to_string(&path) {
        Ok(before) => before,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return None,
    };

    Some(Edit::measure(0, "write_file", target, &before, after).with_hunk(&before, after))
}

/// Cut a rendered line to `width`, keeping its styles and saying it was cut.
///
/// The overlay draws into a fixed-height viewport, where ratatui **clips** a row
/// that does not fit rather than wrapping it — and clips it silently. A diff line
/// is the widest thing this surface draws, so without this a hunk at eighty
/// columns loses its right-hand end with nothing on screen to say so.
///
/// Spans are kept whole while they fit and the one that crosses the edge is cut,
/// so syntax colour survives as far as the line does.
fn fit_line(line: Line<'static>, width: usize, theme: &Theme) -> Line<'static> {
    if line.width() <= width {
        return line;
    }
    // Room for the mark that says something went — its own width, measured off
    // the chosen set. Reserving one cell and then appending the ASCII ellipsis's
    // three is how this function would hand back a line two cells wider than the
    // viewport it was asked to fit, on the one surface that clips silently.
    let mark = theme.glyphs.ellipsis;
    let room = width.saturating_sub(mark.chars().count());
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for span in line.spans {
        let length = span.content.chars().count();
        if used + length <= room {
            used += length;
            spans.push(span);
            continue;
        }
        let take = room - used;
        if take > 0 {
            let cut: String = span.content.chars().take(take).collect();
            spans.push(Span::styled(cut, span.style));
        }
        break;
    }
    spans.push(Span::styled(mark.to_string(), theme.style(Tone::Muted)));
    Line::from(spans)
}

/// What an answer means as an io-harness decision.
///
/// **One place, because two surfaces answer approvals now.** The session's
/// overlay asks a person at a keyboard and `io acp` asks an editor's client;
/// both arrive here with the same three answers and must mean the same three
/// things by them. Left inside the overlay, "allow for this session" would have
/// been re-derived in `src/acp.rs` — and a second derivation that quietly
/// remembered a *wider* rule is a permission the operator did not grant, which
/// no test comparing either surface to itself could see.
///
/// `remember` is applied by the harness for the rest of the **run**, which is one
/// turn. Carrying it into the next turn is the caller's job.
///
/// The rule is the narrowest thing a person means when they say yes to the same
/// question twice: this act, on this target. Not the act alone, which would allow
/// every write; and the pattern is the target as written, because a bare filename
/// is matched against a basename too and would allow the same name anywhere in
/// the tree.
#[must_use]
pub fn decision(answer: Answer, act: Act, target: &str) -> Decision {
    match answer {
        Answer::Once => Decision::approve(),
        Answer::Session => Decision::Approve {
            modified: None,
            remember: vec![Rule {
                act,
                effect: Effect::Allow,
                pattern: target.to_string(),
            }],
        },
        Answer::Deny => Decision::deny(REFUSED_BY_OPERATOR),
    }
}

pub fn act_word(act: Act) -> &'static str {
    match act {
        Act::Read => "read",
        Act::Write => "write",
        Act::Exec => "run",
        Act::Net => "reach",
    }
}

/// The policy a turn runs under, given the session's base policy and everything
/// the operator has allowed for the rest of it.
///
/// This is io-harness's own recipe, not a second one: a permissive layer named
/// `remembered` carrying the rules, merged onto the base. Merging takes the
/// stricter of the two defaults per act, and a later layer may add capability but
/// can never re-allow something an earlier layer denied — so a remembered allow
/// widens an *asking* default and still cannot defeat a deny beneath it.
///
/// It exists because `Decision::Approve { remember }` is **run-scoped**: the
/// harness applies it for the rest of the turn and it dies with it. Without this,
/// *allow for the rest of this session* would ask again on the next prompt. io-cli
/// evaluates nothing here — every value is a harness type and every verdict is
/// still the harness's.
pub fn effective_policy(base: &io_harness::Policy, remembered: &[Rule]) -> io_harness::Policy {
    if remembered.is_empty() {
        return base.clone();
    }
    let mut layer = io_harness::Policy::permissive().layer("remembered");
    for rule in remembered {
        layer = layer.rule(rule.act, rule.effect, rule.pattern.clone());
    }
    base.clone().merge(layer)
}

/// The whole policy a turn runs under: the file's own, with the operator's chosen
/// posture as its tier defaults, plus everything they have allowed for the session.
///
/// The posture replaces the *defaults* and nothing else, which is the reason a key
/// on a keyboard is safe here: a layer that denies a secret is not a default, so no
/// posture can unlock what a layer refused. `None` leaves the file's own policy
/// alone — io-harness's configuration can express far more than three postures, and
/// overwriting one nobody chose would be a keystroke rewriting a boundary.
pub fn session_policy(
    base: &io_harness::Policy,
    posture: Option<crate::settings::Posture>,
    remembered: &[Rule],
) -> io_harness::Policy {
    let mut policy = base.clone();
    if let Some(posture) = posture {
        policy.defaults = posture.defaults();
    }
    effective_policy(&policy, remembered)
}

/// The program `io_harness::tools::git` spawns, spelled the way the harness
/// spells it when it asks the policy about it.
///
/// A bare name and not a path: the harness passes this exact string to
/// `Policy::check(Act::Exec, ..)`, and an `Act::Exec` pattern is compared against
/// the target and against its basename — never against a command line — so the
/// name is the whole vocabulary a rule about git has.
pub const GIT: &str = "git";

// **`refuses_git` was here, and io-harness 0.70.0 both fixed the defect it
// described and removed the reason it existed.**
//
// It answered `check(Act::Exec, GIT).effect != Effect::Allow`, and its doc
// explained that `Ask` had to count as a refusal because `Git::run` returned
// `Error::Refused` on anything short of `Allow` before any approver was
// consulted — the behaviour io-cli 0.25.0 found and reported as io-harness#214.
//
// 0.70.0 closed that issue at all four sites carrying the comparison: `Ask` on
// `Act::Exec` now raises an approval like every other gated act. So the
// inequality this function was built on stopped describing anything — under the
// posture the wizard recommends, git is *asked about* and then runs, and the
// live arm that asserted a refusal came back with `refusals []`.
//
// It is deleted rather than corrected because it had **no production caller** —
// only tests — and because the question it answered is now answered better by
// observation than by prediction. [`crate::app::App::note_git`] fires on an
// `EventKind::Refused` the harness actually emitted, so it cannot explain a
// refusal that did not happen; and `crate::commit::asked` reads the verdict's
// own `rule` field to decide whether the allowance can help. A predicate that
// guessed at the answer in front of them was a sixth instance of the shape this
// product has shipped five times: public, tested, and called by nothing.
//
// What the tests assert now is `check(Act::Exec, GIT).effect` itself, which is
// the fact, and [`git_allowance`] below is unchanged: it is still what lifts a
// `Deny` that came from a tier default rather than from a rule.

/// The rule that lets the git tools run at all.
///
/// **The posture this was built for stopped needing it, and the one that still
/// does is the other one.** [`crate::settings::Posture::AskWrites`] sets
/// `exec: Effect::Ask`, and through io-harness 0.69.0 that was a hard refusal in
/// the git spawn — the seven git tools refused for most operators without anyone
/// ever being asked, which io-cli 0.25.0 reported as io-harness#214. 0.70.0 fixed
/// it: an asking posture now raises an approval and git runs when the operator
/// says so, with no rule needed.
///
/// What still reaches here is a posture whose `exec` is `Deny` — `read only` — and
/// a deny is only liftable when it came from a **tier default** rather than from a
/// rule, which is the discriminator `crate::commit::asked` reads off
/// `Verdict::rule`. The repair is still a rule and not a special case in the spawn
/// path: io-cli offers this through the same `remembered` layer
/// [`effective_policy`] already builds, so a git allowance is exactly as strong as
/// any other thing the operator allowed for the session — and it still cannot
/// re-allow a deny from a layer beneath it, because layers add capability and
/// never take a denial back.
///
/// The pattern is one binary name. `Act::Exec` matching (see [`GIT`]) has no
/// notion of a subcommand, so `"git"` is the narrowest expressible grant that
/// makes the tools work: it says *this program may be spawned* and nothing about
/// any other. A pattern like `"*"` would answer the same criterion by handing over
/// every binary on the machine in order to hand over one.
pub fn git_allowance() -> Rule {
    Rule {
        act: Act::Exec,
        effect: Effect::Allow,
        pattern: GIT.into(),
    }
}
