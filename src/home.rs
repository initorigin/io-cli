//! Where io-cli keeps what it keeps: one directory, `~/.io-cli`.
//!
//! Before 0.15.0 the answer was whatever io-harness's own resolution decided —
//! `~/.config/io` on a Linux box, `$XDG_CONFIG_HOME/io` where that was set,
//! `%APPDATA%\io` on Windows — with the run store beside whichever one applied.
//! Three answers, none of them the product's name, and an operator who wanted to
//! back their sessions up had to reconstruct a ladder from a README paragraph.
//!
//! This module names one. It is deliberately **not** a second configuration
//! system: io-harness resolves `$IO_CONFIG`, then `$IO_CONFIG_HOME`, then the
//! platform's own place, reading the environment at call time
//! (`io-harness-0.78.0/src/config.rs:2241`, the Windows branch at `:2259`), and
//! there is no caller-supplied home
//! anywhere in `Config`'s public surface — no `discover_in`, no builder. So the
//! one lever is `$IO_CONFIG_HOME`, set once, before the first
//! [`io_harness::Config::discover`]. Everything else here follows from that:
//! because the store is derived from the configuration file's directory
//! ([`crate::settings::store_path`]), naming the directory moves both.
//!
//! Two rules the rest of the module exists to keep. An operator who has already
//! chosen is never moved — either variable set to a non-empty value and [`adopt`]
//! does nothing at all. And nothing is destroyed: a file is never moved onto one
//! that exists, and a source is removed only after its copy has been read back.

use std::path::{Path, PathBuf};

/// The directory name, under the operator's own home.
const DIR: &str = ".io-cli";

/// The configuration file's name, which is io-harness's and not ours.
const FILE: &str = "io.toml";

/// The skills directory, created with the home so the default is a real place.
///
/// `Skills::discover` **errors** on a directory that does not exist — it does not
/// walk away from one — and `TaskContract::discover_skills` propagates that with
/// `?` at run start, before the first completion. So a default pointing at a
/// directory nobody has made is not an empty catalogue; it is every turn failing.
/// Making it here is what lets the default be unconditional, and it is also the
/// only way an operator finds out where to put a skill without reading a document.
const SKILLS: &str = "skills";

/// The marketplaces directory, created with the home for the reason [`SKILLS`] is.
///
/// A marketplace is a git repository the operator named, cloned to
/// `<home>/marketplaces/<owner>/<repo>` — two levels, so two owners may carry a
/// repository of the same name and neither has to be qualified on disk.
///
/// Created here as well as by [`crate::fetch`], which has to make it anyway
/// because [`adopt`] does nothing at all for an operator who named their own
/// location. What creating it here buys is the same thing `skills/` buys: an
/// operator who opens their own home finds the directory before they have added
/// anything to it, which is how they learn where marketplaces go without reading a
/// document.
const MARKETPLACES: &str = "marketplaces";

/// Where the `plugin.toml` io writes for a foreign bundle is kept.
///
/// A bundle in the field is a Claude Code or a Codex plugin and carries no
/// `plugin.toml` at all; [`crate::adapt::generate`] writes one that io-harness
/// loads, pointing at the clone. **The generated file is io's own and is never
/// written inside the clone** — a marketplace is a stranger's checkout and
/// `src/marketplace.rs` keeps it untouched — so it needs a directory of its own,
/// and this is it.
///
/// Three levels under it, `<owner>/<repo>/<name>`, which is [`MARKETPLACES`]'s
/// own two-level layout with the bundle's own name under it: one clone publishes
/// many bundles, and a `plugin.toml` is recognised by sitting at a directory's
/// root, so two bundles cannot share one.
///
/// Not created by [`adopt`], for [`STAGING`]'s reason: nothing is here until a
/// bundle has been adapted, and a home carrying an empty `adapters` would be one
/// more directory for an operator to wonder about.
const ADAPTERS: &str = "adapters";

/// Where a clone is assembled before it becomes a marketplace.
///
/// **Dot-named, and deliberately outside [`MARKETPLACES`].** What is in here is by
/// definition unfinished: a process killed in the middle of a clone leaves a
/// directory holding part of somebody's repository, and if that directory sat
/// under `marketplaces/` it would be walked as an `<owner>` and its contents
/// counted as bundles. Outside it, nothing walks it and the worst a kill can leave
/// is wasted disk that the next fetch removes.
///
/// Not created by [`adopt`]: it exists only while a fetch is in flight, and a home
/// carrying an empty `.fetching` would be one more thing for an operator to
/// wonder about — the same argument that keeps [`MEMORY`] out of [`adopt`].
const STAGING: &str = ".fetching";

/// The operator's own guidance file, beside the configuration file it belongs to.
///
/// Named here rather than in [`crate::memory`] because the name is a fact about
/// this directory: it is what sits next to [`FILE`], it is io-cli's own and not
/// io-harness's, and an operator backing the home up gets it with everything
/// else. Deliberately **not** created with the home — an empty guidance file is
/// a file the operator has to wonder about, and [`crate::memory::remember`]
/// makes it with a header the moment there is a first line to put in it.
pub(crate) const MEMORY: &str = "IO.md";

/// The store and the two files SQLite keeps beside it.
///
/// The siblings are not decoration. A `runs.db` moved without its `-wal` is a
/// store missing every transaction the last session did not checkpoint, and
/// SQLite opens it without complaint — so the loss shows up as a session that
/// vanished rather than as an error anybody can act on.
const STORE: [&str; 3] = ["runs.db", "runs.db-wal", "runs.db-shm"];

/// What decided the directory the configuration and the store are in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// io-cli's own home, because the operator named neither variable.
    Default,
    /// `$IO_CONFIG`, which names the file outright and wins over everything.
    Config,
    /// `$IO_CONFIG_HOME`, which names the directory.
    ConfigHome,
}

impl Origin {
    /// The word `/status` prints beside the path.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Origin::Default => "default",
            Origin::Config => io_harness::config::CONFIG_VAR,
            Origin::ConfigHome => io_harness::config::CONFIG_HOME_VAR,
        }
    }
}

/// What [`adopt`] did, in the order it did it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The home in force after adoption.
    pub home: PathBuf,
    /// Each file that moved, as it was and as it is.
    pub moved: Vec<(PathBuf, PathBuf)>,
    /// Each file left where it was because the home already held one, as
    /// (the file left behind, the file in force).
    pub kept: Vec<(PathBuf, PathBuf)>,
    /// The file that could not be moved, where one could not be.
    ///
    /// **A migration that half happens is worse than one that does not**, and
    /// Windows is where it can: that platform refuses to rename a file another
    /// process holds open, so a second `io` running while this one starts would
    /// let `io.toml` move while `runs.db` stayed behind — a configuration in the
    /// new home, a store in the old one, and a `/resume` that finds nothing. When
    /// this is `Some`, everything that had already moved has been moved back, the
    /// environment was not touched, and the directory in force is the one it
    /// always was.
    pub blocked: Option<PathBuf>,
}

impl Report {
    /// One line per file, then one naming the home.
    ///
    /// The home line is last and unconditional: on a run that moved nothing it is
    /// the whole report, which is the product answering "where does it live"
    /// without being asked. Lines rather than a paragraph because the session
    /// commits each one into the scrollback and `io exec` writes each to stderr.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        if let Some(blocked) = &self.blocked {
            return vec![
                format!(
                    "could not move {} — another io may be running",
                    blocked.display()
                ),
                format!("io is still using {}", self.home.display()),
            ];
        }
        let mut out = Vec::with_capacity(self.moved.len() + self.kept.len() + 1);
        for (from, to) in &self.moved {
            out.push(format!("moved {} to {}", from.display(), to.display()));
        }
        for (left, force) in &self.kept {
            out.push(format!(
                "kept {}; {} is left where it was",
                force.display(),
                left.display()
            ));
        }
        out.push(format!("io keeps its files in {}", self.home.display()));
        out
    }
}

/// A variable that names something, treating an empty value as unset.
///
/// io-harness's own `env_dir` does exactly this, and reading it the other way
/// would be worse than inconsistent: an empty `IO_CONFIG_HOME` would leave the
/// harness with no user scope at all while io-cli believed the operator had
/// chosen one, so the session would have neither a configuration file nor a home.
fn named(var: &str) -> Option<PathBuf> {
    let value = std::env::var_os(var)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// The operator's own home directory.
///
/// `HOME` on unix, `USERPROFILE` on Windows — and Windows is why this is io-cli's
/// rule rather than a call into the harness, whose Windows branch reads `APPDATA`
/// and never consults a profile root. One path on every platform was the outcome;
/// `%APPDATA%\io-cli` would have been a fourth answer rather than one.
fn operator() -> Option<PathBuf> {
    #[cfg(windows)]
    let var = "USERPROFILE";
    #[cfg(not(windows))]
    let var = "HOME";
    named(var)
}

/// io-cli's home, whether or not it is the one in force.
///
/// `None` where the operator's home directory cannot be determined, which is the
/// same shape [`io_harness::config::user_path`] returns for the same reason: a
/// program that invents a path when it has no home writes into somebody else's.
#[must_use]
pub fn path() -> Option<PathBuf> {
    Some(operator()?.join(DIR))
}

/// Where marketplaces are cloned to, whether or not the directory exists yet.
///
/// Derived from [`path`] rather than from [`in_force`], which is the opposite of
/// what the default skills directory does: a marketplace is io-cli's own cache of
/// other people's repositories, not something the operator wrote, and authored
/// content follows the home in force while a cache stays with the crate that
/// filled it. An operator who pointed `$IO_CONFIG_HOME` somewhere else moved their
/// *configuration*; following that variable here would put the clones wherever the
/// configuration is and leave the ones already fetched invisible.
///
/// **The directory is not promised to exist.** [`adopt`] returns `None` without
/// creating anything whenever either variable is set, so every caller creates it
/// rather than assuming — which [`crate::fetch::clone`] does on the way past.
#[must_use]
pub fn marketplaces() -> Option<PathBuf> {
    Some(path()?.join(MARKETPLACES))
}

/// Where a foreign bundle's generated `plugin.toml` is written, whether or not the
/// directory exists yet.
///
/// Derived from [`path`] rather than from [`in_force`], for [`marketplaces`]'s
/// reason and it is the same reason one level on: an adapter is **io's own
/// generated file** — a translation of a clone that this crate writes, that
/// nobody authors and that is regenerated rather than edited — so it belongs with
/// the crate's own home the way a cache does, not with the configuration the
/// operator moved. An operator who pointed `$IO_CONFIG_HOME` somewhere else moved
/// their *configuration*; following that variable here would put the adapters
/// wherever the configuration is and leave every one already generated invisible,
/// while the `[[plugin]]` entries naming them still pointed at the old path.
///
/// **The directory is not promised to exist**, exactly as [`marketplaces`] is
/// not: [`adopt`] creates nothing at all when the operator has named their own
/// location, so every caller makes it rather than assuming — which
/// [`crate::adapt::generate`] does on the way past.
#[must_use]
pub fn adapters() -> Option<PathBuf> {
    Some(path()?.join(ADAPTERS))
}

/// Where a clone is assembled before it is renamed into [`marketplaces`].
///
/// Same home, so the rename that finishes a fetch is a same-filesystem rename with
/// no cross-device case to fall back from — which is the whole reason this is here
/// rather than in the platform's temporary directory, where `/tmp` on a Linux box
/// is routinely a different filesystem from `$HOME`.
#[must_use]
pub fn staging() -> Option<PathBuf> {
    Some(path()?.join(STAGING))
}

/// What the operator named, before anything of io-cli's own is set.
///
/// [`adopt`] decides on this and nothing else: a variable that is there at all is
/// the operator having chosen, whatever it points at.
fn chosen() -> Option<Origin> {
    if named(io_harness::config::CONFIG_VAR).is_some() {
        Some(Origin::Config)
    } else if named(io_harness::config::CONFIG_HOME_VAR).is_some() {
        Some(Origin::ConfigHome)
    } else {
        None
    }
}

/// What decided the directory in force, without changing anything.
///
/// This cannot simply be `chosen`, and the difference is the whole point: after
/// [`adopt`] runs there **is** an `IO_CONFIG_HOME` in the environment, because
/// io-cli put it there — so a status row reading the raw variable would credit the
/// operator for this crate's own default. The rule instead is that a variable
/// pointing at io-cli's own home reads as `default`, which is true when io-cli set
/// it and equally true when an operator set it to the same directory. That keeps
/// the answer a pure function of the environment, with no adoption-time state to
/// remember and nothing that can go stale between startup and a `/status` typed an
/// hour later.
#[must_use]
pub fn origin() -> Origin {
    match chosen() {
        Some(Origin::ConfigHome) if named(io_harness::config::CONFIG_HOME_VAR) == path() => {
            Origin::Default
        }
        Some(origin) => origin,
        None => Origin::Default,
    }
}

/// The directory the configuration file and the store are actually in, and what
/// decided it.
///
/// This is what `/status` shows, and it is deliberately derived from
/// [`io_harness::config::user_path`] rather than from [`path`]: under `$IO_CONFIG`
/// the file is somewhere io-cli did not choose, and a row reporting the home this
/// crate *would* have picked would be wrong in exactly the case the row exists for.
/// The directory an operator's **own** content lives in.
///
/// **One answer for reading and for writing, and 0.31.0 exists partly because
/// they were two.** A skill is something the operator wrote, like a memory note:
/// it belongs beside the configuration they are actually using, not beside the one
/// io-cli would have picked. But a directory that only the *read* followed would
/// be worse than either — `/skills add` would write where nothing looks, and an
/// operator who set `$IO_CONFIG_HOME` would watch skills they had installed stop
/// reaching the model with nothing said. So every surface that reads or writes
/// authored content asks this, and there is no second resolution to disagree with.
///
/// [`in_force`] with [`path`] as the fallback, because `user_path` answers `None`
/// on a machine with no home at all and io-cli's own default is still the better
/// guess than nothing. Deliberately **not** what [`marketplaces`] or [`adapters`]
/// use: those hold other people's repositories and io's own generated files, which
/// are a cache and belong with the crate.
#[must_use]
pub fn authored() -> Option<PathBuf> {
    in_force().map(|(dir, _)| dir).or_else(path)
}

#[must_use]
pub fn in_force() -> Option<(PathBuf, Origin)> {
    let dir = io_harness::config::user_path()?.parent()?.to_path_buf();
    Some((dir, origin()))
}

/// Expand a leading `~` against the operator's home directory.
///
/// io-harness substitutes `${env:…}` and `${file:…}` and nothing else
/// (`config.rs:3004`), so a `~` an operator writes in a path reaches the code that
/// uses it verbatim — and `Skills::discover` would then look in a directory
/// literally named `~`. Expanding it here rather than asking the harness to keeps
/// the substitution rules the harness's own.
#[must_use]
pub fn expand(path: &Path) -> PathBuf {
    let Ok(rest) = path.strip_prefix("~") else {
        return path.to_path_buf();
    };
    match operator() {
        Some(home) => home.join(rest),
        None => path.to_path_buf(),
    }
}

/// Take io-cli's home, moving an existing install into it, and report what happened.
///
/// Returns `None` — having changed nothing — when the operator has named a
/// location themselves, and when there is no home directory to work from. A fixed
/// default is not a forced one.
///
/// **Call this before the first [`io_harness::Config::discover`] and never after
/// one.** `user_path`'s own doctest asserts it answers the same thing twice; a
/// home adopted halfway through a process is precisely the moving answer that
/// assertion forbids, and the visible symptom would be a configuration read from
/// one directory while the store answered from another.
pub fn adopt() -> Option<Report> {
    if chosen().is_some() {
        return None;
    }
    let home = path()?;

    // Asked BEFORE the variable is set, because afterwards it answers the home
    // and there is nothing left to migrate from.
    let previous = io_harness::config::user_path();

    // Created BEFORE the variable is set, and the variable is not set at all if it
    // cannot be: pointing io-harness at a directory that does not exist and then
    // returning `None` would move the configuration path with nobody told, which
    // is the one outcome worse than not adopting a home.
    create(&home).ok()?;
    // Best effort, and deliberately not fatal: a home without a skills directory
    // is a home with no skills in it, while a home that could not be created at
    // all is a product with nowhere to live.
    let _ = create(&home.join(SKILLS));
    // The same best-effort, non-fatal shape, and for the same reason: a home
    // without a marketplaces directory is a home nobody has added a marketplace
    // to yet, and `crate::fetch` makes it when somebody does.
    let _ = create(&home.join(MARKETPLACES));

    let mut report = Report {
        home: home.clone(),
        moved: Vec::new(),
        kept: Vec::new(),
        blocked: None,
    };

    // **The variable is set only once every file that had to move has moved**, and
    // that ordering is the whole of the Windows story below: a home named while
    // half an install is still in the old directory is a configuration and a store
    // in two places.
    let Some(from) = previous.as_deref().and_then(Path::parent) else {
        std::env::set_var(io_harness::config::CONFIG_HOME_VAR, &home);
        return Some(report);
    };
    if from == home {
        std::env::set_var(io_harness::config::CONFIG_HOME_VAR, &home);
        return Some(report);
    }

    for name in std::iter::once(FILE).chain(STORE) {
        let source = from.join(name);
        let target = home.join(name);
        // A source that is not there is the ordinary case, not a failure: most
        // installs have no `-shm`, and a second `io` starting at the same moment
        // finds the file the first one already moved.
        if !source.exists() {
            continue;
        }
        if target.exists() {
            report.kept.push((source, target));
            continue;
        }
        if relocate(&source, &target).is_err() {
            // **Everything already moved goes back, and the home is not taken.**
            // Windows refuses to rename a file another process holds open, so a
            // second `io` running while this one starts is exactly how half an
            // install ends up in each directory. Undone rather than reported and
            // left, because the operator cannot put it back themselves without
            // knowing which four names to look for.
            for (was, is) in report.moved.iter().rev() {
                let _ = relocate(is, was);
            }
            return Some(Report {
                home: from.to_path_buf(),
                moved: Vec::new(),
                kept: Vec::new(),
                blocked: Some(source),
            });
        }
        report.moved.push((source, target));
    }

    std::env::set_var(io_harness::config::CONFIG_HOME_VAR, &home);
    Some(report)
}

/// Create the home, readable by its owner alone on unix.
///
/// The configuration file inside it already carries a credential and is written
/// `0600` by [`crate::settings::write`]; a world-readable directory around it is
/// the same mistake one level up.
///
/// Reachable from [`crate::memory`] as well, which writes `IO.md` into this
/// directory and would otherwise have to make it with a bare `create_dir_all` —
/// two answers to what mode io-cli's home has, one of them from a module that
/// has no business deciding. The mode applies only to directories this call
/// actually creates, so passing an existing one changes nothing.
/// The file recording that the import offer has been made.
///
/// **A file rather than a key in `io.toml`.** Declining an import is not a
/// configuration choice the operator should find in their own file later; it is
/// io-cli's bookkeeping about a question it has already asked. Putting it in the
/// configuration would also mean the offer could not be made to an operator whose
/// file will not parse — which is exactly the operator most likely to want it.
const OFFERED: &str = ".import-offered";

/// Whether the import offer has already been made.
///
/// **Absent means "not yet offered", which is the right reading for every install
/// that predates 0.21.0.** That is what makes this need no migration: an existing
/// operator has no marker, so they are offered once, which is the whole intent.
/// A home io-cli cannot locate answers `true` — there is nowhere to record the
/// answer, and an offer that cannot be remembered would be made on every launch.
#[must_use]
pub fn import_offered() -> bool {
    match path() {
        Some(home) => home.join(OFFERED).exists(),
        None => true,
    }
}

/// Record that the import offer has been made, whatever the operator said to it.
///
/// **Written when the offer is *made*, not when it is accepted.** An operator who
/// declines has answered the question, and asking again on the next launch would
/// make declining meaningless. `/import` ignores this entirely, so the choice is
/// never lost — it is only stopped from being asked unprompted a second time.
pub fn mark_import_offered() -> std::io::Result<()> {
    let Some(home) = path() else {
        return Ok(());
    };
    create(&home)?;
    std::fs::write(home.join(OFFERED), [])
}

pub(crate) fn create(home: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(home)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(home)
    }
}

/// Move one file, across filesystems if it has to.
///
/// `rename` is atomic and is what happens whenever the two are on one filesystem,
/// which is the ordinary case. Where they are not it fails with `EXDEV` and the
/// fallback copies — and then compares what was written against what was there
/// before removing the source, because a copy that was cut short and a source
/// removed anyway is the one outcome this whole module exists to avoid.
fn relocate(source: &Path, target: &Path) -> std::io::Result<()> {
    if std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    let expected = std::fs::metadata(source)?.len();
    let written = std::fs::copy(source, target)?;
    if written != expected || std::fs::metadata(target)?.len() != expected {
        return Err(std::io::Error::other(
            "the copy is not the size of what it was copied from",
        ));
    }
    std::fs::remove_file(source)
}
