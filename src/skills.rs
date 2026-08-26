//! The five skills io-cli ships, and how they get onto disk without breaking a
//! directory the operator already owns.
//!
//! A skill is a markdown file with two lines of frontmatter. io-harness discovers
//! every `*.md` in the skills directory, puts each one's `name` and `description`
//! into the system prompt on every turn, and lets the model read the body through
//! its own `read_skill` tool. That is the whole mechanism — there is no registry,
//! no index and no format of io-cli's own — so shipping a skill is writing a file
//! into `~/.io-cli/skills` and nothing else.
//!
//! Writing five files is not, however, three lines of [`std::fs::write`], and the
//! reason is that `Skills::discover` **rejects a set rather than repairing one**.
//! Two files resolving to one name is `Error::Config`, propagated with `?` at run
//! start by every io-harness entry point, which is every turn of that session
//! dead before the first completion. More than [`io_harness::skills::MAX_SKILLS`] in one
//! directory is the same error, and it rejects the whole set rather than the
//! excess — so an operator sitting at 62 skills who upgrades into five more would
//! get a dead session as their upgrade experience. The harness's own anti-collision
//! device, `Skills::namespaced` + `Skills::merged`, is `pub(crate)`, and
//! `TaskContract::with_skills` takes one directory and assigns it. io-cli cannot
//! reach any of it, so the guards are here or nowhere.
//!
//! Hence three rules, and every one of them is a thing this module refuses to do:
//!
//! 1. **It never takes a name the operator already claims.** The directory is read
//!    first, through the same [`io_harness::Skills::discover`] the run will use —
//!    not through io-cli's own idea of what a skill file is, which could disagree
//!    with the oracle in exactly the case that matters.
//! 2. **It counts the ceiling before it writes**, and stops short of it, saying how
//!    many it withheld.
//! 3. **It never resurrects a disabled skill.** `skills/disabled/io-mcp.md` is the
//!    operator having turned that one off; writing a fresh copy beside it on the
//!    next launch would make disabling a thing that undoes itself every time the
//!    product starts. `disabled/` is invisible to discovery for free, because the
//!    walk admits a subdirectory only when it holds a `SKILL.md`.
//!
//! # Refresh, and why it is decided from a manifest
//!
//! An upgrade should bring a shipped skill forward where nobody has touched it and
//! leave an edited one exactly as it is. The only honest question is **are these
//! the bytes io-cli last wrote**, and answering it needs a record of what was
//! written — [`MANIFEST`], one line per skill, in the home and deliberately *not*
//! in the skills directory, because discovery admits every `.md` in there and a
//! state file offered to the model as a skill called `manifest` is a worse bug
//! than the one it would be solving.
//!
//! Two roads not taken, both closed by a gate rather than by taste. Comparing the
//! bytes on disk against **this release's** shipped text instead would fail on the
//! second upgrade, not the first: every skill whose text did not change between
//! two releases would read as edited and stop being refreshed forever. And a
//! timestamp is unavailable — `tests/timing.rs` bans `Instant::now`,
//! `SystemTime::now` and `.elapsed()` in every file under `src/` but the driver,
//! so freshness here is decided by bytes and never by a clock, which is also the
//! only answer that survives a checkout whose mtimes are all identical.
//!
//! The digest is FNV-1a, six lines, written below. `tests/dependencies.rs` pins
//! the direct dependency set in both directions, so `sha2` and `blake3` are gate
//! failures rather than choices, and nothing here is a security boundary: the
//! question is whether a file the operator may have opened is byte-identical to
//! one io-cli wrote, and an adversary who can write into this directory has
//! already won by writing a skill of their own.
//!
//! # Nothing here ever fails a run
//!
//! [`install`] returns report lines, never an error. A read-only skills directory
//! is an operator with no shipped skills, which is the product they had before this
//! release; a session that refuses to start because of a directory they have never
//! heard of is not.

use std::path::{Path, PathBuf};

/// The skills directory, under io-cli's home. The same name
/// [`crate::home::adopt`] creates with the home and
/// [`crate::contract::skills_dir`] resolves to when no key names another.
const DIR: &str = "skills";

/// The subdirectory a disabled skill is moved into.
///
/// Invisible to `Skills::discover` for nothing: the walk admits a subdirectory
/// only when it contains a `SKILL.md` and otherwise skips it, so a folder of
/// loose `.md` files is not a skill, not in the catalogue, and not readable
/// through `read_skill`. That is why disabling is a rename and not a list.
pub const DISABLED: &str = "disabled";

/// What io-cli last wrote, one line per skill: `name<TAB>hex-hash`.
///
/// **In the home, not in the skills directory.** Every `*.md` in `skills/` is
/// offered to the model, so a manifest kept there would be a skill whose
/// description is a hash table. The leading dot is courtesy on top of that.
///
/// Plain text and not TOML on purpose: `tests/dependencies.rs` permits
/// `toml::from_str` in `src/edit.rs` alone, because a second module that parses a
/// configuration file is a second opinion about what one means. Two fields keyed
/// by a name is not a format that needs a parser.
pub const MANIFEST: &str = ".skills-manifest";

/// One skill io-cli ships: the name it resolves to, and its whole text.
///
/// The text is `include_str!`, so the five files are in the binary and there is
/// no install-time source to be missing, no path to resolve and nothing to
/// download. The name is stated here rather than derived from the file, because
/// the file's own `name:` frontmatter is what discovery will resolve and the two
/// must agree — `tests/skills.rs` asserts they do.
pub struct Shipped {
    /// The resolved name: what the model addresses the skill by, what the
    /// installed file is named after, and the manifest's key.
    pub name: &'static str,
    /// The file, frontmatter and all.
    pub text: &'static str,
}

/// The five, in name order, which is the order `Skills::discover` sorts into and
/// therefore the order a report reads in.
pub const SHIPPED: [Shipped; 5] = [
    Shipped {
        name: "io-mcp",
        text: include_str!("../skills/io-mcp.md"),
    },
    Shipped {
        name: "io-permissions",
        text: include_str!("../skills/io-permissions.md"),
    },
    Shipped {
        name: "io-provider",
        text: include_str!("../skills/io-provider.md"),
    },
    Shipped {
        name: "io-remember",
        text: include_str!("../skills/io-remember.md"),
    },
    Shipped {
        name: "io-update",
        text: include_str!("../skills/io-update.md"),
    },
];

/// `<home>/skills`.
#[must_use]
pub fn dir(home: &Path) -> PathBuf {
    home.join(DIR)
}

/// `<home>/skills/disabled`, whether or not it is there.
///
/// Not created here. It is made by the surface that first moves a file into it,
/// because an empty `disabled/` is a directory the operator has to wonder about.
#[must_use]
pub fn disabled_dir(home: &Path) -> PathBuf {
    dir(home).join(DISABLED)
}

/// `<home>/.skills-manifest`.
#[must_use]
pub fn manifest_path(home: &Path) -> PathBuf {
    home.join(MANIFEST)
}

/// The file a shipped skill is installed as, enabled or disabled.
fn file_name(name: &str) -> String {
    format!("{name}.md")
}

/// FNV-1a, 64-bit, over the file's bytes.
///
/// Not a security primitive and not asked to be one — see the module note. It is
/// here rather than in a crate because the dependency set is pinned in both
/// directions by a gate, and because six lines with no state is smaller than the
/// sentence justifying a dependency for it.
#[must_use]
pub fn digest(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// What the manifest says io-cli last wrote, as `(name, hash)` pairs.
///
/// **A manifest that is absent, unreadable or malformed reads as empty**, and the
/// direction of that failure is the whole point: an empty manifest means every
/// file already on disk is treated as the operator's and left alone. Degrading
/// the other way — "no record, so it must be ours" — would let a corrupt state
/// file authorise overwriting somebody's edits.
///
/// A `Vec` and a linear scan over five entries. An index here would be a data
/// structure nobody could justify at the next release.
fn recorded(home: &Path) -> Vec<(String, u64)> {
    let Ok(text) = std::fs::read_to_string(manifest_path(home)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (name, hash) = line.split_once('\t')?;
            let hash = u64::from_str_radix(hash.trim(), 16).ok()?;
            Some((name.to_string(), hash))
        })
        .collect()
}

/// The hash recorded for one name, if there is one.
fn recorded_hash(recorded: &[(String, u64)], name: &str) -> Option<u64> {
    recorded
        .iter()
        .find(|(recorded, _)| recorded == name)
        .map(|(_, hash)| *hash)
}

/// Record what was just written, replacing any earlier line for that name.
fn record(recorded: &mut Vec<(String, u64)>, name: &str, hash: u64) {
    match recorded.iter_mut().find(|(had, _)| had == name) {
        Some(entry) => entry.1 = hash,
        None => recorded.push((name.to_string(), hash)),
    }
}

/// Write the manifest out.
///
/// A plain write rather than the staged rename [`crate::configure`] uses for
/// `io.toml`, because the failure modes are not comparable: a truncated manifest
/// degrades to "every file is the operator's", which costs a refresh and destroys
/// nothing, while a truncated configuration is a session that will not start.
fn write_manifest(home: &Path, recorded: &[(String, u64)]) -> std::io::Result<()> {
    let mut text = String::with_capacity(recorded.len() * 32);
    for (name, hash) in recorded {
        text.push_str(&format!("{name}\t{hash:016x}\n"));
    }
    std::fs::write(manifest_path(home), text)
}

/// Whether the file at `path` is one io-cli wrote and nobody has edited since.
///
/// The provenance question `/skills` has to answer, and it is answered from the
/// manifest rather than from the `io-` prefix. That difference is the surface's
/// entire job: an operator who writes their own `io-thing.md` is told it is
/// theirs, and an operator who has edited a shipped skill is told the same —
/// which is true, and is also exactly what [`install`] will do with it.
#[must_use]
pub fn wrote(home: &Path, name: &str, path: &Path) -> bool {
    let Some(hash) = recorded_hash(&recorded(home), name) else {
        return false;
    };
    std::fs::read(path).is_ok_and(|bytes| digest(&bytes) == hash)
}

/// `n skills`, or `1 skill`.
fn many(n: usize) -> String {
    if n == 1 {
        "1 skill".to_string()
    } else {
        format!("{n} skills")
    }
}

/// Put the five shipped skills in `<home>/skills`, and say what happened.
///
/// Call it on every run, before the contract is built: a skill written after the
/// contract is a skill the first session of a new install is not offered, which is
/// exactly the session in which an operator is most likely to ask for help.
///
/// **Never returns an error, and a run that changed nothing returns no lines.**
/// Every failure — a directory that will not create, a file that will not write, a
/// directory that will not discover — becomes a line in the report and the session
/// carries on with whatever installed. The lines are what `main` already owns and
/// drains into the scrollback, the same `Vec<String>` shape
/// [`crate::home::Report::lines`] produces.
///
/// One skill gets one of six outcomes, decided in this order:
///
/// | If | Then |
/// | --- | --- |
/// | it is in `skills/disabled/` | nothing at all, silently: the operator turned it off |
/// | no file of ours, and another file resolves to the name | withheld, claimant named |
/// | no file, and the directory is at [`io_harness::skills::MAX_SKILLS`] | withheld, and counted |
/// | no file | written, and the manifest records its hash |
/// | a file whose bytes match the manifest | replaced with the new text, hash updated |
/// | a file whose bytes do not, or with no entry at all | left byte for byte, named as kept |
pub fn install(home: &Path) -> Vec<String> {
    let dir = dir(home);
    // The same `0700` [`crate::home::adopt`] gives the home, and a no-op on the
    // directory it already made — an existing directory's mode is not touched.
    if let Err(error) = crate::home::create(&dir) {
        return vec![format!("could not create {}: {error}", dir.display())];
    }

    // **The oracle is the harness's own walk, not a `read_dir` of io-cli's.** A
    // resolved name comes from frontmatter where there is one, so a file called
    // anything at all can claim `io-mcp`, and a second opinion about what this
    // directory holds would disagree with the run in precisely the case the
    // collision guard exists for. A directory that will not discover is already
    // broken; io-cli reports the harness's own sentence and adds nothing to it,
    // because a write into a set that is already ambiguous can only make the
    // session harder to fix.
    let found = match io_harness::Skills::discover(&dir) {
        Ok(found) => found,
        Err(error) => {
            return vec![format!(
                "{error}; io-cli installed none of its own skills into it"
            )];
        }
    };

    let disabled = disabled_dir(home);
    let mut manifest = recorded(home);
    // What the directory holds now, kept in step with every write below so the
    // ceiling is counted against the set as it grows rather than as it was.
    let mut held = found.len();
    let mut installed = 0usize;
    let mut updated = 0usize;
    let mut withheld = 0usize;
    let mut touched_manifest = false;
    // Lines about one named file, kept apart from the summary lines so the report
    // reads counts first and exceptions after.
    let mut notes: Vec<String> = Vec::new();

    for skill in &SHIPPED {
        if disabled.join(file_name(skill.name)).exists() {
            continue;
        }

        let target = dir.join(file_name(skill.name));
        let existing = match std::fs::read(&target) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            // There is a file there and this process cannot read it. It is not
            // io-cli's to overwrite on the strength of not knowing what it says.
            Err(error) => {
                notes.push(format!("kept {}: {error}", target.display()));
                continue;
            }
        };

        let Some(bytes) = existing else {
            // Nothing of ours at that path — but the *name* may still be taken,
            // and taking it anyway is `Error::Config` at run start rather than a
            // listing quirk. `target` does not exist, so anything discovery
            // resolved to this name is necessarily a different file.
            if let Some(claimant) = found.get(skill.name) {
                notes.push(format!(
                    "kept your {}, which is already named `{}`; io-cli did not install its own",
                    claimant.path.display(),
                    skill.name,
                ));
                continue;
            }
            // Counted before the write, not discovered after it. `discover`
            // rejects the whole set above the ceiling rather than truncating it,
            // so one file too many costs the operator every skill they had.
            if held >= io_harness::skills::MAX_SKILLS {
                withheld += 1;
                continue;
            }
            if let Err(error) = std::fs::write(&target, skill.text) {
                notes.push(format!("could not write {}: {error}", target.display()));
                continue;
            }
            record(&mut manifest, skill.name, digest(skill.text.as_bytes()));
            touched_manifest = true;
            held += 1;
            installed += 1;
            continue;
        };

        // **Against the manifest, never against the shipped text.** See the
        // module note: comparing with what this release ships reads every skill
        // unchanged between two releases as edited, and stops refreshing it
        // forever. No entry at all means io-cli has no record of writing this
        // file, which is the operator's file however it is spelled.
        if recorded_hash(&manifest, skill.name) != Some(digest(&bytes)) {
            notes.push(format!(
                "kept {}, which has been edited; io-cli's own `{}` was not written over it",
                target.display(),
                skill.name,
            ));
            continue;
        }
        // Untouched and already the current text: no write, so no churned mtime
        // for a backup to notice and no line about a thing that did not happen.
        if bytes == skill.text.as_bytes() {
            continue;
        }
        if let Err(error) = std::fs::write(&target, skill.text) {
            notes.push(format!("could not write {}: {error}", target.display()));
            continue;
        }
        record(&mut manifest, skill.name, digest(skill.text.as_bytes()));
        touched_manifest = true;
        updated += 1;
    }

    let mut report = Vec::with_capacity(notes.len() + 3);
    if installed > 0 {
        report.push(format!(
            "installed {} into {}",
            many(installed),
            dir.display()
        ));
    }
    if updated > 0 {
        report.push(format!(
            "brought {} in {} up to date",
            many(updated),
            dir.display()
        ));
    }
    if withheld > 0 {
        report.push(format!(
            "withheld {}: {} already holds {} of the {} io-harness allows in one directory",
            many(withheld),
            dir.display(),
            held,
            io_harness::skills::MAX_SKILLS,
        ));
    }
    report.extend(notes);

    if touched_manifest {
        if let Err(error) = write_manifest(home, &manifest) {
            // The skills are on disk and the session is fine; what is lost is the
            // record that io-cli wrote them, so the next upgrade will treat them
            // as the operator's and refuse to refresh them. Worth a line.
            report.push(format!(
                "could not write {}: {error}",
                manifest_path(home).display()
            ));
        }
    }

    report
}
