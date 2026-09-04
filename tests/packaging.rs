//! F1 — the formula and the manifest name the artifacts the release workflow builds.
//! F2 — both of them name one version, and it is a version this repository released.
//! F3 — every checksum in them is a distinct lowercase hex digest.
//! F4 — the generator is idempotent and writes those two files and nothing else.
//! F5 — the generator refuses a checksum file it cannot verify.
//!
//! `Formula/io.rb` and `bucket/io.json` restate, in two more files, a fact that
//! already lives in `.github/workflows/release.yml`: what the release builds and
//! what it is called. Nothing at install time compares them — `brew install`
//! fetches the URL the formula names and fails on the user's machine, days later,
//! if that artifact was never uploaded. So the comparison happens here.
//!
//! The version in those two files is deliberately the newest RELEASED one, which
//! during a release is one behind `CARGO_PKG_VERSION`: an artifact that has not
//! been uploaded has no checksum to name. F2 is therefore written against
//! `CHANGELOG.md`'s released headings and NOT against the crate version — a gate
//! spelled the other way would demand a digest that cannot exist yet, and the
//! release could not satisfy its own criterion.
//!
//! F4 and F5 run `scripts/update-tap.sh` against a `file://` release, the way
//! `tests/installers.rs` runs `install.sh`. The two files are staged into a
//! temporary tree first, so the run edits copies and the working tree it was
//! started from is never touched.

#![cfg(unix)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The version the fixtures release. Not a version this repository has, so a run
/// that leaves the files unchanged cannot pass for a run that rewrote them.
const FIXTURE_VERSION: &str = "9.9.9";

const FORMULA: &str = "Formula/io.rb";
const MANIFEST: &str = "bucket/io.json";
const GENERATOR: &str = "scripts/update-tap.sh";

const DOWNLOADS: &str = "https://github.com/initorigin/io-cli/releases/download";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Read a checked-in file with line endings normalised.
///
/// git hands Windows a CRLF working copy, and every fragment matched below would
/// then be found at the wrong offset or not at all.
fn read(relative: &str) -> String {
    std::fs::read_to_string(repo().join(relative))
        .unwrap_or_else(|error| panic!("{relative}: {error}"))
        .replace("\r\n", "\n")
}

/// The quoted strings on one line, in order.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// `(target, archive extension)` for every row of the release matrix, read out of
/// the workflow rather than restated here — the workflow is what actually builds
/// them, so a row added or renamed there has to reach these two files.
fn matrix() -> Vec<(String, String)> {
    let workflow = read(".github/workflows/release.yml");
    let mut rows = Vec::new();
    let mut target: Option<String> = None;
    for line in workflow.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("- target:") {
            target = Some(value.trim().to_string());
        } else if let Some(value) = line.strip_prefix("archive:") {
            if let Some(target) = target.take() {
                rows.push((target, value.trim().to_string()));
            }
        }
    }
    assert_eq!(
        rows.len(),
        4,
        "the release matrix no longer has four rows; this test read {rows:?}",
    );
    rows
}

/// Every URL in a packaging file, with scoop's `$version` template resolved.
///
/// The autoupdate block is a template for the NEXT release, so its URLs carry
/// `$version` rather than a number. Substituting it here is what lets the same
/// assertion cover the concrete URL and the template it will be replaced by.
fn urls(text: &str, version: &str) -> Vec<String> {
    text.lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with("url \"") || line.starts_with("\"url\":")
        })
        .filter_map(|line| {
            quoted(line)
                .last()
                .map(|url| url.replace("$version", version))
        })
        .collect()
}

/// The version a packaging file declares.
///
/// **The formula declares none, and that is deliberate.** Homebrew scans the
/// version out of the url and `brew audit --strict` refuses an explicit
/// `version` stanza beside it as redundant — which it is, and worse: a second
/// declaration of a fact the urls already carry is one that can disagree with
/// them. So the formula's version is read back out of its own urls, and the
/// disagreement this function used to be able to report is one the file can no
/// longer express. The manifest still declares `"version"`, because scoop has
/// no equivalent scan.
fn declared(text: &str, relative: &str) -> String {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"version\":") {
            if let Some(value) = quoted(trimmed).last() {
                return (*value).to_string();
            }
        }
    }

    // Every url in the formula is `…/download/v<version>/io-<version>-<target>…`,
    // and they must all name one version — asserted here rather than assumed,
    // because a generator that rewrote three urls and missed the fourth is
    // exactly the failure this file exists to catch.
    let mut seen: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("url \"") {
            continue;
        }
        let url = quoted(trimmed).last().copied().unwrap_or_default();
        let Some(tail) = url.split("/download/v").nth(1) else {
            panic!("{relative}: {url} names no release directory");
        };
        let version = tail.split('/').next().unwrap_or_default().to_string();
        if !seen.contains(&version) {
            seen.push(version);
        }
    }
    assert_eq!(
        seen.len(),
        1,
        "{relative} names more than one version in its urls: {seen:?}"
    );
    seen.pop().expect("a formula declares at least one url")
}

/// Every checksum a packaging file names: `sha256 "…"` in the formula and
/// `"hash": "…"` in the manifest. The manifest's autoupdate hash is a URL inside
/// an object rather than a value, so it is not one of these and is not expected
/// to be.
fn digests(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| {
            let line = line.trim();
            line.starts_with("sha256 \"") || line.starts_with("\"hash\": \"")
        })
        .filter_map(|line| quoted(line).last().map(|digest| (*digest).to_string()))
        .collect()
}

/// F1 — the two packaging files name artifacts the release workflow really builds.
///
/// The failure this exists to catch is a target renamed in the workflow, or an
/// archive extension swapped, leaving a formula that points at a name the Release
/// never had. Nothing else compares the two: `brew install` discovers it as a 404
/// on somebody else's machine.
///
/// The target sets are compared with `==` rather than `contains`. A missing arm
/// is an architecture that cannot install at all, and a stray one is a URL that
/// 404s — a subset check would report neither.
#[test]
fn f1_the_formula_and_the_manifest_name_the_artifacts_the_release_workflow_builds() {
    let rows = matrix();
    let formula = read(FORMULA);
    let manifest = read(MANIFEST);
    let version = declared(&formula, FORMULA);

    let extensions: BTreeMap<&str, &str> = rows
        .iter()
        .map(|(target, archive)| (target.as_str(), archive.as_str()))
        .collect();

    let base = format!("{DOWNLOADS}/v{version}/");
    let mut seen: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for (relative, text) in [(FORMULA, &formula), (MANIFEST, &manifest)] {
        let mut targets = BTreeSet::new();
        for url in urls(text, &version) {
            // Every URL either names an artifact or is the SHA256SUMS beside it,
            // and both live under the Release for the version the file declares.
            // A URL under some other base is a download this repository did not
            // publish, whatever it is called.
            let name = url
                .strip_prefix(&base)
                .unwrap_or_else(|| panic!("{relative}: {url} is not under {base}"));
            if name == "SHA256SUMS" {
                continue;
            }
            let target = extensions
                .iter()
                .find(|(target, archive)| name == format!("io-{version}-{target}.{archive}"))
                .map(|(target, _)| (*target).to_string())
                .unwrap_or_else(|| {
                    panic!(
                        "{relative}: {name} is not io-{version}-<target>.<archive> for any target \
                         the release matrix declares ({rows:?})",
                    )
                });
            // A set, not a count: the manifest names its one artifact twice on
            // purpose, once concretely and once as the autoupdate template that
            // replaces it. A target named twice and another not at all is caught
            // by the comparison below, where it belongs.
            targets.insert(target);
        }
        seen.insert(relative, targets);
    }

    let unix: BTreeSet<String> = rows
        .iter()
        .filter(|(target, _)| !target.contains("windows"))
        .map(|(target, _)| target.clone())
        .collect();
    let windows: BTreeSet<String> = rows
        .iter()
        .filter(|(target, _)| target.contains("windows"))
        .map(|(target, _)| target.clone())
        .collect();

    assert_eq!(
        seen[FORMULA], unix,
        "the formula and the workflow disagree about which unix targets exist",
    );
    assert_eq!(
        seen[MANIFEST], windows,
        "the scoop manifest and the workflow disagree about the Windows target",
    );
}

/// F2 — both files name the same version, and it is one this repository released.
///
/// Deliberately NOT `CARGO_PKG_VERSION`. These files carry the digests of
/// uploaded artifacts, so during the release that builds a version they still
/// name the one before it; a gate written against the crate version would demand
/// a checksum for a file that does not exist yet.
///
/// `## [Unreleased]` is in the same list of headings and is excluded by the shape
/// of the number, not by its name — a heading that is not `x.y.z` is not a
/// release, whatever it is called.
#[test]
fn f2_the_formula_and_the_manifest_agree_on_one_released_version() {
    let formula = read(FORMULA);
    let manifest = read(MANIFEST);

    let formula_version = declared(&formula, FORMULA);
    let manifest_version = declared(&manifest, MANIFEST);
    assert_eq!(
        formula_version, manifest_version,
        "{FORMULA} and {MANIFEST} name different versions; one of them installs \
         an artifact the other does not",
    );

    let released: BTreeSet<String> = read("CHANGELOG.md")
        .lines()
        .filter_map(|line| line.trim().strip_prefix("## [").map(str::to_string))
        .filter_map(|rest| rest.split(']').next().map(str::to_string))
        .filter(|heading| {
            let parts: Vec<&str> = heading.split('.').collect();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        })
        .collect();
    assert!(
        released.len() > 1,
        "CHANGELOG.md yielded {} release headings; this test read it wrongly",
        released.len(),
    );
    assert!(
        released.contains(&formula_version),
        "the packaging files name {formula_version}, which CHANGELOG.md does not \
         record as released. The digests of an unreleased version cannot exist: \
         run scripts/update-tap.sh AFTER the Release is cut.",
    );
}

/// F3 — every checksum is a distinct 64-character lowercase hex digest.
///
/// Both halves matter and both are silent failures. A generator that fails to
/// substitute leaves whatever was there — a placeholder, or the previous
/// release's digest — and one that reuses a variable writes one target's digest
/// under another's URL. Neither looks wrong in a diff; both install nothing.
#[test]
fn f3_every_checksum_is_a_distinct_lowercase_hex_digest() {
    let formula = digests(&read(FORMULA));
    let manifest = digests(&read(MANIFEST));
    assert_eq!(
        formula.len(),
        3,
        "the formula names {} checksums, not one per unix target",
        formula.len(),
    );
    assert_eq!(
        manifest.len(),
        1,
        "the manifest names {} checksums, not one for the Windows artifact",
        manifest.len(),
    );

    let all: Vec<String> = formula.into_iter().chain(manifest).collect();
    for digest in &all {
        assert_eq!(digest.len(), 64, "{digest} is not 64 characters");
        // Spelled as a set of characters rather than as ranges. `sha256sum` writes
        // lowercase and every comparison against it is case-sensitive, so an
        // uppercase digest is a checksum nothing will ever match — and the range
        // spellings for hex quietly admit one.
        assert!(
            digest.chars().all(|c| "0123456789abcdef".contains(c)),
            "{digest} is not lowercase hex",
        );
    }
    let distinct: BTreeSet<&String> = all.iter().collect();
    assert_eq!(
        distinct.len(),
        all.len(),
        "two artifacts share a checksum, so at least one of them is wrong: {all:?}",
    );
}

/// A digest that is valid in shape and different for every row.
///
/// Letters rather than digits, because F5 uppercases one of these to build the
/// malformed fixture and `'1'.to_uppercase()` is `'1'` — a fixture of digits
/// makes that arm assert nothing, which is how it first passed here.
fn fixture_digest(row: usize) -> String {
    "abcdef"
        .chars()
        .nth(row)
        .expect("a hex letter")
        .to_string()
        .repeat(64)
}

/// A `SHA256SUMS` covering every artifact of the release matrix.
fn fixture_sums(version: &str) -> String {
    matrix()
        .iter()
        .enumerate()
        .map(|(row, (target, archive))| {
            format!("{}  io-{version}-{target}.{archive}\n", fixture_digest(row))
        })
        .collect()
}

/// Copy the two packaging files and the generator into a throwaway tree.
///
/// The generator resolves its files from its own directory, so a copy of it edits
/// the copies beside it. `README.md` is there as a file it must not touch: "it
/// wrote the two files" and "it wrote ONLY the two files" are different claims,
/// and a tree with one thing in it can only support the first.
fn stage(root: &Path) {
    for directory in ["Formula", "bucket", "scripts"] {
        std::fs::create_dir_all(root.join(directory)).expect("a staged directory");
    }
    for relative in [FORMULA, MANIFEST, GENERATOR] {
        std::fs::copy(repo().join(relative), root.join(relative))
            .unwrap_or_else(|error| panic!("staging {relative}: {error}"));
    }
    std::fs::write(
        root.join("README.md"),
        "a file the generator must not touch\n",
    )
    .expect("the bystander file");
}

fn run(root: &Path, release: &Path, version: &str) -> std::process::Output {
    Command::new("sh")
        .arg(root.join(GENERATOR))
        .arg(version)
        .env("IO_BASE_URL", format!("file://{}", release.display()))
        .output()
        .expect("the generator runs")
}

/// Every file under `root`, by path and by bytes.
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("a readable directory") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .to_string_lossy()
                    .into_owned();
                files.insert(relative, std::fs::read(&path).expect("a readable file"));
            }
        }
    }
    files
}

/// Build a staged tree and a release directory holding `sums`.
fn fixture(dir: &Path, sums: &str) -> (PathBuf, PathBuf) {
    let root = dir.join("root");
    let release = dir.join("release");
    std::fs::create_dir_all(&root).expect("a staged root");
    std::fs::create_dir_all(&release).expect("a release directory");
    stage(&root);
    std::fs::write(release.join("SHA256SUMS"), sums).expect("SHA256SUMS");
    (root, release)
}

/// F4 — a second run over the same Release changes nothing, and no third file moves.
///
/// Idempotence alone is not the property, because a generator that does nothing
/// is perfectly idempotent. The first run is asserted to have actually rewritten
/// both files to the fixture's version and the fixture's digests; the second is
/// asserted to be a byte-for-byte no-op on the whole tree. Together they say the
/// generator is safe to run twice and that `git diff` after it shows the release
/// and nothing else.
#[test]
fn f4_the_generator_is_idempotent_and_writes_only_those_two_files() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let (root, release) = fixture(dir.path(), &fixture_sums(FIXTURE_VERSION));

    let before = snapshot(&root);
    let output = run(&root, &release, FIXTURE_VERSION);
    assert!(
        output.status.success(),
        "the generator failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let after = snapshot(&root);

    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the generator created or removed a file",
    );
    let changed: BTreeSet<&String> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(*bytes))
        .map(|(path, _)| path)
        .collect();
    let expected: BTreeSet<&String> = [FORMULA, MANIFEST]
        .iter()
        .map(|relative| {
            after
                .keys()
                .find(|key| key.as_str() == *relative)
                .expect("a staged file")
        })
        .collect();
    assert_eq!(
        changed, expected,
        "the generator changed a different set of files than the two it packages",
    );

    // It really rewrote them: the version and every digest are the fixture's.
    let formula = String::from_utf8_lossy(&after[FORMULA]).into_owned();
    let manifest = String::from_utf8_lossy(&after[MANIFEST]).into_owned();
    assert_eq!(declared(&formula, FORMULA), FIXTURE_VERSION);
    assert_eq!(declared(&manifest, MANIFEST), FIXTURE_VERSION);
    let written: BTreeSet<String> = digests(&formula)
        .into_iter()
        .chain(digests(&manifest))
        .collect();
    let fixtured: BTreeSet<String> = (0..matrix().len()).map(fixture_digest).collect();
    assert_eq!(
        written, fixtured,
        "the generator did not write the fixture's digests into both files",
    );

    let again = run(&root, &release, FIXTURE_VERSION);
    assert!(again.status.success(), "the second run failed");
    assert_eq!(
        snapshot(&root),
        after,
        "a second run over the same Release changed something",
    );
}

/// F5 — a checksum file the generator cannot verify stops it, before it writes.
///
/// Three shapes, because each takes a different branch and each is a real way for
/// a Release to be wrong: an upload that did not finish leaves an artifact
/// unmentioned, a workflow that failed after creating the file leaves it empty,
/// and a digest of the wrong shape is a checksum nothing will ever match. In all
/// three the tree has to come out byte-identical: reading four checksums before
/// writing any of them is what keeps a half-updated formula off disk.
///
/// Each arm is held to the words of ITS OWN branch rather than to "it exited
/// non-zero". The refusals are over-determined — a missing line leaves an empty
/// digest, which is also not 64 characters long — so an arm that asserted only
/// the status stayed green with the absent-value guard deleted, and the guard
/// this repository has already once printed a success line without would have
/// been unprotected by the test written to protect it.
#[test]
fn f5_the_generator_refuses_a_checksum_file_it_cannot_verify() {
    let windows = matrix()
        .into_iter()
        .find(|(target, _)| target.contains("windows"))
        .expect("a Windows row in the release matrix");
    let full = fixture_sums(FIXTURE_VERSION);
    let missing: String = full
        .lines()
        .filter(|line| !line.contains(windows.0.as_str()))
        .map(|line| format!("{line}\n"))
        .collect();
    let uppercased = full.replacen(&fixture_digest(0), &fixture_digest(0).to_uppercase(), 1);

    for (what, sums, says) in [
        (
            "an artifact with no checksum line",
            missing.as_str(),
            format!(
                "does not mention io-{FIXTURE_VERSION}-{}.{}",
                windows.0, windows.1
            ),
        ),
        ("an empty SHA256SUMS", "", "is empty".to_string()),
        (
            "a digest that is not lowercase hex",
            uppercased.as_str(),
            "is not lowercase hex".to_string(),
        ),
    ] {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let (root, release) = fixture(dir.path(), sums);

        let before = snapshot(&root);
        let output = run(&root, &release, FIXTURE_VERSION);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{what} was accepted\nstdout: {}",
            String::from_utf8_lossy(&output.stdout),
        );
        assert!(
            stderr.contains("update-tap:") && stderr.contains(&says),
            "{what}: the refusal must say {says:?} on stderr, so that the branch \
             that refused is the one this arm is holding open: {stderr}",
        );
        assert_eq!(
            snapshot(&root),
            before,
            "{what}: the generator wrote something before it refused",
        );
    }
}
