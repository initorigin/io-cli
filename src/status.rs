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

/// What separates two fields. Spaced, because a bare separator between two words
/// reads as punctuation inside one of them.
const SEPARATOR: &str = " · ";

/// The frames of the working indicator.
///
/// Braille, because every frame is exactly one cell wide — a spinner built from
/// characters of differing width shifts the whole line right and left as it turns,
/// which is worse than not moving at all. Ten frames and a modulo; a crate for
/// this would be the beginning of the thing this product exists not to become.
///
/// It never carries a meaning of its own. The state is the word beside it, and
/// this is only the evidence that the word is still true.
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
    /// How long the session has been open.
    pub elapsed: Duration,
    /// Which frame of the indicator is showing. Advanced by the tick, never by
    /// the clock: an indicator that read the time would be a second timer.
    frame: usize,
}

impl Status {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            working: false,
            elapsed: Duration::ZERO,
            frame: 0,
        }
    }

    /// Move the indicator on one frame. Called from the tick and from nowhere
    /// else, so the animation cannot outlive the thing it is reporting on.
    pub fn advance(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The indicator, if there is anything to indicate and anywhere to show it.
    ///
    /// `None` under `NO_COLOR`, where an animation is noise a reader cannot use —
    /// and `None` when nothing is running, because a session that spins while it
    /// waits for a prompt is lying about being busy.
    pub fn indicator(&self, theme: &Theme) -> Option<char> {
        (self.working && theme.coloured).then(|| SPINNER[self.frame % SPINNER.len()])
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
        vec![
            Field::new(self.model.clone(), Tone::Accent),
            state,
            Field::new(format_elapsed(self.elapsed), Tone::Muted),
        ]
    }

    /// The line, fitted to `width` by dropping whole fields from the right.
    pub fn line(&self, width: u16, theme: &Theme) -> Line<'static> {
        let fields = self.fields(theme);
        let width = width as usize;

        let mut kept: Vec<&Field> = Vec::new();
        let mut used = 0usize;
        for field in &fields {
            // Counted in characters, not bytes: the separator's middle dot is two
            // bytes and one cell, and `len()` here would reserve room that is not
            // needed and drop a field one column early.
            let extra = field.text.chars().count()
                + if kept.is_empty() {
                    0
                } else {
                    SEPARATOR.chars().count()
                };
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
                spans.push(Span::styled(SEPARATOR, theme.style(Tone::Muted)));
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
