//! F3 — `@` completes paths from the workspace, through the harness, under the
//! session's own policy.
//!
//! Three claims, and they are asserted separately because they fail separately.
//!
//! **The root is the session's**, never the process directory. `io -C <dir>` sets
//! one without changing the other, and that exact defect shipped in 0.3.0 —
//! passing every test, because every fixture handed the code an absolute
//! temporary directory, which is a shape the real input does not have. So the
//! fixtures here include a **relative** root, and one of them lists the
//! directory this file is sitting in while asserting that the directory above it
//! is not what came back.
//!
//! **A path the posture denies is never offered.** That is the product's thesis
//! on one screen, and it is asserted against a policy that denies a file which is
//! really there — with the same fixture read under a permissive policy as the
//! control, so an empty result can never pass for a refusal.
//!
//! **The walk is the harness's and it is one level deep.** Nothing here builds a
//! listing, and no test asks for a tree: what a directory offers is
//! `Workspace::list_dir`, and what the level below offers is the same call again.
//! The number of rows is io-cli's own bound, which the harness deliberately does
//! not impose, and it is asserted at the boundary.
//!
//! The driver in `src/main.rs` has no test that can reach it, so every decision
//! it makes about completion is a library function called here:
//! [`complete::opens`] is the condition it branches on, [`complete::entries`] is
//! the walk it runs, [`complete::rows`] is what the picker is given and
//! [`complete::pick`] is what a chosen row stands for.

use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use io_harness::tools::EntryKind;
use io_harness::Policy;

use io_cli::app::App;
use io_cli::approval;
use io_cli::complete::{self, Picked, MAX_ENTRIES};
use io_cli::settings::Posture;
use io_cli::theme::DARK;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn at() -> KeyEvent {
    key(KeyCode::Char('@'))
}

/// Everything readable, nothing denied — the control every policy assertion is
/// measured against.
fn permissive() -> Policy {
    Policy::permissive()
}

/// The labels a listing puts in front of the operator.
fn labels(entries: &[io_harness::tools::Entry]) -> Vec<String> {
    complete::rows(entries)
        .into_iter()
        .map(|row| row.label)
        .collect()
}

/// A workspace with one file at the root, one directory, one file inside it, and
/// one file the guarded policy below refuses to read.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(dir.path().join("notes.md"), "what to do\n").expect("a file");
    std::fs::write(dir.path().join("secret.txt"), "the key\n").expect("a file");
    std::fs::create_dir(dir.path().join("src")).expect("a directory");
    std::fs::write(dir.path().join("src/app.rs"), "fn main() {}\n").expect("a file");
    std::fs::create_dir(dir.path().join("vault")).expect("a directory");
    std::fs::write(dir.path().join("vault/key.txt"), "-----\n").expect("a file");
    dir
}

/// The session's policy in a read-only posture, over a workspace where one file
/// and one directory's contents are denied outright.
///
/// It is built the way the driver builds it — the file's policy, with the posture
/// `Shift+Tab` chose folded in by [`approval::session_policy`] — rather than
/// hand-assembled, so what this asserts is the policy a turn would actually run
/// under and not a lookalike.
fn guarded() -> Policy {
    let file = Policy::default()
        .layer("io.toml")
        .allow_read("*")
        .deny_read("secret.txt")
        .deny_read("vault/*");
    approval::session_policy(&file, Some(Posture::ReadOnly), &[])
}

/// **F3.** `@` at a word boundary opens completion, and nowhere else.
///
/// The rule is the palette's, moved one step: `/` is special only at an empty
/// prompt, and `@` is special at an empty prompt or after whitespace. Anything
/// tighter would put the completion out of reach of the sentence it belongs in —
/// "read @src/app.rs and say what is wrong" is the whole use — and anything
/// looser takes the keyboard away in the middle of an address.
#[test]
fn f3_at_opens_completion_only_at_a_word_boundary() {
    // An empty prompt, and after a space: both are the start of a word.
    assert!(complete::opens(at(), "", false));
    assert!(complete::opens(at(), "read ", false));
    // A newline is whitespace too, and a prompt that has just wrapped is still at
    // the start of a word.
    assert!(complete::opens(at(), "read\n", false));

    // Mid-word it is a letter, which is the whole of the address case.
    assert!(!complete::opens(at(), "you", false));
    assert!(!complete::opens(at(), "mail me at you", false));
    assert!(!complete::opens(at(), "sha256", false));

    // A chord is a command somebody meant, not a letter they typed.
    assert!(!complete::opens(
        KeyEvent::new(KeyCode::Char('@'), KeyModifiers::CONTROL),
        "",
        false,
    ));
    assert!(!complete::opens(
        KeyEvent::new(KeyCode::Char('@'), KeyModifiers::ALT),
        "",
        false,
    ));

    // Armed, the `@` falls through to the session so the half-pressed chord is
    // cleared by the key that reaches it — the palette's rule, and for the same
    // reason: the one sequence this product ships changes files on its second
    // press.
    assert!(!complete::opens(at(), "", true));

    // Completion is reached from `@` and from nothing else. No other key opens
    // it, which is also why it can never be what the session starts on.
    for code in [
        KeyCode::Char('/'),
        KeyCode::Char('a'),
        KeyCode::Enter,
        KeyCode::Esc,
        KeyCode::Tab,
    ] {
        assert!(
            !complete::opens(key(code), "", false),
            "{code:?} must not open completion",
        );
    }
}

/// **F3.** The `@` the rule declines is an ordinary character, all the way into
/// the prompt.
///
/// Declining is only half of not hijacking the keyboard. The other half is that
/// the keystroke still arrives, so an address types as itself — asserted through
/// `App::key`, which is what the driver falls through to.
#[test]
fn f3_an_at_inside_a_word_still_reaches_the_composer() {
    let mut app = App::new(DARK, "opus-5");
    for c in "mail you".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    // The driver would have asked first; at this point the prompt ends in a
    // letter, so the answer is no and the key goes to the session.
    assert!(!complete::opens(at(), &app.composer.text(), app.armed()));
    app.key(at());
    for c in "example.com".chars() {
        app.key(key(KeyCode::Char(c)));
    }
    assert_eq!(app.composer.text(), "mail you@example.com");
}

/// **F3.** The listing is rooted where the session is, not where the process is.
///
/// The root here is **relative** — `tests`, resolved against the process
/// directory — which is the shape `io -C tests` produces and the shape 0.3.0's
/// four fixtures did not have. Resolving against the process directory instead of
/// the session root returns the directory above this one, which holds
/// `Cargo.toml` and does not hold this file.
#[test]
fn f3_the_listing_is_rooted_at_the_session_root_and_not_the_process_directory() {
    // Stated rather than assumed: cargo runs an integration test from the package
    // root, and everything below reads off that.
    let here = std::env::current_dir().expect("a working directory");
    assert!(
        here.join("Cargo.toml").is_file(),
        "this test reads the package root as the process directory: {here:?}",
    );

    let (found, _) = complete::entries(Path::new("tests"), &permissive(), "").expect("a listing");
    let names = labels(&found);
    assert!(
        names.iter().any(|name| name == "complete.rs"),
        "the session root's own contents are what is offered: {names:?}",
    );
    assert!(
        names.iter().any(|name| name == "support/"),
        "a directory of the session root is offered as one: {names:?}",
    );
    assert!(
        !names.iter().any(|name| name == "Cargo.toml"),
        "the process directory's contents must never be offered: {names:?}",
    );
}

/// **F3.** An absolute root is rooted there too, and what comes back is relative
/// to it.
///
/// The two halves are one claim: a listing whose paths were absolute would put
/// the operator's temporary directory in the prompt, and a prompt that names a
/// path the agent cannot resolve is worse than no completion at all.
#[test]
fn f3_entries_are_relative_to_the_root_they_were_listed_from() {
    let dir = workspace();
    let (found, _) = complete::entries(dir.path(), &permissive(), "").expect("a listing");
    for entry in &found {
        assert!(
            !Path::new(&entry.path).is_absolute(),
            "a listing is relative to the root: {:?}",
            entry.path,
        );
    }

    // And one level down, the path carries the directory it came from — which is
    // what makes a descent join nothing.
    let (inside, _) = complete::entries(dir.path(), &permissive(), "src").expect("a listing");
    assert_eq!(
        inside.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
        ["src/app.rs"],
    );
}

/// **F3, the centrepiece.** A path the posture denies is never offered.
///
/// The control is the same fixture under a permissive policy: `secret.txt` and
/// `vault/key.txt` are really on disk and really readable, so their absence under
/// [`guarded`] is the policy and cannot be an empty directory, a missing file or
/// a walk that failed quietly.
///
/// Nothing in io-cli filters this. The refusal is `Workspace::list_dir`'s, which
/// drops a denied entry before it returns, and that is the reason the completion
/// goes through the harness rather than through a directory walk of its own.
#[test]
fn f3_a_path_the_posture_denies_is_never_offered() {
    let dir = workspace();

    // The control: everything is there, and everything is offered.
    let (open, _) = complete::entries(dir.path(), &permissive(), "").expect("a listing");
    let open = labels(&open);
    assert!(
        open.contains(&"secret.txt".to_string()) && open.contains(&"vault/".to_string()),
        "the fixture must really hold what the policy is about to deny: {open:?}",
    );

    // Under the session's own policy, the denied file is gone and the readable
    // one is not.
    let (found, _) = complete::entries(dir.path(), &guarded(), "").expect("a listing");
    let names = labels(&found);
    assert!(
        names.contains(&"notes.md".to_string()),
        "a readable file is still offered: {names:?}",
    );
    assert!(
        !names.contains(&"secret.txt".to_string()),
        "a denied path was offered: {names:?}",
    );

    // And a denied directory's contents are gone a level down, where the denial
    // is the only thing standing between the operator and the file: the walk
    // reaches `vault`, and comes back with nothing in it.
    let (inside, _) = complete::entries(dir.path(), &guarded(), "vault").expect("a listing");
    assert!(
        inside.is_empty(),
        "the contents of a denied directory were offered: {:?}",
        labels(&inside),
    );
    // The same directory, permissively, is not empty — so the emptiness above is
    // the policy's and not the fixture's.
    let (inside, _) = complete::entries(dir.path(), &permissive(), "vault").expect("a listing");
    assert_eq!(labels(&inside), ["key.txt"]);
}

/// **F3.** A row is a name, and what it stands for is a path.
///
/// The label is the entry's last component for the reason
/// `commands::palette` strips the leading slash: every entry of `src` begins
/// `src/`, so a whole path as a label gives every row the same prefix, and no
/// query the operator can type is ever an exact name or a prefix of one. What is
/// chosen is read back out of the entries, never off the label, which is why the
/// trim costs nothing.
#[test]
fn f3_rows_are_names_and_a_choice_is_a_path() {
    let dir = workspace();
    let (found, _) = complete::entries(dir.path(), &permissive(), "src").expect("a listing");
    assert_eq!(labels(&found), ["app.rs"]);
    assert_eq!(
        complete::pick(&found, 0),
        Some(Picked::Insert("src/app.rs".to_string())),
        "the path is what goes in the prompt, relative to the session root",
    );

    let (root, _) = complete::entries(dir.path(), &permissive(), "").expect("a listing");
    let directory = root
        .iter()
        .position(|entry| entry.kind == EntryKind::Dir && entry.path == "src")
        .expect("the fixture's directory");
    assert_eq!(
        complete::pick(&root, directory),
        Some(Picked::Descend("src".to_string())),
        "a directory descends rather than being typed into the prompt",
    );
    assert_eq!(labels(&root)[directory], "src/");

    // Past the end is the row saying the list was cut. It stands for nothing.
    assert_eq!(complete::pick(&root, root.len()), None);
}

/// **F3.** The number of rows is bounded by io-cli, and the cut is said.
///
/// The harness bounds no listing and should not — a model reading a directory
/// wants all of it — so the bound belongs to the surface that puts one in front
/// of a person. A silently truncated listing would read as *the file is not
/// there*, which is exactly what a denial looks like, so the note is what keeps
/// the two apart.
#[test]
fn f3_the_row_count_is_bounded_and_a_cut_listing_says_so() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for n in 0..MAX_ENTRIES + 5 {
        std::fs::write(dir.path().join(format!("file-{n:04}.txt")), "x").expect("a file");
    }

    let (found, cut) = complete::entries(dir.path(), &permissive(), "").expect("a listing");
    assert_eq!(found.len(), MAX_ENTRIES);
    assert!(cut, "a directory larger than the bound reports the cut");
    assert_eq!(complete::rows(&found).len(), MAX_ENTRIES);
    // The note counts what is on screen, never the constant.
    let note = complete::cut_note(cut, MAX_ENTRIES).expect("a note");
    assert!(
        note.contains(&MAX_ENTRIES.to_string()),
        "the note names what was shown: {note:?}",
    );

    // Exactly at the bound is not a cut, and nothing is said.
    let dir = tempfile::tempdir().expect("a temporary directory");
    for n in 0..MAX_ENTRIES {
        std::fs::write(dir.path().join(format!("file-{n:04}.txt")), "x").expect("a file");
    }
    let (found, cut) = complete::entries(dir.path(), &permissive(), "").expect("a listing");
    assert_eq!(found.len(), MAX_ENTRIES);
    assert!(!cut);
    assert_eq!(complete::cut_note(cut, found.len()), None);
}

/// **F3.** The title says which directory is on screen.
///
/// It is load-bearing rather than decorative: the rows are last components, so
/// `app.rs` under `src` and `app.rs` under `tests` draw identically, and the
/// title is the only thing that tells the operator which descent they are in.
#[test]
fn f3_the_title_names_the_directory_being_listed() {
    assert_eq!(complete::title(""), "Which path?");
    assert!(
        complete::title("src").contains("src"),
        "a descent says where it is: {:?}",
        complete::title("src"),
    );
}

/// **F3.** A directory that cannot be listed is an error naming the path, never
/// an empty listing.
///
/// Empty and refused look the same on screen and mean opposite things, so the
/// seam keeps them apart — with io-harness's own sentence, which already names
/// the path the operator has to go and look at.
#[test]
fn f3_a_directory_that_cannot_be_listed_says_which_one() {
    let dir = workspace();
    let error = complete::entries(dir.path(), &permissive(), "nowhere").expect_err("a refusal");
    assert!(
        error.contains("nowhere"),
        "the failure names the path: {error:?}",
    );
    // And a path that climbs out of the workspace is refused by the harness
    // rather than resolved — the completion cannot be used to look outside the
    // root the session was given.
    assert!(complete::entries(dir.path(), &permissive(), "../..").is_err());
}
