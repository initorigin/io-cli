//! `/skills` — what each skill is for, whose it is, whether it is on, and the
//! file it lives in; with the levers that turn one off, turn it back on, put one
//! there and take one away.
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
//! # A bundle's skills belong here too, and one of them can kill the session
//!
//! 0.20.0 gave a `[[plugin]]` bundle a skills directory, and every skill in one
//! reaches the model — namespaced `bundle__skill`, the form io-harness itself
//! builds out of [`io_harness::NAMESPACE`]. None of them appeared on any surface
//! that lists a skill, so an operator reading `/skills` was reading a list that
//! disagreed with the catalogue the turn was handed. [`view`] takes the bundles
//! and lists them under exactly the name the model addresses.
//!
//! The other half is worse than a missing row. `TaskContract::discover_skills`
//! walks every bundle's declared directory with the same `Skills::discover` this
//! module calls, and every caller of it uses `?` — while `Plugin::skills_dir`
//! does no existence check whatever, it is the manifest's word joined onto the
//! root. So a bundle naming a `skills` directory that is not on disk is a session
//! in which **every turn dies at run start, before the first completion**, and
//! io-cli has had nowhere that says so. [`View::bundles_failed`] is that place.
//! That is why the error may neither be swallowed nor propagated: swallowed, the
//! one surface that could explain the dead session draws nothing; propagated, it
//! takes the surface down with it and the operator is left with the same silence,
//! having also lost every row that was fine. Each bundle costs exactly itself,
//! which is what [`crate::pluginview`] already promises about the same bundles.
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
//! And neither lever is offered a bundle's file at all. [`disable`] would compute
//! its destination as the file's own parent joined to `disabled/`, which for a
//! bundle skill means io-cli creating a directory inside somebody else's bundle
//! and moving their file into it — a bundle io-cli does not own, did not install
//! and cannot put back. Both levers refuse, and they refuse *inside themselves*
//! rather than at the call site; see [`disable`].
//!
//! # A skill arrives and leaves, and the two acts are not the two levers
//!
//! Until 0.30.0 the only thing in this crate that had ever written a skill file
//! was [`crate::import`], driven by a foreign tool the crate happened to detect,
//! so an operator with a skill of their own had no door at all — and nothing
//! anywhere removed one. [`install`] and [`remove`] are that door, and they are
//! deliberately a **copy** and an **unlink** rather than two more renames:
//!
//! * [`install`] copies, because the source is a file the operator still owns —
//!   a repository, a download, a directory of their own — and a lever that
//!   emptied it would be `/skills` deleting something outside the home in order
//!   to put something inside it. It records nothing in
//!   [`crate::skills::MANIFEST`], which is what makes the installed row read
//!   `yours`: the manifest answers *are these the bytes io-cli last wrote*, and
//!   these are not.
//! * [`remove`] unlinks, because [`disable`] already is the reversible act. A
//!   removal implemented as a rename would be a lever that promised deletion and
//!   delivered a hidden copy, and the two would then be one verb under two names
//!   on the surface whose whole subject is what is where.
//!
//! Both refuse a bundle's file, from inside themselves, for [`disable`]'s reason
//! and by [`disable`]'s guard.
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
///
/// **No longer `Copy` from 0.21.0**, because [`Origin::Bundle`] carries the
/// bundle's own name and that name is the only honest word for a row that is
/// neither io-cli's nor the operator's. [`Origin::word`] argues the trade; the
/// cost falls entirely on call sites that already hold a `&Listed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// io-cli shipped it, and the bytes on disk are still the ones it wrote.
    IoCli,
    /// The operator's: a file io-cli never wrote, or one it wrote and they have
    /// since edited. Both are theirs, and both are left alone by an upgrade.
    Yours,
    /// A `[[plugin]]` bundle's, named by the manifest `name` that `/plugin` lists
    /// it under and that its skills are namespaced by.
    ///
    /// **A third answer rather than folding into [`Origin::Yours`].** The
    /// operator owns neither the file nor the directory holding it, and a row
    /// saying `yours` would be telling them io-cli may move it — which is the one
    /// thing [`disable`] and [`enable`] refuse to do.
    Bundle(String),
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
    ///
    /// # Borrowed rather than `&'static str`, so a bundle draws its own name
    ///
    /// The alternative considered was the fixed word `"bundle"` with the id
    /// carried in a second field, and it fails the argument above in exactly the
    /// way the filled dot does: two bundles each contributing a skill would draw
    /// the identical word, and an operator reading it would have no way to know
    /// which of the two directories to go and edit. The name is the useful word,
    /// so the name is the word, and the return type widens to `&str` to say it.
    ///
    /// It does repeat what the row's name already carries, since a bundle skill
    /// is listed namespaced. Deliberately: the origin column is where an operator
    /// reads *whose* a row is, and it has to answer that for every row or it is
    /// not a column, only a column that is sometimes filled in.
    #[must_use]
    pub fn word(&self) -> &str {
        match self {
            Origin::IoCli => "io-cli",
            Origin::Yours => "yours",
            Origin::Bundle(id) => id,
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
    /// The bundles whose skills directory would not discover: the bundle's id and
    /// io-harness's own sentence, one pair per bundle, in the order they were
    /// walked.
    ///
    /// **A separate field rather than folding into [`View::failed`], because they
    /// are separate failures with separate consequences.** `failed` is the
    /// operator's own directory not discovering, which costs the enabled list and
    /// nothing else — the session still runs. A bundle's directory not discovering
    /// costs *the session*: `TaskContract::discover_skills` walks the same
    /// directory with the same call and propagates the error at run start, so
    /// every turn dies before the first completion. One `Option<String>` holding
    /// either would make the surface unable to say which of the two happened, on
    /// the one question where the difference is everything.
    ///
    /// A `Vec` and not an `Option` for the same reason [`crate::pluginview::View`]
    /// keeps a list: two broken bundles are two facts, and reporting the first
    /// would send an operator to fix one directory and meet the same dead session.
    pub bundles_failed: Vec<(String, String)>,
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
///
/// # `bundles` is a slice of plain pairs, and that is a deliberate refusal
///
/// Each pair is a bundle's id and the skills directory it declared — precisely
/// what `crate::pluginview::Listed` already carries in its `id` and `skills`
/// fields, and precisely nothing else. It is **not** an [`io_harness::Config`]
/// and **not** a `pluginview::View`, because either one would tie this module to
/// the configuration types, and every test in this file would then have to build
/// a configuration to ask a question about a directory. The driver is already the
/// thing that holds a `pluginview::View`; turning it into pairs is one `map` at
/// the one call site, and it keeps the whole of this surface answerable from a
/// fixture directory.
///
/// The slice is walked **in the order given**, which the driver takes from
/// `Config::plugins()` — the same order `TaskContract::discover_skills` folds
/// the directories in, so a name that collides collides the same way here.
#[must_use]
pub fn view(home: &Path, dir: &Path, bundles: &[(String, PathBuf)]) -> View {
    let mut listed = Vec::new();
    let mut failed = None;
    let mut bundles_failed = Vec::new();

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

    let (from_bundles, failures) = bundle_rows(bundles);
    listed.extend(from_bundles);
    bundles_failed.extend(failures);

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

    sorted(listed, failed, bundles_failed)
}

/// Every bundle's skills, for a home that has no skills directory of its own.
///
/// **A separate entry point rather than an `Option<&Path>` on [`view`].** The
/// caller either has a directory the run reads or it does not, and threading an
/// `Option` through would put the question in every one of this module's tests
/// rather than in the one place that can answer it. There is no `home` argument
/// because there is nothing to ask the manifest about: every row here is
/// [`Origin::Bundle`] by construction.
///
/// This is not a rare path. `crate::contract::skills_dir` answers `None` whenever
/// the operator has never made `~/.io-cli/skills`, which is the ordinary state of
/// a fresh install — and it is exactly the state in which every skill the model
/// is offered came from a bundle. Returning an empty view here would blank the
/// surface precisely when it is the only listing there is.
#[must_use]
pub fn view_of_bundles(bundles: &[(String, PathBuf)]) -> View {
    let (listed, bundles_failed) = bundle_rows(bundles);
    sorted(listed, None, bundles_failed)
}

/// One row per skill each bundle contributes, and one sentence per bundle that
/// would not discover. Shared by [`view`] and [`view_of_bundles`] so the two can
/// never disagree about what a bundle contributed.
fn bundle_rows(bundles: &[(String, PathBuf)]) -> (Vec<Listed>, Vec<(String, String)>) {
    let mut listed = Vec::new();
    let mut bundles_failed = Vec::new();
    for (id, bundle) in bundles {
        match io_harness::Skills::discover(bundle) {
            // **The namespaced name, rebuilt rather than borrowed.** `Skills`
            // does namespace its own — `merged` and `namespaced` are both
            // `pub(crate)` in io-harness and the type has no public constructor —
            // so the one honest thing io-cli can do is reproduce the format the
            // harness uses, out of the harness's own constant. A row listed under
            // the bare `skill` would be listed under a name the model cannot
            // address, on the surface that exists to say what the model is offered.
            Ok(found) => listed.extend(found.iter().map(|skill| Listed {
                name: format!("{id}{}{}", io_harness::NAMESPACE, skill.name),
                description: skill.description.clone(),
                // Never the manifest's answer. `crate::skills::wrote` asks whether
                // io-cli last wrote these bytes, and for a file in somebody else's
                // bundle the answer is no in a way that means something different
                // from the operator having written it.
                origin: Origin::Bundle(id.clone()),
                // No lever reaches it, so there is no state but on. `disabled/`
                // is io-cli's mechanism inside io-cli's own directory.
                enabled: true,
                path: skill.path.clone(),
            })),
            // Recorded and walked past, never `?`. See [`View::bundles_failed`]:
            // this is the session-killing failure, and it is the only failure on
            // this surface where taking the surface down would take away the one
            // report that explains it.
            Err(error) => bundles_failed.push((id.clone(), error.to_string())),
        }
    }
    (listed, bundles_failed)
}

/// The rows in the order the surface draws them, wrapped in a [`View`].
///
/// By name, then by path. The name is what an operator is looking for and is
/// what discovery sorts by, so the sources interleave rather than sitting in
/// blocks; the path breaks the tie a name claimed in two directories would
/// otherwise leave to `read_dir` order. A bundle's rows sort under their
/// namespaced names, so a bundle's skills land together without this sort
/// needing to know a bundle exists.
fn sorted(
    mut listed: Vec<Listed>,
    failed: Option<String>,
    bundles_failed: Vec<(String, String)>,
) -> View {
    listed.sort_by(|left, right| (&left.name, &left.path).cmp(&(&right.name, &right.path)));
    View {
        skills: listed,
        failed,
        bundles_failed,
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
/// # Not a `[[plugin]]` bundle's file, and the guard is in here rather than up there
///
/// The destination below is `path.parent().join(DISABLED)`. For a bundle's skill
/// that parent is the *bundle's* skills directory, so io-cli would create a
/// `disabled/` inside a directory it does not own and move somebody else's file
/// into it — where the bundle's next update will not find it, `/skills` will not
/// list it, and only the operator's memory says where it went.
///
/// `is_bundle` cannot see this: it decides a `SKILL.md` folder *shape* and knows
/// nothing about whose directory a path is in. Two different senses of one word,
/// and two guards.
///
/// **`bundles` is taken here, and by [`enable`], rather than the call site testing
/// [`Origin::Bundle`] before it calls.** A guard at the call site is a guard the
/// next call site does not have, and this one protects a stranger's files from an
/// operation that cannot be undone by the surface that performed it. Taking the
/// slice costs both callers an argument they already hold — it is the same slice
/// [`view`] took to produce the row being acted on — and buys that the move is
/// unreachable with a bundle path from anywhere.
///
/// ponytail: declines a `SKILL.md` bundle rather than moving its directory. Moving
/// the file alone would leave an empty directory behind and land as `disabled/SKILL.md`,
/// which collides with the next bundle disabled and resolves to the name `SKILL`;
/// moving the directory is the correct answer and is what this grows if an operator
/// asks for it. Declining is the one option that cannot lose a file.
pub fn disable(path: &Path, bundles: &[(String, PathBuf)]) -> Result<PathBuf, String> {
    if let Some(refusal) = refuse_bundle(path, bundles) {
        return Err(refusal);
    }
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
///
/// It refuses a bundle's file for [`disable`]'s reason and by the same guard,
/// even though no bundle file can ever be in a `disabled/` this surface made. The
/// pair is symmetric on purpose: a lever that refuses in one direction and not
/// the other is a lever whose rule a reader has to look up.
pub fn enable(path: &Path, bundles: &[(String, PathBuf)]) -> Result<PathBuf, String> {
    if let Some(refusal) = refuse_bundle(path, bundles) {
        return Err(refusal);
    }
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

/// Copy the file at `source` into this home's skills directory. Answers with
/// where it now is.
///
/// **A copy and never a move.** The source is a file the operator still owns — a
/// repository, a download, a directory of their own — and a verb on `/skills`
/// that emptied it would be this surface deleting something outside the home in
/// order to put something inside it.
///
/// **It records nothing in [`skills::MANIFEST`], and that is what makes the row
/// read `yours`.** The manifest answers one question — *are these the bytes io-cli
/// last wrote?* — and for a file the operator brought, they are not. Recording it
/// would make [`skills::wrote`] answer `true` and the listing would credit this
/// crate with somebody else's work, which is the same misattribution
/// [`crate::home::origin`] exists to prevent one level up.
///
/// Two refusals, and the second is the one that matters. A destination that
/// already exists is refused rather than overwritten, because a skill file is
/// prose somebody wrote and this verb has no undo. And a source whose **resolved
/// name** is already claimed is refused even when its filename is free — that
/// asymmetry is [`enable`]'s whole warning, reachable here in one keystroke: two
/// files answering to one name make `Skills::discover` return `Err`, io-harness
/// propagates it at run start, and every turn of the session is dead before the
/// first completion, with `/skills` unable to help because its list comes from the
/// call that just failed.
pub fn install(home: &Path, source: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err(format!(
            "{} is not a file; a skill is one markdown file with `name:` and \
             `description:` in its frontmatter",
            source.display()
        ));
    }
    let Some(file_name) = source.file_name() else {
        return Err(format!("{} has no file name", source.display()));
    };

    let dir = skills::dir(home);
    // Made here for [`disable`]'s reason: the surface that first moves a file in
    // is what creates it, `0700` like everything else under the home.
    if let Err(error) = crate::home::create(&dir) {
        return Err(format!("could not create {}: {error}", dir.display()));
    }
    let destination = dir.join(file_name);
    if destination.exists() {
        return Err(format!(
            "{} is already there; remove it first, or rename the file you are \
             installing",
            destination.display()
        ));
    }

    // The question the run will ask, asked before the file is in a position to
    // make the run fail.
    let (name, _) = describe(source);
    if let Ok(found) = io_harness::Skills::discover(&dir) {
        if found.get(&name).is_some() {
            return Err(format!(
                "a skill in {} already answers to `{name}`, which is the name in \
                 {}'s frontmatter — two files answering to one name make every \
                 turn of the next session fail before its first completion",
                dir.display(),
                source.display(),
            ));
        }
    }

    std::fs::copy(source, &destination)
        .map_err(|error| format!("could not copy into {}: {error}", destination.display()))?;
    Ok(destination)
}

/// Delete a skill's file. Answers with the path that is now gone.
///
/// **An unlink, and that is the whole difference from [`disable`].** Disable is
/// the reversible act: it renames the file into `disabled/` and [`enable`] brings
/// it back. A removal implemented as a second rename would be a verb that promised
/// deletion and delivered a hidden copy — and the two would then be one act under
/// two names, on the surface whose entire subject is what is where.
///
/// It refuses a bundle's file from inside itself, for [`disable`]'s reason and by
/// [`disable`]'s guard: a bundle's skills are not io-cli's to delete, and the
/// refusal lives here rather than at the call site so a second caller cannot walk
/// past it.
///
/// The folder form is refused too, for [`disable`]'s reason: `<name>/SKILL.md` is
/// one skill spread over a directory, and unlinking the file alone would leave the
/// directory behind holding nothing that discovery can see.
pub fn remove(path: &Path, bundles: &[(String, PathBuf)]) -> Result<PathBuf, String> {
    if let Some(refusal) = refuse_bundle(path, bundles) {
        return Err(refusal);
    }
    if is_bundle(path) {
        return Err(format!(
            "{} is a skill folder rather than a single file; delete the folder by \
             hand to take it away",
            path.display()
        ));
    }
    std::fs::remove_file(path)
        .map_err(|error| format!("could not remove {}: {error}", path.display()))?;
    Ok(path.to_path_buf())
}

/// The sentence for a path that lives inside a `[[plugin]]` bundle, if it does.
///
/// **Containment, not equality.** io-harness walks a skills directory one level
/// down for `<name>/SKILL.md` as well as reading the loose files in it, so a
/// bundle skill's path is not always a direct child of the declared directory.
///
/// Both sides are canonicalised before they are compared, because the two come
/// from different places: a path off [`view`] came through `Skills::discover`,
/// which canonicalises, and a declared directory is `Plugin::skills_dir` — the
/// bundle root joined to whatever the manifest wrote, canonical or not. On macOS
/// a temporary directory is a symlink, which is enough on its own to make an
/// uncanonicalised comparison answer "not a bundle" about a bundle. A path that
/// will not canonicalise is compared as it stands: the file being gone is
/// [`relocate`]'s error to report, not a reason to relax this.
fn refuse_bundle(path: &Path, bundles: &[(String, PathBuf)]) -> Option<String> {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let (id, dir) = bundles.iter().find(|(_, dir)| {
        resolved.starts_with(std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone()))
    })?;
    Some(format!(
        "{} belongs to the plugin `{id}`, so io-cli did not move it — a bundle's \
         skills are turned off in the bundle's own directory, {}",
        path.display(),
        dir.display(),
    ))
}

/// Whether a path is a `SKILL.md` inside its own directory.
///
/// A different sense of the word from [`refuse_bundle`]'s: this one is a *file
/// shape* — one skill spread over a directory — and says nothing about whose
/// directory it is in. Both are checked, and neither substitutes for the other.
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
