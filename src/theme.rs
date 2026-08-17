//! The theme: twelve tokens, two shipped themes, and a rule that colour is never
//! the only thing carrying a meaning.
//!
//! The token set is deliberately small. Restraint is what makes a terminal
//! product look considered; a palette with twenty names is a palette nobody uses
//! consistently, and every extra token is another thing a second theme has to get
//! right.
//!
//! It was nine until 0.3.0, when syntax highlighting inside a diff added three:
//! keyword, string and literal. They are here rather than in the highlighter's
//! own theme format on purpose — `syntect` ships themes and this product does not
//! load them, so a highlighted diff and the rest of the interface stay one
//! aesthetic, and `NO_COLOR` keeps working because there is still exactly one
//! place that decides whether colour happens at all. A comment did not get a
//! fourth token: a comment is muted, `muted` already exists, and a token whose
//! value would always equal another's is a token that can drift out of agreement
//! with itself.
//!
//! Nothing here paints a background. io-cli does not own the screen — the
//! transcript is the terminal's own scrollback, sitting on whatever background
//! the user chose — so a background token exists to *detect* against, not to
//! fill with.

use std::fmt;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::glyphs::Glyphs;

/// Which way round the terminal is. Detected, because the same colour that reads
/// as "muted" on black is invisible on white.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    Dark,
    Light,
}

impl Background {
    /// What the environment says, defaulting to dark.
    ///
    /// `COLORFGBG` is the one signal available without talking to the terminal:
    /// it is a `foreground;background` pair of ANSI colour numbers, exported by
    /// several terminals and by tmux. The alternative is an OSC 11 query, which
    /// means writing an escape sequence and reading the reply back off stdin —
    /// a round trip that hangs on any terminal that does not answer, in a product
    /// whose first frame should already be on screen.
    ///
    /// Defaulting to dark rather than to light is not a coin toss: a dark
    /// terminal is the common case, and the light palette on a dark background is
    /// the less readable of the two mistakes.
    pub fn detect() -> Self {
        match std::env::var("COLORFGBG") {
            Ok(value) => Self::from_colorfgbg(&value),
            Err(_) => Self::Dark,
        }
    }

    /// Parse a `COLORFGBG` value. Its last field is the background colour number;
    /// 0-6 and 8 are the dark ones, 7 and 9-15 the light ones.
    pub fn from_colorfgbg(value: &str) -> Self {
        let background = value.rsplit(';').next().unwrap_or("").trim();
        match background.parse::<u8>() {
            Ok(0..=6) | Ok(8) => Self::Dark,
            Ok(_) => Self::Light,
            Err(_) => Self::Dark,
        }
    }
}

/// A state that colour distinguishes — and that therefore also has a word.
///
/// The pairing is the point. Colour as the sole carrier of a meaning is unusable
/// under `NO_COLOR`, on a monochrome terminal, and for a colour-blind reader, so
/// every tone that means something carries a word that means the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Ordinary text.
    Normal,
    /// Present but secondary: timings, ids, the status line's dimmer fields.
    Muted,
    /// The product's own colour. Prompts and selection.
    Accent,
    Success,
    Warning,
    Error,
    /// An act the permission boundary refused. Its own tone because it is not an
    /// error — the system worked.
    Refused,
    /// A line a change added. The `diff_add` token, which has been in the theme
    /// since 0.1.0 waiting for the release that draws a diff.
    Added,
    /// A line a change removed.
    Removed,
    /// A language keyword inside a diff.
    Keyword,
    /// A string literal inside a diff.
    StringLiteral,
    /// A number, a boolean, a named constant.
    Literal,
}

impl Tone {
    /// The word this tone carries, or `None` where the tone is decoration rather
    /// than meaning.
    pub fn word(self) -> Option<&'static str> {
        match self {
            // A diff line's carrier is the `+` or the `-` the harness already
            // put on it, which is why these two need no word of their own: the
            // meaning survives `NO_COLOR` without one.
            Self::Normal
            | Self::Muted
            | Self::Accent
            | Self::Added
            | Self::Removed
            | Self::Keyword
            | Self::StringLiteral
            | Self::Literal => None,
            Self::Success => Some("ok"),
            Self::Warning => Some("warning"),
            Self::Error => Some("error"),
            Self::Refused => Some("refused"),
        }
    }
}

impl fmt::Display for Tone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word().unwrap_or(""))
    }
}

/// The twelve tokens — and the glyph set that travels with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// Which marks this run draws with.
    ///
    /// **Not a thirteenth token, and not on the same axis.** Colour and drawable
    /// characters are two independent questions with two independent answers:
    /// `NO_COLOR` leaves the Unicode set alone and `--plain` leaves a coloured
    /// theme fully coloured. [`Theme::resolve`] proves it structurally — it takes
    /// the set as an argument and cannot derive one, so nothing about the colour
    /// decision can reach the glyph decision or the other way round.
    ///
    /// It lives here because the theme is the value this product already threads
    /// by hand into every surface that draws. A second parameter beside it would
    /// have been the same value, chosen at the same moment, taking the same route
    /// under another name — and would have had to be added to every function
    /// signature and every call site in the crate to say so. What the argument on
    /// `resolve` buys is that the set can never be *re-derived*: a theme is
    /// re-resolved three times as a session runs, and each of those places is now
    /// obliged to hand over the set that was chosen at startup rather than
    /// quietly working out a new one.
    pub glyphs: Glyphs,
    /// The background this theme was designed for. Never painted; see the module
    /// documentation.
    pub background: Background,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub diff_add: Color,
    pub diff_delete: Color,
    /// The three syntax tokens. Comments are deliberately not among them: a
    /// comment is muted, and `muted` already exists — a fourth token whose value
    /// would always equal an existing one is a token that can drift out of
    /// agreement with itself.
    pub syntax_keyword: Color,
    pub syntax_string: Color,
    pub syntax_literal: Color,
    /// Whether this theme emits colour at all. The `NO_COLOR` theme does not.
    pub coloured: bool,
}

/// For a dark terminal. The default.
pub const DARK: Theme = Theme {
    name: "dark",
    // The Unicode set, so a theme named in a test or a constant is the one
    // this product has always drawn. A run picks its own set and hands it to
    // `resolve`.
    glyphs: crate::glyphs::UNICODE,
    background: Background::Dark,
    // `Reset` rather than a colour: the user's own foreground is the right
    // foreground, and overriding it is how a theme ends up unreadable in somebody
    // else's terminal.
    foreground: Color::Reset,
    muted: Color::DarkGray,
    accent: Color::LightCyan,
    success: Color::LightGreen,
    warning: Color::LightYellow,
    error: Color::LightRed,
    diff_add: Color::LightGreen,
    diff_delete: Color::LightRed,
    // Indexed rather than the sixteen, and the reason is crowding: the eight
    // bright ANSI colours are already spoken for by accent, success, warning,
    // error and the two diff tokens, so a syntax colour taken from them would
    // read as one of those meanings. These three are muted enough to sit under
    // a diff without competing with the green and red that carry it.
    syntax_keyword: Color::Indexed(176),
    syntax_string: Color::Indexed(114),
    syntax_literal: Color::Indexed(180),
    coloured: true,
};

/// For a light terminal.
pub const LIGHT: Theme = Theme {
    name: "light",
    // The Unicode set, so a theme named in a test or a constant is the one
    // this product has always drawn. A run picks its own set and hands it to
    // `resolve`.
    glyphs: crate::glyphs::UNICODE,
    background: Background::Light,
    foreground: Color::Reset,
    muted: Color::DarkGray,
    accent: Color::Blue,
    success: Color::Green,
    // Indexed rather than `Yellow`: ANSI yellow on white is close to invisible,
    // and 130 is the dark orange every 256-colour terminal has. The ceiling is
    // that a 16-colour terminal will approximate it; the alternative was a token
    // the light theme could not express at all.
    warning: Color::Indexed(130),
    error: Color::Red,
    diff_add: Color::Green,
    diff_delete: Color::Red,
    syntax_keyword: Color::Indexed(90),
    syntax_string: Color::Indexed(28),
    syntax_literal: Color::Indexed(130),
    coloured: true,
};

/// No colour at all. What `NO_COLOR` selects.
///
/// It is `MONO` and not `PLAIN` because 0.6.0 gives the word "plain" a second
/// and unrelated meaning — `--plain`, the accessibility mode that stills the
/// animation, stops repainting and draws in ASCII. The two are different axes:
/// `--plain` in a colour terminal is still fully coloured, and `NO_COLOR`
/// leaves every animation running. One word across both would have read as one
/// switch, so the theme takes the word that describes what it actually is.
pub const MONO: Theme = Theme {
    name: "mono",
    // The Unicode set, so a theme named in a test or a constant is the one
    // this product has always drawn. A run picks its own set and hands it to
    // `resolve`.
    glyphs: crate::glyphs::UNICODE,
    background: Background::Dark,
    foreground: Color::Reset,
    muted: Color::Reset,
    accent: Color::Reset,
    success: Color::Reset,
    warning: Color::Reset,
    error: Color::Reset,
    diff_add: Color::Reset,
    diff_delete: Color::Reset,
    syntax_keyword: Color::Reset,
    syntax_string: Color::Reset,
    syntax_literal: Color::Reset,
    coloured: false,
};

/// The themes a user can choose between. `MONO` is not among them: it is not a
/// preference, it is what `NO_COLOR` forces. Its `name` is a label for
/// diagnostics rather than a key — `by_name` searches this list and nothing
/// else, so there is no string a configuration file could carry that selects
/// it, under either of its names.
pub const THEMES: &[Theme] = &[DARK, LIGHT];

impl Theme {
    /// The theme named, or `None`.
    pub fn by_name(name: &str) -> Option<Self> {
        THEMES.iter().copied().find(|theme| theme.name == name)
    }

    /// The same theme, drawn with a different set of marks.
    ///
    /// The one way the glyph set is ever attached, which is what makes "chosen
    /// once" checkable: search for this name and every attachment is in front of
    /// you.
    pub const fn with_glyphs(mut self, glyphs: Glyphs) -> Self {
        self.glyphs = glyphs;
        self
    }

    /// Choose a theme from what the environment says.
    ///
    /// `NO_COLOR` wins outright and is honoured on presence, whatever its value,
    /// which is what the convention specifies.
    ///
    /// `glyphs` is **handed in and never derived**, and that is the whole of how
    /// the two axes stay independent. Nothing in this function reads it and
    /// nothing it reads can change it: `NO_COLOR` selects `MONO` and leaves the
    /// marks alone, and an ASCII set arrives at a fully coloured theme. It is a
    /// parameter rather than a default because a theme is re-resolved three times
    /// as a session runs — `/theme`, the wizard's seed, the wizard's live preview
    /// — and a default there would silently discard a `--plain` at each one.
    pub fn resolve(
        no_color: bool,
        background: Background,
        chosen: Option<&str>,
        glyphs: Glyphs,
    ) -> Self {
        let base = if no_color {
            MONO
        } else if let Some(theme) = chosen.and_then(Self::by_name) {
            theme
        } else {
            match background {
                Background::Dark => DARK,
                Background::Light => LIGHT,
            }
        };
        base.with_glyphs(glyphs)
    }

    /// The same, reading the environment.
    pub fn from_env(chosen: Option<&str>, glyphs: Glyphs) -> Self {
        Self::resolve(
            std::env::var_os("NO_COLOR").is_some(),
            Background::detect(),
            chosen,
            glyphs,
        )
    }

    /// The style for a tone.
    pub fn style(&self, tone: Tone) -> Style {
        if !self.coloured {
            // Not even a modifier. A dim or italic attribute is still a
            // presentation-only carrier of meaning, and the word is already
            // carrying it.
            return Style::default();
        }
        match tone {
            Tone::Normal => Style::default().fg(self.foreground),
            Tone::Muted => Style::default().fg(self.muted),
            Tone::Accent => Style::default().fg(self.accent),
            Tone::Success => Style::default().fg(self.success),
            Tone::Warning => Style::default().fg(self.warning),
            Tone::Error => Style::default().fg(self.error).add_modifier(Modifier::BOLD),
            Tone::Refused => Style::default()
                .fg(self.warning)
                .add_modifier(Modifier::BOLD),
            Tone::Added => Style::default().fg(self.diff_add),
            Tone::Removed => Style::default().fg(self.diff_delete),
            Tone::Keyword => Style::default().fg(self.syntax_keyword),
            Tone::StringLiteral => Style::default().fg(self.syntax_string),
            Tone::Literal => Style::default().fg(self.syntax_literal),
        }
    }

    /// A tone, emphasised — for the words inside a diff line that actually
    /// changed.
    ///
    /// Bold rather than a background colour: a background paints the full cell
    /// width of every character it covers, which on a diff line means a block
    /// that survives being copied out of the terminal as trailing whitespace.
    /// Under `NO_COLOR` this collapses to nothing along with everything else,
    /// which is correct — the `+` and the `-` are the carriers there, and a
    /// modifier is still a presentation-only channel.
    pub fn emphasis(&self, tone: Tone) -> Style {
        if !self.coloured {
            return Style::default();
        }
        self.style(tone).add_modifier(Modifier::BOLD)
    }

    /// One line carrying a state, with the tone's word in front of it.
    ///
    /// This is the only way a toned line is built, so "colour is never the sole
    /// carrier of meaning" is a property of the constructor rather than a rule
    /// each call site has to remember. N6 asserts it.
    pub fn notice<'a>(&self, tone: Tone, text: impl Into<String>) -> Line<'a> {
        let style = self.style(tone);
        match tone.word() {
            Some(word) => Line::from(vec![
                Span::styled(format!("{word}: "), style),
                Span::styled(text.into(), self.style(Tone::Normal)),
            ]),
            None => Line::from(Span::styled(text.into(), style)),
        }
    }
}
