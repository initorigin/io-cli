//! `/skills` — what each skill is for, whose it is, whether it is on, and the
//! file it lives in; with the two levers that turn one off and back on.
//!
//! The palette has listed skills since 0.10.0 and that is a launcher: it puts a
//! name into the composer. This is the management surface, and the four facts it
//! draws are four facts the palette has never carried. **Provenance is the one it
//! exists for.** An operator looking at `io-thing.md` cannot tell from the screen
//! whether io-cli wrote it or they did, and the answer decides whether an upgrade
//! is going to overwrite it.
//!
//! # Origin comes from the manifest, never from the `io-` prefix
//!
//! The prefix is a courtesy. Nothing stops an operator naming a file `io-thing.md`
//! and nothing stops a file called anything at all from declaring `name: io-mcp`
//! in its frontmatter, which is the name discovery resolves. So origin is
//! [`crate::skills::wrote`] — *are these the bytes io-cli last wrote* — and that
//! is F6's whole sabotage arm: deciding it from the prefix tells an operator that
//! a file they wrote themselves came from io-cli, on the surface whose entire job
//! is provenance. It also has the property the prefix cannot have: a shipped skill
//! the operator has since edited reads as **theirs**, which is true, and is
//! exactly what [`crate::skills::install`] will do with it on the next upgrade.
//!
//! # Two sets, read two different ways, and only one of them has an oracle
//!
//! The enabled set is [`io_harness::Skills::discover`] over
//! [`crate::skills::dir`] — the same call the run makes, so what this surface
//! lists is what the model is offered. A `read_dir` of io-cli's own would be a
//! second opinion about what a skill file is, and it would disagree with the run
//! in precisely the case that matters.
//!
//! The disabled set has no oracle by construction. `skills/disabled/` is
//! invisible to discovery — the walk admits a subdirectory only when it holds a
//! `SKILL.md` — which is the entire mechanism by which disabling works, and it
//! means this module has to read that directory itself and resolve names itself.
//! `split_front_matter` is `pub(crate)` in the harness, so `front_matter` below is
//! io-cli's own smaller reader over the same two keys.
//!
//! # A directory that will not discover is a state, not an empty list
//!
//! `Skills::discover` returns `Err` on a missing directory, a directory that is a
//! file, more than `MAX_SKILLS` entries and — the one that matters — two skills
//! resolving to one name, which is every turn of that session dying at run start.
//! Today the operator sees an empty palette and no reason. [`View::failed`] is the
//! harness's own sentence, carried verbatim, and the disabled set is still listed
//! beside it: a broken enabled set says nothing about what is in `disabled/`, and
//! the file the operator needs to move back may well be in there.
//!
//! # Disabling is a rename and never a copy
//!
//! A copy leaves the skill in both directories, which is one resolved name
//! claimed twice, which is `Error::Config` at run start — F2's session-killer
//! arriving through io-cli's own keystroke. So there is exactly one
//! [`std::fs::rename`] here and **no `EXDEV` copy fallback**, deliberately unlike
//! [`crate::home`]'s move, which crosses filesystems because it moves a home
//! between two places an operator chose. These two directories are a parent and
//! its own child; they cannot be on different filesystems, so the fallback would
//! be dead code whose only reachable behaviour is the failure it is meant to
//! avoid.
//!
//! Neither lever rewrites a byte of the file and neither touches `io.toml`. There
//! is no `enabled` concept in the harness and no key for one in the configuration,
//! so a flag would be io-cli's alone and a second list disagreeing with the
//! filesystem is how a product grows two sources of truth.
//!
//! # No terminal I/O and no keys
//!
//! A data model and pure functions, as [`crate::servers`] is. The driver in
//! `src/main.rs` owns the keyboard and performs what [`disable`] and [`enable`]
//! return.

use std::path::{Path, PathBuf};

use crate::glyphs::Glyphs;
use crate::picker::{fit, fit_left, Row};
use crate::skills;

/// What a skill with no description at all is listed as.
///
/// The harness's own words for the same gap, matched on purpose: a file with no
/// frontmatter and no prose reads identically whether it is enabled — where the
/// string comes from `Skills::discover` — or disabled, where it comes from here.
const NO_DESCRIPTION: &str = "(no description)";

/// The narrowest a path may be drawn at, before the separator in front of it.
///
/// Twenty cells is `...cli/skills/io-mcp.md` — the last two segments and the mark
/// saying the front went, which is what identifies a file on a machine where every
/// skill shares the first several segments of its path. Below that what survives is
/// an extension and an ellipsis, which says nothing at all about where the file is,
/// so the path is dropped whole instead. A row is allowed to lose a fact; it is not
/// allowed to draw one that cannot be read.
const PATH_FLOOR: usize = 20;

/// Who wrote a skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// io-cli shipped it, and the bytes on disk are still the ones it wrote.
    IoCli,
    /// The operator's: a file io-cli never wrote, or one it wrote and they have
    /// since edited. Both are theirs, and both are left alone by an upgrade.
    Yours,
}

impl Origin {
    /// The word this origin draws as.
    ///
    /// **A word rather than a mark, and that is the ASCII form N4 asks for.**
    /// [`crate::glyphs`] gives every mark a Unicode form and an ASCII one because
    /// a mark that cannot be drawn has lost its meaning — but the degradation
    /// only works where the ASCII substitute still says the same thing. A filled
    /// dot against a hollow one is a pair `*` and `o` cannot carry: an operator
    /// reading `*` has no way to know which of the two origins it stands for.
    /// So there is one form here, it is already ASCII, and it needs no set.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Origin::IoCli => "io-cli",
            Origin::Yours => "yours",
        }
    }
}

/// One skill, as `/skills` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The name the model addresses it by: the frontmatter `name` where there is
    /// one, else the file stem. **Not the file name** — the two differ exactly
    /// when a collision is possible, which is the case this surface is for.
    pub name: String,
    /// The one line that goes into the prompt catalogue.
    pub description: String,
    /// Whose it is, decided from the manifest. See the module note.
    pub origin: Origin,
    /// Whether the model is offered it. A disabled skill is one whose file has
    /// been moved into `skills/disabled/`, and nothing else.
    pub enabled: bool,
    /// The file it lives in, which is also what [`disable`] and [`enable`] take.
    pub path: PathBuf,
}

/// Everything `/skills` draws.
///
/// Two fields rather than a `Result`, because they are not alternatives: the
/// disabled set is read out of a directory discovery never looks at, so it is
/// still listable when the enabled set has failed — and on the failure that
/// matters most, a duplicate name, the operator's next move is very likely to be
/// in the list that survived.
#[derive(Debug, Clone, Default)]
pub struct View {
    /// Both directories, sorted by name the way discovery sorts.
    pub skills: Vec<Listed>,
    /// The harness's own sentence, when the enabled set would not discover.
    ///
    /// **Verbatim.** It names the directory, and on a duplicate it names both
    /// files — which is the whole of what the operator needs and more than io-cli
    /// knows how to say. The driver draws it above the rows.
    pub failed: Option<String>,
}

/// Every skill in `dir` and in `dir/disabled`.
///
/// **`dir` is the directory the RUN reads, and it is not always io-cli's own.**
/// `[run] skills` and `[app.io-cli] skills` both beat the `~/.io-cli/skills`
/// default, so a surface that walked the home would list five files the model is
/// never offered and hide the ones it is — which would make this module's whole
/// premise false. The caller resolves it through `crate::contract::skills_dir`,
/// the same call that decides what the turn is handed.
///
/// `home` is still needed and is a different question: it is where the manifest
/// lives, and the manifest is what decides whose a file is.
#[must_use]
pub fn view(home: &Path, dir: &Path) -> View {
    let mut listed = Vec::new();
    let mut failed = None;

    match io_harness::Skills::discover(dir) {
        Ok(found) => listed.extend(found.iter().map(|skill| Listed {
            origin: origin(home, &skill.name, &skill.path),
            name: skill.name.clone(),
            description: skill.description.clone(),
            enabled: true,
            path: skill.path.clone(),
        })),
        Err(error) => failed = Some(error.to_string()),
    }

    for path in disabled_files(dir) {
        let (name, description) = describe(&path);
        listed.push(Listed {
            origin: origin(home, &name, &path),
            name,
            description,
            enabled: false,
            path,
        });
    }

    // By name, then by path. The name is what an operator is looking for and is
    // what discovery sorts by, so the two halves interleave rather than sitting
    // in two blocks; the path breaks the tie a name claimed in both directories
    // would otherwise leave to `read_dir` order.
    listed.sort_by(|left, right| (&left.name, &left.path).cmp(&(&right.name, &right.path)));
    View {
        skills: listed,
        failed,
    }
}

/// Whose a file is: the manifest's answer, never the file name's.
fn origin(home: &Path, name: &str, path: &Path) -> Origin {
    if skills::wrote(home, name, path) {
        Origin::IoCli
    } else {
        Origin::Yours
    }
}

/// The `*.md` files sitting in `skills/disabled`, sorted, absolute.
///
/// Top-level files only, which is what [`disable`] ever puts there — see the note
/// on it about the bundle it declines to move. A directory that is not there at
/// all is no disabled skills, which is the ordinary case: `disabled/` is created
/// by the first disable and never before, so that an operator who has turned
/// nothing off has no directory to wonder about.
///
/// Canonicalised the way `Skills::discover` canonicalises, so a path off this
/// surface is comparable with a path off that one and neither depends on the
/// process's working directory.
fn disabled_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir.join(skills::DISABLED)) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        })
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .collect();
    files.sort();
    files
}

/// The name and description a disabled file declares.
///
/// The same fallbacks the harness uses, because a file that is disabled today was
/// enabled yesterday and will be enabled again tomorrow: a skill that changed its
/// name by being turned off would be a different skill on the way back.
///
/// `pub(crate)` because [`crate::skills::install`] asks the same question of the
/// same directory: it has to know whether a *disabled* file already answers to a
/// name before it writes its own file under that name, and answering it any other
/// way would be a second reading of what a skill is called.
pub(crate) fn describe(path: &Path) -> (String, String) {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_default();
    let Ok(text) = std::fs::read_to_string(path) else {
        // Unreadable, and this surface still lists it: a file the operator cannot
        // read is one they especially need to be shown the path of.
        return (stem, NO_DESCRIPTION.to_string());
    };
    let (name, description, body) = front_matter(&text);
    (
        name.unwrap_or(stem),
        description
            .or_else(|| first_prose_line(body))
            .unwrap_or_else(|| NO_DESCRIPTION.to_string()),
    )
}

/// `name` and `description` out of a leading `---` fence, and the body after it.
///
/// io-cli's own, because the harness's `split_front_matter` is `pub(crate)`. Two
/// scalar keys and a fence; an unterminated fence is a file with no frontmatter,
/// which is what the harness decides too — guessing where the operator meant it to
/// close is worse than falling back to the filename.
///
/// ponytail: `description: >` and `description: |` are read as absent, so a block
/// scalar falls through to the first prose line rather than to the block. Reading
/// the block needs the harness's continuation state machine; the day a shipped
/// skill needs one, this grows the same `open`/`buffer` pair it has.
fn front_matter(text: &str) -> (Option<String>, Option<String>, &str) {
    let Some(rest) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return (None, None, text);
    };

    // Byte offsets, so the body comes back borrowed out of the original.
    let mut offset = 0usize;
    let mut fence = None;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            fence = Some((offset, offset + line.len()));
            break;
        }
        offset += line.len();
    }
    let Some((front_len, body_start)) = fence else {
        return (None, None, text);
    };

    let mut name = None;
    let mut description = None;
    for line in rest[..front_len].lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "name" => name = Some(value.to_string()),
            "description" => description = Some(value.to_string()),
            _ => {}
        }
    }
    (name, description, &rest[body_start..])
}

/// The first line of a body that reads as prose, heading and quote marks stripped.
fn first_prose_line(body: &str) -> Option<String> {
    body.lines()
        .map(|line| line.trim().trim_start_matches(['#', '>']).trim())
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

/// The picker rows, fitted for a terminal this wide.
///
/// The name is the label, because content precedes metadata and the name is what
/// the model addresses. The detail carries the other three facts in the order an
/// operator needs them, **composed to a budget rather than assembled and trimmed**
/// — the shape `crate::sessions::rows` was rewritten into after a row was cut at
/// the picker's edge and left a turn count amputated to a single digit.
///
/// # The narrow form, and what gives way in it
///
/// N4 fixes the rule: **the path gives way, and the name and the state never do.**
/// So the origin and the state are unconditional — two short words that together
/// cost about fifteen cells — the description takes what is left after them, and
/// the path is appended only if what remains after *that* still holds a shortened
/// form of it. At eighty columns a row is therefore the name, the origin, the
/// state and as much of the description as fits, with no path at all; widen the
/// terminal and the path arrives. A state word cut in half would be a lie about
/// whether the model is being offered the skill, and a path cut below `PATH_FLOOR`
/// is not where a file is — so the path is the field that is *dropped*, never the
/// field that is drawn illegibly.
///
/// The path is shortened from the **left**, since every skill on one machine
/// shares the first several segments of its path and the end is what identifies it.
pub fn rows(skills: &[Listed], width: u16, glyphs: &Glyphs) -> Vec<Row> {
    let separator = glyphs.separator;
    let separator_width = separator.chars().count();
    // A field is worth appending only if there is room for more than the mark that
    // would say it had been shortened. Measured off the set rather than assumed:
    // the ellipsis is one cell in Unicode and three in ASCII.
    let floor = separator_width + glyphs.ellipsis.chars().count();

    skills
        .iter()
        .map(|skill| {
            let state = if skill.enabled { "enabled" } else { "disabled" };
            let mut detail = format!("{}{separator}{state}", skill.origin.word());

            // The picker's own arithmetic, mirrored: two cells of marker, the
            // label, two cells of gap. Mirrored rather than guessed, because a
            // budget one cell out is how an ellipsis ends up on the floor. Four in
            // either glyph set — the marker is two cells in both.
            let mut left = (width as usize)
                .saturating_sub(4)
                .saturating_sub(skill.name.chars().count())
                .saturating_sub(detail.chars().count());

            if left > floor {
                let described = fit(&skill.description, left - separator_width, glyphs);
                left -= separator_width + described.chars().count();
                detail.push_str(separator);
                detail.push_str(&described);
            }
            if left >= separator_width + PATH_FLOOR {
                detail.push_str(separator);
                detail.push_str(&fit_left(
                    &skill.path.display().to_string(),
                    left - separator_width,
                    glyphs,
                ));
            }

            Row::with_detail(skill.name.clone(), detail)
        })
        .collect()
}

/// Move a skill's file into `skills/disabled/`, creating that directory if it is
/// not there. Answers with where the file now is.
///
/// The next turn stops offering it, because the skills directory is resolved per
/// turn and `disabled/` is invisible to the walk that reads it.
///
/// **One rename, no copy, no partial state.** See the module note: a copy is the
/// same file under one name in two places, which is `Error::Config` at run start
/// and every turn of that session dead. A rename either happened or did not.
///
/// The failure that is not hypothetical is Windows refusing to rename a file
/// another process holds open — a second `io` running, an editor with the skill
/// open — which is how 0.15.0's post-seal defect was found. It comes back as a
/// sentence naming the file, never as a panic.
///
/// ponytail: declines a `SKILL.md` bundle rather than moving its directory. Moving
/// the file alone would leave an empty directory behind and land as `disabled/SKILL.md`,
/// which collides with the next bundle disabled and resolves to the name `SKILL`;
/// moving the directory is the correct answer and is what this grows if an operator
/// asks for it. Declining is the one option that cannot lose a file.
pub fn disable(path: &Path) -> Result<PathBuf, String> {
    if is_bundle(path) {
        return Err(format!(
            "{} is a skill folder rather than a single file; move the folder into the disabled \
             one by hand to turn it off",
            path.display()
        ));
    }
    let Some(dir) = path.parent() else {
        return Err(format!("{} is not in a skills directory", path.display()));
    };
    let disabled = dir.join(skills::DISABLED);
    // Made here and nowhere earlier: the surface that first moves a file in is
    // what creates it. `crate::home::create` rather than a bare `create_dir_all`,
    // so it is `0700` like everything else under the home.
    if let Err(error) = crate::home::create(&disabled) {
        return Err(format!("could not create {}: {error}", disabled.display()));
    }
    relocate(path, &disabled)
}

/// Move a disabled skill's file back out of `skills/disabled/`. Answers with
/// where the file now is.
///
/// The inverse of [`disable`] byte for byte: neither reads the file, so neither
/// can change it.
pub fn enable(path: &Path) -> Result<PathBuf, String> {
    // Out of `disabled/` and into its parent, which is the skills directory. Two
    // levels up from the file rather than a path recomputed from the home,
    // because the caller has a path off [`view`] and the home it came from is not
    // on this call — and a file that is not two levels deep is not one this
    // surface put there.
    let Some(dir) = path.parent().and_then(Path::parent) else {
        return Err(format!(
            "{} is not in a disabled skills directory",
            path.display()
        ));
    };
    // **The destination guard in `relocate` is by FILE NAME, and a skill is
    // addressed by its RESOLVED name.** That asymmetry is the whole subject of
    // this release, and it is reachable from here with two keystrokes: disable an
    // operator's `mine.md` that declares `name: io-mcp`, restart — `install` no
    // longer sees a claimant, because the claimant is in a directory discovery
    // cannot look into, so it writes its own `io-mcp.md` — then enable `mine.md`
    // again. `relocate` would find no `mine.md` in the way and move it, and the
    // directory would hold two files answering to `io-mcp`. `Skills::discover`
    // returns `Err` on that, io-harness propagates it at run start, and every turn
    // of the session is dead before the first completion — with `/skills` itself
    // unable to help, because its list comes from the call that just failed.
    //
    // So the question asked here is the one the run will ask: does anything in
    // that directory already answer to this file's name?
    let (name, _) = describe(path);
    if let Ok(found) = io_harness::Skills::discover(dir) {
        if let Some(claimant) = found.get(&name) {
            return Err(format!(
                "{} already answers to `{name}`, so io-cli did not move {} beside it — \
                 two skills of one name end every turn of the session",
                claimant.path.display(),
                path.display(),
            ));
        }
    }
    relocate(path, dir)
}

/// Whether a path is a `SKILL.md` inside its own directory.
fn is_bundle(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("SKILL.md"))
}

/// Move one file into `into`, keeping its name.
///
/// **Refuses to move over a file that is already there.** `std::fs::rename`
/// replaces the destination silently on unix, so enabling `io-mcp` while the
/// operator has written their own `io-mcp.md` in the meantime would destroy
/// theirs — and this surface's one promise is that neither lever changes a file's
/// contents. The state it declines is one an operator made by hand, so the
/// sentence names both paths and lets them decide.
fn relocate(from: &Path, into: &Path) -> Result<PathBuf, String> {
    let Some(name) = from.file_name() else {
        return Err(format!("{} has no file name", from.display()));
    };
    let to = into.join(name);
    if to.exists() {
        return Err(format!(
            "{} is already there, so io-cli did not move {} over it",
            to.display(),
            from.display()
        ));
    }
    match std::fs::rename(from, &to) {
        Ok(()) => Ok(to),
        Err(error) => Err(format!(
            "could not move {} to {}: {error}",
            from.display(),
            to.display()
        )),
    }
}
