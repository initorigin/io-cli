//! N6 — `NO_COLOR` produces a usable session.
//!
//! Usable is the word that matters. A session that runs but reports a refusal as
//! a line that was going to be yellow, and now is not, has lost the information —
//! so the assertion is that every state colour distinguishes also carries a word.

mod support;

use io_cli::splash;
use io_cli::theme::{Background, Theme, Tone, DARK, LIGHT, PLAIN, THEMES};

/// The tones that mean something, as opposed to the ones that are presentation.
const MEANINGFUL: &[Tone] = &[Tone::Success, Tone::Warning, Tone::Error, Tone::Refused];

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
        for theme in [PLAIN, DARK, LIGHT] {
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
    let theme = Theme::resolve(true, Background::Dark, Some("dark"));
    assert_eq!(theme.name, "plain", "NO_COLOR must win over a chosen theme");

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

    // `ESC [ 3 8 ; 5 ;` and `ESC [ 3 8 ; 2 ;` are indexed and true-colour
    // foregrounds; the 30-37 and 90-97 ranges are the ANSI ones. Under NO_COLOR
    // none of them should be written at all.
    for sequence in ["\x1b[38;5;", "\x1b[38;2;"] {
        assert!(
            !text.contains(sequence),
            "the byte stream carries {} under NO_COLOR",
            sequence.escape_debug(),
        );
    }
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
    let lines = splash::lines(&PLAIN, false, 40);
    let rendered: String = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();
    assert!(rendered.contains("io 0.1.0"), "got {rendered:?}");
    assert!(!rendered.contains('█'), "the mark was drawn anyway");
}

#[test]
fn the_two_shipped_themes_are_the_two_shipped_themes() {
    let names: Vec<_> = THEMES.iter().map(|theme| theme.name).collect();
    assert_eq!(names, ["dark", "light"], "restraint is the design");
    assert!(
        Theme::by_name("plain").is_none(),
        "plain is what NO_COLOR forces, not a theme a user picks",
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
