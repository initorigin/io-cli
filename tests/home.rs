//! `~/.io-cli`: one directory, and an existing install moved into it safely.
//!
//! Every test here writes the process environment, which is shared by the whole
//! binary, so every one of them takes the lock below. That is the same rule
//! `tests/wizard.rs` and `tests/contract.rs` already follow — and it matters more
//! here, because `home::adopt` is the first function in this crate's library that
//! *sets* a variable rather than reading one.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use io_cli::home::{self, Origin};
use io_harness::Store;

/// Held by every test in this file. See the module note.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Point the operator's home at `dir` and clear every variable that could decide
/// the answer instead.
///
/// `XDG_CONFIG_HOME` has to go on unix or the platform's own place is wherever the
/// developer running the suite keeps theirs, and the fixture would migrate their
/// real configuration file. Same reason `APPDATA` is redirected on Windows.
fn fresh(dir: &Path) {
    std::env::remove_var(io_harness::config::CONFIG_VAR);
    std::env::remove_var(io_harness::config::CONFIG_HOME_VAR);
    #[cfg(windows)]
    {
        std::env::set_var("USERPROFILE", dir);
        std::env::set_var("APPDATA", dir.join("AppData").join("Roaming"));
    }
    #[cfg(not(windows))]
    {
        std::env::set_var("HOME", dir);
        std::env::remove_var("XDG_CONFIG_HOME");
    }
}

/// Where io-harness would have put the file before this release, under `dir`.
fn platform(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        dir.join("AppData").join("Roaming").join("io")
    }
    #[cfg(not(windows))]
    {
        dir.join(".config").join("io")
    }
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
    std::fs::write(path, contents).expect("the file");
}

/// **F1.** The home is the operator's own directory, on every platform.
///
/// The Windows arm is the one that cannot be checked on the machine this was
/// written on, and it is the one with a wrong answer available: io-harness reads
/// `%APPDATA%` there, which is a fourth platform-specific location rather than the
/// single path this release exists to give.
#[test]
fn f1_the_home_is_one_path_under_the_operators_own_directory() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    assert_eq!(home::path(), Some(dir.path().join(".io-cli")));

    #[cfg(windows)]
    {
        let roaming = dir.path().join("AppData").join("Roaming");
        assert!(
            !home::path().expect("a home").starts_with(&roaming),
            "the Windows home is the profile root, not %APPDATA% — \
             a path under Roaming is the platform-specific answer this replaces"
        );
    }
}

/// **F1.** No home directory means no home, rather than a path invented from
/// nothing and written into.
#[test]
fn f1_no_home_directory_means_no_home() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    #[cfg(windows)]
    std::env::remove_var("USERPROFILE");
    #[cfg(not(windows))]
    std::env::remove_var("HOME");

    assert_eq!(home::path(), None);
    assert_eq!(home::adopt(), None);

    // Left as the other tests expect to find it.
    fresh(dir.path());
}

/// **F2.** An operator who named a location keeps it, and is not told about a
/// migration that did not happen to them.
#[test]
fn f2_an_operator_who_has_chosen_is_not_moved() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");

    for (var, value) in [
        (io_harness::config::CONFIG_HOME_VAR, dir.path().join("mine")),
        (
            io_harness::config::CONFIG_VAR,
            dir.path().join("mine").join("io.toml"),
        ),
    ] {
        fresh(dir.path());
        let chosen = platform(dir.path()).join("io.toml");
        write(&chosen, "# the file that must not move\n");
        std::env::set_var(var, &value);

        assert_eq!(
            home::adopt(),
            None,
            "{var} names a location; nothing is adopted"
        );
        assert!(
            chosen.exists(),
            "{var} was set, so nothing may have been moved"
        );
        assert!(
            !dir.path().join(".io-cli").exists(),
            "{var} was set, so io-cli's own home is not even created"
        );
        assert_eq!(
            io_harness::config::user_path(),
            Some(match var {
                v if v == io_harness::config::CONFIG_VAR => value.clone(),
                _ => value.join("io.toml"),
            }),
            "{var} still decides where the file is"
        );
    }
}

/// **F2.** An empty variable is not a choice.
///
/// io-harness's `env_dir` ignores one set to nothing, and io-cli reading it the
/// other way would leave a session with neither a user scope nor a home — the
/// worst of both answers.
#[test]
fn f2_an_empty_variable_is_not_a_choice() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());
    std::env::set_var(io_harness::config::CONFIG_HOME_VAR, "");

    assert_eq!(home::origin(), Origin::Default);
    let report = home::adopt().expect("an empty variable is unset, so the home is adopted");
    assert_eq!(report.home, dir.path().join(".io-cli"));
}

/// **F3.** After adoption the configuration file and the run store are both in
/// the home — the store because `settings::store_path` derives it from the file's
/// own directory, which is why naming one names both.
///
/// `settings::store_path` has had no test since it was written: its only callers
/// are `src/main.rs` and `src/exec.rs`, and neither links from here.
#[test]
fn f3_the_file_and_the_store_are_both_in_the_home() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    let report = home::adopt().expect("neither variable is set, so the home is adopted");
    let home = dir.path().join(".io-cli");

    assert_eq!(report.home, home);
    assert_eq!(io_harness::config::user_path(), Some(home.join("io.toml")));
    assert_eq!(io_cli::settings::store_path(), Some(home.join("runs.db")));
    assert_eq!(home::in_force(), Some((home.clone(), Origin::Default)));
    // `Skills::discover` ERRORS on a directory that does not exist rather than
    // walking away from one, and `discover_skills` propagates that at run start —
    // so a skills default pointing at a directory nobody made is every turn
    // failing, not an empty catalogue. Adoption makes the place real.
    assert!(
        home.join("skills").is_dir(),
        "the skills directory is made with the home, so the default is a real place"
    );
}

/// **F5's other half.** A home that cannot be created is not adopted, and the
/// variable is not set behind the operator's back.
///
/// Setting `IO_CONFIG_HOME` and then failing would move the configuration path
/// with nobody told: `adopt` would return `None`, so nothing is reported, while
/// io-harness had already been pointed at a directory that does not exist.
#[test]
fn f5_a_home_that_cannot_be_created_is_not_adopted() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    // A file where the home's parent would have to be a directory.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, "not a directory").expect("the file");
    fresh(&blocked);

    assert_eq!(
        home::adopt(),
        None,
        "the home could not be created, so none was taken"
    );
    assert_eq!(
        std::env::var_os(io_harness::config::CONFIG_HOME_VAR),
        None,
        "and io-harness was not pointed at a directory that does not exist"
    );

    fresh(dir.path());
}

/// **F8's premise, and a defect this test found.** The variable io-cli sets is not
/// the operator having chosen.
///
/// `adopt` puts `IO_CONFIG_HOME` in the environment, so a status row that read the
/// raw variable afterwards would name the operator as the one who decided — which
/// is wrong in the ordinary case and wrong in exactly the direction that hides
/// io-cli's own default behind somebody else's name. The rule is that a variable
/// pointing at io-cli's home reads as `default`; one pointing anywhere else is the
/// operator's.
#[test]
fn f8_the_variable_io_cli_sets_is_not_the_operators_choice() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    assert_eq!(home::origin(), Origin::Default, "before anything is set");
    home::adopt().expect("the home is adopted");
    assert_eq!(
        home::origin(),
        Origin::Default,
        "io-cli set the variable, so io-cli is still what decided the directory"
    );

    std::env::set_var(
        io_harness::config::CONFIG_HOME_VAR,
        dir.path().join("elsewhere"),
    );
    assert_eq!(
        home::origin(),
        Origin::ConfigHome,
        "a variable naming somewhere else is the operator's own choice"
    );
}

/// **F4.** An existing install moves, and the store's write-ahead log moves with
/// it.
///
/// The claim is the run, not the file. A `runs.db` moved without its `-wal`
/// opens without complaint and simply does not contain the last session, so a
/// test counting files would pass over exactly the defect that matters. The
/// connection is deliberately still open across the move, which is what leaves an
/// uncheckpointed `-wal` on disk to be left behind.
#[test]
fn f4_an_existing_install_moves_with_its_write_ahead_log() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    let previous = platform(dir.path());
    write(&previous.join("io.toml"), "# the operator's own file\n");

    let store = Store::open(previous.join("runs.db")).expect("a store");
    let run = store
        .start_run("a goal recorded before the move", "io.toml")
        .expect("a run");
    assert!(
        previous.join("runs.db-wal").exists(),
        "the store is opened in WAL mode, so an uncheckpointed write leaves one"
    );

    let report = home::adopt().expect("the home is adopted");
    drop(store);

    let home = dir.path().join(".io-cli");
    assert_eq!(
        std::fs::read_to_string(home.join("io.toml")).expect("the moved file"),
        "# the operator's own file\n",
        "the configuration file arrives byte for byte"
    );
    let moved = Store::open(home.join("runs.db")).expect("the moved store");
    assert!(
        moved.runs().expect("the runs").contains(&run),
        "the run written before the move is readable after it — which is what a \
         store moved without its write-ahead log silently loses"
    );
    assert!(
        report
            .moved
            .iter()
            .any(|(from, _)| from.ends_with("runs.db-wal")),
        "the report names the write-ahead log, so an operator can see it moved"
    );
    assert!(
        !previous.join("io.toml").exists(),
        "a move leaves nothing behind"
    );
}

/// **F5.** Nothing already in the home is overwritten, and the operator is told
/// which file is the one in force.
#[test]
fn f5_nothing_in_the_home_is_ever_overwritten() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    let previous = platform(dir.path()).join("io.toml");
    write(&previous, "# older\n");
    let home = dir.path().join(".io-cli");
    write(&home.join("io.toml"), "# newer\n");

    let report = home::adopt().expect("the home is adopted");

    assert_eq!(
        std::fs::read_to_string(home.join("io.toml")).expect("the file in force"),
        "# newer\n",
        "the file the operator already had in the home is the one that survives"
    );
    assert!(
        previous.exists(),
        "and the older one is left where it was, not deleted"
    );
    assert!(report.moved.is_empty());
    assert_eq!(report.kept.len(), 1);
    assert!(report.lines().iter().any(|line| line.contains("kept")));
}

/// **F9.** The migration is idempotent, and a source that has already gone —
/// because a second `io` started at the same moment — is not a failure.
#[test]
fn f9_running_twice_moves_nothing_the_second_time() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());
    write(&platform(dir.path()).join("io.toml"), "# once\n");

    let first = home::adopt().expect("the home is adopted");
    assert_eq!(first.moved.len(), 1);

    // The variable the first adoption set is what a second process would find, so
    // clear it to reproduce a genuinely second first-run rather than an opt-out.
    std::env::remove_var(io_harness::config::CONFIG_HOME_VAR);
    let second = home::adopt().expect("the home is adopted again");

    assert!(second.moved.is_empty(), "there is nothing left to move");
    assert!(
        second.kept.is_empty(),
        "and nothing to keep, because the source is gone"
    );
    assert_eq!(
        second.lines().len(),
        1,
        "a run that moved nothing reports the home and nothing else"
    );
    assert!(second.lines()[0].contains(".io-cli"));
}

/// **N3.** The home belongs to the operator alone. A credential sits in the file
/// inside it, which `settings::write` already writes `0600`.
#[cfg(unix)]
#[test]
fn n3_the_home_is_readable_by_its_owner_alone() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    let report = home::adopt().expect("the home is adopted");
    let mode = std::fs::metadata(&report.home)
        .expect("the home")
        .permissions()
        .mode();

    assert_eq!(
        mode & 0o777,
        0o700,
        "the directory around a credential is not world-readable"
    );
}

/// **F7's half of the bargain.** A leading tilde is the operator's home directory;
/// io-harness substitutes `${env:}` and `${file:}` and nothing else, so without
/// this a `~` reaches `Skills::discover` as a directory literally named `~`.
#[test]
fn a_tilde_is_the_home_directory_and_nothing_else_is_touched() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    fresh(dir.path());

    assert_eq!(
        home::expand(Path::new("~/.io-cli/skills")),
        dir.path().join(".io-cli").join("skills")
    );
    let absolute = dir.path().join("elsewhere");
    assert_eq!(
        home::expand(&absolute),
        absolute,
        "a real path is returned unchanged"
    );
    assert_eq!(
        home::expand(Path::new("skills")),
        PathBuf::from("skills"),
        "a relative path with no tilde is not rooted anywhere by this function"
    );
}

/// The origin word is io-harness's own spelling of the variable, so the two can
/// never disagree about what an operator has to type.
#[test]
fn the_origin_word_is_the_variable_the_harness_reads() {
    assert_eq!(Origin::Default.word(), "default");
    assert_eq!(Origin::Config.word(), io_harness::config::CONFIG_VAR);
    assert_eq!(
        Origin::ConfigHome.word(),
        io_harness::config::CONFIG_HOME_VAR
    );
}
