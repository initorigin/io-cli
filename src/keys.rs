//! Which key does what, once `[app.io-cli.keys]` has had its say.
//!
//! The session's own keys were a `match` on a `KeyEvent` written into
//! [`crate::app::App::key`], which is fine until somebody's terminal eats
//! `Ctrl+L`, or their muscle memory says `Ctrl+R` for the thing this product
//! spells `Esc Esc`. This module is the indirection that fixes that, and it is
//! deliberately the smallest one that does: a table of chords, a lookup, and a
//! way to render the table that is *in force* rather than the one that shipped.
//!
//! # The syntax
//!
//! A binding is a chord, or two chords separated by a space:
//!
//! ```toml
//! [app.io-cli.keys]
//! clear = "ctrl+k"
//! rewind = "ctrl+r ctrl+r"
//! ```
//!
//! Modifiers are `ctrl`, `alt` and `shift`, joined to the key with `+`, in any
//! order and in any case. Key names are a single character (`l`), a named key
//! (`esc`, `enter`, `tab`, `backtab`, `space`, `up`, `down`, `left`, `right`,
//! `home`, `end`, `pageup`, `pagedown`, `backspace`, `delete`, `insert`) or a
//! function key (`f1` … `f12`).
//!
//! **This spelling is public contract from 0.6.0 on.** It is the one VS Code,
//! Zed and helix already write, which means it is the one a reader will guess
//! right on the first try, and a syntax nobody has to look up is worth more here
//! than one that could express more.
//!
//! # What is not rebindable
//!
//! [`Action::Interrupt`] — `Ctrl+C` — is fixed, and it is the only one. It is
//! the key that interrupts a running turn and leaves the program, so a
//! configuration file that could take it away is a file that could lock an
//! operator inside a running agent. Both spellings of that mistake are refused
//! out loud: naming `interrupt`, and binding anything *else* onto `ctrl+c`.
//!
//! # What a bad line does
//!
//! Nothing fatal, and nothing silent. An unreadable value leaves its action on
//! the default and says so, naming both the action and the key it kept; a name
//! that is no action of ours says which names there are. The notices come back
//! as text rather than being printed here, because this module has no surface —
//! the session commits them to the scrollback like any other notice, which is
//! also what makes them assertable without a terminal.

use std::collections::BTreeMap;
use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// The modifiers a binding may name.
///
/// Every other bit a terminal can report — `SUPER`, `HYPER`, `META`, and the
/// keypad flag the Kitty protocol adds — is masked off both when a chord is
/// parsed and when a keystroke is turned into one, so a terminal that reports
/// more than another still matches the same binding. A modifier this product
/// cannot name in a binding is one it must not distinguish on either.
fn known() -> KeyModifiers {
    KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT
}

/// One keystroke: a key and the modifiers held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    code: KeyCode,
    mods: KeyModifiers,
}

impl Chord {
    /// The chord a keystroke *is*, normalized.
    ///
    /// The one normalization is `BackTab`, and it is the reason this function
    /// exists rather than a struct literal at the call site. A terminal without
    /// the Kitty keyboard protocol sends `Shift+Tab` as `BackTab` with no
    /// modifier; one that has negotiated it sends `Tab` with shift. They are one
    /// key, and folding them here is what lets a single binding — the string
    /// `"shift+tab"` — match on both terminals. Binding either spelling alone
    /// ships a key that works on the developer's machine and silently does
    /// nothing on somebody else's.
    pub fn of(key: KeyEvent) -> Self {
        let mut chord = Self {
            code: key.code,
            mods: key.modifiers & known(),
        };
        if chord.code == KeyCode::BackTab {
            chord.code = KeyCode::Tab;
            chord.mods |= KeyModifiers::SHIFT;
        }
        // The same fold for a shifted letter, and for the same reason: a
        // terminal may report `Shift+K` as `K`, as `k` with shift, or as both,
        // and a binding that matched only the spelling the author's terminal
        // happened to send would be a binding that "does nothing" on somebody
        // else's machine with no way to tell why. A binding is written
        // lower-case, so this is the side that has to move.
        if let KeyCode::Char(character) = chord.code {
            if character.is_ascii_uppercase() {
                chord.code = KeyCode::Char(character.to_ascii_lowercase());
                chord.mods |= KeyModifiers::SHIFT;
            }
        }
        chord
    }

    /// One chord of a binding, or `None` if it is not a chord this can read.
    fn parse(text: &str) -> Option<Self> {
        let lowered = text.trim().to_ascii_lowercase();
        let mut mods = KeyModifiers::NONE;
        let mut name: Option<&str> = None;
        for part in lowered.split('+') {
            match part.trim() {
                // Which also means `+` itself cannot be bound. That is a real
                // limit of a syntax that joins with `+`, and stating it is
                // better than a rule that silently works for `plus` and not for
                // the character everyone would actually type.
                "" => return None,
                "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
                "alt" | "option" => mods |= KeyModifiers::ALT,
                "shift" => mods |= KeyModifiers::SHIFT,
                other => {
                    // A second key name in one chord is a typo, not a chord.
                    if name.is_some() {
                        return None;
                    }
                    name = Some(other);
                }
            }
        }
        let (code, extra) = code_of(name?)?;
        Some(Self {
            code,
            mods: (mods | extra) & known(),
        })
    }
}

/// A key name, and whatever modifier the name itself implies.
fn code_of(name: &str) -> Option<(KeyCode, KeyModifiers)> {
    let plain = |code| Some((code, KeyModifiers::NONE));
    match name {
        "esc" | "escape" => plain(KeyCode::Esc),
        "enter" | "return" => plain(KeyCode::Enter),
        "tab" => plain(KeyCode::Tab),
        // The name a terminal's own documentation uses for the key this product
        // spells `shift+tab`. Both parse to the same chord — see `Chord::of`.
        "backtab" => Some((KeyCode::Tab, KeyModifiers::SHIFT)),
        "space" => plain(KeyCode::Char(' ')),
        "up" => plain(KeyCode::Up),
        "down" => plain(KeyCode::Down),
        "left" => plain(KeyCode::Left),
        "right" => plain(KeyCode::Right),
        "home" => plain(KeyCode::Home),
        "end" => plain(KeyCode::End),
        "pageup" => plain(KeyCode::PageUp),
        "pagedown" => plain(KeyCode::PageDown),
        "backspace" => plain(KeyCode::Backspace),
        "delete" | "del" => plain(KeyCode::Delete),
        "insert" | "ins" => plain(KeyCode::Insert),
        _ => {
            if let Some(number) = name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                if (1..=12).contains(&number) {
                    return plain(KeyCode::F(number));
                }
            }
            let mut characters = name.chars();
            match (characters.next(), characters.next()) {
                (Some(character), None) => plain(KeyCode::Char(character)),
                _ => None,
            }
        }
    }
}

impl fmt::Display for Chord {
    /// How the table spells this chord.
    ///
    /// Title case with `+`, which is what the shipped table already says and
    /// what a reader recognises — the configuration file's lowercase is for
    /// typing, this is for reading. The two are not the same string on purpose:
    /// a table that printed `ctrl+l` would be a table that had given up on
    /// looking like the product's own documentation.
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.contains(KeyModifiers::CONTROL) {
            write!(out, "Ctrl+")?;
        }
        if self.mods.contains(KeyModifiers::ALT) {
            write!(out, "Alt+")?;
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            write!(out, "Shift+")?;
        }
        match self.code {
            KeyCode::Char(' ') => write!(out, "Space"),
            KeyCode::Char(character) => write!(out, "{}", character.to_uppercase()),
            KeyCode::Esc => write!(out, "Esc"),
            KeyCode::Enter => write!(out, "Enter"),
            KeyCode::Tab | KeyCode::BackTab => write!(out, "Tab"),
            KeyCode::F(number) => write!(out, "F{number}"),
            KeyCode::Up => write!(out, "Up"),
            KeyCode::Down => write!(out, "Down"),
            KeyCode::Left => write!(out, "Left"),
            KeyCode::Right => write!(out, "Right"),
            KeyCode::Home => write!(out, "Home"),
            KeyCode::End => write!(out, "End"),
            KeyCode::PageUp => write!(out, "PageUp"),
            KeyCode::PageDown => write!(out, "PageDown"),
            KeyCode::Backspace => write!(out, "Backspace"),
            KeyCode::Delete => write!(out, "Delete"),
            KeyCode::Insert => write!(out, "Insert"),
            other => write!(out, "{other:?}"),
        }
    }
}

/// What one action is bound to: a chord, or two pressed in sequence.
///
/// Two is the most, and that is a decision rather than a stopping point. One
/// sequence exists in this product — the rewind, which asks for a second press
/// because it is the only key that changes the operator's files on io-cli's own
/// initiative — and a general chord-sequence parser to serve one binding would
/// be machinery nobody asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    first: Chord,
    second: Option<Chord>,
}

impl Binding {
    /// Read a binding, or `None` if any part of it is unreadable.
    pub fn parse(text: &str) -> Option<Self> {
        let mut chords = text.split_whitespace();
        let first = Chord::parse(chords.next()?)?;
        let second = match chords.next() {
            Some(text) => Some(Chord::parse(text)?),
            None => None,
        };
        // A third chord is refused rather than ignored: a binding that silently
        // dropped part of what it was given would be a binding that does
        // something other than what the file says.
        if chords.next().is_some() {
            return None;
        }
        Some(Self { first, second })
    }

    /// Whether this binding presses `chord` at any point, which is what makes
    /// `clear = "ctrl+c"` refusable rather than merely wrong.
    fn uses(&self, chord: Chord) -> bool {
        self.first == chord || self.second == Some(chord)
    }
}

impl fmt::Display for Binding {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.second {
            Some(second) => write!(out, "{} {second}", self.first),
            None => write!(out, "{}", self.first),
        }
    }
}

/// Everything the session itself does with a key.
///
/// The composer's keys — `Enter`, `Shift+Enter`, the history arrows — and the
/// overlays' — `y`/`a`/`n`, and `Esc` to close a picker — are not here. They
/// belong to types that own the keyboard while they are up, and rebinding them
/// is a different question with a different answer: an approval's three letters
/// are the *words* of the answer, not shortcuts for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Interrupt the running turn; twice at an empty prompt, leave. **Fixed.**
    Interrupt,
    /// Leave, on an empty prompt.
    Exit,
    /// Cycle the permission posture, from the next turn.
    Posture,
    /// Clear the viewport, never the scrollback.
    Clear,
    /// Put the whole conversation back into the scrollback.
    Transcript,
    /// Undo the last turn — its files and all. Two presses.
    Rewind,
}

impl Action {
    /// Every action, in the order [`Keys`] indexes them by. `Action as usize` is
    /// that index, which `tests/keys.rs` asserts rather than assumes.
    pub const ALL: &'static [Action] = &[
        Action::Interrupt,
        Action::Exit,
        Action::Posture,
        Action::Clear,
        Action::Transcript,
        Action::Rewind,
    ];

    /// The name this action is called by in `[app.io-cli.keys]`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "interrupt",
            Self::Exit => "exit",
            Self::Posture => "posture",
            Self::Clear => "clear",
            Self::Transcript => "transcript",
            Self::Rewind => "rewind",
        }
    }

    /// The action that name means, if it is one.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.name() == name)
    }

    /// What this action is bound to when nobody has said otherwise, in the
    /// configuration file's own syntax.
    ///
    /// The string is the single source: the table's label is this parsed and
    /// rendered back, so the documented default and the working default cannot
    /// be two different keys.
    pub fn default_binding(self) -> &'static str {
        match self {
            Self::Interrupt => "ctrl+c",
            Self::Exit => "ctrl+d",
            Self::Posture => "shift+tab",
            Self::Clear => "ctrl+l",
            Self::Transcript => "ctrl+t",
            Self::Rewind => "esc esc",
        }
    }

    /// Whether a configuration file may move this action.
    ///
    /// One answer is `false`, and see the module documentation for why it is
    /// that one.
    pub fn rebindable(self) -> bool {
        !matches!(self, Self::Interrupt)
    }

    fn binding(self) -> Binding {
        Binding::parse(self.default_binding())
            .expect("every default binding parses; tests/keys.rs asserts it")
    }
}

/// What a keystroke did: started a sequence, or ran an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// The first chord of a two-chord binding. Nothing has happened yet.
    Arm(Action),
    /// Do it.
    Fire(Action),
}

impl Hit {
    pub fn action(self) -> Action {
        match self {
            Self::Arm(action) | Self::Fire(action) => action,
        }
    }
}

/// The bindings in force.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    bound: Vec<Binding>,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            bound: Action::ALL.iter().map(|action| action.binding()).collect(),
        }
    }
}

impl Keys {
    /// The bindings a configuration file asks for, and what could not be
    /// honoured about it.
    ///
    /// Every notice is a sentence a session can commit as it starts. Nothing
    /// here fails: a file that names one key wrongly still gets the other ten,
    /// because a session that refuses to start over a keybinding is a worse
    /// answer than one that starts and says what it ignored.
    pub fn resolve(configured: Option<&BTreeMap<String, String>>) -> (Self, Vec<String>) {
        let mut keys = Self::default();
        let mut notices = Vec::new();
        let Some(configured) = configured else {
            return (keys, notices);
        };
        for (name, value) in configured {
            let Some(action) = Action::named(name) else {
                let names: Vec<&str> = Action::ALL
                    .iter()
                    .filter(|action| action.rebindable())
                    .map(|action| action.name())
                    .collect();
                notices.push(format!(
                    "[app.io-cli.keys] names no action `{name}`; the ones it can name are {}",
                    names.join(", ")
                ));
                continue;
            };
            if !action.rebindable() {
                notices.push(refusal(&format!("`{name}` cannot be moved")));
                continue;
            }
            let Some(binding) = Binding::parse(value) else {
                notices.push(format!(
                    "[app.io-cli.keys] `{name} = \"{value}\"` is not a key I can read, so \
                     {name} stays on {}",
                    action.binding()
                ));
                continue;
            };
            if binding.uses(Action::Interrupt.binding().first) {
                notices.push(refusal(&format!("`{name}` cannot be put on it")));
                continue;
            }
            keys.bound[action as usize] = binding;
        }
        (keys, notices)
    }

    /// What this action is bound to right now.
    pub fn binding(&self, action: Action) -> Binding {
        self.bound[action as usize]
    }

    /// What a keystroke means, given whichever action a previous keystroke armed.
    ///
    /// The armed action is passed in rather than held here, because arming is
    /// session state with a lifetime of exactly one keystroke — see
    /// [`crate::app::App::key`], which takes it before the lookup so that every
    /// path clears it without having to remember to.
    ///
    /// A chord bound to two actions answers with the first in [`Action::ALL`].
    // ponytail: first wins on a collision, and the session says nothing about
    // it. A conflict detector would be a second thing to keep true; if operators
    // hit this in the wild, refuse the later binding in `resolve` with a notice.
    pub fn hit(&self, chord: Chord, armed: Option<Action>) -> Option<Hit> {
        if let Some(action) = armed {
            if self.binding(action).second == Some(chord) {
                return Some(Hit::Fire(action));
            }
        }
        let action = Action::ALL
            .iter()
            .copied()
            .find(|action| self.binding(*action).first == chord)?;
        if self.binding(action).second.is_some() {
            Some(Hit::Arm(action))
        } else {
            Some(Hit::Fire(action))
        }
    }
}

/// The one refusal, worded once so both spellings of the mistake get the reason
/// rather than only the verdict.
fn refusal(what: &str) -> String {
    format!(
        "Ctrl+C is not rebindable, so {what}: it is the key that interrupts a running turn \
         and leaves io, and a configuration file able to take it away is one able to lock \
         you inside a running agent"
    )
}
