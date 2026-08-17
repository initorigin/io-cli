//! The glyph set: ten classes of non-letter mark, in a Unicode form and an ASCII
//! form, chosen once at startup and carried to every surface.
//!
//! Until 0.6.0 every one of these was a literal typed at the place it was drawn —
//! a middle dot in the status line, an arrow in the picker, a horizontal bar in
//! the transcript's edges — and there were four separate spellings of the same
//! selection marker. That is fine until the terminal cannot draw one. A terminal
//! whose locale does not claim UTF-8, a serial console, a CI log capture and a
//! braille display all render an unsupported code point as a replacement
//! character or as nothing at all, and the marks this product leans on are
//! exactly the ones that carry meaning without words: the row that is selected,
//! the fact that a line was shortened, the count of what an elision hid. A
//! product whose selection marker vanishes has lost the selection, not a
//! decoration.
//!
//! So the set is a value, and there are two of them. **Every ASCII form carries
//! the same meaning as its Unicode counterpart rather than merely standing in the
//! same column**: the marker still marks, the elision still says how many lines
//! went, the spinner still turns. Where a substitution would have changed a
//! meaning it was not made — see the note on the splash below.
//!
//! **It is chosen exactly once**, by the same shape [`crate::theme::Theme`] is:
//! a pure [`Glyphs::resolve`] that a test drives directly, and a
//! [`Glyphs::from_env`] that reads the environment in one place and calls it.
//! Nothing downstream re-derives it. That is not tidiness — the theme is
//! re-resolved in three places as a session runs (`/theme`, the wizard's live
//! preview, the wizard's seed), and a glyph set derived a second time in any of
//! them would silently throw away a `--plain` the operator asked for.
//!
//! **It rides on the `Theme`.** The two are independent *axes* — `NO_COLOR` must
//! not force ASCII and ASCII must not force monochrome, and neither resolver
//! reads the other's inputs — but they are the same *wire*: the theme is already
//! threaded by hand to every surface that draws, so a second parameter beside it
//! would have been the same value taking the same route under a different name.
//! What keeps the axes honest is that [`crate::theme::Theme::resolve`] takes the
//! chosen set as an argument and never derives one, so the only way a theme can
//! exist is for somebody to have handed it the set that was chosen at startup.
//!
//! **The eleventh class is deliberately absent.** The IO CLI mark in
//! `crate::splash` is drawn in box-drawing characters, and it is *suppressed*
//! when it cannot be drawn rather than transliterated — `splash::visible` already
//! decides that. A wordmark redrawn in `#` is not the wordmark; it is a different
//! and worse image wearing its name. Every other class here degrades because a
//! degraded separator is still a separator, and that is the whole test of whether
//! a class belongs in this file.

/// The frames of the ASCII working indicator.
///
/// Four frames of a rotating bar. Each is exactly one cell wide, which is the
/// same constraint the braille set is documented with at
/// [`crate::status::SPINNER`] and for the same reason: a spinner whose frames
/// differ in width shifts the whole status line right and left as it turns,
/// which is worse than not moving at all.
///
/// Four rather than ten because there are only four distinguishable rotations in
/// ASCII. A longer cycle would have to repeat frames, and a frame that repeats
/// looks like an indicator that has stopped.
pub const ASCII_SPINNER: [char; 4] = ['|', '/', '-', '\\'];

/// One set of marks.
///
/// Every field is the whole run of characters that gets drawn, including the
/// spaces around it, so a call site is a substitution and never an assembly. The
/// two sets agree on the width of everything except [`Glyphs::ellipsis`] and
/// [`Glyphs::elision`], which are one cell in Unicode and three in ASCII — see
/// [`crate::picker::fit`] for the arithmetic that depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyphs {
    /// A label for diagnostics and the key a configuration file can name.
    pub name: &'static str,
    /// What separates two fields on one row, spaces included.
    ///
    /// Spaced, because a bare separator between two words reads as punctuation
    /// inside one of them. One separator in the product, so a transcript and the
    /// status line under it read as one surface.
    ///
    /// The ASCII form is a pipe rather than a hyphen, and is three cells exactly
    /// as the Unicode form is. A hyphen would read as a minus sign inside the
    /// numeric fields it sits between (`12s - 1.5k tok`), and a run of a
    /// different width would change the arithmetic in
    /// [`crate::status::Status::line`], which drops whole fields by counting this
    /// string.
    pub separator: &'static str,
    /// The bullet in front of a tool call.
    pub bullet: &'static str,
    /// The marker in front of the selected row, **and the space after it**.
    ///
    /// Two cells in both sets, which is what keeps it interchangeable with the
    /// two spaces an unmarked row is drawn with and keeps the terminal cursor,
    /// which is placed just past it, on the first character of the label.
    pub marker: &'static str,
    /// What says a piece of text was shortened to fit.
    ///
    /// Three cells in ASCII against one in Unicode. Every fitter measures this
    /// rather than assuming a width; a fitter that reserved one cell and then
    /// appended three is how a row gets clipped.
    pub ellipsis: &'static str,
    /// What says whole lines were left out, and is always followed by how many.
    ///
    /// A different class from [`Glyphs::ellipsis`] even though ASCII spells them
    /// the same way: one means *this text continues*, the other means *lines are
    /// missing here*, and the Unicode set draws them differently because the
    /// distinction is worth drawing.
    pub elision: &'static str,
    /// The dash that joins a fact to its explanation in a sentence.
    ///
    /// One cell in both sets. Only the dashes in **rendered strings** are here;
    /// the ones in this crate's own prose are source, not output.
    pub dash: &'static str,
    /// One character of the rule that opens and closes a committed transcript.
    pub rule: char,
    /// The opening quote around a prompt being quoted back.
    pub quote_open: &'static str,
    /// The closing quote. A separate field because the Unicode forms differ and
    /// the ASCII ones do not.
    pub quote_close: &'static str,
    /// What a credential field shows instead of what was typed.
    pub mask: char,
    /// The frames of the working indicator, every one of them one cell wide.
    pub spinner: &'static [char],
}

/// What a terminal that can draw anything gets. The default.
pub const UNICODE: Glyphs = Glyphs {
    name: "unicode",
    separator: " · ",
    bullet: "⋅",
    marker: "› ",
    ellipsis: "…",
    elision: "⋯",
    dash: "—",
    rule: '─',
    quote_open: "“",
    quote_close: "”",
    mask: '•',
    // The braille frames, which live in `status` with the note explaining why
    // they are braille. Referenced rather than copied: two spellings of one
    // spinner is two things to keep in agreement.
    spinner: &crate::status::SPINNER,
};

/// What a terminal that does not claim UTF-8 gets, and what `--plain` forces.
pub const ASCII: Glyphs = Glyphs {
    name: "ascii",
    separator: " | ",
    bullet: "*",
    marker: "> ",
    ellipsis: "...",
    elision: "...",
    dash: "-",
    rule: '-',
    quote_open: "\"",
    quote_close: "\"",
    mask: '*',
    spinner: &ASCII_SPINNER,
};

/// The sets a configuration file can name.
///
/// Both are here, unlike [`crate::theme::THEMES`], which hides `MONO`. `MONO` is
/// not a preference — it is what `NO_COLOR` forces — whereas asking for the ASCII
/// set on a terminal that would have taken the Unicode one is a legitimate thing
/// to want: a font with no braille in it renders the spinner as boxes long before
/// the locale admits to anything.
pub const SETS: &[Glyphs] = &[UNICODE, ASCII];

impl Glyphs {
    /// The set named, or `None`.
    pub fn by_name(name: &str) -> Option<Self> {
        SETS.iter().copied().find(|set| set.name == name)
    }

    /// Choose a set from what was asked for and what the terminal claims.
    ///
    /// `plain` wins outright, exactly as `NO_COLOR` does in
    /// [`crate::theme::Theme::resolve`]: it is the accessibility escape hatch, and
    /// an escape hatch a configuration file can overrule is not one.
    ///
    /// `utf8` is the terminal's own claim, and it is the last word rather than the
    /// first. A configuration file naming a set is a person who has looked at
    /// their terminal saying what it can actually draw, which beats a locale
    /// variable — the locale is frequently right about the encoding and wrong
    /// about the font.
    pub fn resolve(plain: bool, utf8: bool, chosen: Option<&str>) -> Self {
        if plain {
            return ASCII;
        }
        if let Some(set) = chosen.and_then(Self::by_name) {
            return set;
        }
        if utf8 {
            UNICODE
        } else {
            ASCII
        }
    }

    /// The same, reading the environment for the terminal's claim.
    ///
    /// `plain` is a parameter rather than something read here because it is a
    /// command-line flag and not a variable; this is still the one place the
    /// environment is consulted.
    pub fn from_env(plain: bool, chosen: Option<&str>) -> Self {
        Self::resolve(plain, utf8_locale(), chosen)
    }
}

/// Whether the environment's locale claims UTF-8.
///
/// `LC_ALL`, then `LC_CTYPE`, then `LANG`, which is POSIX's own order of
/// precedence: `LC_ALL` overrides every category, `LC_CTYPE` is the category that
/// actually governs character classification, and `LANG` is the fallback for any
/// category not otherwise set. The first one **present** decides, whatever it
/// says — a `LC_ALL=C` with a UTF-8 `LANG` underneath it is a person or a script
/// having deliberately asked for C, and reading past it to the variable it
/// overrides would ignore them.
///
/// Defaulting to true when nothing is set is not a coin toss. An empty
/// environment is a container, a CI runner or a `env -i`, where the terminal is
/// almost always a modern one and the locale simply was not exported; and of the
/// two ways to be wrong, drawing a box instead of a dot is recoverable with
/// `--plain` while drawing ASCII on a terminal that could have done better is a
/// downgrade nobody asked for and nobody is told about.
pub fn utf8_locale() -> bool {
    for name in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Some(value) = std::env::var_os(name) {
            return claims_utf8(&value.to_string_lossy());
        }
    }
    true
}

/// Whether one locale value names UTF-8.
///
/// Case-insensitive, and both spellings: the codeset suffix is written `UTF-8` by
/// glibc and `UTF8` or `utf8` by macOS, BSD and a good deal of shell
/// configuration, and a check that knew only one of them would put half the
/// world's terminals into the ASCII set.
pub fn claims_utf8(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    lowered.contains("utf-8") || lowered.contains("utf8")
}
