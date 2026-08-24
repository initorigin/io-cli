//! F9 — a rebound key takes effect and the table says so.
//! F10 — a malformed `[app.io-cli]` is disclosed and does not take the session
//! down.
//!
//! Two halves that fail independently on purpose. The rebinding half is easy to
//! get half right: apply the file to the key handler, render the table from the
//! constants, and every manual test passes while `/help` quietly lies to
//! everybody who moved a key. So the handler and the table are asserted
//! separately, against the same `Keys`, and the second is the one the release's
//! named sabotage attacks.
//!
//! The disclosure half is the opposite shape: its defect is that *nothing*
//! happens. `.unwrap_or_default()` on `Config::app`'s `Result` turned an
//! unreadable section into an absent one, which is a session running on defaults
//! with no way to find out why — so what is asserted here is the presence of a
//! sentence, and the sentence carrying io-harness's own words rather than a
//! paraphrase that would drop the key's name.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::app::{App, Command};
use io_cli::commands::{self, KEYS};
use io_cli::keys::{Action, Binding, Chord, Hit, Keys, Newline};
use io_cli::settings::{self, Posture};
use io_cli::theme::DARK;
use io_harness::Config;

fn app(keys: Keys) -> App {
    let mut app = App::new(DARK, "opus-5");
    app.set_keys(keys);
    app
}

fn asked(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

fn ctrl(character: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
}

fn plain(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **F9.** A key named in the file drives the action, and the key it replaced
/// stops driving it.
///
/// Both directions, because only the first is what a person notices. A
/// rebinding that added a key without moving one would leave the old chord live
/// on a terminal where it was the reason for rebinding in the first place —
/// `Ctrl+L` is precisely the key a multiplexer eats — and the operator would
/// have no way to tell the difference from the outside.
#[test]
fn f9_a_rebound_key_drives_the_action_it_names() {
    let (keys, notices) = Keys::resolve(Some(&asked(&[("clear", "ctrl+k")])));
    assert!(
        notices.is_empty(),
        "a readable binding earns no notice: {notices:?}"
    );

    let mut session = app(keys.clone());
    assert_eq!(
        session.key(ctrl('k')),
        Command::ClearViewport,
        "the key the file names has to be the key that clears",
    );
    assert_eq!(
        session.key(ctrl('l')),
        Command::None,
        "the default is not a second binding kept alongside the new one; on a \
         terminal that eats Ctrl+L, leaving it live is the whole reason the \
         rebinding was needed",
    );

    // The other four, one line each, so a table-driven mistake in `Keys::hit`
    // cannot pass on the strength of `clear` alone.
    let (keys, _) = Keys::resolve(Some(&asked(&[
        ("exit", "f10"),
        ("transcript", "alt+t"),
        ("posture", "f9"),
    ])));
    let mut session = app(keys);
    assert_eq!(session.key(plain(KeyCode::F(9))), Command::None);
    assert_eq!(
        session.posture(),
        Some(Posture::Workspace),
        "F9 was bound to the posture key, and a posture key that does not move \
         the posture is a key that only looks bound",
    );
    assert_eq!(
        session.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT)),
        Command::Transcript,
    );
    assert_eq!(session.key(plain(KeyCode::F(10))), Command::Exit);
}

/// **F9, and the half the named sabotage removes.** The table renders the
/// bindings in force.
///
/// Applying the file to the handler and rendering the table from the constants
/// passes every test that presses a key. It ships a help screen that is
/// confidently wrong about the machine in front of the reader, which is worse
/// than no rebinding at all: the operator's only way to learn the real key is to
/// press things until something happens.
#[test]
fn f9_the_table_shows_the_binding_in_force() {
    let (keys, _) = Keys::resolve(Some(&asked(&[("clear", "ctrl+k")])));

    // The advertised naming, so this test stays about rebinding: which key the
    // newline row names is a terminal's answer and `tests/keyboard.rs` owns it.
    let printed = text(&commands::help(&keys, &DARK, Newline::of(true)));
    assert!(
        printed.contains("Ctrl+K"),
        "/help must show the key this session actually clears with: {printed:?}",
    );
    assert!(
        !printed.contains("Ctrl+L"),
        "/help is still showing the shipped default beside a description of what \
         a different key now does: {printed:?}",
    );
    assert!(
        printed.contains("clear the viewport, never the scrollback"),
        "the description belongs to the action, and travels with it",
    );

    // The rows a surface that is not `/help` would render, asserted at the
    // source so a second consumer cannot be given the defaults by accident.
    let rows = commands::rows(&keys, Newline::of(true));
    assert_eq!(
        rows.len(),
        KEYS.len(),
        "rebinding changes what a row says, never how many rows there are",
    );
    assert!(rows.iter().any(|(key, _)| key == "Ctrl+K"), "{rows:?}",);
    // A row this release cannot move is a row it must not touch.
    assert!(
        rows.iter().any(|(key, _)| key == "Shift+Enter"),
        "the composer's keys still belong in the table: {rows:?}",
    );
}

/// **F9's exception.** `Ctrl+C` is refused, out loud, in both spellings of the
/// mistake.
///
/// Naming `interrupt` is the obvious one. Putting some *other* action on
/// `ctrl+c` is the one that matters more: it takes the interrupt away just as
/// completely, and it is what a file written by somebody following another
/// product's conventions would do.
#[test]
fn f9_ctrl_c_is_refused_as_rebindable() {
    for asking in [
        asked(&[("interrupt", "ctrl+x")]),
        asked(&[("clear", "ctrl+c")]),
    ] {
        let (keys, notices) = Keys::resolve(Some(&asking));
        let said = notices.join("\n");
        assert!(
            said.contains("Ctrl+C is not rebindable"),
            "the refusal has to be visible; a silently ignored line looks like a \
             line that worked: {said:?}",
        );
        assert!(
            said.contains("lock you inside a running agent"),
            "a refusal that gives no reason is one the operator will read as a \
             bug and work around: {said:?}",
        );

        let mut session = app(keys);
        assert_eq!(
            session.key(ctrl('c')),
            Command::None,
            "Ctrl+C at an idle empty prompt still asks for a second press",
        );
        assert_eq!(
            session.key(ctrl('c')),
            Command::Exit,
            "and twice still leaves; the file did not take the interrupt away",
        );
    }

    // The chord the refused file asked for does nothing of its own, because a
    // refusal that quietly bound the key anyway would be the worst of both.
    let (keys, _) = Keys::resolve(Some(&asked(&[("clear", "ctrl+c")])));
    assert_eq!(
        keys.binding(Action::Clear),
        Keys::default().binding(Action::Clear),
        "the refused action stays where it was",
    );
}

/// **F9.** The generated table says which key cannot be moved.
///
/// The reader consulting the table is exactly the reader about to try rebinding
/// something, and a table showing one immovable key beside five movable ones
/// without saying which is which invites the attempt.
#[test]
fn f9_the_table_marks_the_fixed_key() {
    let rows = commands::rows(&Keys::default(), Newline::of(true));
    let (_, what) = rows
        .iter()
        .find(|(key, _)| key == "Ctrl+C")
        .expect("Ctrl+C is in the table");
    assert!(what.contains("(fixed)"), "{what:?}");

    for movable in ["Ctrl+D", "Ctrl+L", "Ctrl+T", "Shift+Tab", "Esc Esc"] {
        let (_, what) = rows
            .iter()
            .find(|(key, _)| key == movable)
            .unwrap_or_else(|| panic!("{movable} is in the table"));
        assert!(
            !what.contains("(fixed)"),
            "{movable} is rebindable and must not be marked as though it were not",
        );
    }
}

/// **F10.** An unreadable `[app.io-cli]` is disclosed, carrying the harness's own
/// message, and the session starts anyway.
///
/// The sabotage is to keep `.unwrap_or_default()`, which is what shipped: the
/// section becomes `None`, the theme, the diff style, the glyph set, plain mode
/// and every keybinding revert together, and nothing is said about any of it.
/// This test is the only thing that fails under it.
#[test]
fn f10_a_malformed_section_is_disclosed() {
    let config = Config::from_toml("[app.io-cli]\ntheme = \"dark\"\nplain = \"yes\"\n")
        .expect("the file is valid TOML; it is the section that cannot be read");

    let (stored, complaint) = settings::stored(&config);
    assert!(
        stored.is_none(),
        "a section that cannot be read must not be half-applied",
    );
    let complaint = complaint.expect(
        "an unreadable section has to say so; silence here is a session on defaults \
         with no thread to pull",
    );
    assert!(
        complaint.contains("[app.io-cli]"),
        "the notice has to name the section: {complaint:?}",
    );
    // Verbatim, rather than an assertion about wording io-harness is free to
    // change: what matters is that the harness's own sentence survives into the
    // notice, because it is the half that says which key broke and rewording it
    // here would drop the only part that says where to look.
    let harness = config
        .app::<io_cli::settings::CliSettings>(settings::APP_KEY)
        .expect_err("the section cannot be read");
    assert!(
        complaint.contains(&harness.to_string()),
        "the notice dropped the harness's own message {harness}: {complaint:?}",
    );
    assert!(
        complaint.contains("default"),
        "and it has to say what the session is running on instead: {complaint:?}",
    );

    // The session still starts, on the defaults, with the keys still working.
    let (keys, _) = Keys::resolve(None);
    assert_eq!(keys, Keys::default());
    let mut session = app(keys);
    assert_eq!(session.key(ctrl('l')), Command::ClearViewport);
}

/// **F10, the other half.** One unreadable binding costs one key, says which,
/// and leaves the rest of the file standing.
#[test]
fn f10_an_unreadable_binding_falls_back_and_says_which() {
    let (keys, notices) = Keys::resolve(Some(&asked(&[
        ("clear", "ctrl+"),
        ("transcript", "alt+t"),
        ("wobble", "ctrl+w"),
    ])));

    let said = notices.join("\n");
    assert!(
        said.contains("clear"),
        "the notice has to name the binding that was dropped: {said:?}",
    );
    assert!(
        said.contains("Ctrl+L"),
        "and the key it fell back to, or the operator is told something is wrong \
         without being told what is now true: {said:?}",
    );
    assert!(
        said.contains("wobble") && said.contains("transcript"),
        "a name that is no action says which names there are: {said:?}",
    );

    let mut session = app(keys);
    assert_eq!(
        session.key(ctrl('l')),
        Command::ClearViewport,
        "the action falls back to its default rather than becoming unreachable",
    );
    assert_eq!(
        session.key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::ALT)),
        Command::Transcript,
        "one bad line does not cost the good ones",
    );
}

/// The defaults still behave exactly as they did before any of this existed.
///
/// Including both spellings of `Shift+Tab`. A terminal without the Kitty
/// keyboard protocol sends `BackTab` with no modifier and one that has
/// negotiated it sends `Tab` with shift; the two used to be two arms of a
/// `match` and are now one binding plus a normalization, which is exactly the
/// kind of change that ships working on the developer's terminal and dead on
/// somebody else's.
#[test]
fn the_defaults_are_unchanged() {
    let mut session = app(Keys::default());
    assert_eq!(session.key(plain(KeyCode::BackTab)), Command::None);
    assert_eq!(session.posture(), Some(Posture::Workspace));
    assert_eq!(
        session.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)),
        Command::None,
    );
    assert_eq!(
        session.posture(),
        Some(Posture::AskWrites),
        "the other spelling of the same key moved it again",
    );

    assert_eq!(session.key(ctrl('l')), Command::ClearViewport);
    assert_eq!(session.key(ctrl('t')), Command::Transcript);
    assert_eq!(session.key(ctrl('d')), Command::Exit);

    // The sequence, and the arming that any other key clears.
    assert_eq!(session.key(plain(KeyCode::Esc)), Command::ArmRewind);
    assert!(session.armed());
    assert_eq!(session.key(ctrl('l')), Command::ClearViewport);
    assert!(!session.armed(), "an unrelated key disarms it");
    assert_eq!(session.key(plain(KeyCode::Esc)), Command::ArmRewind);
    assert_eq!(session.key(plain(KeyCode::Esc)), Command::Rewind);
}

/// A sequence is rebindable as a sequence, and it still asks twice.
///
/// The rewind is the one key in the product that changes the operator's files on
/// io-cli's own initiative, and the second press is the whole of its consent. A
/// rebinding that collapsed it to one chord would be a rebinding that removed a
/// confirmation, which is not a preference.
#[test]
fn a_rebound_sequence_still_asks_twice() {
    let (keys, notices) = Keys::resolve(Some(&asked(&[("rewind", "ctrl+r ctrl+r")])));
    assert!(notices.is_empty(), "{notices:?}");

    let mut session = app(keys);
    assert_eq!(session.key(plain(KeyCode::Esc)), Command::None, "Esc moved");
    assert_eq!(session.key(ctrl('r')), Command::ArmRewind);
    assert!(session.armed());
    assert_eq!(session.key(ctrl('r')), Command::Rewind);
    assert!(!session.armed());
}

/// The syntax, exercised where it is read rather than through a session.
///
/// Every spelling here is public contract from this release on: what parses now
/// has to keep parsing, and what a chord renders as is what the table shows.
#[test]
fn the_binding_syntax_is_what_it_says_it_is() {
    for (written, shown) in [
        ("ctrl+l", "Ctrl+L"),
        ("CTRL+L", "Ctrl+L"),
        ("Ctrl+Shift+K", "Ctrl+Shift+K"),
        ("shift+tab", "Shift+Tab"),
        ("backtab", "Shift+Tab"),
        ("esc", "Esc"),
        ("f12", "F12"),
        ("space", "Space"),
        ("pagedown", "PageDown"),
        ("esc esc", "Esc Esc"),
        ("alt+x ctrl+y", "Alt+X Ctrl+Y"),
    ] {
        let binding = Binding::parse(written)
            .unwrap_or_else(|| panic!("`{written}` is documented as readable"));
        assert_eq!(binding.to_string(), shown, "`{written}`");
    }

    for refused in ["", "ctrl+", "ctrl", "l+k", "f13", "nonsense", "esc esc esc"] {
        assert!(
            Binding::parse(refused).is_none(),
            "`{refused}` parsed, and a binding that reads as something other than \
             what was written is worse than one that is refused",
        );
    }

    // `backtab` and `shift+tab` are one chord, which is what lets a single
    // binding work on a terminal with the Kitty protocol and one without.
    assert_eq!(
        Chord::of(plain(KeyCode::BackTab)),
        Chord::of(KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT)),
    );
    // A modifier this product cannot name in a binding is one it must not
    // distinguish on either, or a terminal reporting more bits than another
    // stops matching the same file.
    assert_eq!(
        Chord::of(KeyEvent::new(
            KeyCode::Char('l'),
            KeyModifiers::CONTROL | KeyModifiers::SUPER,
        )),
        Chord::of(ctrl('l')),
    );
}

/// The defaults are the table, and the table is the defaults.
///
/// `commands::rows` joins an [`Action`] to a `KEYS` row on the rendered default
/// binding. That join is the one place this design can rot silently: a default
/// changed in `keys.rs` and not in `commands.rs` would drop a row out of the
/// rebindable set, and every other test here would still pass because they all
/// go through `Keys`.
#[test]
fn every_default_binding_is_a_row_of_the_table() {
    let defaults = Keys::default();
    for action in Action::ALL {
        let binding = Binding::parse(action.default_binding())
            .unwrap_or_else(|| panic!("{}'s default has to parse", action.name()));
        assert_eq!(binding, defaults.binding(*action));
        let shown = binding.to_string();
        assert!(
            KEYS.iter().any(|(key, _)| *key == shown),
            "{} defaults to {shown}, which is in no row of the documented table",
            action.name(),
        );
    }

    // `Action as usize` is the index `Keys` stores by. Nothing enforces that an
    // enum's order matches a constant's, so it is asserted rather than assumed:
    // getting it wrong would bind every action to its neighbour's key.
    for (index, action) in Action::ALL.iter().enumerate() {
        assert_eq!(index, *action as usize, "{}", action.name());
    }
}

/// A chord bound to nothing is nothing, and a half-pressed sequence is not a
/// press.
#[test]
fn an_unbound_chord_reaches_the_prompt() {
    let keys = Keys::default();
    assert_eq!(keys.hit(Chord::of(ctrl('q')), None), None);
    assert_eq!(
        keys.hit(Chord::of(plain(KeyCode::Esc)), None),
        Some(Hit::Arm(Action::Rewind)),
        "the first chord of a sequence arms; it does not fire",
    );
    assert_eq!(
        keys.hit(Chord::of(plain(KeyCode::Esc)), Some(Action::Rewind)),
        Some(Hit::Fire(Action::Rewind)),
    );
}
