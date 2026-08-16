//! N7 — an installer never runs an unverified binary.
//! N8 — an install needs no administrator rights.
//!
//! Both scripts are exercised against a **local release**: a directory holding a
//! real archive and a real `SHA256SUMS`, served over `file://`. That is enough to
//! prove the verification and the no-privileges properties, which are the two
//! this repository can assert. What it cannot prove is O5 — that the script works
//! on a clean machine, in a new shell, with nothing else installed — and that is
//! deliberately a human step against the published Release, because an installer
//! verified only on the machine that built it is not verified.

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
