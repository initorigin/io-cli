//! The status line: one row, always at the bottom of the viewport.
//!
//! This release fills three of its fields — the model answering, whether a turn
//! is running, and how long the session has been going. The rest of the line is
//! 0.2.0's: the policy layer in force, context pressure, spend against the tree
//! ceiling, and containment. They are named in [`Field`] now so that adding them
//! is filling in a value rather than redesigning the line.
//!
//! Its narrow form drops fields from the right rather than wrapping, because a
//! status line that becomes two lines has taken a row from the transcript and
//! stopped being a status line.

use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::{Theme, Tone};

/// The frames of the working indicator, in the Unicode set.
///
/// Braille, because every frame is exactly one cell wide — a spinner built from
/// characters of differing width shifts the whole line right and left as it turns,
/// which is worse than not moving at all. Ten frames and a modulo; a crate for
/// this would be the beginning of the thing this product exists not to become.
///
/// It never carries a meaning of its own. The state is the word beside it, and
/// this is only the evidence that the word is still true.
///
/// Reached through [`crate::glyphs::Glyphs::spinner`] rather than named directly
/// by the renderer, so a terminal that cannot draw braille turns
/// [`crate::glyphs::ASCII_SPINNER`] instead — which is held to the same one-cell
/// rule, for the same reason.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// A field of the status line, in priority order: the first is the last to be
/// dropped when the terminal is narrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub tone: Tone,
}

impl Field {
    fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
        }
    }
}

/// What the status line is currently saying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The model answering. First, because it is the field a reader looks for.
    pub model: String,
    /// Whether a turn is running.
    pub working: bool,
    /// The permission posture in force, by its short name — or `None` before one
    /// is known, which is the wizard's first moments and nothing else.
    ///
    /// It is a *posture*, which is an `io_harness::Defaults` set, and never a flag
    /// of io-cli's own. That is what makes this field an explanation rather than a
    /// decoration: the word here is the same thing the agent is actually bounded
    /// by, and a refusal can name the rule and the layer underneath it.
    pub policy: Option<String>,
    /// How long the session has been open.
    pub elapsed: Duration,
    /// Tokens this session has spent, accumulated from the steps that reported
    /// them. `None` until one does — a session that has spent nothing yet is not
    /// a session that has spent zero, and the difference is the whole of F9.
    pub tokens: Option<u64>,
    /// How full the assembled context was the last time io-harness said so, as a
    /// share of the budget io-harness itself declares.
    ///
    /// `None` until a fold reports one, which is the honest answer: `Compacted` is
    /// the only event carrying an observation-section size, and between folds
    /// nothing on the event stream knows it.
    ///
    /// ponytail: derived from the last fold. The per-step estimate is durable in
    /// the harness store as `ContextEvent::est_tokens`, so a live share is one
    /// store read away if this field turns out too quiet to be useful.
    pub context: Option<u8>,
    /// How this run's commands are contained: the mode asked for and the backend
    /// that actually answered on this host.
    ///
    /// Both, always. io-harness's own documentation is explicit that a surface
    /// showing the mode alone is reading an intention — `workspace-write` reaching
    /// a portable floor means resource caps and nothing else.
    pub containment: Option<String>,
    /// How much of the agent's plan the agent says is done, as done over total.
    ///
    /// `None` until the agent writes a list, and that is the whole of F12: a
    /// session with no plan has not written a plan of nothing, so this renders as
    /// nothing at all rather than as `0/0`. Set from a `TodoWrote`'s own items,
    /// which carry the whole list on every write and are never a delta — there is
    /// nothing to read back out of the store to complete it.
    ///
    /// It is drawn as a *claim*. io-harness's own documentation is explicit that
    /// nothing verifies a plan item, so an item saying `Done` is what the agent
    /// said about its own work; a field that stated it as a fact would be the one
    /// place in this product where the plan stopped being the agent's account.
    pub plan: Option<(usize, usize)>,
    /// Whether this session runs in plain mode.
    ///
    /// It lives on the status line rather than beside it because the status line
    /// is the only surface in this product that animates — so this is the field
    /// the mode is *about*, and putting it here means there is one boolean in the
    /// session rather than two that have to agree. [`crate::app::App`] reads it
    /// back off this struct for the same reason.
    ///
    /// A separate axis from the theme's colour, and from the glyph set. A
    /// monochrome terminal is not a reason to still the indicator, and a terminal
    /// that cannot draw braille gets an ASCII spinner that turns perfectly well —
    /// `NO_COLOR` and the ASCII set are both about what can be *drawn*, and this
    /// is about whether anything should *move*.
    pub plain: bool,
    /// Which frame of the indicator is showing. Advanced by the tick, never by
    /// the clock: an indicator that read the time would be a second timer.
    frame: usize,
}

impl Status {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            policy: None,
            tokens: None,
            context: None,
            containment: None,
            plan: None,
            working: false,
            elapsed: Duration::ZERO,
            plain: false,
            frame: 0,
        }
    }

    /// Move the indicator on one frame. Called from the tick and from nowhere
    /// else, so the animation cannot outlive the thing it is reporting on.
    pub fn advance(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Forget everything the *run* said, keeping what the *session* is.
    ///
    /// Called when the conversation under this line changes: `/resume` onto
    /// another session, `/fork` away from this one, a rewind that undoes the turn
    /// that set a field. Every field cleared here is a per-run fact — the tokens
    /// that run spent, how full its context got, how its commands were contained,
    /// how much of its plan the agent claimed — and none of them outlives the run
    /// that reported it. A line that goes on asserting them is describing a
    /// conversation that is no longer on the screen.
    ///
    /// The whole class rather than `plan` alone. `tokens`, `context` and
    /// `containment` have had the same hole since they were added and would want
    /// the same call at the same three sites, and four methods to make one moment
    /// true is three more than the moment has.
    ///
    /// Nothing is read back to replace them, though the store holds the resumed
    /// run's plan: F12 sets that field from `TodoWrote`'s own items with no store
    /// read, and absent is the honest answer until the agent writes a list in the
    /// run that is now on screen.
    ///
    /// The model, the posture, plain mode and the session's age are not run facts
    /// and are left alone — the session is the same session either way.
    pub fn forget_run(&mut self) {
        self.tokens = None;
        self.context = None;
        self.containment = None;
        self.plan = None;
    }

    /// The indicator, if there is anything to indicate and anywhere to show it.
    ///
    /// `None` under `NO_COLOR`, where an animation is noise a reader cannot use —
    /// and `None` when nothing is running, because a session that spins while it
    /// waits for a prompt is lying about being busy.
    ///
    /// **`None` in plain mode, which is the whole of the animation half of F1.**
    /// This is the one gate: the frames are reached through here and nowhere
    /// else, so a mode threaded to every other surface and missed here would
    /// still turn — which is the exact shape the criterion's sabotage arm names,
    /// and the reason this method is what `tests/plain.rs` asserts on directly
    /// rather than only through the bytes it eventually produces.
    pub fn indicator(&self, theme: &Theme) -> Option<char> {
        let frames = theme.glyphs.spinner;
        if self.plain || !self.working || !theme.coloured || frames.is_empty() {
            return None;
        }
        Some(frames[self.frame % frames.len()])
    }

    /// The fields, most important first.
    pub fn fields(&self, theme: &Theme) -> Vec<Field> {
        // The WORD is the state, and the animation is only beside it. A spinner
        // carries a meaning solely for a reader who can see it move, and this
        // line has to work in a screen reader, under `NO_COLOR` and in a log — so
        // the indicator is a prefix on the field, never the field itself.
        let state = match (self.working, self.indicator(theme)) {
            (true, Some(frame)) => Field::new(format!("{frame} working"), Tone::Normal),
            (true, None) => Field::new("working", Tone::Normal),
            (false, _) => Field::new("ready", Tone::Muted),
        };
        let mut fields = vec![Field::new(self.model.clone(), Tone::Accent)];
        // Second, and the last field to be dropped after the model. What the agent
        // is allowed to do outranks how long it has been doing it.
        if let Some(policy) = &self.policy {
            fields.push(Field::new(format!("policy:{policy}"), Tone::Normal));
        }
        fields.push(state);
        // Elapsed stays fourth: it is the field 0.1.1 exists for, and the one a
        // reader checks to answer "is this alive". Everything this release adds
        // goes to the right of it, which is the order they drop in.
        fields.push(Field::new(format_elapsed(self.elapsed), Tone::Muted));
        if let Some(tokens) = self.tokens {
            fields.push(Field::new(
                format!("{} tok", format_tokens(tokens)),
                Tone::Muted,
            ));
        }
        if let Some(context) = self.context {
            fields.push(Field::new(format!("ctx {context}%"), Tone::Muted));
        }
        if let Some(containment) = &self.containment {
            fields.push(Field::new(containment.clone(), Tone::Muted));
        }
        // Rightmost, and so the first field to go when the terminal narrows. It is
        // the only field on this line that is not an observation — everything to
        // its left is something the harness reported happening, and this is what
        // the agent says about its own work — and the plan itself is in the
        // transcript a row above for a reader who wants more than the count.
        //
        // `claimed` rather than `done` for that reason, in the same words the
        // transcript's own plan header uses. The one-word form is what fits beside
        // six other fields at eighty columns.
        if let Some((done, total)) = self.plan {
            fields.push(Field::new(
                format!("plan {done}/{total} claimed"),
                Tone::Muted,
            ));
        }
        fields
    }

    /// The line, fitted to `width` by dropping whole fields from the right.
    pub fn line(&self, width: u16, theme: &Theme) -> Line<'static> {
        let fields = self.fields(theme);
        let width = width as usize;
        // Measured off the chosen set rather than off a constant. Both sets spell
        // the separator in three cells, so the arithmetic below lands on the same
        // answer either way — but a set that did not would have shifted every
        // drop decision on this line, and this is the input that says so.
        let separator = theme.glyphs.separator;
        let separator_width = separator.chars().count();

        let mut kept: Vec<&Field> = Vec::new();
        let mut used = 0usize;
        for field in &fields {
            // Counted in characters, not bytes: the separator's middle dot is two
            // bytes and one cell, and `len()` here would reserve room that is not
            // needed and drop a field one column early.
            let extra =
                field.text.chars().count() + if kept.is_empty() { 0 } else { separator_width };
            if used + extra > width {
                break;
            }
            used += extra;
            kept.push(field);
        }

        // Even at a width that fits nothing whole, the model is what gets shown,
        // shortened. A blank status line is worse than a truncated one.
        if kept.is_empty() {
            let model: String = self.model.chars().take(width).collect();
            return Line::from(Span::styled(model, theme.style(Tone::Accent)));
        }

        let mut spans = Vec::new();
        for (index, field) in kept.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(separator, theme.style(Tone::Muted)));
            }
            spans.push(Span::styled(field.text.clone(), theme.style(field.tone)));
        }
        Line::from(spans)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        frame.render_widget(Paragraph::new(self.line(area.width, theme)), area);
    }
}

/// `12s`, `1m12s`, `1h02m`. Never a bare number of seconds past a minute, which
/// is unreadable at the point a session has been going long enough to care.
pub fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// `840`, `1.5k`, `12.4k`. A running total is read for its magnitude, and six
/// digits of it are six characters of a line that has to fit in eighty columns.
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    format!("{:.1}k", tokens as f64 / 1_000.0)
}

/// What a `Contained` event reads as: the mode, then the backend that answered.
///
/// Never the mode alone. The two disagree often — a `workspace-write` run on a
/// host with no sandbox available reaches the portable floor — and it is the
/// second word that says what is actually enforcing anything.
pub fn format_containment(mode: &str, backend: &str) -> String {
    format!("{mode}/{backend}")
}
