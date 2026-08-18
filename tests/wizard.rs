//! F2 — the wizard writes what it showed.
//! F3 — a bad key does not get past the wizard.
//! N5 — the credential never reaches the screen, the scrollback, a log line or
//!      the trace.
//!
//! The flow is driven with a scripted key sequence over a real `CrosstermBackend`
//! writing into a recorder, so the byte stream N5 asserts over is the one the
//! process would have written to a terminal.

mod support;

use std::sync::{Mutex, MutexGuard, OnceLock};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use io_cli::settings::{self, CliSettings, Posture};
use io_cli::theme::DARK;
use io_cli::wizard::{Progress, Step, Wizard};
use io_harness::{Config, Effect, ProviderSpec};

/// A value that would be catastrophic to render, distinctive enough that a
/// substring search over the byte stream cannot match it by accident.
const TEST_KEY: &str = "sk-or-v1-NEVERRENDERTHIS0123456789";

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Held by every test that reads or writes an `IO_CONFIG*` variable.
///
/// The environment is process-wide and these tests share a process, so two of
/// them setting `IO_CONFIG` at once would make each other's `user_path` answer
/// wrong — intermittently, on a loaded machine, which is the most expensive kind
/// of failure to diagnose. Serialised rather than discovered later.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One config home for this whole file, set once. `settings::user_path` reads the
/// environment at call time, and two tests setting it at once would race.
fn config_home() -> &'static std::path::Path {
    static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::env::set_var("IO_CONFIG_HOME", dir.path());
        // `IO_CONFIG` names a file outright and would win over the directory, so
        // an inherited one has to go or the test writes somewhere real.
        std::env::remove_var("IO_CONFIG");
        // Same for a key the developer running the suite happens to have: the
        // wizard offers "press Enter to use $OPENROUTER_API_KEY" when one is set,
        // and the scripted sequence below types a key instead.
        std::env::remove_var("OPENROUTER_API_KEY");
        dir
    })
    .path()
}

#[test]
fn f2_n5_the_wizard_writes_what_it_showed_and_never_shows_the_key() {
    let _guard = env_lock();
    let home = config_home();
    let expected = home.join("io.toml");
    assert!(!expected.exists(), "the test starts from no configuration");

    let (mut screen, recorder) = support::screen_of(100, 40, 20);
    let mut wizard = Wizard::new(DARK);

    macro_rules! draw {
        () => {
            screen
                .draw(|frame| wizard.render(frame, frame.area()))
                .expect("frame")
        };
    }

    // 1. Welcome.
    draw!();
    assert_eq!(wizard.step(), Step::Welcome);
    assert!(matches!(
        wizard.key(key(KeyCode::Enter)),
        Progress::Commit(_)
    ));

    // 2. Provider — the first row is OpenRouter.
    draw!();
    assert_eq!(wizard.step(), Step::Provider);
    assert_eq!(wizard.key(key(KeyCode::Enter)), Progress::Idle);

    // 3. Credential, typed one character at a time with a frame after each, which
    //    is what would happen at a real keyboard.
    assert_eq!(wizard.step(), Step::Credential);
    for character in TEST_KEY.chars() {
        wizard.key(key(KeyCode::Char(character)));
        draw!();
    }
    assert!(
        !expected.exists(),
        "a file appeared before the confirmation screen",
    );

    // 4. Verification is the driver's job; here it succeeds.
    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Verify(spec) = progress else {
        panic!("submitting a credential should ask for verification, got {progress:?}");
    };
    assert!(
        matches!(&spec, ProviderSpec::OpenRouter { api_key: Some(k), .. } if k == TEST_KEY),
        "the spec handed to verification should carry the typed key",
    );
    draw!();
    assert!(matches!(wizard.verified(), Progress::Catalogue(_)));

    // 5. Model. The catalogue arrives from the driver; the second row is chosen.
    wizard.catalogue(vec![
        "anthropic/claude-sonnet-4".into(),
        "openai/gpt-5".into(),
        "google/gemini-3-pro".into(),
    ]);
    draw!();
    assert_eq!(wizard.step(), Step::Model);
    wizard.key(key(KeyCode::Down));
    draw!();
    wizard.key(key(KeyCode::Enter));

    // 6. Theme, with the sample re-rendering behind the picker.
    assert_eq!(wizard.step(), Step::Theme);
    draw!();
    assert_eq!(wizard.theme().name, "dark");
    wizard.key(key(KeyCode::Down));
    draw!();
    assert_eq!(
        wizard.theme().name,
        "light",
        "the preview should follow the selection, not wait for the choice",
    );
    wizard.key(key(KeyCode::Enter));

    // 7. Posture — the second row asks before writes.
    assert_eq!(wizard.step(), Step::Posture);
    draw!();
    wizard.key(key(KeyCode::Down));
    draw!();
    wizard.key(key(KeyCode::Enter));

    // 8. Confirm. Still nothing on disk.
    assert_eq!(wizard.step(), Step::Confirm);
    draw!();
    assert!(
        !expected.exists(),
        "the confirmation screen is shown before anything is written, not after",
    );

    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Write(path, contents) = progress else {
        panic!("the confirmation screen should produce a write, got {progress:?}");
    };
    assert_eq!(path, expected, "the path shown is the path written");
    settings::write(&path, &contents).expect("the file is written");

    // --- F2: the file says what the wizard showed. ---
    let config = Config::from_toml(&contents).expect("io-harness parses its own file");
    let spec = config.provider_spec().expect("a provider was written");
    assert!(
        matches!(
            spec,
            ProviderSpec::OpenRouter { model, api_key: Some(k) }
                if model == "openai/gpt-5" && k == TEST_KEY
        ),
        "got {spec:?}",
    );

    let policy = config.policy().expect("a policy was written");
    assert_eq!(policy.defaults.write, Effect::Ask, "the chosen posture");
    assert_eq!(policy.defaults.net, Effect::Deny);

    let app: Option<CliSettings> = config.app("io-cli").expect("the app section parses");
    assert_eq!(
        app.and_then(|settings| settings.theme).as_deref(),
        Some("light"),
        "the chosen theme",
    );

    // --- F2: mode 0600 on unix. ---
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "the file is readable only by its owner");
    }

    // --- N5: the key never reached the terminal. ---
    let written = recorder.text();
    assert!(
        !written.contains(TEST_KEY),
        "the credential appears in the byte stream the terminal received",
    );
    for length in [12usize, 20, 28] {
        let fragment = &TEST_KEY[..length];
        assert!(
            !written.contains(fragment),
            "a {length}-character prefix of the credential reached the terminal",
        );
    }
    assert!(
        written.contains('•'),
        "the credential field should have been masked, not simply absent",
    );

    // ...and the confirmation screen described the credential without showing it.
    let summary: String = wizard
        .summary()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();
    assert!(!summary.contains(TEST_KEY), "got {summary:?}");
    assert!(summary.contains("0600"), "got {summary:?}");
    assert!(summary.contains("openai/gpt-5"), "got {summary:?}");
}

/// Walk to the credential screen, which is where a key gets pasted.
fn at_the_credential_step() -> Wizard {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Credential);
    wizard
}

#[test]
fn a_pasted_key_lands_in_the_field_and_does_not_close_the_wizard() {
    // The defect this exists for: the driver matched `Event::Key` and treated
    // everything else as "the user left", so Cmd+V — which arrives as
    // `Event::Paste` when bracketed paste is on — silently ended the wizard on
    // the one screen where pasting is the expected thing to do. Three runs in a
    // row died this way before anyone typed a character.
    let mut wizard = at_the_credential_step();

    let progress = wizard.event(&Event::Paste(TEST_KEY.to_string()));
    assert_eq!(progress, Progress::Idle, "a paste is not a decision");
    assert_eq!(
        wizard.step(),
        Step::Credential,
        "the paste closed the wizard",
    );

    // ...and it actually went into the field, which the old code would not have
    // done even once it stopped exiting.
    let progress = wizard.event(&Event::Key(key(KeyCode::Enter)));
    let Progress::Verify(spec) = progress else {
        panic!("the pasted key should be submitted for verification, got {progress:?}");
    };
    assert!(
        matches!(&spec, ProviderSpec::OpenRouter { api_key: Some(k), .. } if k == TEST_KEY),
        "the pasted key did not reach the provider spec",
    );
}

#[test]
fn a_paste_with_a_trailing_newline_does_not_submit_the_field() {
    // A key copied out of a web page usually carries one. Treating it as Enter
    // would submit a field the user was still filling in.
    let mut wizard = at_the_credential_step();
    wizard.event(&Event::Paste(format!("{TEST_KEY}\n")));
    assert_eq!(wizard.step(), Step::Credential);

    let Progress::Verify(spec) = wizard.event(&Event::Key(key(KeyCode::Enter))) else {
        panic!("the field should still have been submittable by hand");
    };
    assert!(
        matches!(&spec, ProviderSpec::OpenRouter { api_key: Some(k), .. } if k == TEST_KEY),
        "the newline should have been stripped, not carried into the key",
    );
}

#[test]
fn no_event_other_than_a_keypress_can_end_the_wizard() {
    // A resize, a focus change, a key RELEASE on Windows, a mouse report from a
    // terminal that sends them unasked. None of these is a decision.
    let noise = [
        Event::Resize(120, 40),
        Event::FocusGained,
        Event::FocusLost,
        Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        }),
    ];

    for event in noise {
        let mut wizard = at_the_credential_step();
        let progress = wizard.event(&event);
        assert_eq!(progress, Progress::Idle, "{event:?} produced {progress:?}");
        assert_eq!(
            wizard.step(),
            Step::Credential,
            "{event:?} moved or ended the wizard",
        );
    }
}

#[test]
fn a_paste_on_a_picker_screen_is_ignored_rather_than_swallowed() {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Provider);

    assert_eq!(wizard.event(&Event::Paste("noise".into())), Progress::Idle,);
    assert_eq!(wizard.step(), Step::Provider);
    // The picker still works afterwards.
    wizard.event(&Event::Key(key(KeyCode::Enter)));
    assert_eq!(wizard.step(), Step::Credential);
}

/// Walk to the theme step, which is where the last two picker screens are.
///
/// The credential is typed rather than taken from the environment, so this walk
/// is the same one on a machine that happens to have `$OPENROUTER_API_KEY` set.
fn at_the_theme_step() -> Wizard {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter)); // welcome
    wizard.key(key(KeyCode::Enter)); // OpenRouter
    for character in "sk-test".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    wizard.key(key(KeyCode::Enter)); // -> verifying
    wizard.verified();
    wizard.catalogue(vec!["a/one".into(), "a/two".into()]);
    wizard.key(key(KeyCode::Enter)); // model chosen -> theme
    assert_eq!(wizard.step(), Step::Theme);
    wizard
}

#[test]
fn f9_the_provider_step_resolves_the_row_the_query_left_visible() {
    // `Kind::ALL[index]` reads the chosen index back **raw**, so this site does
    // not merely misbehave on a stale index — it panics, on the first screen of
    // the first run.
    //
    // The query is the whole of what makes the assertion mean anything. An
    // unfiltered picker cannot tell the two index spaces apart, because with
    // nothing typed they are the same list; only a narrowed list can say whether
    // what came back addresses `Kind::ALL` or the view drawn on top of it.
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Provider);

    // `h` admits `Anthropic` and nothing else — no other provider label carries
    // one — so the one visible row is row 1 of `Kind::ALL`, and row 0 is exactly
    // what a filtered index would report.
    wizard.key(key(KeyCode::Char('h')));

    let (mut screen, recorder) = support::screen_of(100, 40, 10);
    screen
        .draw(|frame| wizard.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();
    assert!(
        !viewport.contains("OpenRouter"),
        "the query did not narrow the list, so the choice below proves nothing: {viewport:?}",
    );
    assert!(
        recorder.contains("Anthropic"),
        "the row the operator is about to choose never reached the terminal",
    );

    wizard.key(key(KeyCode::Enter));
    assert_eq!(
        wizard.step(),
        Step::Credential,
        "Anthropic has a vendor endpoint, so no base URL is asked for",
    );
    let spec = wizard.spec();
    assert!(
        matches!(&spec, Some(ProviderSpec::Anthropic { .. })),
        "the wizard resolved a provider that was not on the screen: {spec:?}",
    );
}

#[test]
fn f9_the_theme_step_previews_and_writes_the_row_the_query_left_visible() {
    // The theme step is the separate case, and the worse one. It reads
    // `picker.selected()` on **every** keystroke to redraw the sample behind the
    // list, so a filtered index does not wait for Enter to do damage: it previews
    // one theme, and `settings::render` then writes that name into `io.toml`,
    // where it outlives the run.
    let _guard = env_lock();
    config_home();
    let mut wizard = at_the_theme_step();
    assert_eq!(
        wizard.theme().name,
        "dark",
        "the theme in use opens the list"
    );

    // `l` admits `light` and not `dark`, so the one visible row is row 1 of
    // `THEMES` — and row 0, the filtered position, is the theme the preview was
    // already showing, which is how this defect hides.
    wizard.key(key(KeyCode::Char('l')));
    assert_eq!(
        wizard.theme().name,
        "light",
        "the preview followed the filtered position rather than the visible row",
    );

    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Posture);
    wizard.key(key(KeyCode::Enter)); // the first posture, unfiltered
    assert_eq!(wizard.step(), Step::Confirm);

    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Write(_, contents) = progress else {
        panic!("the confirmation screen should produce a write, got {progress:?}");
    };
    assert!(
        contents.contains("light"),
        "the file records a theme the operator never saw: {contents}",
    );
    assert!(
        !contents.contains("dark"),
        "the previewed theme and the written theme parted company: {contents}",
    );
}

#[test]
fn f9_a_query_that_matches_no_theme_leaves_the_preview_alone() {
    // The theme step reads the picker after EVERY keystroke to redraw the sample
    // behind the list, and what it read was `selected()`, which answers 0 when
    // nothing matches. Zero is a real theme — so `z`, a letter no theme name
    // carries, recoloured the whole wizard and reassigned `theme_name`, which is
    // the exact string `settings::render` writes into `io.toml`. Nothing on the
    // screen said so, and backspacing the `z` out did not undo it.
    let _guard = env_lock();
    config_home();
    let mut wizard = at_the_theme_step();
    wizard.key(key(KeyCode::Down)); // light, deliberately
    assert_eq!(wizard.theme().name, "light");

    wizard.key(key(KeyCode::Char('z')));
    assert_eq!(
        wizard.theme().name,
        "light",
        "a query admitting no row previewed a theme nobody chose",
    );

    // And the marker comes back to it, so the choice below is the one that was
    // being previewed all along.
    wizard.key(key(KeyCode::Backspace));
    wizard.key(key(KeyCode::Enter)); // theme -> posture
    wizard.key(key(KeyCode::Enter)); // the first posture, unfiltered
    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Write(_, contents) = progress else {
        panic!("the confirmation screen should produce a write, got {progress:?}");
    };
    assert!(
        contents.contains("light"),
        "a typo and a backspace wrote a theme the operator never chose: {contents}",
    );
}

#[test]
fn f8_a_model_typed_while_the_catalogue_loads_survives_its_arrival() {
    // `verified()` opens a one-row picker holding the provider's default while the
    // catalogue request is in flight, and the catalogue then replaced every row by
    // replacing the whole picker. A four-hundred-model list is a list nobody
    // scrolls, so typing is the first thing anybody does — and everything typed
    // during the wait went out with the placeholder, with the marker jumping at
    // the same moment.
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter)); // welcome
    wizard.key(key(KeyCode::Enter)); // OpenRouter
    for character in "sk-test".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    wizard.key(key(KeyCode::Enter)); // -> verifying
    wizard.verified();
    assert_eq!(wizard.step(), Step::Model);

    // Typed against the placeholder, before a single catalogue row exists.
    for character in "gemini".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    wizard.catalogue(vec![
        "anthropic/claude-sonnet-4".into(),
        "openai/gpt-5".into(),
        "google/gemini-3-pro".into(),
    ]);

    let (mut screen, recorder) = support::screen_of(100, 40, 10);
    screen
        .draw(|frame| wizard.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();
    let drawn: Vec<&str> = viewport.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        drawn[0], "gemini",
        "the query typed during the wait was discarded: {viewport:?}",
    );
    assert!(
        !viewport.contains("openai/gpt-5"),
        "the catalogue arrived unfiltered: {viewport:?}",
    );
    assert!(recorder.contains("google/gemini-3-pro"));

    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Theme);
    let spec = wizard.spec().expect("a spec once the kind is known");
    assert!(
        matches!(&spec, ProviderSpec::OpenRouter { model, .. } if model == "google/gemini-3-pro"),
        "the wizard took a model the query never left on the screen: {spec:?}",
    );
}

#[test]
fn f9_the_posture_step_resolves_the_row_the_query_left_visible() {
    // `Posture::ALL[index]` is the second of the two raw slice reads, and the one
    // whose consequence lasts longest: a stale index in range writes a permission
    // boundary nobody chose into a file every later run reads.
    let _guard = env_lock();
    config_home();
    let mut wizard = at_the_theme_step();
    wizard.key(key(KeyCode::Enter)); // the theme in use, unfiltered
    assert_eq!(wizard.step(), Step::Posture);

    // `only` admits `Read only` and neither of the other two: `Sandboxed
    // workspace` and `Ask before writes` each carry an `o` with no `n` after it.
    // So the one visible row is row 2 of `Posture::ALL`, two rows away from the
    // filtered position.
    for character in "only".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Confirm);

    let summary: String = wizard
        .summary()
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        summary.contains("Read only"),
        "the confirmation screen names a posture that was not chosen: {summary:?}",
    );

    let progress = wizard.key(key(KeyCode::Enter));
    let Progress::Write(_, contents) = progress else {
        panic!("the confirmation screen should produce a write, got {progress:?}");
    };
    // Read the way io-harness reads it. A narrowing posture parses in any scope,
    // and row 0 — `Sandboxed workspace`, which is what a filtered index would
    // have resolved to — widens, so a wrong row fails here twice over: on this
    // parse, and on the effect below.
    let config = Config::from_toml(&contents).expect("a narrowing posture parses in any scope");
    let policy = config.policy().expect("a policy was written");
    assert_eq!(
        policy.defaults.write,
        Effect::Deny,
        "the file grants a permission the chosen row refuses: {contents}",
    );
}

#[test]
fn the_theme_step_draws_its_picker_and_labels_its_sample() {
    // The defect this exists for: `render_theme` reserved rows for the live
    // sample out of a viewport that had exactly that many, so the PICKER never
    // drew. A live first run saw only the sample — which contains a refusal and a
    // success, by design, so the palette can be judged — and read it as the
    // session having gone wrong.
    let mut wizard = at_the_theme_step();

    let (mut screen, _recorder) = support::screen_of(100, 40, io_cli::term::WIZARD_VIEWPORT_HEIGHT);
    screen
        .draw(|frame| wizard.render(frame, frame.area()))
        .expect("frame");

    let viewport = screen.viewport_text();
    assert!(
        viewport.contains("Which theme?"),
        "the theme picker did not draw at all: {viewport:?}",
    );
    for name in ["dark", "light"] {
        assert!(viewport.contains(name), "{name} is missing: {viewport:?}");
    }
    assert!(
        viewport.contains("preview"),
        "the sample must say it is a preview, or it reads as real output: {viewport:?}",
    );
    // And the sample is still there, below the picker, doing its job.
    assert!(viewport.contains("refused"), "{viewport:?}");
}

#[test]
fn the_wizard_viewport_shows_a_usable_number_of_choices() {
    // Three visible rows made a four-hundred-model catalogue unusable. The
    // wizard's viewport is sized for its screens rather than for the session's,
    // and the assertion is on what actually renders rather than on the constant —
    // the constant is only a means to it.
    let mut picker = io_cli::picker::Picker::new(
        "Which model?",
        (0..400)
            .map(|n| io_cli::picker::Row::new(format!("vendor/model-{n}")))
            .collect(),
    );
    let (mut screen, _recorder) = support::screen_of(100, 40, io_cli::term::WIZARD_VIEWPORT_HEIGHT);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &DARK))
        .expect("frame");

    let rows = screen
        .viewport_text()
        .lines()
        .filter(|line| line.contains("vendor/model-"))
        .count();
    assert!(
        rows >= 7,
        "only {rows} choices are visible; three is what made this unusable",
    );
}

#[test]
fn f3_a_rejected_key_returns_to_the_credential_step_with_the_providers_own_words() {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter)); // welcome
    wizard.key(key(KeyCode::Enter)); // OpenRouter
    for character in "sk-wrong".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    assert!(matches!(
        wizard.key(key(KeyCode::Enter)),
        Progress::Verify(_)
    ));
    assert_eq!(wizard.step(), Step::Verifying);

    // The provider's own message, not a generic failure: every provider reports a
    // bad credential differently and the difference is the information.
    let message = "401 No auth credentials found";
    assert_eq!(wizard.rejected(message), Progress::Idle);
    assert_eq!(wizard.step(), Step::Credential);

    let (mut screen, _recorder) = support::screen_of(100, 40, 10);
    screen
        .draw(|frame| wizard.render(frame, frame.area()))
        .expect("frame");
    let viewport = screen.viewport_text();
    assert!(
        viewport.contains(message),
        "the provider's message should be on the credential screen: {viewport:?}",
    );
    assert!(
        viewport.contains("error"),
        "and it should carry a word, not only a colour: {viewport:?}",
    );
}

#[test]
fn f3_a_rejected_key_writes_nothing() {
    // The write is reachable from exactly one place — the confirmation screen —
    // and a rejection never gets there. Asserted as a property of the type: no
    // step other than Confirm can produce a `Write`.
    let mut wizard = Wizard::new(DARK);
    let mut produced = Vec::new();
    produced.push(wizard.key(key(KeyCode::Enter)));
    produced.push(wizard.key(key(KeyCode::Enter)));
    for character in "sk-wrong".chars() {
        produced.push(wizard.key(key(KeyCode::Char(character))));
    }
    produced.push(wizard.key(key(KeyCode::Enter)));
    produced.push(wizard.rejected("401 Unauthorized"));
    // Try to push on regardless.
    for _ in 0..5 {
        produced.push(wizard.key(key(KeyCode::Enter)));
    }

    assert!(
        !produced
            .iter()
            .any(|progress| matches!(progress, Progress::Write(..))),
        "a rejected credential reached a write",
    );
}

#[test]
fn escape_leaves_without_writing() {
    for step_keys in 0..4 {
        let mut wizard = Wizard::new(DARK);
        for _ in 0..step_keys {
            wizard.key(key(KeyCode::Enter));
        }
        let progress = wizard.key(key(KeyCode::Esc));
        assert!(
            matches!(progress, Progress::Cancelled | Progress::Idle),
            "escape at step {step_keys} produced {progress:?}",
        );
        assert!(!matches!(progress, Progress::Write(..)));
    }
}

#[test]
fn a_compatible_endpoint_asks_for_a_base_url_first() {
    let mut wizard = Wizard::new(DARK);
    wizard.key(key(KeyCode::Enter));
    for _ in 0..3 {
        wizard.key(key(KeyCode::Down));
    }
    wizard.key(key(KeyCode::Enter));
    assert_eq!(
        wizard.step(),
        Step::BaseUrl,
        "a compatible endpoint has no vendor to assume a URL from",
    );

    for character in "http://localhost:11434/v1".chars() {
        wizard.key(key(KeyCode::Char(character)));
    }
    wizard.key(key(KeyCode::Enter));
    assert_eq!(wizard.step(), Step::Credential);

    let spec = wizard.spec().expect("a spec once the kind is known");
    assert!(
        matches!(&spec, ProviderSpec::Compatible { base_url: Some(url), .. }
            if url == "http://localhost:11434/v1"),
        "got {spec:?}",
    );
}

#[test]
fn every_posture_is_a_policy_rather_than_a_flag_of_our_own() {
    // A posture has to be an io-harness policy, or the status line can never name
    // the layer in force and a refusal can never name the rule that produced it.
    assert_eq!(Posture::Workspace.defaults().write, Effect::Allow);
    assert_eq!(Posture::AskWrites.defaults().write, Effect::Ask);
    assert_eq!(Posture::ReadOnly.defaults().write, Effect::Deny);
    for posture in Posture::ALL {
        assert_eq!(
            posture.defaults().net,
            Effect::Deny,
            "{} should not open the network by default",
            posture.label(),
        );
        assert!(
            !posture.detail().is_empty(),
            "{} needs to say what it means in plain words",
            posture.label(),
        );
    }
}

#[test]
fn the_rendered_file_is_the_harness_schema_and_nothing_of_our_own() {
    let _guard = env_lock();
    let spec = ProviderSpec::Anthropic {
        model: "claude-sonnet-4".into(),
        api_key: None,
    };
    let text = settings::render(&spec, Posture::Workspace, "dark").expect("render");

    // Read the way it will actually be read, and NOT with `Config::from_toml`.
    //
    // `from_toml` parses text as the PROJECT scope, and io-harness refuses a
    // project-scoped file that widens the permission boundary: `exec = "allow"` in
    // a committed `io.toml` would let a cloned repository grant itself permission.
    // The wizard writes the USER scope, where widening is the operator's own
    // decision and is allowed — so the default "sandboxed workspace" posture is a
    // correct file that `from_toml` is right to reject. Discovering it through the
    // scope it is written to is the faithful check; the narrower postures below
    // parse either way.
    let dir = tempfile::tempdir().expect("a temporary directory");
    // The user file lives outside the workspace it is discovered from. Put it at
    // `<root>/io.toml` and it is ALSO the project-scope candidate, which is read
    // under the project rules and refused — the same trap this test exists to
    // document.
    let elsewhere = dir.path().join("home");
    std::fs::create_dir_all(&elsewhere).expect("a home directory");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace).expect("a workspace");

    let path = elsewhere.join("io.toml");
    settings::write(&path, &text).expect("write");
    std::env::set_var("IO_CONFIG", &path);
    let config = Config::discover(&workspace).expect("io-harness reads its own user-scope file");
    std::env::remove_var("IO_CONFIG");
    assert!(matches!(
        config.provider_spec(),
        Some(ProviderSpec::Anthropic { .. })
    ));
    assert_eq!(
        config.policy().expect("a policy").defaults.exec,
        Effect::Allow,
        "the user scope may widen, which is the whole reason the wizard writes there",
    );

    // A posture that only narrows is legal in any scope, project included.
    let narrow = settings::render(&spec, Posture::ReadOnly, "dark").expect("render");
    Config::from_toml(&narrow).expect("a narrowing file parses as a project file too");

    // No key written means the provider's own environment variable, which is the
    // better outcome and not a fallback: a key that is never on disk cannot leak
    // from it.
    assert!(
        !text.contains("api_key"),
        "an absent credential should be absent from the file: {text}",
    );
    assert!(text.contains("[app.io-cli]"), "got {text}");
}
