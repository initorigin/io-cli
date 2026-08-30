//! F3, F12 and F13 — a value chosen on the surface, the file it lands in, and the
//! promise that no surface ever opens a bare composer again.
//!
//! **Every claim about what a written value means goes back through
//! `Config::discover`**, never through `edit::value_at`. Quoting back the bytes
//! just written proves only that the splice ran; the question is whether
//! io-harness reads what the surface displayed, and only io-harness can answer it.
//! That distinction is F3's own sabotage arm — writing an enum's *label* instead
//! of its serialized value passes a bytes-level check and fails here.
//!
//! Nothing here writes to the developer's own `~/.io-cli`. Every configuration
//! write goes to a temporary root, and the user file is written *outside* the
//! workspace, because a file inside it is project-scoped whatever variable names
//! it — a fact this repository paid two live runs for in 0.14.0.

use std::sync::{Mutex, MutexGuard};

use io_cli::configure::{self, Kind};
use io_harness::config::{Config, Scope};

/// `Config::discover` reads `IO_CONFIG` at call time, so two tests setting it at
/// once would each see the other's file.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

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

// F3 — a value chosen on the surface is the value io-harness reads back.

/// One key of each chooseable kind, written and re-read through the harness.
///
/// Sabotage: write the label instead of the serialized value — under which the
/// enum arm fails here while a bytes-level check would pass, because
/// `theme = dark` unquoted is not a TOML string and `Config::discover` refuses it.
#[test]
fn f3_a_chosen_value_is_what_the_harness_reads_back() {
    // (key, kind-provided value, what the harness must then say)
    let cases: Vec<(&str, &str)> = vec![
        ("app.io-cli.theme", "light"),
        ("policy.defaults.read", "deny"),
        ("sandbox.mode", "read-only"),
        ("run.max_steps", "20"),
        ("app.io-cli.plain", "true"),
    ];
    for (key, value) in cases {
        let s = scopes("", "", "");
        let kind = configure::kind_of(key).unwrap_or_else(|| panic!("{key} has no kind"));
        let spelled = configure::spell_value(&kind, value);
        let edit = io_cli::edit::Edit::set(key, spelled);
        configure::write(s.root.path(), Scope::Project, &[edit])
            .unwrap_or_else(|error| panic!("{key}: {error}"));

        // The harness's own discovery, never `value_at`.
        let config = s.config();
        let after = configure::setting(&config, key);
        assert_eq!(
            after.value.as_deref().map(|v| v.trim().trim_matches('"')),
            Some(value),
            "{key} read back as something other than what was chosen"
        );
    }
}

/// A number is written bare and a string is written quoted.
///
/// The two halves of the same sabotage: quoting a number makes `run.max_steps` a
/// string the schema refuses, and not quoting an enum makes it a bare token TOML
/// cannot parse.
#[test]
fn f3_a_value_is_spelled_the_way_its_kind_is_written() {
    assert_eq!(
        configure::spell_value(&Kind::Number { signed: false }, "20"),
        "20"
    );
    assert_eq!(configure::spell_value(&Kind::Flag, "true"), "true");
    assert_eq!(
        configure::spell_value(&Kind::Choice(vec!["dark".into()]), "dark"),
        "\"dark\""
    );
    // A list goes through the one renderer that already knows how.
    let rendered = configure::spell_value(&Kind::List, "cargo test --all");
    assert!(
        rendered.starts_with('[') && rendered.contains("\"cargo\""),
        "a list must be written as a TOML array: {rendered}"
    );
}

/// A list value round-trips as a list io-harness can read back.
///
/// `app.io-cli.gates.command` is `Option<Vec<String>>`; a scalar written there is
/// a value the harness cannot read at all, which is why it has its own kind.
#[test]
fn f3_the_command_list_reads_back_as_a_list() {
    let s = scopes("", "", "");
    let key = "app.io-cli.gates.command";
    let spelled = configure::spell_value(&Kind::List, "cargo test --all");
    configure::write(
        s.root.path(),
        Scope::Project,
        &[io_cli::edit::Edit::set(key, spelled)],
    )
    .expect("the list writes");

    let config = s.config();
    let (stored, _) = io_cli::settings::stored(&config);
    let command = stored
        .and_then(|s| s.gates)
        .and_then(|g| g.command)
        .expect("io-harness reads the command back as a list");
    assert_eq!(command, vec!["cargo", "test", "--all"]);
}

// F12 — no surface opens the composer with a bare key and no candidates.

/// Every catalogue key either offers its values or states its shape.
///
/// The criterion in one assertion. Sabotage: restore the old prefill for one key
/// — under which F12 fails by naming that key, because that key would resolve to a
/// typed kind with no shape sentence behind it.
#[test]
fn f12_every_key_offers_options_or_states_a_shape() {
    let s = scopes("", "", "");
    let config = s.config();
    let mut bare: Vec<&str> = Vec::new();
    for key in configure::CATALOGUE {
        let offers = matches!(
            configure::kind_of(key),
            Some(Kind::Flag)
                | Some(Kind::Choice(_))
                | Some(Kind::Number { .. })
                | Some(Kind::Model)
                | Some(Kind::File)
        );
        if !offers && configure::shape_of(key, &config).is_none() {
            bare.push(key);
        }
    }
    assert!(
        bare.is_empty(),
        "these keys would open a bare composer with no candidates and no stated shape: {bare:?}"
    );
}

/// A stated shape carries a worked example, not just a noun.
///
/// "a URL" is not a shape an operator can act on; "for example: https://…" is.
#[test]
fn f12_a_stated_shape_shows_an_example() {
    let s = scopes("", "", "");
    let config = s.config();
    for key in [
        "app.io-cli.gates.command",
        "app.io-cli.gates.contains",
        "app.io-cli.gates.rubric",
        "app.io-cli.prices.source_url",
    ] {
        let said =
            configure::shape_of(key, &config).unwrap_or_else(|| panic!("{key} states no shape"));
        assert!(
            said.contains("for example:"),
            "{key} states a shape with no worked example: {said}"
        );
    }
}

/// The machine-written key says so rather than offering to be typed, and points at
/// where the act actually is.
///
/// **The second assertion is 0.33.0's.** This sentence used to send an operator to
/// "the last row of `/config`", and since the refresh moved one descent below
/// `prices.as_of` that instruction names a row that is not there any more —
/// directions to a door that has been moved are worse than no directions.
#[test]
fn f12_the_machine_written_key_says_it_is_not_typed() {
    let s = scopes("", "", "");
    let config = s.config();
    let said = configure::shape_of("prices.as_of", &config).expect("it says something");
    assert!(
        said.contains("rather than typed"),
        "prices.as_of must say it is written by machinery: {said}"
    );
    assert!(
        !said.contains("last row"),
        "the refresh is no longer a row of the bare `/config` list, and this sends \
         the operator to it: {said}"
    );
    assert!(
        configure::descent(&config, "prices.as_of").is_some(),
        "the sentence describes a descent this key does not offer"
    );
}

// F13 — a write lands in the file that already decides the key.

/// A key the project file decides is written back into the project file.
///
/// Sabotage: always write the user scope — under which this fails, and the failure
/// is the change an operator is least able to see: a personal file silently
/// shadowing a committed one.
#[test]
fn f13_a_write_inherits_the_deciding_file() {
    let s = scopes(
        "[run]\nmax_steps = 10\n",
        "[app.io-cli]\ntheme = \"dark\"\n",
        "",
    );
    let config = s.config();
    assert_eq!(
        configure::destination(&config, "app.io-cli.theme"),
        (Scope::Project, true)
    );
    assert_eq!(
        configure::destination(&config, "run.max_steps"),
        (Scope::User, true)
    );
}

/// A key no file names goes to the operator's own file, and says it was not
/// inherited.
#[test]
fn f13_a_key_no_file_names_goes_to_the_user_scope() {
    let s = scopes("", "", "");
    assert_eq!(
        configure::destination(&s.config(), "app.io-cli.diff"),
        (Scope::User, false)
    );
}

/// The local file wins over the project file, because that is the precedence in
/// force — the destination follows what decided the key, not a fixed order.
#[test]
fn f13_the_deciding_file_is_the_one_actually_in_force() {
    let s = scopes(
        "",
        "[app.io-cli]\ntheme = \"dark\"\n",
        "[app.io-cli]\ntheme = \"light\"\n",
    );
    let config = s.config();
    assert_eq!(
        configure::destination(&config, "app.io-cli.theme"),
        (Scope::Local, true)
    );
}

// The cycling keys, and the property that makes them possible.

/// A boolean and a closed enum cycle; a number does not.
///
/// A number's ladder cannot be shown on the row it is changed on, and a held arrow
/// would write the file once per key repeat.
#[test]
fn n4_only_booleans_and_closed_enums_cycle_in_place() {
    assert_eq!(
        configure::cycled(&Kind::Flag, Some("false"), true).as_deref(),
        Some("true")
    );
    assert_eq!(
        configure::cycled(&Kind::Flag, Some("true"), true).as_deref(),
        Some("false"),
        "a closed set wraps rather than stopping"
    );
    let effects = Kind::Choice(configure::effects());
    assert_eq!(
        configure::cycled(&effects, Some("allow"), true).as_deref(),
        Some("ask")
    );
    assert_eq!(
        configure::cycled(&effects, Some("allow"), false).as_deref(),
        Some("deny"),
        "backwards from the first option wraps to the last"
    );
    for kind in [
        Kind::Number { signed: false },
        Kind::Model,
        Kind::File,
        Kind::List,
        Kind::Text,
        Kind::Machine,
    ] {
        assert_eq!(
            configure::cycled(&kind, Some("x"), true),
            None,
            "{kind:?} must not cycle in place"
        );
    }
}

/// An unset boolean turns on with one press of the forward key.
#[test]
fn n4_an_unset_flag_enters_the_list_at_its_far_end() {
    assert_eq!(
        configure::cycled(&Kind::Flag, None, true).as_deref(),
        Some("true"),
        "forward on an unset flag enters at the last option, which turns it on"
    );
    assert_eq!(
        configure::cycled(&Kind::Flag, None, false).as_deref(),
        Some("false"),
        "and backward enters at the first"
    );
}

/// **The property the whole binding rests on: the picker's filter keeps every
/// printable character, the space included.**
///
/// The owner's literal request was toggles on the spacebar, and it cannot be
/// built: `Picker` consumes every printable as a fuzzy filter, so a two-word query
/// would toggle a setting on its own space. This asserts the filter still owns the
/// space, which is what makes the arrows the right binding rather than a
/// preference.
///
/// Sabotage: bind the cycle to Space. Under it this fails, and it fails on the
/// query an operator types to find a key among thirty-seven.
#[test]
fn n4_the_picker_filter_still_owns_the_space() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use io_cli::picker::{Picker, Row};

    let mut picker = Picker::new(
        "Which setting?",
        vec![
            Row::new("app.io-cli.theme".to_string()),
            Row::new("run.max_steps".to_string()),
        ],
    );
    for character in "max steps".chars() {
        picker.key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
    }
    assert_eq!(
        picker.query(),
        "max steps",
        "the space must reach the query, or a two-word search is impossible"
    );

    // And the arrows the cycling uses are still free: neither moves the marker
    // nor chooses a row, so intercepting them takes nothing from any surface.
    let before = picker.selected();
    for code in [KeyCode::Left, KeyCode::Right] {
        assert!(matches!(
            picker.key(KeyEvent::new(code, KeyModifiers::NONE)),
            io_cli::picker::Outcome::Idle
        ));
    }
    assert_eq!(
        picker.selected(),
        before,
        "an arrow must not move the marker"
    );
}
