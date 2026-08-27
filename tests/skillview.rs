//! F6, F7 and N4 — the `/skills` surface: provenance, the two levers, and what
//! the rows look like on a terminal that can draw nothing but ASCII.
//!
//! Every claim about what the *enabled* set holds goes through
//! [`io_harness::Skills::discover`], for the reason `tests/skills.rs` opens with:
//! a resolved name comes from frontmatter where there is one, so a test that
//! counted `io-*.md` files would agree with io-cli and disagree with the run. The
//! harness's walk is the only oracle whose verdict is the one every turn gets.
//!
//! Nothing here touches the environment — [`skillview::view`] takes the home as an
//! argument — so each test owns a temporary directory and they run in parallel.
//! The bundles are the same shape: plain `(id, directory)` pairs, so a question
//! about a `[[plugin]]` bundle's skills is still asked of a fixture directory and
//! never of a loaded configuration.

mod support;

use std::path::{Path, PathBuf};

use io_cli::skills;
use io_cli::skillview::{self, Listed, Origin, View};

/// A home with nothing in it.
fn home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = dir.path().join("home");
    (dir, home)
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the parent directory");
    }
    std::fs::write(path, text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// A skill file: the two keys the harness reads, and a line of body.
fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\nDo the thing.\n")
}

/// Record the files at `paths` in the manifest as io-cli's own.
///
/// Written out here in the format the module documents rather than through an
/// io-cli helper, so a release that quietly changed where or how provenance is
/// recorded turns this red instead of passing through an accessor that changed
/// with it. This is the fixture the whole of F6 turns on: without it every file
/// on disk is the operator's, which is exactly the direction
/// [`skills::wrote`] degrades in.
fn recorded(home: &Path, entries: &[(&str, &Path)]) {
    let mut text = String::new();
    for (name, path) in entries {
        text.push_str(&format!("{name}\t{:016x}\n", skills::digest(&read(path))));
    }
    write(&home.join(".skills-manifest"), &text);
}

/// The row for one name, or a panic naming what was listed instead.
fn listed<'a>(view: &'a View, name: &str) -> &'a Listed {
    view.skills
        .iter()
        .find(|skill| skill.name == name)
        .unwrap_or_else(|| {
            let names: Vec<&str> = view.skills.iter().map(|s| s.name.as_str()).collect();
            panic!("`{name}` is not listed; the surface shows {names:?}")
        })
}

/// The names io-harness resolves out of a directory, sorted as it sorts them.
///
/// Panics on a directory that will not discover, which is the point: a criterion
/// asserting the session still runs is asserting exactly that this returns `Ok`.
fn discovered(dir: &Path) -> Vec<String> {
    io_harness::Skills::discover(dir)
        .unwrap_or_else(|error| {
            panic!(
                "{} does not discover, so every turn of that session would fail at run \
                 start: {error}",
                dir.display()
            )
        })
        .iter()
        .map(|skill| skill.name.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// F6 — what it is, whose it is, whether it is on, and where it lives.
// ---------------------------------------------------------------------------

#[test]
fn f6_origin_is_decided_by_the_manifest_and_never_by_the_io_prefix() {
    // **The sabotage arm.** `io-thing.md` carries the prefix io-cli ships under and
    // is the operator's; `mine.md` carries no prefix and is io-cli's, because the
    // manifest says io-cli wrote those bytes. A surface that read the file name
    // would get both of them backwards, and it would be telling an operator that a
    // file they wrote themselves came from io-cli — on the surface whose entire job
    // is provenance.
    let (_dir, home) = home();
    let dir = skills::dir(&home);

    let ours = dir.join("mine.md");
    write(
        &ours,
        &skill("mine", "A skill io-cli happens to have written."),
    );
    let theirs = dir.join("io-thing.md");
    write(&theirs, &skill("io-thing", "A skill the operator wrote."));

    // Only the un-prefixed one is recorded, so the prefix and the truth disagree.
    recorded(&home, &[("mine", ours.as_path())]);

    let view = skillview::view(&home, &skills::dir(&home), &[]);
    assert_eq!(view.failed, None, "the fixture discovers");
    assert_eq!(listed(&view, "mine").origin, Origin::IoCli);
    assert_eq!(listed(&view, "io-thing").origin, Origin::Yours);
    assert_eq!(listed(&view, "io-thing").origin.word(), "yours");

    // And the second half of the same rule: a shipped skill the operator has since
    // edited is theirs. The bytes no longer hash to what io-cli recorded, which is
    // true, and is also what the next upgrade will decide about it.
    write(&ours, &skill("mine", "Edited by the operator."));
    let view = skillview::view(&home, &skills::dir(&home), &[]);
    assert_eq!(
        listed(&view, "mine").origin,
        Origin::Yours,
        "an edited shipped skill belongs to whoever edited it"
    );
}

#[test]
fn f6_a_skill_carries_its_description_its_state_and_the_file_it_lives_in() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    let path = dir.join("alpha.md");
    write(&path, &skill("alpha", "Does the alpha thing."));

    let view = skillview::view(&home, &skills::dir(&home), &[]);
    let row = listed(&view, "alpha");
    assert_eq!(row.description, "Does the alpha thing.");
    assert!(row.enabled, "a file in skills/ is offered to the model");
    assert!(
        row.path.ends_with("alpha.md"),
        "the surface says which file, not just which name: {}",
        row.path.display()
    );
}

#[test]
fn f6_a_disabled_skill_is_listed_as_disabled_with_its_path_inside_disabled() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    let path = dir.join("alpha.md");
    write(&path, &skill("alpha", "Does the alpha thing."));

    let moved = skillview::disable(&path, &[]).expect("the move");

    let view = skillview::view(&home, &skills::dir(&home), &[]);
    let row = listed(&view, "alpha");
    assert!(!row.enabled, "a file under disabled/ is not offered");
    // Canonicalised on both sides: the surface resolves a path the way
    // `Skills::discover` does, so a row off this list and a row off that one are
    // comparable — and on macOS a temporary directory is a symlink, which is the
    // difference that would otherwise make this assertion about the platform.
    assert_eq!(
        row.path,
        std::fs::canonicalize(&moved).expect("the moved file")
    );
    assert_eq!(
        row.path.parent().and_then(Path::file_name),
        Some(std::ffi::OsStr::new(skills::DISABLED)),
        "the path says where it went: {}",
        row.path.display()
    );
    // Still named, still described. A disabled skill that lost its description
    // would be a row an operator cannot decide about.
    assert_eq!(row.description, "Does the alpha thing.");
}

#[test]
fn f6_a_directory_that_will_not_discover_shows_the_harness_sentence() {
    // Two files resolving to one name: `Error::Config` at run start, and every turn
    // of that session dead before the first completion. Today the operator sees an
    // empty palette and no reason, which is the hole F6 names.
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    write(&dir.join("a.md"), &skill("dup", "One of two."));
    write(&dir.join("b.md"), &skill("dup", "The other of two."));
    // And one skill already turned off, in the directory discovery never looks at.
    write(
        &skills::disabled_dir(&home).join("parked.md"),
        &skill("parked", "Turned off earlier."),
    );

    let view = skillview::view(&home, &skills::dir(&home), &[]);

    let sentence = view
        .failed
        .clone()
        .expect("a failed discovery is a state, not silence");
    let harness = io_harness::Skills::discover(&dir)
        .expect_err("the fixture is ambiguous")
        .to_string();
    assert_eq!(
        sentence, harness,
        "the surface carries the harness's own sentence verbatim; it names both files"
    );
    assert!(sentence.contains("dup"), "the sentence names the collision");

    // Not an empty list. The disabled set is read out of a directory discovery
    // never walks, so it survives the failure — and on this failure in particular
    // the operator's next move is very likely to be in it.
    assert_eq!(
        view.skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["parked"],
    );
    assert!(!listed(&view, "parked").enabled);
}

// ---------------------------------------------------------------------------
// F7 — enable and disable are renames, and the next turn sees them.
// ---------------------------------------------------------------------------

#[test]
fn f7_disable_then_enable_returns_the_file_byte_for_byte_and_leaves_one_copy() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    let path = dir.join("alpha.md");
    let before = skill("alpha", "Does the alpha thing.");
    write(&path, &before);

    // `disabled/` is made by the first disable and never before it, so an operator
    // who has turned nothing off has no directory to wonder about.
    assert!(
        !skills::disabled_dir(&home).exists(),
        "disabled/ is created on demand"
    );

    let parked = skillview::disable(&path, &[]).expect("the move");
    assert!(skills::disabled_dir(&home).is_dir());
    assert_eq!(parked, skills::disabled_dir(&home).join("alpha.md"));

    // **The absence is the assertion.** A copy would leave the file in both
    // directories, which is one resolved name claimed twice — F2's session-killer
    // arriving through io-cli's own keystroke.
    assert!(
        !path.exists(),
        "{} is still there, so the skill is in two directories at once",
        path.display()
    );
    assert_eq!(read(&parked), before.as_bytes(), "a move rewrites no byte");

    let back = skillview::enable(&parked, &[]).expect("the move back");
    assert_eq!(back, path);
    assert!(
        !parked.exists(),
        "{} is still there after enabling it",
        parked.display()
    );
    assert_eq!(read(&path), before.as_bytes(), "byte for byte, both ways");
}

#[test]
fn f7_a_disabled_skill_is_gone_from_the_harnesss_own_discovery() {
    // The oracle, not io-cli's opinion: what the run will be offered on the next
    // turn, since the skills directory is resolved per turn.
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    write(&dir.join("alpha.md"), &skill("alpha", "One."));
    write(&dir.join("beta.md"), &skill("beta", "Two."));

    assert_eq!(discovered(&dir), vec!["alpha", "beta"]);

    skillview::disable(&dir.join("alpha.md"), &[]).expect("the move");

    // Still `Ok` — `disabled/` holds no `SKILL.md`, so the walk skips it rather
    // than reading the loose file inside as a second `alpha`.
    assert_eq!(discovered(&dir), vec!["beta"]);
}

#[test]
fn f7_neither_lever_writes_to_io_toml() {
    // There is no `enabled` key in the harness and none in io-cli's configuration.
    // A flag would be a second list disagreeing with the filesystem.
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    let path = dir.join("alpha.md");
    write(&path, &skill("alpha", "One."));
    let config = home.join("io.toml");
    write(&config, "model = \"a-model\"\n");

    let parked = skillview::disable(&path, &[]).expect("the move");
    skillview::enable(&parked, &[]).expect("the move back");

    assert_eq!(
        std::fs::read_to_string(&config).expect("io.toml"),
        "model = \"a-model\"\n"
    );
}

#[test]
fn f7_a_move_that_would_overwrite_a_file_is_a_sentence_and_not_a_loss() {
    // The state Windows reports as an error and unix performs silently: enabling
    // `alpha` while the operator has written their own `alpha.md` in the meantime.
    // A rename here would destroy theirs, and this surface's one promise is that
    // neither lever changes a file's contents.
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    let path = dir.join("alpha.md");
    write(&path, &skill("alpha", "Theirs."));
    let parked = skills::disabled_dir(&home).join("alpha.md");
    write(&parked, &skill("alpha", "Turned off earlier."));

    let error = skillview::enable(&parked, &[]).expect_err("it declines");
    assert!(
        error.contains("alpha.md"),
        "the message names the file: {error}"
    );
    assert_eq!(
        read(&path),
        skill("alpha", "Theirs.").as_bytes(),
        "the operator's file is untouched"
    );
    assert!(
        parked.exists(),
        "and nothing was lost from disabled/ either"
    );
}

#[test]
fn f7_a_rename_that_fails_is_a_readable_sentence_and_never_a_panic() {
    // Windows refuses to rename a file another process holds open, which is how
    // 0.15.0's post-seal defect was found. A missing file is the portable stand-in
    // for the same shape: the call answers, the message names the file, and there
    // is no half-move to clean up.
    let (_dir, home) = home();
    let path = skills::dir(&home).join("gone.md");
    std::fs::create_dir_all(skills::dir(&home)).expect("the directory");

    let error = skillview::disable(&path, &[]).expect_err("it fails rather than panicking");
    assert!(
        error.contains("gone.md"),
        "the message names the file: {error}"
    );
    assert!(!skills::disabled_dir(&home).join("gone.md").exists());
}

// ---------------------------------------------------------------------------
// N4 — ASCII, eighty columns, and the field that gives way.
// ---------------------------------------------------------------------------

/// A home holding one io-cli skill, one of the operator's, and one turned off.
///
/// Every byte of it is ASCII on purpose, so the render below can be asserted whole
/// rather than field by field: anything non-ASCII on the screen came from this
/// surface's own marks, which is exactly N4's sabotage.
fn drawable() -> (tempfile::TempDir, PathBuf) {
    let (dir, home) = home();
    let skills_dir = skills::dir(&home);

    let ours = skills_dir.join("io-mcp.md");
    write(
        &ours,
        &skill(
            "io-mcp",
            "Add, change or remove an MCP server by proposing an edit to io.toml.",
        ),
    );
    write(
        &skills_dir.join("mine.md"),
        &skill("mine", "Something the operator wrote for this machine."),
    );
    write(
        &skills::disabled_dir(&home).join("parked.md"),
        &skill(
            "parked",
            "Turned off by the operator, and still listed here.",
        ),
    );
    recorded(&home, &[("io-mcp", ours.as_path())]);
    (dir, home)
}

#[test]
fn n4_every_row_draws_in_ascii_with_its_meaning_intact_inside_eighty_columns() {
    let (_dir, home) = drawable();
    let view = skillview::view(&home, &skills::dir(&home), &[]);
    assert_eq!(view.failed, None);
    assert_eq!(view.skills.len(), 3);

    let theme = io_cli::theme::DARK.with_glyphs(io_cli::glyphs::ASCII);
    let rows = skillview::rows(&view.skills, 80, &theme.glyphs);
    let mut picker = io_cli::picker::Picker::new("skills", rows);

    let (mut screen, _recorder) = support::screen(80, 24);
    screen
        .draw(|frame| picker.render(frame, frame.area(), &theme))
        .expect("frame");
    let drawn = screen.viewport_text().to_string();

    for line in drawn.lines() {
        assert!(
            line.is_ascii(),
            "a row drew a character the ASCII terminal cannot: {line:?}"
        );
        assert!(
            line.chars().count() <= 80,
            "a row overflowed eighty columns: {line:?}"
        );
    }

    // The meaning, not merely the bytes: the two markers are words, so they say
    // which origin and which state without a legend and without a glyph set.
    for word in [
        "io-mcp", "mine", "parked", "io-cli", "yours", "enabled", "disabled",
    ] {
        assert!(
            drawn.contains(word),
            "`{word}` is not on the screen:\n{drawn}"
        );
    }
}

#[test]
fn n4_the_path_is_what_gives_way_and_the_name_and_the_state_never_do() {
    let (_dir, home) = drawable();
    let view = skillview::view(&home, &skills::dir(&home), &[]);
    let ascii = io_cli::glyphs::ASCII;
    let glyphs = &ascii;

    // Eighty columns: the description takes what the two markers leave, and the
    // path is dropped whole rather than shortened past legibility.
    for row in skillview::rows(&view.skills, 80, glyphs) {
        let detail = row.detail.expect("every row has a detail");
        assert!(
            !detail.contains(".md"),
            "the path should have given way at eighty columns: {detail:?}"
        );
        assert!(
            detail.contains("enabled") || detail.contains("disabled"),
            "the state never gives way: {detail:?}"
        );
        assert!(
            detail.starts_with("io-cli") || detail.starts_with("yours"),
            "the origin never gives way: {detail:?}"
        );
        assert!(!row.label.ends_with(glyphs.ellipsis), "the name is not cut");
    }

    // Widen the terminal and the path arrives, shortened from the LEFT, because
    // every skill on one machine shares the first segments of its path.
    let wide = skillview::rows(&view.skills, 240, glyphs);
    let details: Vec<String> = wide
        .into_iter()
        .map(|row| row.detail.expect("a detail"))
        .collect();
    assert!(
        details.iter().all(|detail| detail.contains(".md")),
        "the path is on a wide terminal: {details:?}"
    );
    assert!(
        details.iter().any(|detail| detail.contains("parked.md")),
        "and the end of the path is what survives, since the front is shared: {details:?}"
    );
}

// ---------------------------------------------------------------------------
// The adversarial review of 0.19.0 found the defect below behind a green suite
// of 1,020 tests. It is here as its own section because it is the one failure
// mode this whole release exists to prevent, reached through io-cli's own keys.
// ---------------------------------------------------------------------------

/// **Enabling may not put a second file under a name already answered to.**
///
/// The destination guard in `relocate` is by FILE NAME, and a skill is addressed
/// by its RESOLVED name — the asymmetry this whole release is built on. So a
/// file moving back out of `disabled/` can land beside a different file that
/// already answers to its name, with nothing in the way. `Skills::discover` then
/// returns `Err`, io-harness propagates it at run start, and every turn of the
/// session dies before the first completion. `/skills` cannot even report it: its
/// list comes from the call that just failed.
///
/// The review that found this reached it through io-cli's own five — disable a
/// claimant, let the next start install io-cli's file into the freed name, enable
/// the claimant again. That route is now closed one step earlier, by
/// `install_skips_a_name_that_is_disabled_under_a_different_file_name` below, and
/// this test deliberately does **not** use it: a control that only fails when a
/// second guard is also removed is not a control. What is left is the route
/// neither installer guard can ever see, because io-cli never wrote either file.
///
/// Sabotage: delete the resolved-name check in `skillview::enable`. Under it only
/// this test fails, and it fails with `discovered` panicking — which is the exact
/// sentence an operator would have got on every turn.
#[test]
fn enabling_never_puts_a_second_file_under_a_name_already_answered_to() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);

    // Two files of the OPERATOR'S OWN, which is where the hole survives both of
    // the other guards: `install` only ever withholds files io-cli would write,
    // so nothing in it looks at a pair like this one. The names are what collide;
    // the file names never do, which is precisely why the destination check in
    // `relocate` cannot see it.
    let enabled = dir.join("a.md");
    write(&enabled, &skill("alpha", "The one they kept on."));
    let other = dir.join("b.md");
    write(&other, &skill("alpha", "The one they turned off."));
    let parked = skillview::disable(&other, &[]).expect("the move out");
    let kept = read(&parked);

    // One `alpha` in the directory, so the session is fine as it stands.
    assert_eq!(discovered(&dir), vec!["alpha".to_string()]);

    // And this is the keystroke that used to end it.
    let back = skillview::enable(&parked, &[]);
    assert!(
        back.is_err(),
        "enabling moved a second `alpha` in beside the first; every turn of that \
         session is now dead at run start",
    );
    let why = back.unwrap_err();
    assert!(
        why.contains("alpha"),
        "the refusal does not name the name that is taken: {why}",
    );

    // The operator's file is where it was, byte for byte. A refusal that also
    // lost a file would be worse than the defect.
    assert_eq!(read(&parked), kept, "the refused move touched the file");

    // And the property the whole thing is about: the session still starts.
    assert_eq!(discovered(&dir), vec!["alpha".to_string()]);
}

/// **A disabled skill is skipped by the name it answers to, not by its file
/// name.** This is step 3 above, and closing it is what stops the sequence from
/// ever reaching step 4.
///
/// Sabotage: test `disabled/<name>.md` for existence instead of resolving the
/// names in that directory. Under it only this test fails, and io-cli writes an
/// `io-mcp.md` beside a disabled file that already answers to `io-mcp`.
#[test]
fn install_skips_a_name_that_is_disabled_under_a_different_file_name() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);

    let theirs = dir.join("mine.md");
    write(&theirs, &skill("io-mcp", "The operator's own."));
    skillview::disable(&theirs, &[]).expect("the move out");

    skills::install(&home);

    assert!(
        !dir.join("io-mcp.md").exists(),
        "io-cli wrote its own `io-mcp` while a disabled file already answers to that name",
    );
    let mut expected: Vec<String> = skills::SHIPPED
        .iter()
        .map(|skill| skill.name.to_string())
        .filter(|name| name != "io-mcp")
        .collect();
    expected.sort();
    assert_eq!(discovered(&dir), expected);
}

// ---------------------------------------------------------------------------
// 0.21.0 — a `[[plugin]]` bundle's skills reach the model, so they belong on the
// surface that says what the model is offered; and the two levers may not touch
// a file in a directory io-cli does not own.
// ---------------------------------------------------------------------------

/// A bundle's skills directory, and the `(id, dir)` pair [`skillview::view`] takes.
///
/// Plain pairs rather than a configuration with `[[plugin]]` entries in it, which
/// is the whole point of the signature: this surface is answerable from a fixture
/// directory, and a test that had to write a manifest to ask about a directory
/// would be testing the loader instead.
fn bundle(root: &Path, id: &str, files: &[(&str, &str)]) -> (String, PathBuf) {
    let dir = root.join(id).join("skills");
    for (name, description) in files {
        write(&dir.join(format!("{name}.md")), &skill(name, description));
    }
    (id.to_string(), dir)
}

/// Every file under `dir`, with its bytes.
///
/// Sorted, so two snapshots compare as values. Used to assert an **absence**: a
/// refused lever left the bundle exactly as it found it — no file moved, no file
/// rewritten, and no `disabled/` directory conjured inside somebody else's bundle.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            found.extend(snapshot(&path));
        } else {
            let bytes = read(&path);
            found.push((path, bytes));
        }
    }
    found.sort();
    found
}

/// The namespaced name io-harness itself builds, spelled here the way the module
/// spells it, out of the harness's own constant rather than a literal `"__"`.
fn namespaced(id: &str, name: &str) -> String {
    format!("{id}{}{name}", io_harness::NAMESPACE)
}

#[test]
fn a_bundle_skill_is_listed_under_the_name_the_model_addresses() {
    // The gap 0.20.0 shipped with: a bundle's skills are folded into the turn's
    // catalogue, namespaced, and no surface in io-cli listed one. An operator
    // reading `/skills` was reading a list that disagreed with what the model had.
    let (dir, home) = home();
    let ours = skills::dir(&home).join("mine.md");
    write(&ours, &skill("mine", "The operator's own."));
    let plug = bundle(dir.path(), "acme", &[("helper", "Something contributed.")]);

    let view = skillview::view(&home, &skills::dir(&home), std::slice::from_ref(&plug));
    assert_eq!(view.failed, None);
    assert!(view.bundles_failed.is_empty());

    let row = listed(&view, &namespaced("acme", "helper"));
    assert_eq!(row.description, "Something contributed.");
    assert!(row.enabled, "a bundle's skill has no state but on");
    assert_eq!(row.origin, Origin::Bundle("acme".to_string()));
    assert_eq!(
        row.origin.word(),
        "acme",
        "the origin column names the bundle, which is where the operator has to go"
    );
    assert!(
        row.path.ends_with("helper.md"),
        "the row says which file: {}",
        row.path.display()
    );

    // **The absences.** A row under the bare name would be a row under a name the
    // model cannot address; and the bundle's file is not the operator's, whatever
    // the manifest does or does not say about it.
    let names: Vec<&str> = view.skills.iter().map(|s| s.name.as_str()).collect();
    assert!(
        !names.contains(&"helper"),
        "listed un-namespaced, which is a name no turn resolves: {names:?}"
    );
    assert_eq!(listed(&view, "mine").origin, Origin::Yours);
}

#[test]
fn two_bundles_each_contributing_one_name_do_not_collide() {
    // The reason io-harness namespaces at all, asserted from this side: two
    // bundles may both ship a `helper`, and the surface has to draw two rows an
    // operator can tell apart — including two different origin words.
    let (dir, home) = home();
    std::fs::create_dir_all(skills::dir(&home)).expect("the skills directory");
    let bundles = [
        bundle(dir.path(), "acme", &[("helper", "Acme's.")]),
        bundle(dir.path(), "widget", &[("helper", "Widget's.")]),
    ];

    let view = skillview::view(&home, &skills::dir(&home), &bundles);

    let acme = namespaced("acme", "helper");
    let widget = namespaced("widget", "helper");
    assert_eq!(
        view.skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec![acme.clone(), widget.clone()],
        "two bundles, two rows, sorted by the names the model uses"
    );
    assert_eq!(listed(&view, &acme).description, "Acme's.");
    assert_eq!(listed(&view, &widget).description, "Widget's.");
    assert_eq!(listed(&view, &acme).origin.word(), "acme");
    assert_eq!(listed(&view, &widget).origin.word(), "widget");
    assert_ne!(
        listed(&view, &acme).path,
        listed(&view, &widget).path,
        "two rows standing for one file would be a surface that lost one of them"
    );
}

#[test]
fn a_bundle_whose_directory_is_missing_is_named_and_costs_only_itself() {
    // **The session-killer.** `Plugin::skills_dir` does no existence check, and
    // `TaskContract::discover_skills` walks every declared directory with `?` at
    // run start — so a bundle naming a `skills` directory that is not on disk is a
    // session where every turn dies before the first completion. This surface is
    // the only place in io-cli that can say so, which is why the error is neither
    // swallowed nor propagated.
    let (dir, home) = home();
    let ours = skills::dir(&home).join("mine.md");
    write(&ours, &skill("mine", "The operator's own."));
    write(
        &skills::disabled_dir(&home).join("parked.md"),
        &skill("parked", "Turned off earlier."),
    );
    let missing = dir.path().join("broken").join("skills");
    let bundles = [
        ("broken".to_string(), missing.clone()),
        bundle(dir.path(), "acme", &[("helper", "Acme's.")]),
    ];

    let view = skillview::view(&home, &skills::dir(&home), &bundles);

    let (id, sentence) = view
        .bundles_failed
        .first()
        .expect("a bundle that would end every turn is a state, not silence");
    assert_eq!(id, "broken", "the report names which bundle to go and fix");
    let harness = io_harness::Skills::discover(&missing)
        .expect_err("the fixture directory is not there")
        .to_string();
    assert_eq!(
        sentence, &harness,
        "the surface carries the harness's own sentence verbatim; it names the directory"
    );
    assert_eq!(view.bundles_failed.len(), 1, "one broken bundle, one entry");

    // **And it cost exactly itself.** The operator's directory discovered, so
    // `failed` stays empty — one field holding either failure could not say which
    // of the two happened, on the question where the difference is everything.
    assert_eq!(view.failed, None);
    assert_eq!(
        view.skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            namespaced("acme", "helper"),
            "mine".to_string(),
            "parked".to_string()
        ],
        "every other row still draws"
    );
}

#[test]
fn the_operators_own_skills_survive_a_bundle_that_will_not_discover() {
    // The same rule from the other side, and the one that would be lost to a `?`:
    // a broken bundle must not take away the rows the operator came to read. The
    // enabled set, the disabled set and their provenance are all still here.
    let (dir, home) = home();
    let ours = skills::dir(&home).join("mine.md");
    write(
        &ours,
        &skill("mine", "A skill io-cli happens to have written."),
    );
    write(
        &skills::disabled_dir(&home).join("parked.md"),
        &skill("parked", "Turned off earlier."),
    );
    recorded(&home, &[("mine", ours.as_path())]);
    let bundles = [("broken".to_string(), dir.path().join("nowhere"))];

    let view = skillview::view(&home, &skills::dir(&home), &bundles);

    assert_eq!(view.bundles_failed.len(), 1);
    assert_eq!(listed(&view, "mine").origin, Origin::IoCli);
    assert!(listed(&view, "mine").enabled);
    assert!(!listed(&view, "parked").enabled);
    assert!(
        view.skills
            .iter()
            .all(|s| !matches!(s.origin, Origin::Bundle(_))),
        "a bundle that did not discover contributed no row"
    );
}

#[test]
fn disable_is_refused_on_a_bundle_skill_and_leaves_the_bundle_untouched() {
    // **`disable` computes its destination as the file's own parent joined to
    // `disabled/`.** For a bundle's skill that parent is the bundle's directory,
    // so without this guard io-cli would create a `disabled/` inside somebody
    // else's bundle and move their file into it — where the bundle's next update
    // will not find it and no surface will list it. `is_bundle` cannot see this:
    // it decides a `SKILL.md` folder shape and knows nothing about whose directory
    // a path is in.
    //
    // Sabotage: delete the `refuse_bundle` call in `skillview::disable`. Under it
    // this test fails on the snapshot, with the bundle holding a `disabled/` it
    // never had.
    let (dir, home) = home();
    std::fs::create_dir_all(skills::dir(&home)).expect("the skills directory");
    let plug = bundle(dir.path(), "acme", &[("helper", "Acme's.")]);
    let before = snapshot(&plug.1);
    let names = discovered(&plug.1);

    let view = skillview::view(&home, &skills::dir(&home), std::slice::from_ref(&plug));
    let row = listed(&view, &namespaced("acme", "helper"));

    let error = skillview::disable(&row.path, std::slice::from_ref(&plug))
        .expect_err("io-cli does not move another product's files");
    assert!(
        error.contains("acme"),
        "the refusal names the bundle: {error}"
    );
    assert!(
        error.contains(&plug.1.display().to_string()),
        "and names the directory the operator has to change it in: {error}"
    );

    // The absences, and they are the assertion: nothing moved, nothing was
    // rewritten, and no `disabled/` appeared inside the bundle.
    assert_eq!(
        snapshot(&plug.1),
        before,
        "the refused lever touched the bundle"
    );
    assert!(
        !plug.1.join(skills::DISABLED).exists(),
        "io-cli made a disabled/ inside a bundle it does not own"
    );
    assert_eq!(discovered(&plug.1), names, "and the bundle still discovers");
}

#[test]
fn enable_is_refused_on_a_bundle_skill_and_leaves_the_bundle_untouched() {
    // The same refusal in the other direction. No bundle file can be in a
    // `disabled/` this surface made — that is the point of the test above — so
    // this asserts the pair is symmetric rather than a second route to the same
    // hole: a lever that refuses one way and not the other is a lever whose rule
    // a reader has to look up.
    //
    // Sabotage: delete the `refuse_bundle` call in `skillview::enable`. Under it
    // this test fails, and the file lands two levels up, outside the bundle's
    // skills directory entirely.
    let (dir, home) = home();
    std::fs::create_dir_all(skills::dir(&home)).expect("the skills directory");
    let plug = bundle(dir.path(), "acme", &[("helper", "Acme's.")]);
    let before = snapshot(&plug.1);

    let view = skillview::view(&home, &skills::dir(&home), std::slice::from_ref(&plug));
    let row = listed(&view, &namespaced("acme", "helper"));

    let error = skillview::enable(&row.path, std::slice::from_ref(&plug))
        .expect_err("io-cli does not move another product's files");
    assert!(
        error.contains("acme"),
        "the refusal names the bundle: {error}"
    );

    assert_eq!(
        snapshot(&plug.1),
        before,
        "the refused lever touched the bundle"
    );
    assert!(
        row.path.exists(),
        "{} is gone, so the refusal lost the file it was protecting",
        row.path.display()
    );
    // And the guard did not spill over onto the operator's own directory: their
    // levers still work while a bundle is in the list.
    let ours = skills::dir(&home).join("mine.md");
    write(&ours, &skill("mine", "The operator's own."));
    let parked = skillview::disable(&ours, std::slice::from_ref(&plug)).expect("their own move");
    skillview::enable(&parked, std::slice::from_ref(&plug)).expect("and back again");
    assert!(ours.exists());
}
