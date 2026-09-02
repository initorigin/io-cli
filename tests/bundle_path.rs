//! F9 and F10 — where a bundle's declared program goes, and what happens to a
//! bundle that declares none.
//!
//! **io-harness places nothing.** It parses a `[[bin]]`, refuses one declared from
//! a scope that may not contribute an executing thing, validates the path
//! lexically, and offers the result on an accessor it never calls itself — its own
//! contract says outright that where a contributed binary goes is the host's
//! decision. So every assertion here is about io-cli's answer to a question
//! io-harness deliberately does not answer.
//!
//! The three functions that decide anything are pure and take what they read, so
//! the order, the deduplication and the appending are assertable without touching
//! a process environment. `install` is the one line that is not, and it is one
//! line for that reason.

mod support;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use io_cli::bundle_path;
use io_harness::PLUGIN_FILE;

/// The entries this fixture's own bundles contributed.
///
/// **Filtered to the temporary root, which is the only thing this test put
/// anything in.** `support::user_scope` points `IO_CONFIG` at a file of its own,
/// so the operator's real one is out of the way for the duration — but the filter
/// stays, because `Config::discover` still reads two more candidates and an
/// assertion of equality against the whole list is the shape that goes green on a
/// clean runner and red on a developer's machine.
fn ours(plugins: &io_harness::Plugins, root: &Path) -> Vec<PathBuf> {
    bundle_path::entries(plugins)
        .into_iter()
        .filter(|dir| dir.starts_with(root))
        .collect()
}

/// A bundle declaring one executable whose declared name is the file's own name.
const SHIPS_A_PROGRAM: &str = r#"
name = "ultraship"
description = "Ships things."

[[bin]]
name = "ultraship"
path = "bin/ultraship"
"#;

/// And one whose declared name is not what the file answers to.
///
/// The case the chosen mechanism cannot honour: appending a directory puts a file
/// on `PATH` under the name it already has, so a declaration that renames its
/// program does not resolve. Reported rather than made true by writing a link,
/// which would be io-cli installing a program.
const RENAMES_ITS_PROGRAM: &str = r#"
name = "caveman"
description = "Renames things."

[[bin]]
name = "caveman"
path = "bin/cm.mjs"
"#;

/// Two programs in one directory, so the deduplication has something to do.
///
/// Named `zebra` and declared first, so the sort has something to do as well: an
/// unsorted `entries` returns it before `alpha` and the test says so.
const ZEBRA_TWO_PROGRAMS: &str = r#"
name = "zebra"
description = "Ships two programs from one directory."

[[bin]]
name = "zebra"
path = "bin/zebra"

[[bin]]
name = "zebra-too"
path = "bin/zebra-too"
"#;

/// A bundle contributing something, and nothing that runs a program.
const NO_PROGRAM: &str = r#"
name = "quiet"
description = "Contributes no program."
skills = "skills"
"#;

fn bundle(root: &Path, at: &str, manifest: &str) -> PathBuf {
    let dir = root.join(at);
    std::fs::create_dir_all(dir.join("skills")).expect("the skills directory");
    std::fs::create_dir_all(dir.join("bin")).expect("the bin directory");
    std::fs::write(dir.join(PLUGIN_FILE), manifest).expect("the manifest");
    dir
}

/// The plugins a configuration declaring each of `bundles` loads.
///
/// **`root` is a directory outside every workspace, and the declaration is made
/// from the user scope. Neither half is a detail.** Since io-harness 0.74.0 a
/// `plugin.toml` sitting *inside* the discovery root may not contribute a
/// `[[bin]]` at all — it names a program this machine would run, and a file in the
/// workspace root arrives with a `git clone` or is written by the run's own agent —
/// and the refusal drops the whole bundle. Trust follows where the manifest is
/// rather than which file named it, so both halves are required: a user-scope
/// `[[plugin]]` pointing back *into* the workspace is graded as the workspace file
/// it is, and a workspace file pointing *out* has its entry dropped before the
/// manifest is read. What is left is the arrangement an installed bundle actually
/// has, which is the one this surface exists for.
///
/// A fixture that got either half wrong asserts over an empty vector while
/// passing, which is what the drop check below exists to catch.
///
/// The path is spelled as a TOML *literal* string: it is absolute, and an absolute
/// Windows path is a run of backslashes a basic string would read as escapes.
/// `tempfile` never puts a `'` in a directory name.
fn loaded(root: &Path, bundles: &[(&str, &str)]) -> io_harness::Plugins {
    let text: String = bundles
        .iter()
        .map(|(at, manifest)| {
            let dir = bundle(root, at, manifest);
            format!("[[plugin]]\npath = '{}'\n\n", dir.display())
        })
        .collect();
    let plugins = support::user_scope(&text).config.plugins();
    // **A dropped bundle contributes nothing and looks exactly like a bundle that
    // declared nothing**, so a fixture that silently drops asserts nothing while
    // passing. io-harness drops a bundle *whole* for a manifest it refuses — an
    // executable declared from a scope that may not contribute one, or a path it
    // will not accept — and this is where that becomes a readable failure rather
    // than an empty vector.
    let ours: Vec<(PathBuf, String)> = plugins
        .dropped()
        .iter()
        .filter(|d| d.path.starts_with(root))
        .map(|d| (d.path.clone(), d.error.to_string()))
        .collect();
    assert!(
        ours.is_empty(),
        "a fixture bundle was dropped, so this test would assert over nothing: {ours:?}",
    );
    plugins
}

/// **F9 — a declared program's own directory is what goes on the path.**
#[test]
fn f9_a_declared_program_puts_its_own_directory_on_the_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let plugins = loaded(root, &[("ultraship", SHIPS_A_PROGRAM)]);
    assert_eq!(
        ours(&plugins, root),
        vec![root.join("ultraship").join("bin")],
        "the entry is the directory the declaring file sits in, so the program \
         is found under the name it already has",
    );
    assert!(
        !bundle_path::mismatched(&plugins)
            .iter()
            .any(|(declared, _)| declared == "ultraship"),
        "the declared name is the file's own name, so there is nothing to report",
    );
}

/// **A bundle switched off contributes no program, and nothing asserted this.**
///
/// Found by the adversarial review. A disabled bundle comes back from io-harness
/// as a fully parsed `Plugin` on `disabled()`, contributing nothing — and putting
/// its programs on `PATH` would be the one contribution it still made. Changing
/// `entries` to `iter().chain(disabled())` left the whole suite green before this
/// test existed.
#[test]
fn a_switched_off_bundle_puts_no_program_on_the_path() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let installed = bundle(root, "ultraship", SHIPS_A_PROGRAM);
    // Outside the workspace and declared from the user scope, for `loaded`'s
    // reason: a `[[bin]]` in a manifest inside the discovery root is refused, and a
    // bundle dropped for that is indistinguishable here from one switched off.
    let plugins = support::user_scope(&format!(
        "[[plugin]]\npath = '{}'\nenabled = false\n",
        installed.display()
    ))
    .config
    .plugins();
    assert!(
        !plugins.disabled().is_empty(),
        "the fixture must actually declare a switched-off bundle, or this test \
         asserts over nothing",
    );
    assert!(
        ours(&plugins, root).is_empty(),
        "a switched-off bundle's program reached the path: {:?}",
        ours(&plugins, root),
    );
    assert!(
        !bundle_path::mismatched(&plugins)
            .iter()
            .any(|(declared, _)| declared == "ultraship"),
        "a switched-off bundle was reported on",
    );
}

/// **Two bundles, two directories: deduplicated and in a stable order.**
///
/// `entries` is where the sort and the dedup live, and the test that named them
/// drove `appended` instead — so a plain `Vec` with no sort passed. A `PATH` that
/// reorders itself between sessions makes a collision intermittent, which is the
/// worst way for one to present.
#[test]
fn f9_entries_are_deduplicated_and_ordered_across_bundles() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let plugins = loaded(
        root,
        &[("zebra", ZEBRA_TWO_PROGRAMS), ("alpha", SHIPS_A_PROGRAM)],
    );
    assert_eq!(
        ours(&plugins, root),
        vec![
            root.join("alpha").join("bin"),
            root.join("zebra").join("bin")
        ],
        "two programs in one directory are one entry, and the order is the same \
         on every run rather than the manifest's",
    );
}

/// **F10 — and a bundle declaring no program contributes no entry.**
#[test]
fn f10_a_bundle_declaring_no_program_changes_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let plugins = loaded(root, &[("quiet", NO_PROGRAM)]);
    assert!(
        ours(&plugins, root).is_empty(),
        "a bundle that declares no executable must not touch the path",
    );
}

/// **A declaration that renames its program is reported, not staged.**
///
/// The honest failure. Appending a directory cannot honour a declared name that
/// differs from its file, and writing a link under the name a stranger chose is
/// the thing this product's contract excludes. So the operator is told, at
/// startup, rather than discovering it when the model reports that a command does
/// not exist.
#[test]
fn a_declared_name_that_is_not_the_files_name_is_reported() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    let plugins = loaded(root, &[("caveman", RENAMES_ITS_PROGRAM)]);
    assert!(
        bundle_path::mismatched(&plugins).contains(&("caveman".to_string(), "cm.mjs".to_string())),
        "the declared name and the name the file actually answers to must both \
         be reported: {:?}",
        bundle_path::mismatched(&plugins),
    );
    // The directory still goes on the path: the program is reachable, under the
    // name it has. Reporting the difference is not a reason to withhold it.
    assert_eq!(ours(&plugins, root), vec![root.join("caveman").join("bin")]);
}

/// **Appended, never prepended — the whole security argument in one assertion.**
///
/// `PATH` resolves by first match, so a prepended entry would let anything an
/// operator installed answer to `git`, `cargo` or `ls` for every tool call in the
/// process, including the ones io-harness makes on the model's behalf — whose
/// permission gate matches a *binary name* and would be satisfied by the wrong
/// program under the right name.
#[test]
fn f10_entries_are_appended_so_a_bundle_cannot_shadow_a_system_command() {
    // **Joined with the platform's own separator, not a literal `:`.** Windows
    // splits on `;`, so a hand-spelled unix `PATH` arrives there as one entry
    // whose name contains a colon and the assertion fails for a reason that has
    // nothing to do with the order being tested. CI caught exactly that; the
    // production code was right and the fixture was not.
    let existing = std::env::join_paths([PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
        .expect("two ordinary directories join");
    let added = PathBuf::from("/home/someone/.io-cli/plugins/x/bin");
    let joined = bundle_path::appended(Some(&existing), std::slice::from_ref(&added))
        .expect("an entry was added");
    let order: Vec<PathBuf> = std::env::split_paths(&joined).collect();
    assert_eq!(
        order,
        vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin"), added],
        "a bundle's directory goes last, so a collision resolves to the system \
         command and never to the stranger's program",
    );
}

/// **And appending is idempotent, because the inventory is re-read every turn.**
///
/// A `PATH` that grows once per turn boundary is a variable that eventually stops
/// fitting in an environment block, and the growth would be invisible until it
/// did.
#[test]
fn appending_the_same_entry_twice_adds_it_once() {
    let added = PathBuf::from("/opt/x/bin");
    let first = bundle_path::appended(
        Some(&OsString::from("/usr/bin")),
        std::slice::from_ref(&added),
    )
    .expect("the first append adds it");
    assert!(
        bundle_path::appended(Some(&first), std::slice::from_ref(&added)).is_none(),
        "an entry already on the path is not appended again, and `None` says so \
         rather than rewriting the variable with the same value",
    );
}

/// Two programs in one directory are one entry, and the order does not wander.
///
/// A path that reorders itself between sessions makes a collision intermittent,
/// which is the worst way for one to present.
#[test]
fn two_programs_in_one_directory_are_one_entry() {
    let dirs = vec![PathBuf::from("/a/bin"), PathBuf::from("/a/bin")];
    let joined = bundle_path::appended(None, &dirs).expect("something is added to an empty path");
    assert_eq!(
        std::env::split_paths(&joined).collect::<Vec<_>>(),
        vec![PathBuf::from("/a/bin")],
    );
}
