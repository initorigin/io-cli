//! F10 — the fetch spawns `git` and nothing else, and says so when it cannot.
//!
//! Four things are under test here and they fail for four different reasons, which
//! is the point of writing them apart: what an operator is allowed to name, what
//! that name becomes, what argv the spawn actually receives, and what each way of
//! failing leaves behind on the disk.
//!
//! **The two tests that spawn are deliberate, and they need no network.** A clone
//! from a local directory that is not a repository is a real non-zero exit with
//! real stderr; a clone from a directory `git init` just made is a real success
//! with a real tree to rename. Between them they exercise the whole of
//! `fetch::clone` — the spawn, the exit-code branch, the rename, and the removal
//! of the staging directory on both paths — without asking anything of a network
//! that a CI runner may not have. What they do ask for is `git` on the machine
//! running the suite, and they say so by name when it is missing rather than
//! quietly passing: a test that skips itself is a test that reports green for a
//! path nobody ran, and this product has shipped a vacuous gate in three separate
//! releases.
//!
//! **Nothing here mutates the process environment.** Every path a test needs is
//! passed in, because `fetch::clone` takes its destination and its staging
//! directory as arguments rather than reading the home — a decision that lived in
//! the driver would be a decision no test could reach.

use std::ffi::OsString;
use std::path::Path;

use io_cli::fetch::{self, Fetched, Named};

/// A name the tests use throughout, spelled once.
fn named() -> Named {
    Named {
        owner: "zeroonething".to_string(),
        repo: "ultraship".to_string(),
    }
}

/// Run a real `git` with `args`, and fail by name on a machine that has none.
///
/// Only the fixtures use this. The code under test builds its own argv, and a
/// helper that built it too would be a second answer to the question this file
/// exists to ask.
fn git(args: &[&str]) {
    let ran = std::process::Command::new("git")
        .args(args)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "this test needs `git` on PATH to build its fixture, and starting it \
                 failed: {error}. That is the assumption product.yaml:253-258 records \
                 about developer machines; it is not something this suite can work \
                 around."
            )
        });
    assert!(
        ran.status.success(),
        "the fixture command `git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&ran.stderr),
    );
}

/// **A marketplace is named by an owner and a repository, and by nothing else.**
///
/// Fails when the parse widens: a URL, a bare word, a third segment or an empty
/// half accepted here is a string reaching `git clone` that nobody wrote a rule
/// for. The two paste-shaped inputs — a trailing `/` and a trailing `.git` — are
/// asserted to land on the *same* `Named` as the plain spelling rather than merely
/// to be accepted, so a parse that kept `.git` on the repository name (and so put
/// it into the destination path) fails here rather than in a directory listing.
#[test]
fn f10_a_marketplace_is_named_by_an_owner_and_a_repository() {
    for spelling in [
        "zeroonething/ultraship",
        "  zeroonething/ultraship  ",
        "zeroonething/ultraship/",
        "zeroonething/ultraship.git",
        "zeroonething/ultraship.git/",
    ] {
        assert_eq!(
            fetch::resolve(spelling),
            Some(named()),
            "`{spelling}` should name the same marketplace as the plain spelling",
        );
    }

    for refused in [
        "",
        "   ",
        "ultraship",
        "/",
        "/ultraship",
        "zeroonething/",
        "zeroonething/ultraship/extra",
        "https://github.com/zeroonething/ultraship",
        "git@github.com:zeroonething/ultraship.git",
        "zeroonething ultraship",
        "zero onething/ultraship",
    ] {
        assert_eq!(
            fetch::resolve(refused),
            None,
            "`{refused}` is not an owner and a repository and must not be treated as \
             one — a URL in particular, because the only host this release can reach \
             is the one `fetch::url` writes",
        );
    }
}

/// **A name that could become an argument, or leave the directory, is refused.**
///
/// Separated from the shape test above because these are the refusals with a
/// consequence rather than a tidiness: a leading `-` is an option to any program
/// that parses options, and `..` is a path component that walks out of the
/// marketplaces directory and into the operator's home. Both are what a name is
/// for, if whoever chose the name chose it against you.
///
/// Fails the moment `segment` is loosened — including by the plausible-looking
/// edit of allowing "anything without a slash".
#[test]
fn f10_a_name_that_could_become_an_argument_or_leave_the_directory_is_refused() {
    for hostile in [
        "-upload-pack=touch;/ultraship",
        "zeroonething/-x",
        "../ultraship",
        "zeroonething/..",
        ".ssh/ultraship",
        "zeroonething/.ssh",
        "zeroonething/ultra..ship",
        "zeroonething/ultra ship",
        "zeroonething/ultra\\ship",
        "zeroonething/ultra;ship",
        "zeroonething/ultra$ship",
        "zeroonething/ultra\nship",
    ] {
        assert_eq!(
            fetch::resolve(hostile),
            None,
            "`{hostile}` must not resolve: it would become an option, a path \
             component outside the marketplaces directory, or both",
        );
    }
}

/// **The clone URL is built from the name and from nothing else.**
///
/// A whole-string equality rather than a `contains`, because every part of it is
/// load bearing: the scheme, the one host this release reaches, the order of the
/// two halves, and the `.git` that avoids a redirect. Fails if any of them moves.
#[test]
fn f10_the_clone_url_is_built_from_the_name_and_nothing_else() {
    assert_eq!(
        fetch::url(&named()),
        "https://github.com/zeroonething/ultraship.git",
    );
}

/// **The program is the literal `git` and the argv is five owned elements.**
///
/// The centre of F10. The URL and the destination go in as a `&str` and a `&Path`
/// and are asserted to come out as *whole elements, byte for byte* — so a version
/// that quoted them, escaped them, joined them with a space, or built a single
/// command line out of them fails here, and fails whichever of those it did.
///
/// The destination deliberately holds a space and a quote. A path like that is
/// ordinary on macOS and Windows, it is exactly what a shell would take apart, and
/// asserting on a tidy path would be asserting that nothing had gone wrong in the
/// only case where something could.
#[test]
fn f10_the_program_is_the_literal_git_and_the_argv_is_five_owned_elements() {
    assert_eq!(fetch::PROGRAM, "git");

    let url = fetch::url(&named());
    let into = Path::new("/tmp/an operator's home/marketplaces/zeroonething/ultraship");
    let argv = fetch::argv(&url, into);

    assert_eq!(
        argv,
        vec![
            OsString::from("clone"),
            OsString::from("--depth"),
            OsString::from("1"),
            OsString::from("https://github.com/zeroonething/ultraship.git"),
            OsString::from("/tmp/an operator's home/marketplaces/zeroonething/ultraship"),
        ],
        "the argv is `clone --depth 1 <url> <dir>`, five elements, each one whole",
    );

    // Said again as two direct identities, because the vector comparison above
    // would also be satisfied by a rewrite that happened to produce the same
    // bytes, and what is being asserted is that these two inputs are *passed
    // through*.
    assert_eq!(argv[3], OsString::from(url));
    assert_eq!(argv[4], OsString::from(into));

    // And no element is a pair. A `--depth 1` collapsed into one element, or a URL
    // and a directory joined into a command line, would still be four or five
    // elements on some other spelling — this is the assertion that says none of
    // them contains a second argument.
    assert!(
        !argv.iter().any(|element| element
            .to_string_lossy()
            .contains("clone --depth")),
        "two arguments have been joined into one element: {argv:?}",
    );
}

/// **A marketplace lands two levels under the home, and the repository keeps its
/// own name.**
///
/// Fails on the two plausible layouts that are wrong: flattening `<owner>/<repo>`
/// into one directory name, and carrying the `.git` suffix into the path. Both
/// would work for a single fetch and both break the listing that walks the tree.
#[test]
fn f10_a_marketplace_lands_two_levels_under_the_home() {
    let root = io_cli::home::marketplaces()
        .expect("a home directory; every platform this ships on sets one");
    assert_eq!(
        fetch::at(&named()),
        Some(root.join("zeroonething").join("ultraship")),
    );
}

/// **An existing marketplace is never cloned over, and no clone is even started.**
///
/// The destination may hold a marketplace an operator has already installed
/// plugins from, with a `[[plugin]] path` in their configuration pointing inside
/// it. So the assertion is not only the outcome: the marker file has to survive,
/// and the staging directory must never have been created — which is what says the
/// existence check happens *before* the spawn rather than after it.
#[test]
fn f10_an_existing_marketplace_is_not_cloned_over() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let into = home.path().join("marketplaces/zeroonething/ultraship");
    let staging = home.path().join(".fetching/1");
    std::fs::create_dir_all(&into).expect("the destination");
    let marker = into.join("plugin.toml");
    std::fs::write(&marker, b"name = \"ultraship\"\n").expect("the marker");

    let fetched = fetch::clone(&fetch::url(&named()), &into, &staging);

    assert_eq!(fetched, Fetched::Already(into.clone()));
    assert_eq!(
        std::fs::read(&marker).expect("the marker survives"),
        b"name = \"ultraship\"\n",
        "the existing marketplace was written over",
    );
    assert!(
        !staging.exists(),
        "a clone was started for a destination that already existed",
    );
    assert!(
        fetched
            .sentence()
            .expect("an outcome the operator is told about")
            .contains("already"),
        "the operator is told the marketplace is already here",
    );
}

/// **A `git` that is not on `PATH` is a sentence, and every other failure is
/// not.**
///
/// Both halves matter. Without the first, a machine with no git gets an
/// `io::Error`'s own words — `No such file or directory (os error 2)` — which
/// names neither git nor what to do. Without the second, an operator whose git is
/// present but not executable is told to install the thing they already have:
/// mapping every spawn error to "no git" is the plausible simplification, and this
/// is the assertion that refuses it.
///
/// Driven through the mapping rather than by emptying `PATH`, because a test
/// process that mutates its own environment mutates it for every other test in the
/// binary.
#[test]
fn f10_a_git_that_is_not_on_path_is_a_sentence_and_every_other_failure_is_not() {
    let missing = Fetched::unstartable(&std::io::Error::from(std::io::ErrorKind::NotFound));
    assert_eq!(missing, Fetched::NoGit);

    let said = missing.sentence().expect("a sentence, not a stack trace");
    assert!(
        said.contains("git") && said.contains("PATH"),
        "the sentence names neither git nor where it was looked for: {said}",
    );
    assert!(
        said.contains("directory"),
        "the sentence should say that installing a plugin from a directory still \
         works, or a machine without git reads as a machine io-cli does not run \
         on: {said}",
    );

    let refused = Fetched::unstartable(&std::io::Error::from(std::io::ErrorKind::PermissionDenied));
    assert_ne!(
        refused,
        Fetched::NoGit,
        "a git that cannot be executed is not a git that is not there, and telling \
         that operator to install git is telling them to do what they have done",
    );
    assert!(
        refused
            .sentence()
            .expect("a sentence")
            .contains("permission"),
        "the platform's own words are more use here than any sentence written in \
         this crate",
    );
}

/// **A clone that fails leaves nothing at the destination and no staging
/// directory, and carries git's own stderr.**
///
/// A real spawn against a directory that is not a repository: a real non-zero
/// exit, real stderr, no network. What it asserts is the non-functional criterion
/// in full — *a failure at any point leaves no half-cloned directory presented as
/// a marketplace* — plus the reason reaching the operator, which is git's sentence
/// and not a paraphrase of it.
#[test]
fn f10_a_clone_that_fails_leaves_nothing_at_the_destination_and_no_staging() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let absent = home.path().join("not-a-repository");
    let into = home.path().join("marketplaces/zeroonething/ultraship");
    let staging = home.path().join(".fetching/1");

    let fetched = fetch::clone(&absent.to_string_lossy(), &into, &staging);

    let Fetched::Failed { status, stderr } = &fetched else {
        panic!(
            "cloning a path that is not a repository should fail, and be seen to: \
             {fetched:?}. A `NoGit` here means this machine has no `git`, which the \
             fixture helper in this file explains."
        );
    };
    assert_ne!(*status, Some(0), "a failure with a successful exit code");
    assert!(
        !stderr.trim().is_empty(),
        "git's own reason was dropped, so the operator is told only that something \
         went wrong",
    );
    assert!(
        !into.exists(),
        "a failed clone left something at the destination, where the listing would \
         count it as a marketplace",
    );
    assert!(!staging.exists(), "the staging directory was left behind");
    assert!(
        fetched
            .sentence()
            .expect("a sentence")
            .starts_with("git could not clone that"),
        "the failure is reported in io-cli's voice with git's reason inside it",
    );
}

/// **A clone that works is renamed into place, and the staging directory goes.**
///
/// The success path, end to end, against a repository `git init` made a moment
/// ago. `git clone` of an empty repository exits zero and writes a real `.git`, so
/// this exercises the spawn, the rename and the cleanup without a network and
/// without a commit — and it is the only test that can catch a rename that never
/// happened, which would otherwise show up as a marketplace nobody could find.
#[test]
fn f10_a_clone_that_works_is_renamed_into_place_and_the_staging_goes() {
    let home = tempfile::tempdir().expect("a temporary directory");
    let source = home.path().join("source");
    std::fs::create_dir_all(&source).expect("the source directory");
    git(&["init", "--quiet", &source.to_string_lossy()]);

    let into = home.path().join("marketplaces/zeroonething/ultraship");
    let staging = home.path().join(".fetching/1");

    let fetched = fetch::clone(&source.to_string_lossy(), &into, &staging);

    assert_eq!(
        fetched,
        Fetched::Cloned(into.clone()),
        "a clone that git completed should be reported as one",
    );
    assert!(
        into.join(".git").is_dir(),
        "the clone is not at the destination, so the rename did not happen",
    );
    assert!(
        !staging.exists(),
        "the staging directory survived a successful fetch",
    );
    assert_eq!(
        fetched.sentence(),
        None,
        "a fetch that worked has nothing to say; the surface that asked for it says \
         what it now has",
    );
}
