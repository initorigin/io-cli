//! Where this binary came from, and the one command that updates it.
//!
//! Three install paths reach the same four artifacts — the scripts, a Homebrew
//! tap, and a scoop bucket — and each is updated by a different command. A
//! binary that cannot say which one placed it has made updating harder than the
//! single-path release it replaced.
//!
//! **This module contacts nothing and runs nothing.** It reads
//! [`std::env::current_exe`] and prints. Asking GitHub for the latest version
//! would need an HTTP client, which `tests/dependencies.rs` fails the build
//! over; running the upgrade would need a third permitted process spawn beside
//! `src/shell.rs` and `src/fetch.rs`, and widening that set — compared with `==`
//! precisely so it cannot widen itself — to save an operator a copy and paste is
//! the worst trade available here. The README's standing sentence, that this
//! product contacts no server it was not asked to, stays true because the
//! decision is taken from a path.
//!
//! The decision is a free function over a path rather than a read of the real
//! executable, because nothing under `tests/` links `src/main.rs` and a branch
//! only reachable from an installed binary is a branch no test can drive. The
//! driver supplies the path; everything decidable is here.

use std::path::Path;

/// How the running binary was installed, as far as its own path can say.
///
/// `Unknown` is a real answer and not a failure. A binary copied to
/// `/usr/local/bin` by hand, installed by a distribution package, or built from
/// source is none of the three, and telling such an operator to re-run the
/// installer would be advice that replaces a binary the installer never placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    /// Under a Homebrew Cellar or prefix, on macOS or Linux.
    Homebrew,
    /// Under a scoop `apps` directory.
    Scoop,
    /// Where `install.sh` and `install.ps1` put it — `~/.local/bin`, or
    /// `%LOCALAPPDATA%\io\bin`.
    Installer,
    /// Somewhere none of the three put it.
    Unknown,
}

/// The command that updates a Homebrew install.
pub const HOMEBREW: &str = "brew upgrade io";

/// The command that updates a scoop install.
pub const SCOOP: &str = "scoop update io";

/// The command that updates a script install on macOS or Linux.
pub const INSTALLER_UNIX: &str =
    "curl -fsSL https://raw.githubusercontent.com/initorigin/io-cli/main/install.sh | sh";

/// The command that updates a script install on Windows.
pub const INSTALLER_WINDOWS: &str =
    "irm https://raw.githubusercontent.com/initorigin/io-cli/main/install.ps1 | iex";

/// Which of the three placed the binary at `exe`.
///
/// **Homebrew is recognised by `Cellar` and by the prefix both**, because
/// `current_exe` does not promise a resolved path. On Linux it comes from
/// `/proc/self/exe` and is fully resolved, so a tap install reads as
/// `…/Cellar/io/<version>/bin/io`; on macOS it can come back as the symlink in
/// `/opt/homebrew/bin`, which contains no `Cellar` component at all. Matching
/// either spelling costs one more arm and covers both platforms' truth.
///
/// The comparison is on whole path components and never on a substring. A
/// directory called `scoop-notes` under `$HOME` is not a scoop install, and a
/// substring match is a classifier that widens itself — the same argument
/// `tests/dependencies.rs` makes for its permitted-path set.
#[must_use]
pub fn installed(exe: &Path) -> Installed {
    let parts: Vec<&str> = exe
        .components()
        .filter_map(|part| part.as_os_str().to_str())
        .collect();
    let has = |name: &str| parts.iter().any(|part| *part == name);

    if has("Cellar") || has("homebrew") || has(".linuxbrew") {
        return Installed::Homebrew;
    }
    // scoop keeps every package under `<root>/apps/<name>/<version>/`, and the
    // root is relocatable through `$SCOOP` — so the marker is the pair, not the
    // default location. `apps` alone is far too common a directory name to
    // decide anything on.
    if has("scoop") && has("apps") {
        return Installed::Scoop;
    }
    if let Some(dir) = exe.parent() {
        let ends_with = |a: &str, b: &str| {
            let mut tail = dir.components().rev().filter_map(|p| p.as_os_str().to_str());
            tail.next() == Some(b) && tail.next() == Some(a)
        };
        // `install.sh` writes `$HOME/.local/bin` and `install.ps1` writes
        // `%LOCALAPPDATA%\io\bin`. Both are matched by their last two
        // components rather than by an absolute path, because `IO_INSTALL_DIR`
        // moves the first and the second is under a variable this process would
        // have to re-read to spell.
        if ends_with(".local", "bin") || ends_with("io", "bin") {
            return Installed::Installer;
        }
    }
    Installed::Unknown
}

/// What `io upgrade` prints for a binary at `exe`.
///
/// The first line is the command, alone on its line so it can be copied without
/// picking prose out of it. Anything else is context beneath it.
#[must_use]
pub fn advice(exe: &Path, windows: bool) -> Vec<String> {
    let installer = if windows {
        INSTALLER_WINDOWS
    } else {
        INSTALLER_UNIX
    };
    match installed(exe) {
        Installed::Homebrew => vec![
            HOMEBREW.to_string(),
            String::new(),
            "io was installed by Homebrew, from the tap in this repository."
                .to_string(),
        ],
        Installed::Scoop => vec![
            SCOOP.to_string(),
            String::new(),
            "io was installed by scoop, from the bucket in this repository.".to_string(),
        ],
        Installed::Installer => vec![
            installer.to_string(),
            String::new(),
            "io was installed by the script; re-running it is how it updates.".to_string(),
        ],
        // Named as a limit of what a path can tell rather than as a default, so
        // an operator whose binary is somewhere unexpected is told that this
        // command did not recognise it — which is a different statement from
        // "use the installer", and the only honest one available.
        Installed::Unknown => vec![
            installer.to_string(),
            String::new(),
            format!(
                "io is at {}, which is not where Homebrew, scoop or the \
                 installer put a binary — so this is a guess. If a package \
                 manager placed it, update it with that.",
                exe.display()
            ),
        ],
    }
}
