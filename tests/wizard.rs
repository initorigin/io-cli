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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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
