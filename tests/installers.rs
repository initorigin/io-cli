//! N7 — an installer never runs an unverified binary.
//! N8 — an install needs no administrator rights.
//! F10 — an install narrates itself.
//! F11 — a failing install fails exactly as it did.
//!
//! Both scripts are exercised against a **local release**: a directory holding a
//! real archive and a real `SHA256SUMS`, served over `file://`. That is enough to
//! prove the verification and the no-privileges properties, which are the two
//! this repository can assert. What it cannot prove is O5 — that the script works
//! on a clean machine, in a new shell, with nothing else installed — and that is
//! deliberately a human step against the published Release, because an installer
//! verified only on the machine that built it is not verified.
//!
//! The narration (F10/F11) is asserted by RUNNING `install.sh` against that local
//! release and matching the ordered sequence its stdout actually contains — never
//! by reading the source, because a script can contain a print and still not reach
//! it. `install.ps1` gets what this repository can honestly give it: its refusal
//! path is executed under `pwsh` when `pwsh` is here, and its narration is matched
//! in source order, because `Invoke-WebRequest` cannot fetch the `file://` release
//! the shell fixture is built from. The behavioural half of the Windows story is
//! the same human step against the published Release.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;

const VERSION: &str = "9.9.9";
const TARGET_HINT: &str = "the target this machine installs";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The artifact name `install.sh` will ask for on this machine.
fn target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-musl",
        (os, arch) => panic!("{TARGET_HINT} is unknown for {os} {arch}"),
    }
}

/// Build a release directory holding one archive and its checksums.
///
/// The "binary" is a shell script that prints a version, which is all the
/// installer has to move into place — nothing here is testing the compiler.
fn release(dir: &Path, corrupt: bool) -> PathBuf {
    let stage_name = format!("io-{VERSION}-{}", target());
    let stage = dir.join(&stage_name);
    std::fs::create_dir_all(&stage).expect("a stage directory");
    std::fs::write(
        stage.join("io"),
        "#!/bin/sh\necho \"io 9.9.9 (a stand-in)\"\n",
    )
    .expect("the stand-in binary");

    let archive = dir.join(format!("{stage_name}.tar.gz"));
    let status = Command::new("tar")
        .arg("czf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .arg(&stage_name)
        .status()
        .expect("tar runs");
    assert!(status.success(), "tar failed");
    std::fs::remove_dir_all(&stage).expect("clean up the stage");

    // The checksums are computed from the archive as it is now, and the archive
    // is corrupted AFTERWARDS — so the mismatch is a real one rather than a
    // deliberately wrong number written into the file.
    let sums = sha256(&archive);
    std::fs::write(
        dir.join("SHA256SUMS"),
        format!("{sums}  {stage_name}.tar.gz\n"),
    )
    .expect("SHA256SUMS");

    if corrupt {
        let mut bytes = std::fs::read(&archive).expect("read the archive");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&archive, bytes).expect("corrupt the archive");
    }

    archive
}

fn sha256(path: &Path) -> String {
    for (program, args) in [("sha256sum", vec![]), ("shasum", vec!["-a", "256"])] {
        if let Ok(output) = Command::new(program).args(&args).arg(path).output() {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                return text
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }
    panic!("neither sha256sum nor shasum is available");
}

fn run_installer(release_dir: &Path, install_dir: &Path) -> std::process::Output {
    Command::new("sh")
        .arg(repo().join("install.sh"))
        .env("IO_VERSION", VERSION)
        .env("IO_BASE_URL", format!("file://{}", release_dir.display()))
        .env("IO_INSTALL_DIR", install_dir)
        // A PATH that does not contain the install directory, so the script takes
        // the branch that prints the line to add.
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .output()
        .expect("the installer runs")
}

/// Assert that every fragment appears in `text`, each one after the last.
///
/// Substrings rather than whole lines, so that a path or a checksum can be
/// matched without the test having to reproduce the rest of its line — but the
/// cursor only ever moves forward, so the ORDER is what is being asserted.
fn assert_ordered(what: &str, text: &str, sequence: &[String]) {
    let mut cursor = 0;
    for fragment in sequence {
        match text[cursor..].find(fragment.as_str()) {
            Some(at) => cursor += at + fragment.len(),
            None => panic!(
                "{what}: {fragment:?} is missing, or comes before something it should follow\n\
                 --- the output was ---\n{text}\n---",
            ),
        }
    }
}

fn uname(flag: &str) -> String {
    let out = Command::new("uname")
        .arg(flag)
        .output()
        .expect("uname runs");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn n7_a_good_artifact_installs_and_the_binary_is_runnable() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let release_dir = dir.path().join("release");
    let install_dir = dir.path().join("bin");
    std::fs::create_dir_all(&release_dir).expect("a release directory");
    release(&release_dir, false);

    let output = run_installer(&release_dir, &install_dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the installer failed\nstdout: {stdout}\nstderr: {stderr}",
    );
    assert!(stdout.contains("checksum ok"), "{stdout}");

    let installed = install_dir.join("io");
    assert!(installed.is_file(), "nothing was installed\n{stdout}");
    let run = Command::new(&installed).output().expect("the binary runs");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("9.9.9"),
        "the installed file is not the one from the archive",
    );

    // It printed the PATH line rather than editing a shell profile.
    assert!(
        stdout.contains("export PATH="),
        "the script should print the PATH line to add: {stdout}",
    );
}

#[test]
fn n7_a_corrupt_artifact_aborts_non_zero_and_leaves_nothing_behind() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let release_dir = dir.path().join("release");
    let install_dir = dir.path().join("bin");
    std::fs::create_dir_all(&release_dir).expect("a release directory");
    release(&release_dir, true);

    let output = run_installer(&release_dir, &install_dir);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a corrupt artifact must abort non-zero",
    );
    assert!(
        stderr.contains("checksum mismatch"),
        "the failure should say what happened: {stderr}",
    );
    assert!(
        stderr.contains("Nothing was installed"),
        "and should say what it did not do: {stderr}",
    );
    assert!(
        !install_dir.exists(),
        "the target directory was created or written to despite the mismatch",
    );
}

#[test]
fn n7_a_missing_checksum_line_is_a_refusal_rather_than_a_skip() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let release_dir = dir.path().join("release");
    let install_dir = dir.path().join("bin");
    std::fs::create_dir_all(&release_dir).expect("a release directory");
    release(&release_dir, false);
    // A SHA256SUMS that does not cover this artifact. Verification cannot be
    // skipped just because there is nothing to compare against — that is the one
    // path where an installer would do the thing it exists to prevent.
    std::fs::write(
        release_dir.join("SHA256SUMS"),
        "deadbeef  something-else.tar.gz\n",
    )
    .expect("SHA256SUMS");

    let output = run_installer(&release_dir, &install_dir);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not mention"),
        "{}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!install_dir.exists());
}

#[test]
fn n8_neither_installer_asks_for_administrator_rights() {
    let shell = std::fs::read_to_string(repo().join("install.sh")).expect("install.sh");
    let powershell = std::fs::read_to_string(repo().join("install.ps1")).expect("install.ps1");

    for (name, text) in [("install.sh", &shell), ("install.ps1", &powershell)] {
        for forbidden in ["sudo ", "doas ", "runas", "RunAs", "-Verb Administrator"] {
            assert!(
                !text.contains(forbidden),
                "{name} contains {forbidden:?}; an install must need no privileges",
            );
        }
    }

    // The unix default is under the user's own home, and the Windows one is under
    // the user's own profile. Neither is a system path.
    assert!(shell.contains("$HOME/.local/bin"), "{shell}");
    assert!(powershell.contains("LOCALAPPDATA"), "{powershell}");
    assert!(
        powershell.contains("'User'"),
        "install.ps1 must set the USER PATH; the machine PATH needs administrator rights",
    );
    assert!(
        !powershell.contains("'Machine'"),
        "install.ps1 must not touch the machine PATH",
    );
}

#[test]
fn n7_both_scripts_verify_before_they_unpack() {
    // Asserted on the text as well as by running it, because the ORDER is the
    // property: a script that unpacked first and checked afterwards would pass
    // the behavioural test above and still have written an unverified file.
    let shell = std::fs::read_to_string(repo().join("install.sh")).expect("install.sh");
    let check = shell.find("checksum mismatch").expect("a mismatch branch");
    let unpack = shell.find("tar xzf").expect("an unpack step");
    assert!(check < unpack, "install.sh unpacks before it verifies",);

    let powershell = std::fs::read_to_string(repo().join("install.ps1")).expect("install.ps1");
    let check = powershell
        .find("checksum mismatch")
        .expect("a mismatch branch");
    let expand = powershell.find("Expand-Archive").expect("an expand step");
    assert!(check < expand, "install.ps1 expands before it verifies",);
}

#[test]
fn the_artifact_names_match_what_the_release_workflow_builds() {
    // The install command is the first thing a user depends on, so the naming the
    // installers resolve and the naming the workflow produces are one fact. They
    // are in two files, so they are checked against each other here.
    let workflow =
        std::fs::read_to_string(repo().join(".github/workflows/release.yml")).expect("release.yml");
    let shell = std::fs::read_to_string(repo().join("install.sh")).expect("install.sh");
    let powershell = std::fs::read_to_string(repo().join("install.ps1")).expect("install.ps1");

    assert!(
        workflow.contains(r#"stage="io-$VERSION-$TARGET""#),
        "the workflow's artifact naming changed",
    );
    assert!(
        shell.contains(r#"stage="$BIN-$version-$target""#),
        "{shell}"
    );
    assert!(powershell.contains(r#"$stage = "io-$Version-$target""#));

    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-musl",
        "x86_64-pc-windows-msvc",
    ] {
        assert!(
            workflow.contains(target),
            "{target} is not in the release matrix",
        );
    }
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-musl",
    ] {
        assert!(shell.contains(target), "install.sh cannot resolve {target}");
    }
    assert!(powershell.contains("x86_64-pc-windows-msvc"));
}

/// F10 — `install.sh` prints the whole install, in order, with the values in it.
///
/// The archive's real checksum is computed here and matched against BOTH the
/// expected and the computed line, which is the sabotage this test exists for:
/// drop the computed number and print only "checksum ok" and this fails on the
/// `computed <sha>` line — the one whose value is that an operator can see the
/// comparison rather than be told its result. Everything else is matched by its
/// value too (the resolved target, the URLs actually fetched, the destination,
/// the installed binary's own `--version` output), because a narration of
/// constants would pass a test of constants and tell an operator nothing.
#[test]
fn f10_the_shell_installer_narrates_the_install_in_order() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let release_dir = dir.path().join("release");
    let install_dir = dir.path().join("bin");
    std::fs::create_dir_all(&release_dir).expect("a release directory");
    let archive = release(&release_dir, false);
    let sum = sha256(&archive);

    let output = run_installer(&release_dir, &install_dir);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "the installer failed\n{stdout}");

    let base = format!("file://{}", release_dir.display());
    let stage = format!("io-{VERSION}-{}", target());
    assert_ordered(
        "install.sh",
        &stdout,
        &[
            // The detected OS and architecture are the machine's own words for
            // itself, not this test's guess at them, and then the target.
            format!(
                "detected {} {} -> target {}",
                uname("-s"),
                uname("-m"),
                target()
            ),
            format!("version {VERSION} (from IO_VERSION)"),
            format!("downloading {base}/{stage}.tar.gz"),
            format!("downloading {base}/SHA256SUMS"),
            format!("expected {sum}"),
            format!("computed {sum}"),
            "checksum ok".to_string(),
            format!("unpacked {stage}.tar.gz"),
            format!("installed {}", install_dir.join("io").display()),
            format!("{} is not on your PATH", install_dir.display()),
            "io 9.9.9 (a stand-in)".to_string(),
        ],
    );

    // The count is part of the criterion, not an accident of it: a script that
    // prints twenty lines for a two-second install is worse than one that prints
    // one, so the sequence above is the whole narration and nothing has been
    // slipped in beside it. Sixteen is nine narration lines, the four-line PATH
    // advice with its blank lines, and the binary's own version line.
    assert_eq!(
        stdout.lines().count(),
        16,
        "the narration grew or shrank; it is a decided sequence, not a volume\n{stdout}",
    );
}

/// F11 — the narration is on stdout and every diagnostic stays on stderr.
///
/// Two runs, because one alone proves nothing: a good install must leave stderr
/// completely empty, and a checksum mismatch must put its whole message on stderr
/// with none of it on stdout. That is the sabotage — send the narration to stderr
/// and the good run fails here, because the one line an operator greps a log for
/// would be buried in a report of what went right.
#[test]
fn f11_the_narration_is_on_stdout_and_the_diagnostics_are_on_stderr() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let release_dir = dir.path().join("release");
    std::fs::create_dir_all(&release_dir).expect("a release directory");
    release(&release_dir, false);

    let good = run_installer(&release_dir, &dir.path().join("bin"));
    assert!(good.status.success());
    assert_eq!(
        String::from_utf8_lossy(&good.stderr).trim(),
        "",
        "an install that went right must say nothing on stderr",
    );
    assert!(String::from_utf8_lossy(&good.stdout).contains("checksum ok"));

    let bad_dir = tempfile::tempdir().expect("a temporary directory");
    let bad_release = bad_dir.path().join("release");
    std::fs::create_dir_all(&bad_release).expect("a release directory");
    release(&bad_release, true);

    let bad = run_installer(&bad_release, &bad_dir.path().join("bin"));
    let stdout = String::from_utf8_lossy(&bad.stdout);
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert_eq!(
        bad.status.code(),
        Some(1),
        "a mismatch exits 1, exactly as it always has",
    );
    assert!(stderr.contains("checksum mismatch"), "{stderr}");
    assert!(stderr.contains("Nothing was installed"), "{stderr}");
    assert!(
        !stdout.contains("checksum mismatch") && !stdout.contains("Nothing was installed"),
        "the failure leaked onto stdout: {stdout}",
    );
    // It still narrated everything it did before it refused.
    assert!(
        stdout.contains("expected ") && stdout.contains("computed "),
        "{stdout}"
    );
    assert!(!stdout.contains("checksum ok"), "{stdout}");
}

/// F11 — a missing tool is the same refusal, with the same status, as before.
///
/// Run with a PATH that has nothing on it: `uname` is the first thing the script
/// asks for, so this is the missing-tool `die` and not another one. The sabotage
/// is any change that makes a missing tool quieter (a warning and a guess at the
/// target) or louder (a stack of narration before the refusal): stdout has to be
/// empty and the exit status has to be 1.
#[test]
fn f11_a_missing_tool_is_the_refusal_it_has_always_been() {
    let output = Command::new("/bin/sh")
        .arg(repo().join("install.sh"))
        .env("PATH", "/nonexistent")
        .env("IO_VERSION", VERSION)
        .output()
        .expect("the installer runs");

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("this script needs uname"),
        "{}",
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "",
        "a refusal narrates nothing on stdout",
    );
}

/// F11 — `install.ps1` refuses an unsupported architecture on stderr, exit 1.
///
/// This is the half of the Windows story that can be run anywhere: no download,
/// no archive, no `file://` URL that `Invoke-WebRequest` would not accept — just
/// the first `Fail` in the script. Where `pwsh` is not installed the test SKIPS
/// rather than fails, so the other half (the ordered narration) is covered by
/// `f10_the_powershell_installer_prints_the_same_ordered_sequence`, which reads
/// the source. When `pwsh` is here, both halves have run.
#[test]
fn f11_the_powershell_installer_refuses_an_unsupported_architecture() {
    let output = Command::new("pwsh")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-File")
        .arg(repo().join("install.ps1"))
        .env("PROCESSOR_ARCHITECTURE", "ARM64")
        .env("IO_VERSION", VERSION)
        .output();
    let Ok(output) = output else {
        eprintln!("skipped: pwsh is not installed on this machine");
        return;
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "stderr: {stderr}");
    assert!(
        stderr.contains("there is no Windows ARM64 build yet"),
        "the refusal must keep its message: {stderr}",
    );
    assert!(
        !stdout.contains("downloading") && !stdout.contains("version "),
        "it narrated an install it refused to do: {stdout}",
    );
}

/// F10 — `install.ps1` prints the same ordered sequence as `install.sh`.
///
/// Read from the source, which is second best and admitted as such: the fixture
/// the shell test installs from is a `file://` release, and `Invoke-WebRequest`
/// will not fetch one, so there is no way to run this script end to end here. The
/// order is still the property being asserted — a narration whose lines come in a
/// different order is a different narration — and the same sabotage fails it:
/// remove the computed checksum line and this stops finding it.
#[test]
fn f10_the_powershell_installer_prints_the_same_ordered_sequence() {
    // git hands Windows a CRLF working copy, and every fragment below would then
    // be found at the wrong offsets or not at all.
    let powershell = std::fs::read_to_string(repo().join("install.ps1"))
        .expect("install.ps1")
        .replace("\r\n", "\n");

    assert_ordered(
        "install.ps1",
        &powershell,
        &[
            r#"Write-Host "detected Windows $arch -> target $target""#.to_string(),
            r#"Write-Host "version $Version (from $versionFrom)""#.to_string(),
            r#"Write-Host "downloading $BaseUrl/$archive""#.to_string(),
            r#"Write-Host "downloading $BaseUrl/SHA256SUMS""#.to_string(),
            r#"Write-Host "expected $expected""#.to_string(),
            r#"Write-Host "computed $actual""#.to_string(),
            "Write-Host 'checksum ok'".to_string(),
            r#"Write-Host "unpacked $archive""#.to_string(),
            r#"Write-Host "installed $installed""#.to_string(),
            "your user PATH".to_string(),
            "& $installed --version".to_string(),
        ],
    );
}
