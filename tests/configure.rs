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
    assert_eq!(
        config.origin("run.max_steps").last().unwrap().scope,
        Scope::Local
    );
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
    assert_eq!(
        configure::redact("app.io-cli.theme", "\"dark\""),
        "\"dark\""
    );
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

        // **And the section it lives under, because the last segment alone stopped
        // being a check.** `model` is the last segment of two of 0.26.0's routing
        // keys and occurs dozens of times in that file, so a mistyped sub-table —
        // `routing.escalate.model` — satisfied the assertion above and every other
        // gate. A key nested two or more levels deep names its own table, and the
        // table header is what an operator actually has to type.
        if let Some(parent) = key.rsplit_once('.').map(|(head, _)| head) {
            if parent.matches('.').count() >= 2 {
                assert!(
                    example.contains(&format!("[{parent}]")),
                    "`{key}` sits under `[{parent}]`, and docs/config.example.toml \
                     has no such table. The last segment matching somewhere in the \
                     file is not evidence that the path is right."
                );
            }
        }
    }
}

// --- F3: the write lands in the scope that was picked, and takes effect -------

#[test]
fn f3_a_change_lands_in_the_picked_scope_and_nowhere_else() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &s.user);

    configure::write(
        s.root.path(),
        Scope::Local,
        &[io_cli::edit::Edit::set("run.max_steps", "42")],
    )
    .unwrap();

    let local = std::fs::read_to_string(s.root.path().join("io.local.toml")).unwrap();
    assert!(local.contains("max_steps = 42"));

    // The user file is untouched: a write to one scope is a write to one file.
    let user = std::fs::read_to_string(&s.user).unwrap();
    assert!(
        user.contains("max_steps = 10"),
        "the user file was edited too"
    );
    assert!(
        !s.root.path().join("io.toml").exists(),
        "a project file was created by a write to the local scope"
    );

    std::env::remove_var("IO_CONFIG");
}

#[test]
fn f3_the_reloaded_config_is_what_the_next_turn_is_built_from() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &s.user);

    let (before, _) = configure::reload(s.root.path()).unwrap();
    let opening = io_cli::contract::configured("go", s.root.path().to_path_buf(), &before);
    assert_eq!(opening.max_steps, 10);

    configure::write(
        s.root.path(),
        Scope::User,
        &[io_cli::edit::Edit::set("run.max_steps", "42")],
    )
    .unwrap();

    let (after, _) = configure::reload(s.root.path()).unwrap();
    let next = io_cli::contract::configured("go", s.root.path().to_path_buf(), &after);
    assert_eq!(
        next.max_steps, 42,
        "the next turn was built from the configuration as it was at session start"
    );

    std::env::remove_var("IO_CONFIG");
}

#[test]
fn f3_reload_refreshes_io_cli_s_own_settings_too() {
    // The half a reload forgets: `main` derives CliSettings from the Config once.
    // A reload that returned only the Config would leave the theme, the glyph set
    // and every capability as they were at session start.
    let s = scopes("[app.io-cli]\ntheme = \"dark\"\n", "", "");
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &s.user);

    let (_, before) = configure::reload(s.root.path()).unwrap();
    assert_eq!(before.unwrap().theme.as_deref(), Some("dark"));

    configure::write(
        s.root.path(),
        Scope::User,
        &[io_cli::edit::Edit::set("app.io-cli.theme", "\"light\"")],
    )
    .unwrap();

    let (_, after) = configure::reload(s.root.path()).unwrap();
    assert_eq!(
        after.unwrap().theme.as_deref(),
        Some("light"),
        "io-cli's own settings were not re-derived from the reloaded configuration"
    );

    std::env::remove_var("IO_CONFIG");
}

#[test]
fn f3_a_scope_with_no_file_gets_one() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &s.user);

    configure::write(
        s.root.path(),
        Scope::Project,
        &[io_cli::edit::Edit::set("run.max_tokens", "50000")],
    )
    .unwrap();

    let project = std::fs::read_to_string(s.root.path().join("io.toml")).unwrap();
    assert!(project.contains("max_tokens = 50000"));

    std::env::remove_var("IO_CONFIG");
}

// --- F4: a project-scoped widening is refused in the harness's own words ------

/// Every case io-harness refuses in a project-scoped file: the two whole sections
/// and each of `PROJECT_WIDENING`'s five key/value pairs.
const WIDENINGS: &[(&str, &str)] = &[
    ("policy.defaults.exec", "\"allow\""),
    ("policy.defaults.net", "\"allow\""),
    ("sandbox.allow_network", "true"),
    ("sandbox.force_floor", "false"),
    ("sandbox.mode", "\"full-access\""),
];

#[test]
fn f4_a_project_scoped_widening_is_refused_with_the_harness_s_sentence() {
    for (key, value) in WIDENINGS {
        let s = scopes("[run]\nmax_steps = 10\n", "", "");
        let _guard = env_lock();
        std::env::set_var("IO_CONFIG", &s.user);

        let err = configure::write(
            s.root.path(),
            Scope::Project,
            &[io_cli::edit::Edit::set(*key, *value)],
        )
        .expect_err(&format!(
            "{key} = {value} should be refused at project scope"
        ));

        // io-harness's own words, not a summary of them. The half an operator
        // needs is WHY, and only the harness says it.
        assert!(
            err.contains("narrow it and never widen it"),
            "the refusal for {key} was re-worded by io-cli: {err}"
        );
        assert!(
            err.contains(key),
            "the refusal does not name the key: {err}"
        );

        // And the file is back as it was — a refused write leaves nothing behind.
        assert!(
            !s.root.path().join("io.toml").exists(),
            "{key}: a refused write left a project file behind"
        );

        std::env::remove_var("IO_CONFIG");
    }
}

#[test]
fn f4_the_same_value_is_accepted_in_the_local_scope() {
    // The rule is about the scope, not the value. Every one of these is legal in
    // the file a repository does not deliver.
    for (key, value) in WIDENINGS {
        let s = scopes("[run]\nmax_steps = 10\n", "", "");
        let _guard = env_lock();
        std::env::set_var("IO_CONFIG", &s.user);

        configure::write(
            s.root.path(),
            Scope::Local,
            &[io_cli::edit::Edit::set(*key, *value)],
        )
        .unwrap_or_else(|e| panic!("{key} = {value} should be legal in io.local.toml: {e}"));

        std::env::remove_var("IO_CONFIG");
    }
}

#[test]
fn f4_a_refused_write_does_not_disturb_a_file_that_already_existed() {
    let s = scopes(
        "[run]\nmax_steps = 10\n",
        "# a project file with a comment\n[run]\nmax_tokens = 9000\n",
        "",
    );
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &s.user);
    let before = std::fs::read_to_string(s.root.path().join("io.toml")).unwrap();

    let err = configure::write(
        s.root.path(),
        Scope::Project,
        &[io_cli::edit::Edit::set("policy.defaults.exec", "\"allow\"")],
    )
    .unwrap_err();
    assert!(err.contains("narrow it and never widen it"));

    assert_eq!(
        std::fs::read_to_string(s.root.path().join("io.toml")).unwrap(),
        before,
        "a refused write left the operator's project file changed"
    );

    std::env::remove_var("IO_CONFIG");
}

#[test]
fn f4_the_two_whole_sections_are_refused_at_project_scope_as_well() {
    // The other two of F4's seven: `[[hook]]` and `[browser]` are refused
    // WHOLESALE in a project file rather than by value, because each names a
    // program to run on this machine and io.toml arrives with a `git clone`.
    let cases: Vec<(&str, io_cli::edit::Edit)> = vec![
        (
            "hook",
            io_cli::edit::Edit::append("hook", "event = \"run_start\"\nrun = [\"echo\", \"hi\"]"),
        ),
        (
            "browser",
            io_cli::edit::Edit::set("browser.command", "\"firefox\""),
        ),
    ];

    for (name, edit) in cases {
        let s = scopes("[run]\nmax_steps = 10\n", "", "");
        let _guard = env_lock();
        std::env::set_var("IO_CONFIG", &s.user);

        let err = configure::write(s.root.path(), Scope::Project, &[edit])
            .expect_err(&format!("a project-scoped [{name}] should be refused"));

        assert!(
            err.contains(name),
            "the refusal for {name} does not name it: {err}"
        );
        assert!(
            err.contains("git clone"),
            "the refusal for {name} is not the harness's own sentence: {err}"
        );
        assert!(
            !s.root.path().join("io.toml").exists(),
            "{name}: a refused write left a project file behind"
        );

        std::env::remove_var("IO_CONFIG");
    }
}

// --- F10: named profiles ------------------------------------------------------

#[test]
fn f10_the_profiles_a_file_declares_are_listed() {
    // io-harness has NO accessor for these: `with_profile` applies one by name
    // and nothing lists them, because the merged table is private and profile
    // keys do not appear in `Config::origins`. So they come from the file.
    let s = scopes(
        "[run]\nmax_steps = 10\n\n[profile.fast]\n[profile.fast.run]\nmax_steps = 99\n\n\
         [profile.careful]\n[profile.careful.run]\nmax_steps = 3\n",
        "",
        "",
    );
    let config = s.config();
    assert_eq!(
        configure::profiles(&config),
        vec!["careful".to_string(), "fast".to_string()],
        "a profile with sub-tables is one profile, not two"
    );
}

#[test]
fn f10_a_file_with_no_profiles_lists_none() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    assert!(configure::profiles(&s.config()).is_empty());
}

#[test]
fn f10_applying_a_profile_overlays_it_for_the_session() {
    let s = scopes(
        "[run]\nmax_steps = 10\n\n[profile.fast]\n[profile.fast.run]\nmax_steps = 99\n",
        "",
        "",
    );
    let config = s.config();
    assert_eq!(
        io_cli::contract::configured("go", s.root.path().to_path_buf(), &config).max_steps,
        10
    );

    let fast = configure::with_profile(&config, "fast").unwrap();
    assert_eq!(
        io_cli::contract::configured("go", s.root.path().to_path_buf(), &fast).max_steps,
        99,
        "the profile did not reach the contract a turn is built from"
    );
}

#[test]
fn f10_a_profile_that_is_not_there_reports_the_harness_s_own_sentence() {
    let s = scopes("[run]\nmax_steps = 10\n", "", "");
    let err = configure::with_profile(&s.config(), "nope").unwrap_err();
    assert!(
        err.contains("no `[profile.nope]`"),
        "io-cli re-worded the harness's refusal: {err}"
    );
}

// F1 — every key has a kind, and only the named exceptions permit free text.

/// Every catalogue key resolves to a kind, and the failure names the key.
///
/// Sabotage: add a key to `CATALOGUE` without a kind. Under it only this test
/// fails, and it fails by naming the key — a count would say a number is wrong and
/// leave someone to find which one.
#[test]
fn f1_every_catalogue_key_has_a_kind() {
    let missing: Vec<&str> = configure::CATALOGUE
        .iter()
        .copied()
        .filter(|key| configure::kind_of(key).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "these catalogue keys have no value kind: {missing:?}"
    );
}

/// The keys that stay free text are exactly three, written out.
///
/// A literal rather than a count, so a key that drifted into free text is named
/// rather than merely changing a number. Free text is the state this release
/// exists to remove, so growing this set has to be a deliberate act.
#[test]
fn f1_only_three_keys_are_authored_text() {
    let mut text: Vec<&str> = configure::CATALOGUE
        .iter()
        .copied()
        .filter(|key| configure::kind_of(key) == Some(configure::Kind::Text))
        .collect();
    text.sort_unstable();
    // Sorted on both sides, so this asserts the set and not the ordering of a
    // constant it does not own — and it is a written-out literal, so a key that
    // drifted into free text is named by the failure.
    assert_eq!(
        text,
        vec![
            "app.io-cli.gates.contains",
            "app.io-cli.gates.rubric",
            "app.io-cli.prices.source_url",
        ]
    );
}

/// `app.io-cli.gates.command` is a list and not text.
///
/// Sabotage: classify it as authored text. Under it this test fails, and the
/// surface writes a bare string into a key io-harness reads as `Vec<String>` —
/// a value the harness cannot read back at all.
#[test]
fn f1_the_command_key_is_the_one_list() {
    let lists: Vec<&str> = configure::CATALOGUE
        .iter()
        .copied()
        .filter(|key| configure::kind_of(key) == Some(configure::Kind::List))
        .collect();
    assert_eq!(lists, vec!["app.io-cli.gates.command"]);
}

/// `prices.as_of` is written by machinery and is never offered for typing.
#[test]
fn f1_the_dated_price_table_is_machine_written() {
    assert_eq!(
        configure::kind_of("prices.as_of"),
        Some(configure::Kind::Machine)
    );
    let machine: Vec<&str> = configure::CATALOGUE
        .iter()
        .copied()
        .filter(|key| configure::kind_of(key) == Some(configure::Kind::Machine))
        .collect();
    assert_eq!(machine, vec!["prices.as_of"]);
}

/// A key no catalogue entry names has no kind, and that is deliberate.
///
/// `settings` lists keys an operator wrote that io-cli does not know about; a kind
/// guessed for one of those would be io-cli inventing a schema.
#[test]
fn f1_an_unknown_key_has_no_kind() {
    assert_eq!(configure::kind_of("something.nobody.declared"), None);
}

/// The one signed number is the only signed number.
#[test]
fn f1_only_the_expected_exit_status_is_signed() {
    let signed: Vec<&str> = configure::CATALOGUE
        .iter()
        .copied()
        .filter(|key| configure::kind_of(key) == Some(configure::Kind::Number { signed: true }))
        .collect();
    assert_eq!(signed, vec!["app.io-cli.gates.expect_exit"]);
}

// F2 — the enum kinds agree with the dependency rather than with a literal.

/// The effects are the dependency's three, and each one round-trips through
/// io-harness's own deserializer.
///
/// The census itself is a build break — `Effect` is not `#[non_exhaustive]`, so
/// `configure::effects`'s array and match both fail on a variant change. What this
/// adds is the half the build cannot catch: the *spellings* are serde's
/// `rename_all`, io-harness exposes no `as_str` for `Effect`, and a rename would
/// leave io-cli compiling and writing a word the schema rejects.
///
/// Sabotage: add a fourth effect to io-cli's list. Under it F2 fails, because the
/// dependency knows three.
#[test]
fn f2_the_effects_are_the_dependency_s_own() {
    let effects = configure::effects();
    assert_eq!(effects, vec!["allow", "ask", "deny"]);
    for word in &effects {
        let text = format!("[policy]\ndefaults = {{ read = \"{word}\" }}\n");
        assert!(
            Config::from_toml(&text).is_ok(),
            "io-harness refuses the effect io-cli offers: {word}"
        );
    }
}

/// The exec modes are spelled by io-harness, not by io-cli.
///
/// Weaker than the effects' guarantee and the difference is the dependency's:
/// `ExecMode` **is** `#[non_exhaustive]` (`sandbox.rs:379`), so a variant added
/// later falls into a required wildcard and cannot break this build. What is
/// assertable is that every spelling comes from `ExecMode::as_str` and that
/// io-harness accepts each one.
#[test]
fn f2_the_exec_modes_are_spelled_by_the_dependency() {
    let modes = configure::exec_modes();
    assert_eq!(modes, vec!["read-only", "workspace-write", "full-access"]);
    // **In the user scope, because `Config::from_toml` parses as the PROJECT
    // scope and `full-access` is refused there.** io-harness's `PROJECT_WIDENING`
    // (`config.rs:1759-1769`) refuses five (key, value) pairs in a committed file,
    // `sandbox.mode = "full-access"` among them, because `io.toml` arrives with a
    // `git clone`. Every mode io-cli offers is a mode io-harness accepts *somewhere*,
    // and that is what this asserts; where each one is legal is
    // `f2_a_widening_value_is_legal_in_one_scope_and_refused_in_another` below.
    for mode in &modes {
        let s = scopes(&format!("[sandbox]\nmode = \"{mode}\"\n"), "", "");
        assert_eq!(
            s.config()
                .sandbox()
                .expect("a sandbox section")
                .mode
                .as_str(),
            mode,
            "io-harness refuses the mode io-cli offers: {mode}"
        );
    }
    // Every spelling is the variant's own, never io-cli's.
    for mode in [
        io_harness::ExecMode::ReadOnly,
        io_harness::ExecMode::WorkspaceWrite,
        io_harness::ExecMode::FullAccess,
    ] {
        assert!(
            modes.contains(&mode.as_str().to_string()),
            "{} is a variant with no row",
            mode.as_str()
        );
    }
}

/// The five values a committed file may not carry are exactly io-harness's five.
///
/// **The whole file is refused, not the key**, because `refuse_widening` runs
/// before deserialization — so an operator who picks one of these on a key their
/// project `io.toml` decides gets a configuration that no longer parses. A menu
/// that offered it without saying so would be a menu that lies.
///
/// Round-tripped through the dependency in both directions, so a pair io-harness
/// adds or drops is caught here rather than by an operator: every value io-cli
/// calls widening must actually be refused, and the narrowing value of each of the
/// same keys must actually be accepted.
///
/// Sabotage: drop `sandbox.mode`/`full-access` from `widens_project`. Under it this
/// test fails, and the surface goes back to offering the one value that breaks the
/// file it is about to be written into.
#[test]
fn f2_a_widening_value_is_legal_in_one_scope_and_refused_in_another() {
    let widening = [
        (
            "policy.defaults.exec",
            "allow",
            "[policy]\ndefaults = { exec = \"allow\" }\n",
        ),
        (
            "policy.defaults.net",
            "allow",
            "[policy]\ndefaults = { net = \"allow\" }\n",
        ),
        (
            "sandbox.allow_network",
            "true",
            "[sandbox]\nallow_network = true\n",
        ),
        (
            "sandbox.force_floor",
            "false",
            "[sandbox]\nforce_floor = false\n",
        ),
        (
            "sandbox.mode",
            "full-access",
            "[sandbox]\nmode = \"full-access\"\n",
        ),
    ];
    for (key, value, project) in widening {
        assert!(
            configure::widens_project(key, value),
            "{key} = {value} widens and io-cli does not say so"
        );
        // The dependency's own answer, at the scope `from_toml` parses as.
        let refusal = Config::from_toml(project)
            .expect_err("io-harness must refuse this in a committed file")
            .to_string();
        assert!(
            refusal.contains("widens"),
            "io-harness refused {key} for another reason: {refusal}"
        );
        // And the same text is accepted where it is the operator's own file.
        let s = scopes(project, "", "");
        let _ = s.config();
    }
    // The narrowing values of the same keys stay legal in a committed file, which
    // is what the scope is for — and are not marked.
    for (key, value, project) in [
        (
            "policy.defaults.exec",
            "deny",
            "[policy]\ndefaults = { exec = \"deny\" }\n",
        ),
        (
            "sandbox.mode",
            "read-only",
            "[sandbox]\nmode = \"read-only\"\n",
        ),
        (
            "sandbox.allow_network",
            "false",
            "[sandbox]\nallow_network = false\n",
        ),
    ] {
        assert!(
            !configure::widens_project(key, value),
            "{key} = {value} narrows and must not be marked"
        );
        assert!(
            Config::from_toml(project).is_ok(),
            "io-harness refuses a narrowing value in a committed file: {key}"
        );
    }
}

/// A mode this build does not know is reported, not dropped.
///
/// Sabotage: make the wildcard omit the row. Under it F2 fails — and it fails on
/// the only failure mode a `#[non_exhaustive]` enum leaves available, a menu
/// quietly missing an option nobody can detect.
#[test]
fn f2_a_known_mode_is_labelled_by_its_own_name() {
    for mode in [
        io_harness::ExecMode::ReadOnly,
        io_harness::ExecMode::WorkspaceWrite,
        io_harness::ExecMode::FullAccess,
    ] {
        assert_eq!(configure::exec_mode_label(mode), mode.as_str());
    }
}

// F4 — the number ladder is one-two-five, anchored on the value in force.

/// The rungs are 1, 2, 5 at each magnitude and nothing else.
#[test]
fn f4_the_ladder_is_one_two_five() {
    let rungs = configure::ladder(Some(8), false);
    for rung in &rungs {
        if *rung == 0 || *rung == 8 {
            continue;
        }
        let mut value = rung.abs();
        while value % 10 == 0 {
            value /= 10;
        }
        assert!(
            matches!(value, 1 | 2 | 5),
            "{rung} is not a one-two-five rung"
        );
    }
}

/// Stepping up lands on the next rung and stepping down reverses it exactly.
#[test]
fn f4_stepping_up_and_down_are_inverses() {
    let rungs = {
        let mut sorted = configure::ladder(Some(200), false);
        sorted.sort_unstable();
        sorted
    };
    let at = rungs
        .iter()
        .position(|rung| *rung == 200)
        .expect("the value in force is a rung");
    let up = rungs[at + 1];
    let down = rungs[at - 1];
    assert_eq!(up, 500);
    assert_eq!(down, 100);
    // And back again.
    let mut around = configure::ladder(Some(up), false);
    around.sort_unstable();
    let back = around
        .iter()
        .position(|rung| *rung == up)
        .expect("the new value is a rung");
    assert_eq!(around[back - 1], 200);
}

/// The value in force is always on the ladder, even when it is not a rung.
///
/// A list that silently omits what the file currently says is a list an operator
/// cannot find their own setting in.
#[test]
fn f4_an_odd_value_in_force_is_still_offered() {
    assert!(configure::ladder(Some(12_345), false).contains(&12_345));
}

/// The nearest rungs come first, because that is the change an operator wants.
#[test]
fn f4_the_ladder_is_ordered_from_the_value_in_force() {
    let rungs = configure::ladder(Some(200), false);
    assert_eq!(rungs.first(), Some(&200));
    // The two either side lead, in the order their distance puts them.
    assert!(
        rungs[1..3].contains(&100) && rungs[1..3].contains(&500),
        "the neighbours must be the next rows: {:?}",
        &rungs[..4]
    );
}

/// A key with no value in any file ladders from 1.
///
/// The audit's finding, not a corner case: there is no per-key io-harness default
/// to anchor on for *any* number key, so this is the ordinary state. An empty
/// picker was the alternative and a surface whose argument is that a value is
/// chosen cannot offer nothing.
///
/// Sabotage: offer an anchor row for `run.max_steps`. Under it F4 fails, because
/// `TaskContract::new` says 8 and `TaskContract::workspace` says 12 and neither is
/// "the default".
#[test]
fn f4_an_unset_key_ladders_from_one() {
    let rungs = configure::ladder(None, false);
    assert_eq!(rungs.first(), Some(&1));
    assert!(!rungs.is_empty());
}

/// A signed key ladders through zero into the negatives.
#[test]
fn f4_a_signed_key_reaches_below_zero() {
    let rungs = configure::ladder(Some(0), true);
    assert!(rungs.contains(&0));
    assert!(rungs.contains(&-1), "a signed ladder reaches below zero");
    assert!(rungs.contains(&1));
    // And an unsigned one never does.
    assert!(!configure::ladder(Some(0), false).iter().any(|r| *r < 0));
}

/// Zero is reachable on an unsigned key, because zero is a legal ceiling.
#[test]
fn f4_zero_is_a_rung() {
    assert!(configure::ladder(Some(1), false).contains(&0));
}
