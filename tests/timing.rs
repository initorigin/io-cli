//! N1 — no test in this repository sleeps, and none measures elapsed time.
//!
//! This is the constraint the whole clock seam exists to satisfy: `App::tick`
//! takes the session's age as an argument rather than reading a clock precisely
//! so that liveness can be asserted without a timer being involved in the
//! asserting. A constraint checked by reading is a constraint that decays, and
//! this release adds a repaint timer — which is exactly the kind of work that
//! grows a sleeping test in the next one.
//!
//! What it does NOT forbid: `Duration`, and the production clock read in the
//! driver. Something has to read a real clock; the driver is the only thing that
//! may, and it is the reason nothing else has to. The second test below is what
//! keeps that "only".
//!
//! **0.17.0 widens the second test in three ways, and relaxes nothing.** The
//! release adds a mid-turn prompt queue, and a queue is the single most natural
//! place in this product for a clock to appear: an entry wants a timestamp, a
//! burst of keystrokes wants a debounce, and both are one `now()` away. So the
//! sweep that used to look for one spelling in one directory level now looks for
//! all three spellings the criterion names, in every file under `src/` however
//! deeply it is nested, and refuses the aliases that would let a fourth spelling
//! through. See `aliases_a_clock` for the last of those, which is the hole a
//! string match always leaves.

use std::path::{Path, PathBuf};

/// The two ways a test can wait, assembled from pieces so that this file does not
/// match itself. A scanner that has to skip its own path is a scanner with a hole
/// in it exactly the size of the thing it skips.
fn sleeps() -> Vec<(String, &'static str)> {
    vec![
        (
            format!("thread::{}", "sleep"),
            "a test that sleeps is a test that fails on a loaded runner",
        ),
        (
            format!("time::{}", "sleep"),
            "an async sleep is a wall-clock assertion wearing a different hat",
        ),
    ]
}

/// The three ways anything can ask what time it is. N1 names exactly these, and
/// until 0.17.0 only the first of them was swept for across `src/` — which meant
/// a module could have taken a wall clock, or measured a span off one it was
/// handed, without a gate saying so. Both halves of the file now use this list,
/// so the rule a test is held to and the rule a module is held to are one list
/// rather than two that drift.
fn clock_reads() -> Vec<(String, &'static str)> {
    vec![
        (
            format!("{}::now", "Instant"),
            "reading a clock is how a timing assertion, or a timestamped queue \
             entry, gets written",
        ),
        (
            format!("{}::now", "SystemTime"),
            "the same, with a clock that can also move backwards",
        ),
        (
            format!(".{}()", "elapsed"),
            "measuring how long something took is the assertion N1 forbids",
        ),
    ]
}

fn forbidden() -> Vec<(String, &'static str)> {
    let mut all = sleeps();
    all.extend(clock_reads());
    all
}

/// Every needle above is a string, and a string match is defeated by a rename.
/// `use std::time::Instant as Clock` puts `Clock::now()` in a module that no
/// needle in this file will ever see, and so does a glob over the time module or
/// a re-export renamed on its way through. There is no honest reason to alias a
/// clock, so the alias is forbidden outright rather than chased — the same move
/// `tests/dependencies.rs` makes for `use std::process as`, for the same reason:
/// it leaves one string for this test, and for a reader, to look for.
///
/// Deliberately scoped to import lines. This crate's modules argue their design
/// in prose, and prose that says "treat the Instant as the session's age" is a
/// file explaining itself, not a file hiding a clock. A gate that reddened on
/// that sentence would be a gate somebody weakens next release, which is worse
/// than one that is narrow.
fn aliases_a_clock(line: &str) -> Option<&'static str> {
    let line = line.trim_start();
    let rest = line
        .strip_prefix("pub use ")
        .or_else(|| line.strip_prefix("use "))?;

    let time = format!("std::{}", "time");
    if rest.starts_with(&format!("{time}::*")) {
        return Some("a glob over the time module brings a clock in without naming it");
    }
    if rest.starts_with(&format!("{time} as ")) {
        return Some("the time module renamed is every clock in it renamed");
    }
    if rest.contains(" as ") && (rest.contains("Instant") || rest.contains("SystemTime")) {
        return Some("a clock imported under another name is a clock no string match finds");
    }
    None
}

/// Every `.rs` file under `src/`, at any depth.
///
/// Recursive since 0.17.0. The previous sweep read one directory level, which was
/// true of `src/` and would have stayed true right up until the release that
/// grows a submodule directory — at which point the gate would have kept passing
/// while covering less, and nothing would have said so. `tests/dependencies.rs`
/// has walked the tree since 0.1.0; this is the same walk, and the two now agree
/// about what "every source file" means.
fn crate_sources() -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", directory.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
                found.push((path, source));
            }
        }
    }
    found
}

/// Every `.rs` file under `tests/`, including this one.
fn test_sources() -> Vec<(std::path::PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("{} is readable: {error}", directory.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()));
                found.push((path, source));
            }
        }
    }
    found
}

#[test]
fn n1_no_test_sleeps_or_measures_elapsed_time() {
    let sources = test_sources();

    // A scan that read nothing would pass for the wrong reason, and a silent
    // empty run is the failure mode this line of repositories has paid for
    // before. The floor is deliberately loose — it is a tripwire against a
    // scanner that found no files, not a count to maintain.
    assert!(
        sources.len() >= 10,
        "the scan found only {} test files, which means it is not scanning",
        sources.len(),
    );
    assert!(
        sources.iter().any(|(path, _)| path.ends_with("timing.rs")),
        "the scan did not even find itself",
    );

    let needles = forbidden();
    let mut violations = Vec::new();
    for (path, source) in &sources {
        for (number, line) in source.lines().enumerate() {
            let at = format!("{}:{}", path.display(), number + 1);
            for (needle, why) in &needles {
                if line.contains(needle.as_str()) {
                    violations.push(format!("{at}: {needle} — {why}"));
                }
            }
            if let Some(why) = aliases_a_clock(line) {
                violations.push(format!("{at}: {} — {why}", line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "N1 is broken:\n{}",
        violations.join("\n"),
    );
}

#[test]
fn n1_the_driver_is_the_only_thing_that_reads_a_clock() {
    let driver = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let sources = crate_sources();

    // The same tripwire the sweep above carries, for the same reason: a walk that
    // found nothing passes every assertion below it, and the pass would be
    // indistinguishable from the real one.
    assert!(
        sources.len() >= 20,
        "the walk found only {} source files, which means it is not walking",
        sources.len(),
    );

    let needles = clock_reads();
    let mut readers = Vec::new();
    let mut aliases = Vec::new();
    for (path, source) in &sources {
        // The driver is the one module permitted to read a clock — it is not
        // permitted to reach one under a false name, because the permission is
        // what makes every other module's `age` argument trustworthy and an
        // aliased clock in the driver is a clock a reader of `main.rs` cannot
        // find either.
        for (number, line) in source.lines().enumerate() {
            if let Some(why) = aliases_a_clock(line) {
                aliases.push(format!(
                    "{}:{}: {} — {why}",
                    path.display(),
                    number + 1,
                    line.trim(),
                ));
            }
        }

        if path == &driver {
            continue;
        }
        for (needle, why) in &needles {
            for (number, line) in source.lines().enumerate() {
                // **A comment is not a clock read, and 0.19.0 is why this line
                // is here.** `clock_reads` already builds its own needles with
                // `format!` so that *this* file can name them without matching
                // itself — the same problem, solved for the scanner and not for
                // the scanned. `src/skills.rs` explains that it decides freshness
                // by bytes rather than by a clock, which means naming the three
                // calls it is deliberately not making, and a gate that read prose
                // would forbid a module from documenting the rule it obeys.
                //
                // Line-oriented on purpose: a `//` inside a string literal still
                // counts as code here, which can only make the gate stricter.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains(needle.as_str()) {
                    readers.push(format!(
                        "{}:{}: {needle} — {why}",
                        path.display(),
                        number + 1,
                    ));
                }
            }
        }
    }

    // `read_dir` yields in whatever order the filesystem chose, which differs
    // between the three platforms CI runs on. Sorted, so the failure message is
    // the same everywhere and the assertion is about the set rather than the
    // directory's mood.
    readers.sort();
    aliases.sort();

    assert!(
        aliases.is_empty(),
        "a clock is reached under another name:\n{}",
        aliases.join("\n"),
    );

    // One reader, and it is the driver. Every other module is handed the age it
    // needs, which is what makes the session's liveness testable at all — and
    // what stops a second, unsynchronised clock appearing in a later release. A
    // queue entry stamped when it was queued is the shape this release makes
    // likeliest, and it fails here by name.
    assert!(
        readers.is_empty(),
        "something other than the driver started reading a clock:\n{}",
        readers.join("\n"),
    );

    // And the driver still reads one, spelled in full. A gate watching for a
    // string is a gate that stops covering anything the day the string moves, so
    // the string is asserted present as well as absent — the same pairing
    // `tests/dependencies.rs` makes for `std::process::Command` in `src/shell.rs`.
    // Taken from the walk rather than read again, which also says the walk found
    // the one file it exempts.
    let (_, source) = sources
        .iter()
        .find(|(path, _)| path == &driver)
        .expect("the walk found src/main.rs, which is the file it exempts");
    assert!(
        source.contains(&format!("{}::now", "Instant")),
        "src/main.rs no longer reads a clock in full spelling. Either the session's \
         age comes from somewhere this file cannot see, or the needle above is \
         watching a string that is no longer written anywhere.",
    );
}
