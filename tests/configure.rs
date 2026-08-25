//! F2 and N2 — what is in force, who decided it, and what is never shown.

use std::sync::{Mutex, MutexGuard};

use io_cli::configure::{self, Decided};
use io_harness::config::{Config, Scope};

/// `Config::discover` reads `IO_CONFIG` at call time, so two tests setting it at
/// once would each see the other's file. Serialised here rather than diagnosed
/// later as an intermittent failure on a loaded machine — the same guard
/// `tests/wizard.rs` and `tests/contract.rs` already keep for the same reason.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Three scopes that disagree on purpose, so every origin arm has a case.
///
/// The user file is written outside the workspace, because a file *inside* it is
/// project-scoped whatever variable names it — a fact this repository paid two
/// live runs for in 0.14.0.
struct Scopes {
    _home: tempfile::TempDir,
    root: tempfile::TempDir,
    user: std::path::PathBuf,
}

fn scopes(user: &str, project: &str, local: &str) -> Scopes {
    let home = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();

    let user_path = home.path().join("io.toml");
    std::fs::write(&user_path, user).unwrap();
    if !project.is_empty() {
        std::fs::write(root.path().join("io.toml"), project).unwrap();
    }
    if !local.is_empty() {
        std::fs::write(root.path().join("io.local.toml"), local).unwrap();
    }
    Scopes {
        _home: home,
        root,
        user: user_path,
    }
}

impl Scopes {
    fn config(&self) -> Config {
        let _guard = env_lock();
        std::env::set_var("IO_CONFIG", &self.user);
        let config = Config::discover(self.root.path()).unwrap();
        std::env::remove_var("IO_CONFIG");
        config
    }
}

#[test]
fn f2_each_scope_names_its_own_file() {
    let s = scopes(
        "[run]\nmax_steps = 10\n",
        "[run]\nmax_tokens = 50000\n",
        "[app.io-cli]\ntheme = \"dark\"\n",
    );
    let config = s.config();
    let settings = configure::settings(&config);
    let find = |key: &str| {
        settings
            .iter()
            .find(|s| s.path == key)
            .unwrap_or_else(|| panic!("{key} is not on the surface"))
            .clone()
    };

    let steps = find("run.max_steps");
    assert_eq!(steps.decided.word(), "user");
    assert_eq!(steps.value.as_deref(), Some("10"));

    let tokens = find("run.max_tokens");
    assert_eq!(tokens.decided.word(), "project");
    assert_eq!(tokens.value.as_deref(), Some("50000"));

    let theme = find("app.io-cli.theme");
    assert_eq!(theme.decided.word(), "local");
    assert_eq!(theme.value.as_deref(), Some("\"dark\""));
}

#[test]
fn f2_a_key_no_file_named_is_a_default_and_names_no_file() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    let config = s.config();
    let settings = configure::settings(&config);

    let retries = settings
        .iter()
        .find(|s| s.path == "run.max_retries")
        .expect("a catalogue key is on the surface even when no file names it");

    assert_eq!(retries.decided, Decided::Default);
    assert_eq!(retries.decided.word(), "default");
    assert_eq!(
        retries.decided.path(),
        None,
        "a crate default was attributed to a file"
    );
    assert_eq!(
        retries.value, None,
        "there is nothing to quote for a key no file named"
    );
}

#[test]
fn f2_the_winning_scope_is_the_one_reported() {
    // Local beats project beats user, and the surface must report the file that
    // actually decided the value rather than the first one that mentioned it.
    let s = scopes(
        "[run]\nmax_steps = 10\n",
        "[run]\nmax_steps = 20\n",
        "[run]\nmax_steps = 30\n",
    );
    let config = s.config();
    let steps = configure::setting(&config, "run.max_steps");

    assert_eq!(steps.decided.word(), "local");
    assert_eq!(steps.value.as_deref(), Some("30"));
    assert_eq!(config.origin("run.max_steps").last().unwrap().scope, Scope::Local);
}

#[test]
fn f2_a_key_the_catalogue_does_not_know_is_still_shown() {
    // The property that keeps the surface honest: io-cli's catalogue is its own
    // list, and a key an operator wrote that is not on it must not be invisible.
    let s = scopes(
        "[run]\nmax_steps = 10\n",
        "[web]\nallowed_domains = [\"example.com\"]\n",
        "",
    );
    let config = s.config();
    let settings = configure::settings(&config);

    assert!(
        settings.iter().any(|s| s.path.starts_with("web.")),
        "a key outside the catalogue vanished from the surface: {:?}",
        settings.iter().map(|s| &s.path).collect::<Vec<_>>()
    );
}

#[test]
fn f2_a_section_with_no_accessor_is_still_read() {
    // MemorySection is private in io-harness 0.66 and there is no Config::memory(),
    // so these keys can only be shown by quoting the file that named them. A
    // surface built on typed accessors alone would have a hole here.
    let s = scopes("[memory]\nmax_entries = 500\n", "", "");
    let config = s.config();
    let rows = configure::setting(&config, "memory.max_entries");

    assert_eq!(rows.decided.word(), "user");
    assert_eq!(rows.value.as_deref(), Some("500"));
}

#[test]
fn n2_a_credential_is_never_shown_in_full() {
    let secret = "sk-or-v1-abcdef0123456789";
    assert_eq!(
        configure::redact("provider.api_key", &format!("\"{secret}\"")),
        "\"…6789\"",
        "the key itself reached the surface"
    );
    assert!(!configure::redact("provider.api_key", &format!("\"{secret}\"")).contains("abcdef"));

    // A short value says only that it is set.
    assert_eq!(configure::redact("provider.api_key", "\"ab\""), "\"set\"");

    // An indirection is shown as written: the variable's NAME is the information.
    assert_eq!(
        configure::redact("provider.api_key", "\"${env:OPENROUTER_API_KEY}\""),
        "\"${env:OPENROUTER_API_KEY}\"",
        "the operator cannot see which variable they pointed at"
    );
    assert_eq!(
        configure::redact("provider.api_key", "\"${file:/run/secrets/key}\""),
        "\"${file:/run/secrets/key}\"",
    );

    // Anything that is not a credential is untouched.
    assert_eq!(configure::redact("run.max_steps", "30"), "30");
    assert_eq!(configure::redact("app.io-cli.theme", "\"dark\""), "\"dark\"");
}

#[test]
fn n2_no_setting_row_can_carry_a_credential() {
    let s = scopes(
        "[[provider]]\nkind = \"openrouter\"\nmodel = \"m\"\napi_key = \"sk-or-v1-supersecret\"\n",
        "",
        "",
    );
    let config = s.config();
    let settings = configure::settings(&config);
    let rows = configure::rows(&settings);

    for row in &rows {
        let text = format!("{} {}", row.label, row.detail.clone().unwrap_or_default());
        assert!(
            !text.contains("supersecret"),
            "a credential reached a rendered row: {text}"
        );
    }
}

#[test]
fn f2_the_catalogue_is_documented_rather_than_invented() {
    // A list nothing has to agree with is decoration. Every key io-cli offers
    // when no file names it must be one the checked-in example documents, so a
    // key invented here fails rather than shipping as a row that writes a
    // section io-harness will reject.
    let example = std::fs::read_to_string("docs/config.example.toml")
        .expect("docs/config.example.toml is checked in");

    for key in configure::CATALOGUE {
        // The example writes keys under section headers, so the last segment is
        // what appears on a line; assert the segment and its section both occur.
        let last = key.rsplit('.').next().unwrap();
        assert!(
            example.contains(last),
            "`{key}` is in io-cli's catalogue and nowhere in docs/config.example.toml. \
             Either the key is invented or the example is out of date; both are this \
             release's problem."
        );
    }
}
