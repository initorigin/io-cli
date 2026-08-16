//! The theme: nine tokens, two shipped themes, and a rule that colour is never
//! the only thing carrying a meaning.
//!
//! The token set is deliberately small. Restraint is what makes a terminal
//! product look considered; a palette with twenty names is a palette nobody uses
//! consistently, and every extra token is another thing a second theme has to get
//! right.
//!
//! Nothing here paints a background. io-cli does not own the screen — the
//! transcript is the terminal's own scrollback, sitting on whatever background
//! the user chose — so a background token exists to *detect* against, not to
//! fill with.

use std::fmt;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

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
}

impl Tone {
    /// The word this tone carries, or `None` where the tone is decoration rather
    /// than meaning.
    pub fn word(self) -> Option<&'static str> {
        match self {
            Self::Normal | Self::Muted | Self::Accent => None,
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

/// The nine tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
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
    /// Whether this theme emits colour at all. The `NO_COLOR` theme does not.
    pub coloured: bool,
}

/// For a dark terminal. The default.
pub const DARK: Theme = Theme {
    name: "dark",
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
    coloured: true,
};

/// For a light terminal.
pub const LIGHT: Theme = Theme {
    name: "light",
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
    coloured: true,
};

/// No colour at all. What `NO_COLOR` selects.
pub const PLAIN: Theme = Theme {
    name: "plain",
    background: Background::Dark,
    foreground: Color::Reset,
    muted: Color::Reset,
    accent: Color::Reset,
    success: Color::Reset,
    warning: Color::Reset,
    error: Color::Reset,
    diff_add: Color::Reset,
    diff_delete: Color::Reset,
    coloured: false,
};

/// The themes a user can choose between. `PLAIN` is not among them: it is not a
/// preference, it is what `NO_COLOR` forces.
pub const THEMES: &[Theme] = &[DARK, LIGHT];

impl Theme {
    /// The theme named, or `None`.
    pub fn by_name(name: &str) -> Option<Self> {
        THEMES.iter().copied().find(|theme| theme.name == name)
    }

    /// Choose a theme from what the environment says.
    ///
    /// `NO_COLOR` wins outright and is honoured on presence, whatever its value,
    /// which is what the convention specifies.
    pub fn resolve(no_color: bool, background: Background, chosen: Option<&str>) -> Self {
        if no_color {
            return PLAIN;
        }
        if let Some(theme) = chosen.and_then(Self::by_name) {
            return theme;
        }
        match background {
            Background::Dark => DARK,
            Background::Light => LIGHT,
        }
    }

    /// The same, reading the environment.
    pub fn from_env(chosen: Option<&str>) -> Self {
        Self::resolve(
            std::env::var_os("NO_COLOR").is_some(),
            Background::detect(),
            chosen,
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
        }
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
