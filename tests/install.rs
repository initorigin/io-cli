//! F1 and O2 — the skills io-cli ships reach the contract the run is built from,
//! and an install that cannot write does not take the session down with it.
//!
//! **What this file can prove and what it cannot.** io-harness composes the
//! prompt catalogue itself — `with_skill_catalog` and the composer around it are
//! private, and `PromptComposed` carries a byte count rather than the text — so
//! the deterministic half of F1 is that the contract the turn is built from names
//! a directory in which `Skills::discover` finds exactly the five, which is the
//! input the catalogue is composed from and the only thing io-cli decides. The
//! other half, that the five names are on the wire, is a live claim and belongs
//! to the pty capture, which reads it off the real `CompletionRequest`.
//!
//! **The wiring itself is in `src/main.rs` and nothing under `tests/` links the
//! binary**, so the ordering F1's sabotage names — installing after the contract
//! is built rather than before — cannot be driven from here. What is asserted
//! instead is the property that ordering exists to produce: after `install`, the
//! contract built from that home carries a directory whose discovery names all
//! five. A driver that installed too late would leave the first session's
//! directory empty, and `f1_the_contract_a_turn_is_built_from_names_all_five`
//! would then be describing a state the product never reaches.

mod support;

use std::path::{Path, PathBuf};
use std::sync::MutexGuard;

/// `HOME` and `USERPROFILE` are process-wide and every test here rewrites them.
///
/// **Delegated to [`support::env_lock`] rather than owning a `Mutex` of its own.**
/// Two different mutexes in one binary exclude nothing from each other, so a file
/// that kept a private lock would let an `IO_CONFIG` fixture and a `HOME` fixture
/// run at the same time and each see the other's environment. The name and the
/// signature stay, because every call site here reads correctly either way; only
/// the lock behind them changes.
fn env_lock() -> MutexGuard<'static, ()> {
    support::env_lock()
}

/// A home of this test's own, put back when it drops.
struct HomeFixture {
    dir: tempfile::TempDir,
    previous: [(&'static str, Option<std::ffi::OsString>); 4],
}

impl HomeFixture {
    /// **The fixture names the home in force as well as the operator's home,
    /// because that is what `io` itself does.** `contract::default_skills` anchors
    /// on `home::in_force` since 0.31.0 — a skill is authored content and belongs
    /// wherever the operator put the rest of what they wrote — and in the binary
    /// `home::adopt` runs immediately above the one `Config::discover` either arm
    /// reaches and sets `IO_CONFIG_HOME` to `~/.io-cli`.
    ///
    /// A fixture that set only `HOME` would therefore be asserting about a state
    /// the product never reaches, and would inherit whatever another test file had
    /// last pointed the variable at — which is exactly how this test failed on the
    /// first full-suite run after that change, with `configured` answering `None`
    /// for a directory it had just installed five skills into.
    ///
    /// `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`, so an
    /// inherited one has to go. Every variable touched here is restored on drop.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a home directory");
        let home = dir.path().to_path_buf();
        let previous = ["HOME", "USERPROFILE", "IO_CONFIG_HOME", "IO_CONFIG"]
            .map(|var| (var, std::env::var_os(var)));
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
        std::env::set_var("IO_CONFIG_HOME", home.join(".io-cli"));
        std::env::remove_var("IO_CONFIG");
        Self { dir, previous }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join(".io-cli")
    }
}

impl Drop for HomeFixture {
    fn drop(&mut self) {
        for (var, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(var, value),
                None => std::env::remove_var(var),
            }
        }
    }
}

/// A configuration discovered from an empty workspace, which is what a first run
/// of a new install actually has.
fn discovered() -> (tempfile::TempDir, io_harness::Config) {
    let dir = tempfile::tempdir().expect("a workspace");
    let config = io_harness::Config::discover(dir.path()).expect("an empty workspace discovers");
    (dir, config)
}

fn names(dir: &Path) -> Vec<String> {
    io_harness::Skills::discover(dir)
        .expect("the installed directory discovers")
        .iter()
        .map(|skill| skill.name.clone())
        .collect()
}

/// **F1 — the contract a turn is built from names all five.**
///
/// Both arms, because `contract::configured` is the half `io exec` and a session
/// share and a skill that reached only the terminal would be the 0.14.0 asymmetry
/// this product deleted once already.
#[test]
fn f1_the_contract_a_turn_is_built_from_names_all_five() {
    let _guard = env_lock();
    let fixture = HomeFixture::new();
    let home = fixture.home();

    let report = io_cli::skills::install(&home);
    assert!(
        report.iter().any(|line| line.contains("installed 5")),
        "a first install into an empty home says what it did: {report:?}",
    );

    let dir = io_cli::skills::dir(&home);
    let mut found = names(&dir);
    found.sort();
    let mut shipped: Vec<String> = io_cli::skills::SHIPPED
        .iter()
        .map(|skill| skill.name.to_string())
        .collect();
    shipped.sort();
    assert_eq!(
        found, shipped,
        "the directory the contract will name does not resolve to the five io-cli ships",
    );

    let (workspace, config) = discovered();
    let headless = io_cli::contract::configured(
        "read the notes",
        workspace.path().to_path_buf(),
        &config,
        &config.plugins(),
    );
    assert_eq!(
        headless.skills.as_deref(),
        Some(dir.as_path()),
        "`io exec` is built from a contract that names the installed directory",
    );
}

/// **F1's other half, stated as the absence it really is.**
///
/// A name is offered to the model because a file resolving to it is in the
/// directory the contract names. Assert that relationship rather than a count:
/// a count is satisfied by five files of any kind, and what the catalogue carries
/// is names.
#[test]
fn f1_every_shipped_name_is_resolvable_in_the_directory_the_contract_names() {
    let _guard = env_lock();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    io_cli::skills::install(&home);

    let found = names(&io_cli::skills::dir(&home));
    for skill in &io_cli::skills::SHIPPED {
        assert!(
            found.iter().any(|name| name == skill.name),
            "`{}` is shipped and is not resolvable in the installed directory: {found:?}",
            skill.name,
        );
    }
}

/// **O2 — an install that cannot write is reported and never fatal.**
///
/// The sabotage the criterion names is propagating the error out of `run`. It
/// cannot be written as a type error, because `install` returns no `Result` at
/// all — which is the point: the signature is what makes the criterion structural
/// rather than a promise about a call site. What is asserted here is the
/// behaviour that signature buys — a home that cannot be written into yields
/// report lines and a return, and the caller has nothing to propagate.
#[test]
#[cfg(unix)]
fn o2_a_home_that_cannot_be_written_into_is_reported_and_not_fatal() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = env_lock();
    let fixture = HomeFixture::new();
    let home = fixture.home();
    std::fs::create_dir_all(&home).expect("the home");
    let mode = std::fs::metadata(&home)
        .expect("the home is there")
        .permissions();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500))
        .expect("a read-only home");

    let report = io_cli::skills::install(&home);

    // Restored before any assertion, so a failing assertion cannot leave a
    // directory the temporary-directory cleanup is unable to remove.
    std::fs::set_permissions(&home, mode).expect("the mode goes back");

    assert!(
        !report.is_empty(),
        "a home that could not be written into said nothing at all",
    );
    assert!(
        report.iter().any(|line| line.contains(
            io_cli::skills::dir(&home)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("skills")
        )),
        "the report does not name what it could not write: {report:?}",
    );
}

/// **Running it twice changes nothing, which is what makes it safe on every
/// start.** The installer is on the startup path, so it runs on every launch and
/// not once — an install that was not idempotent would rewrite five files, and
/// therefore five mtimes, every time the product opened.
#[test]
fn installing_twice_writes_nothing_the_second_time() {
    let _guard = env_lock();
    let fixture = HomeFixture::new();
    let home = fixture.home();

    io_cli::skills::install(&home);
    let before: Vec<Vec<u8>> = io_cli::skills::SHIPPED
        .iter()
        .map(|skill| {
            std::fs::read(io_cli::skills::dir(&home).join(format!("{}.md", skill.name)))
                .expect("an installed skill")
        })
        .collect();

    let again = io_cli::skills::install(&home);
    assert!(
        again.is_empty(),
        "a second install had something to say about a directory it did not change: {again:?}",
    );

    let after: Vec<Vec<u8>> = io_cli::skills::SHIPPED
        .iter()
        .map(|skill| {
            std::fs::read(io_cli::skills::dir(&home).join(format!("{}.md", skill.name)))
                .expect("an installed skill")
        })
        .collect();
    assert_eq!(before, after, "a second install rewrote the files");
}
