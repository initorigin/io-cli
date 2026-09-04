//! 0.38.0 F6, F7 — `io upgrade` names the command for the way the binary was
//! installed, and the argv door really opens.
//!
//! The classification is a free function over a path, so every case is reachable
//! here without installing anything. That is the point of its shape: a branch
//! only a Homebrew install could reach is a branch no test on this machine could
//! ever drive, and `src/main.rs` links from no test binary at all.
//!
//! The last test spawns the real binary. Nothing under `tests/` links
//! `src/main.rs`, so a subcommand can be declared, implemented, documented and
//! still answer `unrecognized subcommand` — which is exactly what 0.30.0 shipped
//! for `io skill` behind 1,609 green tests.

use io_cli::upgrade::{self, Installed};
use std::path::PathBuf;

/// F6 — the path decides, and the table is written for what it must NOT claim.
///
/// The three negative rows carry the weight. `/usr/local/bin/io` is where a
/// hand-copied binary lands, `target/release/io` is a build, and
/// `scoop-notes/bin/io` exists to fail a classifier written with `contains`
/// instead of a component comparison — the same widening a substring match makes
/// of `tests/dependencies.rs`'s permitted-path set.
#[test]
fn f6_a_path_decides_which_installer_placed_the_binary() {
    let table = [
        ("/opt/homebrew/Cellar/io/0.38.0/bin/io", Installed::Homebrew),
        ("/opt/homebrew/bin/io", Installed::Homebrew),
        ("/usr/local/Cellar/io/0.38.0/bin/io", Installed::Homebrew),
        ("/home/me/.linuxbrew/bin/io", Installed::Homebrew),
        ("/c/Users/me/scoop/apps/io/current/io.exe", Installed::Scoop),
        ("/srv/scoop/apps/io/0.38.0/io.exe", Installed::Scoop),
        ("/home/me/.local/bin/io", Installed::Installer),
        (
            "/c/Users/me/AppData/Local/io/bin/io.exe",
            Installed::Installer,
        ),
        ("/usr/local/bin/io", Installed::Unknown),
        ("/home/me/src/io-cli/target/release/io", Installed::Unknown),
        ("/home/me/scoop-notes/bin/io", Installed::Unknown),
    ];
    for (path, want) in table {
        assert_eq!(
            upgrade::installed(&PathBuf::from(path)),
            want,
            "{path} should be classified {want:?}"
        );
    }
}

/// F6 — the command is the first line and stands alone on it.
///
/// An operator copies the first line. A line carrying prose beside the command
/// is a line that does not run when pasted, so this asserts the shape rather
/// than the wording.
#[test]
fn f6_the_command_is_the_first_line_and_nothing_else_is_on_it() {
    for path in [
        "/opt/homebrew/Cellar/io/0.38.0/bin/io",
        "/srv/scoop/apps/io/0.38.0/io.exe",
        "/home/me/.local/bin/io",
        "/usr/local/bin/io",
    ] {
        let lines = upgrade::advice(&PathBuf::from(path), false);
        let first = &lines[0];
        assert!(
            [
                upgrade::HOMEBREW,
                upgrade::SCOOP,
                upgrade::INSTALLER_UNIX
            ]
            .contains(&first.as_str()),
            "{path} led with {first:?}, which is not one of the three commands"
        );
        assert_eq!(lines[1], "", "the command should stand alone on its line");
    }
}

/// F6 — an unrecognised path is named as a guess, and names itself.
///
/// A guess presented as an instruction is how an operator ends up with two
/// installs of one binary, one of which never updates again.
#[test]
fn f6_an_unrecognised_path_is_named_as_a_guess() {
    let lines = upgrade::advice(&PathBuf::from("/usr/local/bin/io"), false);
    let said = lines.join("\n");
    assert!(said.contains("this is a guess"), "{said}");
    assert!(
        said.contains("/usr/local/bin/io"),
        "the operator is not told which path was not recognised: {said}"
    );
}

/// F6 — the installer line follows the platform.
///
/// The wrong one is not merely unhelpful: it names an interpreter the operator
/// does not have.
#[test]
fn f6_the_installer_line_follows_the_platform() {
    let path = PathBuf::from("/home/me/.local/bin/io");
    assert_eq!(upgrade::advice(&path, false)[0], upgrade::INSTALLER_UNIX);
    assert_eq!(upgrade::advice(&path, true)[0], upgrade::INSTALLER_WINDOWS);
}

/// F6 — no arm claims a package manager placed a binary it did not.
///
/// The complement of the table above, asserted over the rendered text rather
/// than over the enum: a classifier that answered `Unknown` correctly and then
/// printed "installed by Homebrew" would pass every test above this one.
///
/// **The needle is the claim and not the word**, and the first draft of this
/// test had it the other way round and failed on correct output. The
/// unrecognised arm names Homebrew and scoop deliberately — it says the path is
/// *not* where either of them puts a binary, which is the sentence that stops an
/// operator running the wrong updater. Banning the vendor's name would forbid
/// the module from explaining itself, which is the same shape as a prose gate
/// that forbids a file from naming what it does not do.
#[test]
fn f6_no_advice_claims_a_manager_that_did_not_place_it() {
    for path in [
        "/usr/local/bin/io",
        "/home/me/src/io-cli/target/release/io",
        "/home/me/.local/bin/io",
    ] {
        let said = upgrade::advice(&PathBuf::from(path), false).join("\n");
        for claim in ["installed by Homebrew", "installed by scoop"] {
            assert!(
                !said.contains(claim),
                "{path} was told it was {claim}: {said}"
            );
        }
    }
    // And the control, so the two negations above cannot pass vacuously: the
    // arms that ARE a package manager say so in exactly those words.
    let brew = upgrade::advice(&PathBuf::from("/opt/homebrew/bin/io"), false).join("\n");
    assert!(brew.contains("installed by Homebrew"), "{brew}");
    let scoop = upgrade::advice(&PathBuf::from("/srv/scoop/apps/io/1/io.exe"), false).join("\n");
    assert!(scoop.contains("installed by scoop"), "{scoop}");
}

/// F7 — the argv door opens, on the real binary.
///
/// **This one spawns the binary and it has to.** `Subcommand::Upgrade` is
/// matched in `src/main.rs`, which no integration test links, so every assertion
/// above this line passes whether or not clap routes the word. `io skill` was
/// documented, implemented and shipped in 0.30.0 with no clap variant, and the
/// whole suite was green over a door that answered `unrecognized subcommand`.
///
/// The command needs no configuration, no store, no provider and no terminal, so
/// it is safe to run here with nothing set up — which is itself the property
/// being asserted by the exit status.
#[test]
fn f7_the_argv_door_prints_a_command_and_exits_zero() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_io"))
        .arg("upgrade")
        .output()
        .expect("the built binary runs");

    assert!(
        out.status.success(),
        "`io upgrade` exited {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout.lines().next().unwrap_or_default();
    assert!(
        [
            upgrade::HOMEBREW,
            upgrade::SCOOP,
            upgrade::INSTALLER_UNIX,
            upgrade::INSTALLER_WINDOWS
        ]
        .contains(&first),
        "`io upgrade` led with {first:?}, which is not one of the commands"
    );
}

/// N4 — the command reaches no network, and the module says so rather than
/// being trusted to.
///
/// A source-text gate over `src/upgrade.rs`, normalised at the read because git
/// hands Windows a CRLF working copy and a gate that does not normalise fails
/// there for a reason unrelated to its subject — twice in this repository
/// already. `tests/dependencies.rs` bans the network types across all of `src/`;
/// this adds the spellings a version check would arrive as.
#[test]
fn n4_the_upgrade_module_asks_nobody_what_the_latest_version_is() {
    let source = std::fs::read_to_string("src/upgrade.rs")
        .expect("this crate's source is readable")
        .replace("\r\n", "\n");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for needle in [
        "releases/latest",
        "api.github.com",
        "Command::new",
        "reqwest",
        "TcpStream",
    ] {
        assert!(
            !code.contains(needle),
            "`src/upgrade.rs` names {needle:?}; this command reads a path and prints"
        );
    }
}
