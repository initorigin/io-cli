//! N6 — `NO_COLOR` produces a usable session.
//! F3 — `NO_COLOR` survives the first run.
//!
//! Usable is the word that matters. A session that runs but reports a refusal as
//! a line that was going to be yellow, and now is not, has lost the information —
//! so the assertion is that every state colour distinguishes also carries a word.

mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_cli::glyphs::UNICODE;
use io_cli::settings::{self, CliSettings};
use io_cli::splash;
use io_cli::theme::{Background, Theme, Tone, DARK, LIGHT, MONO, THEMES};
use io_cli::wizard::{Progress, Step, Wizard};
use io_harness::Config;

/// The tones that mean something, as opposed to the ones that are presentation.
const MEANINGFUL: &[Tone] = &[Tone::Success, Tone::Warning, Tone::Error, Tone::Refused];

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Assert that no foreground colour reached the terminal.
///
/// Every form one can take is checked, because the two shipped themes between
/// them use all three: `ESC [ 3 8 ; 5 ;` and `ESC [ 3 8 ; 2 ;` are the indexed
/// and true-colour foregrounds, and the 30-37 and 90-97 ranges are the sixteen
/// ANSI ones, which crossterm writes as their own SGR parameter. Checking only
/// the indexed form would have passed against a session running the dark theme,
/// whose every token is one of the sixteen.
///
/// 39 is deliberately absent: that is the sequence that turns colour *off*, and
/// an uncoloured stream is full of it.
fn assert_uncoloured(text: &str, what: &str) {
    let mut forbidden: Vec<String> = (30..=37)
        .chain(90..=97)
        .map(|code: u8| format!("\x1b[{code}m"))
        .collect();
    forbidden.push("\x1b[38;5;".to_string());
    forbidden.push("\x1b[38;2;".to_string());
    for sequence in forbidden {
        assert!(
            !text.contains(&sequence),
            "{what} carries {} under NO_COLOR",
            sequence.escape_debug(),
        );
    }
}

#[test]
fn n6_every_meaningful_tone_carries_a_word() {
    for tone in MEANINGFUL {
        assert!(
            tone.word().is_some(),
            "{tone:?} distinguishes a state by colour alone",
        );
    }
    for tone in [Tone::Normal, Tone::Muted, Tone::Accent] {
        assert!(
            tone.word().is_none(),
            "{tone:?} is presentation and should not prefix every line with a word",
        );
    }
}

#[test]
fn n6_the_word_survives_no_color() {
    for tone in MEANINGFUL {
        let word = tone.word().expect("a meaningful tone has a word");
        for theme in [MONO, DARK, LIGHT] {
            let line = theme.notice(*tone, "write to /etc/hosts");
            let rendered: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            assert!(
                rendered.contains(word),
                "the {} theme rendered {tone:?} as {rendered:?}, without the word {word:?}",
                theme.name,
            );
            assert!(
                rendered.contains("write to /etc/hosts"),
                "the {} theme lost the text of the notice",
                theme.name,
            );
        }
    }
}

#[test]
fn n6_a_scripted_session_runs_with_no_color_and_writes_no_colour() {
    let theme = Theme::resolve(true, Background::Dark, Some("dark"), UNICODE);
    assert_eq!(theme.name, "mono", "NO_COLOR must win over a chosen theme");

    let (mut screen, recorder) = support::screen(100, 30);
    screen
        .commit(&splash::lines(&theme, true, 100))
        .expect("splash");
    for tone in MEANINGFUL {
        screen
            .commit(&[theme.notice(*tone, "something happened")])
            .expect("commit");
    }
    screen.draw(|_| {}).expect("frame");

    let text = recorder.text();
    for tone in MEANINGFUL {
        let word = tone.word().expect("a meaningful tone has a word");
        assert!(text.contains(word), "the session never wrote {word:?}");
    }

    assert_uncoloured(&text, "the session's byte stream");
}

/// F3 — `NO_COLOR` survives the first run.
///
/// The test above is the configured case: a theme is stored, the variable beats
/// it, and it beats it inside `Theme::resolve`. This is the other half, and the
/// half that was broken. A first run has no stored theme at all — it has a
/// wizard, whose theme step used to hand its picker's row back as the session's
/// theme by assignment, so a user with the variable set picked "dark" and got a
/// coloured session out of it. Driven through the real screens rather than
/// through `Theme::resolve`, because the assignment was the defect and only a
/// driven wizard reaches it.
#[test]
fn f3_no_color_survives_the_first_run() {
    // Process-wide, and no other test in this file reads any of the three, so
    // the lock the wizard suite needs would be guarding nothing here.
    std::env::set_var("NO_COLOR", "1");
    // Pinned so the theme the picker opens on is the same on every machine; a
    // developer whose terminal exports a light background would otherwise start
    // the picker one row further down.
    std::env::set_var("COLORFGBG", "15;0");
    let home = tempfile::tempdir().expect("a temporary directory");
    std::env::set_var("IO_CONFIG_HOME", home.path());
    // `IO_CONFIG` names a file outright and would win over the directory.
    std::env::remove_var("IO_CONFIG");

    // What the binary does before the wizard is reached, verbatim.
    let theme = Theme::from_env(None, UNICODE);
    assert_eq!(theme.name, "mono", "no file, and the variable is set");

    let (mut screen, recorder) = support::screen_of(100, 40, 20);
    let mut wizard = Wizard::new(theme);
    macro_rules! draw {
        () => {
            screen
                .draw(|frame| wizard.render(frame, frame.area()))
                .expect("frame")
        };
    }

    draw!();
    wizard.key(key(KeyCode::Enter)); // Welcome.
    draw!();
    wizard.key(key(KeyCode::Enter)); // Provider: the first row, OpenRouter.
    for character in "sk-or-v1-not-a-key".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    draw!();
    assert!(matches!(
        wizard.key(key(KeyCode::Enter)),
        Progress::Verify(_)
    ));
    // Verification and the catalogue are the driver's calls; here they succeed,
    // because F3 is about colour and not about the network.
    wizard.verified();
    wizard.catalogue(vec![
        "anthropic/claude-sonnet-4".into(),
        "openai/gpt-5".into(),
    ]);
    draw!();
    wizard.key(key(KeyCode::Enter)); // Model.

    assert_eq!(wizard.step(), Step::Theme);
    draw!();
    // Read out of the viewport rather than out of the byte stream: a rendered
    // line reaches the stream in however many pieces the frame diff decides on,
    // and the question here is what is on the screen.
    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("NO_COLOR is set"),
        "the theme step offered a choice without saying what outranks it: {viewport:?}",
    );
    wizard.key(key(KeyCode::Down));
    draw!();
    assert_eq!(
        wizard.theme().name,
        "mono",
        "the picker moved onto a coloured theme and the wizard took it",
    );
    wizard.key(key(KeyCode::Enter)); // Theme.
    draw!();
    wizard.key(key(KeyCode::Enter)); // Posture: the first row.
    draw!();
    assert_eq!(wizard.step(), Step::Confirm);

    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Write(_, contents) = progress else {
        panic!("the confirmation screen should produce a write, got {progress:?}");
    };

    // What is written down is a preference for the run that does not have the
    // variable set — so it has to be a name a later launch can actually resolve.
    // "mono" is not one: `Theme::by_name` searches the selectable themes and
    // `MONO` is not among them, so persisting it is persisting nothing.
    // Read the way it will actually be read, and NOT with `Config::from_toml`,
    // which parses text as the PROJECT scope. io-harness refuses a project-scoped
    // file that widens the boundary — the default "sandboxed workspace" posture
    // this wizard run chose writes `exec = "allow"`, which a committed `io.toml`
    // may not say, because a repository you cloned must not be able to grant
    // itself permission. The wizard writes the USER scope, where the widening is
    // the operator's own decision. `tests/wizard.rs` paid for this once already;
    // this is the same trap reached from the other side.
    let workspace = home.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    settings::write(&home.path().join("io.toml"), &contents).expect("write");
    let config = Config::discover(&workspace).expect("io-harness reads its own user-scope file");
    let app: Option<CliSettings> = config
        .app(settings::APP_KEY)
        .expect("the app section parses");
    let stored = app
        .and_then(|settings| settings.theme)
        .expect("a theme was written");
    assert_eq!(stored, "light", "the row the picker was left on");
    assert!(
        Theme::by_name(&stored).is_some(),
        "{stored:?} was persisted, and no later launch can resolve it",
    );

    // The session the driver starts next, on the theme the driver carries into
    // it — this expression is `src/main.rs`'s, not a paraphrase of it.
    let session_theme = Theme::from_env(Some(wizard.theme().name), wizard.theme().glyphs);
    assert_eq!(
        session_theme.name, "mono",
        "the wizard's answer must go back through resolution, not around it",
    );
    let (mut session, bytes) = support::screen(100, 30);
    session
        .commit(&splash::lines(&session_theme, true, 100))
        .expect("splash");
    for tone in MEANINGFUL {
        session
            .commit(&[session_theme.notice(*tone, "something happened")])
            .expect("commit");
    }
    session.draw(|_| {}).expect("frame");

    assert_uncoloured(&bytes.text(), "the session after the wizard");
    assert_uncoloured(&recorder.text(), "the wizard itself");
}

#[test]
fn the_splash_is_suppressed_without_colour_a_tty_or_the_width_for_it() {
    assert!(
        splash::visible(true, true, 100),
        "the ordinary case shows it"
    );
    assert!(!splash::visible(false, true, 100), "NO_COLOR suppresses it");
    assert!(
        !splash::visible(true, false, 100),
        "a non-tty suppresses it"
    );
    assert!(
        !splash::visible(true, true, 40),
        "a terminal narrower than the mark suppresses it",
    );

    // Suppressed does not mean silent: the version line is still committed, so a
    // session always says what it is.
    let lines = splash::lines(&MONO, false, 40);
    let rendered: String = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();
    // Read from the manifest rather than written out, so the assertion is that
    // the splash names THIS build; a literal here goes stale at every bump and
    // says nothing about the version actually shipping.
    let version = format!("io {}", env!("CARGO_PKG_VERSION"));
    assert!(rendered.contains(&version), "got {rendered:?}");
    assert!(!rendered.contains('█'), "the mark was drawn anyway");
}

#[test]
fn the_two_shipped_themes_are_the_two_shipped_themes() {
    let names: Vec<_> = THEMES.iter().map(|theme| theme.name).collect();
    assert_eq!(names, ["dark", "light"], "restraint is the design");
    assert!(
        Theme::by_name("mono").is_none(),
        "mono is what NO_COLOR forces, not a theme a user picks",
    );
    // The old name is asserted too: 0.6.0 gives `--plain` an unrelated meaning,
    // and a configuration written by an earlier version must not start
    // resolving to something now that it never resolved to before.
    assert!(
        Theme::by_name("plain").is_none(),
        "the theme's former name must not become selectable either",
    );
}

#[test]
fn the_background_is_detected_from_colorfgbg() {
    // A `foreground;background` pair. The last field decides.
    assert_eq!(Background::from_colorfgbg("15;0"), Background::Dark);
    assert_eq!(Background::from_colorfgbg("0;15"), Background::Light);
    assert_eq!(Background::from_colorfgbg("15;default;0"), Background::Dark);
    assert_eq!(Background::from_colorfgbg("7"), Background::Light);
    // Anything unparseable is dark, because the light palette on a dark terminal
    // is the less readable of the two mistakes.
    assert_eq!(Background::from_colorfgbg(""), Background::Dark);
    assert_eq!(Background::from_colorfgbg("nonsense"), Background::Dark);
}
