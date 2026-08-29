//! The operator memory writer: three scopes, three files, and never a byte lost.
//!
//! **F4 is the second half and it is where "never a byte lost" gets teeth.**
//! Through 0.29.0 the only verb was an append, and an append cannot damage what is
//! already in the file. `memory::amend` and `memory::forget` can: they rewrite a
//! file somebody else's prose lives in. Every F4 test below therefore asserts the
//! **whole file, byte for byte**, against an expectation built by editing the
//! original string — never `contains`, which passes for an implementation that
//! kept the line it was asked about and normalised everything around it.
//!
//! The named sabotage is one line of plausible code: read the file, split it into
//! lines, change one, write them back joined. It passes every `contains` in this
//! file and fails the three fixtures below that have something to normalise — a
//! last line with no newline after it, a `\r\n` checkout, and a bullet somebody
//! indented and wrote with a `*`.
//!
//! Every test in this file writes `IO_CONFIG_HOME`, because that is the only way
//! to move [`memory::path`]'s answer for [`Scope::User`] without touching the
//! machine the suite is running on — `home::in_force` reads the environment at
//! call time, deliberately, so that a `/status` typed an hour into a session
//! answers about the directory in force rather than one cached at startup.
//!
//! The environment is process-wide and this binary's tests share a process, so
//! every one of them takes the lock below. That is the shape `tests/home.rs`,
//! `tests/wizard.rs`, `tests/contract.rs` and `tests/docs.rs` already use, and it
//! is why this file invents nothing: an existing pattern that serialises the
//! writers is worth more than a clever one that avoids them.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use io_cli::memory;
use io_harness::config::Scope;

/// Held by every test in this file. See the module note.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The three, in one place, so a test that means "all of them" cannot quietly
/// mean two.
const SCOPES: [Scope; 3] = [Scope::User, Scope::Project, Scope::Local];

/// Point io's home at `home` and clear the variable that would win over it.
///
/// `IO_CONFIG` names a file outright and beats `IO_CONFIG_HOME`, so a developer
/// who has one exported would otherwise have this suite writing `IO.md` next to
/// their own configuration file.
fn home_at(home: &Path) {
    std::env::remove_var(io_harness::config::CONFIG_VAR);
    std::env::set_var(io_harness::config::CONFIG_HOME_VAR, home);
}

/// A workspace root and an io home, in one temporary directory that cleans
/// itself up.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("the workspace");
    home_at(&dir.path().join("home"));
    (dir, root)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn at(root: &Path, scope: Scope) -> PathBuf {
    memory::path(root, scope).expect("every scope has a path once a home is named")
}

/// The three names, and the two roots they sit under.
#[test]
fn each_scope_names_its_own_file_beside_the_configuration_it_belongs_to() {
    let _guard = env_lock();
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path().join("workspace");
    let home = dir.path().join("home");
    home_at(&home);

    assert_eq!(memory::file_name(Scope::User), "IO.md");
    assert_eq!(memory::file_name(Scope::Project), "AGENTS.md");
    assert_eq!(memory::file_name(Scope::Local), "AGENTS.local.md");

    assert_eq!(
        at(&root, Scope::User),
        home.join("IO.md"),
        "the user file sits beside the io.toml in force, not in the workspace",
    );
    assert_eq!(at(&root, Scope::Project), root.join("AGENTS.md"));
    assert_eq!(at(&root, Scope::Local), root.join("AGENTS.local.md"));
}

/// **The sabotage arm.** A line remembered in one scope lands in that scope's
/// file and in no other.
///
/// This is the criterion. An implementation that ignored the scope and wrote the
/// committed `AGENTS.md` every time would satisfy every other test in this file
/// — the bullet is there, the bytes are preserved, the order is right — and
/// would put a private note into a pull request the first time an operator typed
/// one. So each scope is written in a fixture of its own and the other two paths
/// are asserted absent, which is the only assertion that can tell the two
/// implementations apart.
#[test]
fn a_line_lands_in_the_scope_it_was_written_for_and_nowhere_else() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();

        let written = memory::remember(&root, scope, "prefer small diffs").expect("the line lands");
        assert_eq!(written, at(&root, scope), "remember answers with the file");
        assert!(
            read(&written).contains("- prefer small diffs"),
            "{written:?} does not contain the line it was given",
        );

        for other in SCOPES.into_iter().filter(|s| *s != scope) {
            let path = at(&root, other);
            assert!(
                !path.exists(),
                "a line remembered for {scope:?} created {} — the scope decides the \
                 file, and writing the committed one whatever was asked is how a \
                 private note reaches a pull request",
                path.display(),
            );
        }
    }
}

/// A file that did not exist is created saying what it is and, above all, who
/// else will read it.
#[test]
fn an_absent_file_is_created_with_a_header_that_says_whether_it_is_committed() {
    let _guard = env_lock();

    // The phrase each header has to carry. Not the whole sentence — that is
    // prose and may be reworded — but the fact an operator has to know before
    // typing into the file, which may not be dropped.
    for (scope, said) in [
        (Scope::User, "every project"),
        (Scope::Project, "shared with everyone who clones"),
        (Scope::Local, "not committed"),
    ] {
        let (_dir, root) = fixture();

        let written = memory::remember(&root, scope, "a first line").expect("the line lands");
        let text = read(&written);

        assert!(
            text.starts_with(&format!("# {}\n", memory::file_name(scope))),
            "{scope:?}'s new file does not name itself:\n{text}",
        );
        assert!(
            text.contains(said),
            "{scope:?}'s header does not say `{said}` — whether a guidance file is \
             committed is the whole difference between the three, and the moment \
             the file is made is the only time this module gets to say it:\n{text}",
        );
        assert!(
            text.ends_with("- a first line\n"),
            "the header comes before the first bullet, and the file ends with it:\n{text}",
        );
    }
}

/// Every byte that was there is still there, in the order it was in.
///
/// The file is somebody's prose — a person wrote it, and another agent may have
/// written into it. A release that rewrote one rather than appending would eat
/// notes nobody has another copy of, and would do it silently.
#[test]
fn every_byte_already_in_the_file_is_still_there() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = at(&root, scope);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");

        // Deliberately not the shape this module writes: a heading it did not
        // author, a bullet with different spacing, an indented block, a trailing
        // blank line. Anything that normalises rather than appends loses one of
        // these.
        let before = "# notes somebody wrote by hand\n\n  * an old bullet\n\n\
                      ```\n  a fenced block\n```\n\n";
        std::fs::write(&path, before).expect("the existing file");

        memory::remember(&root, scope, "and one more").expect("the line lands");

        let after = read(&path);
        assert!(
            after.starts_with(before),
            "{scope:?} did not append — the original bytes are no longer a prefix \
             of the file:\n{after}",
        );
        assert_eq!(
            after,
            format!("{before}- and one more\n"),
            "and nothing but the bullet was added",
        );
    }
}

/// A file whose last byte is not a newline does not have its last line joined to
/// the new one.
///
/// `remember to run the linter` with no trailing newline, appended to naively,
/// becomes `remember to run the linter- and the formatter`: one instruction
/// turned into a different one, in a file that reaches the model on every run.
#[test]
fn a_file_that_does_not_end_in_a_newline_is_not_joined_to() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = at(&root, scope);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
        std::fs::write(&path, "- remember to run the linter").expect("the existing file");

        memory::remember(&root, scope, "and the formatter").expect("the line lands");

        let after = read(&path);
        assert_eq!(
            after, "- remember to run the linter\n- and the formatter\n",
            "{scope:?} joined the previous author's last line to the new bullet",
        );
        assert!(
            !after.contains("linter- "),
            "the two lines are still two lines:\n{after}",
        );
    }
}

/// An empty line is refused, and refusing creates nothing.
///
/// A blank line remembered successfully is the failure an operator cannot see:
/// the surface says it was recorded, the file says nothing, and the next session
/// behaves as though they never typed it.
#[test]
fn an_empty_or_whitespace_only_line_is_refused_and_creates_no_file() {
    let _guard = env_lock();

    for scope in SCOPES {
        for blank in ["", "   ", "\t\n  \n"] {
            let (_dir, root) = fixture();

            let refused = memory::remember(&root, scope, blank);
            assert!(
                refused.is_err(),
                "{scope:?} accepted {blank:?}, which records nothing while reporting \
                 success",
            );
            assert!(
                !at(&root, scope).exists(),
                "{scope:?} created a file for {blank:?} — a refusal that leaves an \
                 empty guidance file behind is a file the operator has to wonder \
                 about",
            );
        }
    }
}

/// Guidance is a list and a list has an order. Two lines come back in the order
/// they were given.
#[test]
fn two_lines_appear_in_the_order_they_were_remembered() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();

        memory::remember(&root, scope, "first").expect("the first line");
        let path = memory::remember(&root, scope, "second").expect("the second line");

        let text = read(&path);
        let first = text.find("- first").expect("the first line is in the file");
        let second = text
            .find("- second")
            .expect("the second line is in the file");
        assert!(
            first < second,
            "{scope:?} wrote the second line before the first:\n{text}",
        );
        assert_eq!(
            text.matches("# ").count(),
            1,
            "the header is written once, when the file is made, and not again on \
             every line:\n{text}",
        );
    }
}

/// The line is trimmed before it is written, so a bullet is a bullet.
#[test]
fn surrounding_whitespace_does_not_reach_the_file() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    let path =
        memory::remember(&root, Scope::Project, "  prefer small diffs \n").expect("the line lands");

    assert!(
        read(&path).ends_with("- prefer small diffs\n"),
        "a pasted line brings its own whitespace, and the file is markdown",
    );
}

// ---------------------------------------------------------------------------
// F4 — an instruction note is edited in place and forgotten, by line
// ---------------------------------------------------------------------------

/// A file with everything a splice can damage: a header this module did not
/// write, prose that is not a list, a bullet with an indent and a `*`, a fenced
/// block, blank lines, and a last bullet after all of it.
const HAND_WRITTEN: &str = "# a heading somebody wrote\n\nsome prose\n\n\
                            - keep this one\n  * indented, and starred\n\n\
                            ```\n  a fenced block\n```\n\n- and the last one\n";

/// Put `text` in the file `scope` names, making the directory it needs.
fn written(root: &Path, scope: Scope, text: &str) -> PathBuf {
    let path = at(root, scope);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("the directory");
    std::fs::write(&path, text).expect("the existing file");
    path
}

/// The bullets come back with the address of the line they are **on**, which is
/// not their position in the list.
///
/// This is the criterion's "by line" and the assertion that separates an address
/// from an index. The three notes below are the first, second and third bullets
/// and they are on lines 5, 6 and 8 — so an implementation that handed back the
/// row number would look right in every other test in this file and would, the
/// first time an operator edited the third note, replace the blank line above it.
#[test]
fn f4_a_note_is_addressed_by_the_line_it_is_on_and_not_by_its_place_in_the_list() {
    let _guard = env_lock();
    let (_dir, root) = fixture();
    written(
        &root,
        Scope::Project,
        "# AGENTS.md\n\nprose, not a bullet\n\n- first\n  * second\n---\n- third\n",
    );

    let notes = memory::notes(&root, Scope::Project);
    assert_eq!(
        notes
            .iter()
            .map(|note| note.text.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third"],
        "the marker and the indent belong to the file and are not part of the \
         note; the heading, the prose and the `---` are not notes at all",
    );
    assert_eq!(
        notes.iter().map(memory::Note::numbered).collect::<Vec<_>>(),
        [5, 6, 8],
        "SABOTAGE: hand back the position in the list and this is [1, 2, 3]. The \
         two agree for a file that is nothing but bullets, which is the file every \
         other test uses, and disagree for every file a person has actually \
         written — where the difference is which of somebody's lines gets \
         overwritten.",
    );
    assert!(
        notes.iter().all(|note| note.scope == Scope::Project),
        "each note says which file it came from, so a position read out of one \
         cannot be applied to another that also has a line 5",
    );
}

/// Nothing to read is an empty list rather than an error or a created file.
#[test]
fn f4_a_file_that_is_not_there_holds_no_notes_and_is_not_made_by_asking() {
    let _guard = env_lock();
    let (_dir, root) = fixture();

    assert!(memory::notes(&root, Scope::Project).is_empty());
    assert!(
        !at(&root, Scope::Project).exists(),
        "reading is a read — the page an operator opens to look at their notes may \
         not leave a guidance file behind",
    );
}

/// **The criterion, first half.** Editing line N replaces that line and changes
/// no other byte of the file.
#[test]
fn f4_editing_one_note_replaces_that_line_and_changes_no_other_byte() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = written(&root, scope, HAND_WRITTEN);

        let notes = memory::notes(&root, scope);
        assert_eq!(
            notes.len(),
            3,
            "{scope:?}: the fixture holds three bullets, and the rest is not a list",
        );

        let amended = memory::amend(&root, &notes[1], "  now it says something else \n")
            .unwrap_or_else(|error| panic!("{scope:?}: {error}"));
        assert_eq!(amended, path, "amend answers with the file it wrote");

        assert_eq!(
            read(&path),
            HAND_WRITTEN.replace("indented, and starred", "now it says something else"),
            "{scope:?}: the whole file, byte for byte. Only the text of the second \
             bullet moved — its two spaces of indent, its `*`, the fenced block, \
             the blank lines and the heading are the bytes that were there.",
        );
        assert!(
            read(&path).contains("  * now it says something else\n"),
            "{scope:?}: the indent and the marker are the file's own and are not \
             this module's to normalise — an operator who wrote a nested `*` list \
             gets it back",
        );
        assert_eq!(
            memory::notes(&root, scope)
                .iter()
                .map(|note| note.text.as_str())
                .collect::<Vec<_>>(),
            [
                "keep this one",
                "now it says something else",
                "and the last one"
            ],
            "{scope:?}: and the list reads back as three notes still, in order",
        );
    }
}

/// **The criterion, second half.** Forgetting line N removes that line and leaves
/// the header and every sibling bullet intact.
#[test]
fn f4_forgetting_one_note_takes_its_line_and_leaves_the_header_and_its_siblings() {
    let _guard = env_lock();

    for scope in SCOPES {
        let (_dir, root) = fixture();
        let path = written(&root, scope, HAND_WRITTEN);

        let notes = memory::notes(&root, scope);
        memory::forget(&root, &notes[1]).unwrap_or_else(|error| panic!("{scope:?}: {error}"));

        assert_eq!(
            read(&path),
            HAND_WRITTEN.replace("  * indented, and starred\n", ""),
            "{scope:?}: the line and its newline went together and nothing else \
             moved. A blank line left where the bullet was would be this module \
             editing the shape of somebody's document.",
        );
        assert!(
            read(&path).starts_with("# a heading somebody wrote\n"),
            "{scope:?}: the header a person wrote is still the top of the file",
        );
        assert_eq!(
            memory::notes(&root, scope)
                .iter()
                .map(|note| note.text.as_str())
                .collect::<Vec<_>>(),
            ["keep this one", "and the last one"],
            "{scope:?}: both siblings are still there, and still notes",
        );
    }
}

/// **The named sabotage.** A file whose last line has no trailing newline
/// round-trips unchanged.
///
/// Read the file into lines, change one, write them back joined by `\n` with a
/// trailing one: every assertion above still passes, and this file quietly grows a
/// byte. It is the same defect `remember`'s `prelude` exists for, reached from the
/// other side — and the file it happens to is one io-harness reads into the model's
/// prompt at the start of every run.
#[test]
fn f4_a_file_whose_last_line_has_no_newline_after_it_round_trips_unchanged() {
    let _guard = env_lock();
    let ragged = "- run the linter\n- and the formatter";

    // Editing the last line keeps the file ragged.
    let (_dir, root) = fixture();
    let path = written(&root, Scope::Project, ragged);
    let notes = memory::notes(&root, Scope::Project);
    memory::amend(&root, &notes[1], "and the checker").expect("the edit lands");
    assert_eq!(
        read(&path),
        "- run the linter\n- and the checker",
        "SABOTAGE: rewrite the file from the parsed lines and this is \
         `- run the linter\\n- and the checker\\n` — one byte the operator did not \
         write, in a file this crate does not own",
    );
    assert!(
        !read(&path).ends_with('\n'),
        "the file ended without a newline and it still does",
    );

    // Forgetting the last line keeps the line above it exactly as it was.
    let (_dir, root) = fixture();
    let path = written(&root, Scope::Project, ragged);
    let notes = memory::notes(&root, Scope::Project);
    memory::forget(&root, &notes[1]).expect("the removal lands");
    assert_eq!(
        read(&path),
        "- run the linter\n",
        "the removed line took its own bytes and none of its neighbour's",
    );

    // Forgetting the FIRST line leaves the ragged last line ragged.
    let (_dir, root) = fixture();
    let path = written(&root, Scope::Project, ragged);
    let notes = memory::notes(&root, Scope::Project);
    memory::forget(&root, &notes[0]).expect("the removal lands");
    assert_eq!(
        read(&path),
        "- and the formatter",
        "SABOTAGE: the same rewrite adds the trailing newline here too",
    );
}

/// A `\r\n` checkout stays a `\r\n` checkout.
///
/// The second thing the rewrite sabotage normalises, and the one a developer on a
/// Unix machine cannot see at all: every line ending in the operator's file is
/// replaced, so the diff is the whole file and `git` reports it as one.
#[test]
fn f4_windows_line_endings_survive_both_verbs() {
    let _guard = env_lock();
    let crlf = "# IO.md\r\n\r\n- one\r\n- two\r\n";

    let (_dir, root) = fixture();
    let path = written(&root, Scope::User, crlf);
    let notes = memory::notes(&root, Scope::User);
    assert_eq!(
        notes
            .iter()
            .map(|note| note.text.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"],
        "the `\\r` is a line ending and not part of the note's text",
    );

    memory::amend(&root, &notes[0], "uno").expect("the edit lands");
    assert_eq!(
        read(&path),
        "# IO.md\r\n\r\n- uno\r\n- two\r\n",
        "SABOTAGE: join the parsed lines with `\\n` and every ending in the file is \
         rewritten — four lines changed to edit one",
    );

    memory::forget(&root, &memory::notes(&root, Scope::User)[1]).expect("the removal lands");
    assert_eq!(
        read(&path),
        "# IO.md\r\n\r\n- uno\r\n",
        "and the removal takes the `\\r\\n` with the line rather than leaving it",
    );
}

/// A note that has moved since the page was drawn is refused, and nothing is
/// written.
///
/// The file is markdown a person edits in their own editor and another agent may
/// write into, so the address a picker is holding can go stale between the draw
/// and the keystroke. Acting on it anyway is how an operator loses a line they
/// never looked at.
#[test]
fn f4_a_note_that_has_changed_underneath_is_refused_rather_than_overwritten() {
    let _guard = env_lock();
    let (_dir, root) = fixture();
    let path = written(&root, Scope::Local, "- first\n- second\n");
    let notes = memory::notes(&root, Scope::Local);

    // Somebody else edits the file while the picker is open.
    let moved = "- somebody else got here first\n- first\n- second\n";
    std::fs::write(&path, moved).expect("the other writer");

    let refused = memory::amend(&root, &notes[1], "a replacement");
    assert!(
        refused.is_err(),
        "line 2 now says `first`, not `second`: the address is stale and the \
         replacement would have landed on a line the operator never chose",
    );
    assert_eq!(read(&path), moved, "and a refusal writes nothing at all",);
    assert!(
        memory::forget(&root, &notes[1]).is_err(),
        "the same check guards the removal, which is the one that cannot be undone",
    );
    assert_eq!(read(&path), moved, "still nothing");

    // The same removal, re-read, is fine — which is what makes the refusal a
    // staleness check rather than a refusal to work.
    let fresh = memory::notes(&root, Scope::Local);
    memory::forget(&root, &fresh[2]).expect("a note read from the file as it stands");
    assert_eq!(read(&path), "- somebody else got here first\n- first\n");
}

/// An empty replacement is refused, and so is one holding a line break.
///
/// A blanked note is the same failure a blank `remember` is: the surface says it
/// was recorded and the file says nothing. A replacement with a newline in it is
/// the other half — one line of the file becomes two, and the second is a line
/// `notes` cannot address and no verb can reach.
#[test]
fn f4_an_empty_or_multi_line_replacement_is_refused_and_nothing_is_written() {
    let _guard = env_lock();
    let (_dir, root) = fixture();
    let before = "- prefer small diffs\n";
    let path = written(&root, Scope::Project, before);
    let note = memory::notes(&root, Scope::Project).remove(0);

    for refused in ["", "   ", "\t\n  \n", "one line\nand another"] {
        assert!(
            memory::amend(&root, &note, refused).is_err(),
            "{refused:?} was accepted, which either blanks the note while reporting \
             success or puts an unaddressable line in the file",
        );
        assert_eq!(read(&path), before, "and {refused:?} wrote nothing");
    }
}

/// **Both verbs are reachable from a keystroke.**
///
/// A driver-text gate, and it is here for the reason this repository already has
/// four of them: nothing under `tests/` links `src/main.rs`, so a public function
/// with a thorough test file and no caller is invisible to the suite. 0.20.0
/// shipped seven of those behind a green run, and `recall::unforget` spent a whole
/// release as an eighth — so the rule now is that a new public item names its call
/// site and a gate traces it.
///
/// The comments come off first: every one of the three names below appears in a
/// paragraph of prose in the driver, and a gate satisfied by prose is the defect
/// 0.14.0 shipped. Copied from `tests/structure.rs:137`, which cannot be imported
/// across test binaries.
///
/// Sabotage: wire the page and delete the `memory::amend` arm — `contains` on the
/// other two still passes, and this fails on the one that is missing.
#[test]
fn f4_reading_editing_and_forgetting_a_note_are_all_reachable_from_the_driver() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    let text: String = text
        .lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");

    for (call, why) in [
        (
            "io_cli::memory::notes(",
            "the bullets have to be read back before either verb can address one, \
             and reading them from the file at the moment the verb is offered is \
             what makes the address good",
        ),
        (
            "io_cli::memory::amend(",
            "editing a note is one of the two verbs this release exists to add; a \
             `memory::amend` no keystroke reaches is a tested function, not a \
             surface",
        ),
        (
            "io_cli::memory::forget(",
            "and so is removing one — until now the only verb these files had was \
             an append, so a line typed by mistake stayed in the model's prompt \
             for the life of the checkout",
        ),
    ] {
        assert!(text.contains(call), "`{call}` has no call site: {why}");
    }
}
