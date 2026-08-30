//! The plugin set, resolved once for the session instead of twice for every turn.
//!
//! **`Config::plugins()` is not an accessor.** It is
//! `Plugins::load(&self.plugin_decls, &self.dir)` — a fresh read of every declared
//! bundle's `plugin.toml` off disk, parsed, validated and trust-checked, every
//! single call. Until 0.32.0 io-cli called it twice on the build path of every
//! turn (`contract::configured` and `contract::hooks`), twice more each time
//! `/plugin` opened, and again for `/skills` — and `Skills::discover` alongside it
//! reads the **whole body** of every skill file to keep a name and a description,
//! then throws the body away.
//!
//! io-harness says "once per run" and is right. The trouble is that every io-cli
//! turn *is* a run, so a session spent the cost on every message, and it scales
//! with the number of installed bundles — which 0.29.0 through 0.31.0 spent three
//! releases making it easy to grow. A marketplace that works is a marketplace that
//! makes the product slower on every turn.
//!
//! # What this is not
//!
//! It is not a cache in front of a call anyone may still make. `config.plugins()`
//! is confined **by exact path** to this module, in the same shape
//! `tests/dependencies.rs` already uses for the two permitted process spawns — so
//! a later reader cannot accidentally reintroduce the per-turn resolution, which
//! is the only way a confinement like this stays true.
//!
//! (Spelled without naming that type, because the sweep enforcing it reads this
//! file's text and does **not** strip comments first — which this module found out
//! by tripping it.)
//!
//! # Freshness
//!
//! [`Resolved::stale`] **re-stats the paths it recorded and nothing else.** It
//! opens no manifest and calls no loader, which is the whole point: a check that
//! resolved the set in order to decide whether to resolve the set would leave the
//! per-turn cost exactly where it was. The first draft did precisely that — it
//! compared `stamp_of(&config.plugins())`, and `Config::plugins()` is the full
//! read — so the release halved a cost it claimed to have removed. The adversarial
//! review caught it; the claim and the code now agree.
//!
//! Two kinds of path are stamped. Each bundle's `plugin.toml`, so an **edit** is
//! seen; and every file [`Config::sources`] names, so a bundle **declared or
//! undeclared** is seen — a new `[[plugin]]` entry changes the file that holds it.
//! A bundle whose directory is removed fails its stat, which is a change too.
//!
//! Each path is compared on its modified time **and** its length. Two writes
//! inside one second that leave the length unchanged are the case a filesystem
//! with one-second mtime granularity cannot distinguish, and this will not see the
//! second one until something else about the bundle changes. That is stated here,
//! in `docs/guide/plugins.md`, and asserted — a cache that cannot prove freshness
//! should say so rather than imply a guarantee it has not got.

use std::path::PathBuf;
use std::time::SystemTime;

use io_harness::{Config, Plugins};

/// What was on disk when the plugin set was last resolved.
///
/// The manifest's path, and the two facts a stat gives cheaply. `None` for a
/// manifest that could not be stated at all, which is itself a change worth
/// noticing when it comes back.
type Stamp = Vec<(PathBuf, Option<(SystemTime, u64)>)>;

/// The plugin set this session is running under.
pub struct Resolved {
    plugins: Plugins,
    stamp: Stamp,
}

impl Resolved {
    /// Read every declared bundle, once.
    ///
    /// **Blocking, and deliberately not hidden.** The whole point of this module
    /// is that the read happens where the caller can put it somewhere sensible —
    /// the driver runs it under `block_in_place`, off the task turning the event
    /// loop — rather than inside a contract builder that had no idea it was doing
    /// disk I/O.
    pub fn load(config: &Config) -> Self {
        let plugins = config.plugins();
        let stamp = stamp_of(watched(&plugins, config));
        Self { plugins, stamp }
    }

    /// The resolved set, for everything that used to resolve it itself.
    ///
    /// **Named `loaded` and not `plugins` on purpose.** The gate that confines
    /// the resolution bans the bare method call by name, so an accessor sharing
    /// that name would make the ban unenforceable without also knowing what every
    /// receiver in the crate is — which is a permission list that widens itself.
    /// A different name keeps the needle exact.
    pub fn loaded(&self) -> &Plugins {
        &self.plugins
    }

    /// Whether anything on disk has moved since this was resolved.
    ///
    /// **Stats only.** No manifest is opened and no loader is called, so asking
    /// costs a bounded number of `metadata` calls rather than the resolution it is
    /// deciding whether to repeat — which is the difference between removing the
    /// per-turn cost and merely halving it.
    ///
    /// The paths re-stated here are the ones recorded at load: each bundle's
    /// manifest, and every configuration file `Config::sources` named. Comparing
    /// the whole set is what catches a bundle declared or undeclared, because a
    /// new `[[plugin]]` entry changes the file that holds it, and what catches a
    /// bundle directory that has been removed, because its stat then fails.
    pub fn stale(&self, config: &Config) -> bool {
        let paths: Vec<PathBuf> = self
            .stamp
            .iter()
            .map(|(path, _)| path.clone())
            .chain(config.sources().iter().map(|(_, path)| path.clone()))
            .collect();
        stamp_of(paths) != self.stamp
    }
}

/// Every path whose movement means the plugin set has to be read again: each
/// bundle's manifest, and each configuration file that could declare one.
///
/// **Disabled bundles are watched too.** A bundle switched off is still declared,
/// still parsed by the loader, and still something an operator can edit — leaving
/// it out would mean an edit to a disabled bundle never showed up when it was
/// switched back on.
fn watched(plugins: &Plugins, config: &Config) -> Vec<PathBuf> {
    plugins
        .iter()
        .chain(plugins.disabled())
        .map(|plugin| plugin.root().join(io_harness::PLUGIN_FILE))
        .chain(config.sources().iter().map(|(_, path)| path.clone()))
        .collect()
}

/// What a stat says about each of `paths`, right now.
fn stamp_of(paths: impl IntoIterator<Item = PathBuf>) -> Stamp {
    let mut out: Stamp = paths
        .into_iter()
        .map(|path| {
            let facts = std::fs::metadata(&path)
                .ok()
                .and_then(|meta| meta.modified().ok().map(|at| (at, meta.len())));
            (path, facts)
        })
        .collect();
    // Ordered and deduplicated, so two readings of the same disk compare equal
    // however the declarations were listed — and so a path named by both a bundle
    // and a configuration file is stamped once.
    out.sort();
    out.dedup();
    out
}
