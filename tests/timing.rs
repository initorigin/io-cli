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

use std::path::Path;

/// The needles, assembled from pieces so that this file does not match itself.
/// A scanner that has to skip its own path is a scanner with a hole in it.
fn forbidden() -> Vec<(String, &'static str)> {
    vec![
        (
            format!("thread::{}", "sleep"),
            "a test that sleeps is a test that fails on a loaded runner",
        ),
        (
            format!("time::{}", "sleep"),
            "an async sleep is a wall-clock assertion wearing a different hat",
        ),
        (
            format!("{}::now", "Instant"),
            "reading a clock in a test is how a timing assertion gets written",
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

    let mut violations = Vec::new();
    for (path, source) in &sources {
        for (needle, why) in forbidden() {
            for (number, line) in source.lines().enumerate() {
                if line.contains(&needle) {
                    violations.push(format!(
                        "{}:{}: {needle} — {why}",
                        path.display(),
                        number + 1,
                    ));
                }
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
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut readers = Vec::new();
    for entry in std::fs::read_dir(&src).expect("src is readable").flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a source file is readable");
        if source.contains(&format!("{}::now", "Instant")) {
            readers.push(
                path.file_name()
                    .expect("a file has a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }

    // `read_dir` yields in whatever order the filesystem chose, which differs
    // between the three platforms CI runs on. Sorted, so the failure message is
    // the same everywhere and the assertion is about the set rather than the
    // directory's mood.
    readers.sort();

    // One reader, and it is the driver. Every other module is handed the age it
    // needs, which is what makes the session's liveness testable at all — and
    // what stops a second, unsynchronised clock appearing in a later release.
    assert_eq!(
        readers,
        vec!["main.rs".to_string()],
        "something other than the driver started reading a clock",
    );
}
