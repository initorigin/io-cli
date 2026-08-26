//! The shipped skills and the installer that puts them on disk.
//!
//! Every assertion about what a skills directory *holds* goes through
//! [`io_harness::Skills::discover`] rather than through a `read_dir` of this
//! file's own. That is not fastidiousness: the resolved name of a skill comes
//! from its frontmatter where there is one, so a file called anything at all can
//! answer to `io-mcp`, and a test that counted `io-*.md` files would agree with
//! io-cli and disagree with the run. The harness's walk is the only oracle whose
//! verdict is the one every turn of the session will get.
//!
//! Nothing here touches the environment. [`skills::install`] takes the home as an
//! argument, so each test gets a temporary directory of its own and they are free
//! to run in parallel — unlike `tests/memory.rs`, whose subject reads
//! `IO_CONFIG_HOME` at call time and therefore has to serialise.

use std::path::{Path, PathBuf};

use io_cli::skills;

/// A home with nothing in it. `install` makes the directories it needs.
fn home() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = dir.path().join("home");
    (dir, home)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The names io-harness resolves out of a directory, sorted as it sorts them.
///
/// Panics on a directory that will not discover, which is the point: a criterion
/// that says "and the session still runs" is asserting exactly that this call
/// returns `Ok`, and an `unwrap_or_default` here would turn the session-killer
/// into a green test with an empty list.
fn discovered(dir: &Path) -> Vec<String> {
    io_harness::Skills::discover(dir)
        .unwrap_or_else(|error| {
            panic!(
                "{} does not discover, so every turn of that session would fail at run start: \
                 {error}",
                dir.display()
            )
        })
        .iter()
        .map(|skill| skill.name.clone())
        .collect()
}

/// The five shipped names, in the order `SHIPPED` declares them.
fn shipped_names() -> Vec<String> {
    skills::SHIPPED
        .iter()
        .map(|skill| skill.name.to_string())
        .collect()
}

/// The manifest as pairs, parsed the way the module documents its format.
///
/// Written out here rather than read through a helper, so that a release which
/// quietly changed the format — to TOML, to JSON, to the skills directory — turns
/// this red instead of passing through an accessor that changed with it.
fn manifest(home: &Path) -> Vec<(String, u64)> {
    let path = home.join(".skills-manifest");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    text.lines()
        .map(|line| {
            let (name, hash) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("`{line}` is not `name<TAB>hex-hash`"));
            (
                name.to_string(),
                u64::from_str_radix(hash, 16)
                    .unwrap_or_else(|error| panic!("`{hash}` is not hex: {error}")),
            )
        })
        .collect()
}

fn hash_of(manifest: &[(String, u64)], name: &str) -> Option<u64> {
    manifest
        .iter()
        .find(|(had, _)| had == name)
        .map(|(_, hash)| *hash)
}

/// Put `count` skills of the operator's own in the directory, none of them named
/// anything io-cli ships.
fn operator_skills(dir: &Path, count: usize) {
    std::fs::create_dir_all(dir).expect("the skills directory");
    for index in 0..count {
        std::fs::write(
            dir.join(format!("mine-{index:02}.md")),
            format!("A skill the operator wrote, number {index}.\n"),
        )
        .expect("an operator skill");
    }
}

// ---------------------------------------------------------------------------
// The five files themselves
// ---------------------------------------------------------------------------

/// Each shipped file declares the name the installer files it under.
///
/// The two are stated in different places — `name:` in the markdown, `name` in
/// `SHIPPED` — and if they disagree the installed file is `io-mcp.md` answering
/// to something else, which makes every collision check in this module look at
/// the wrong name.
#[test]
fn every_shipped_file_declares_the_name_it_is_installed_under() {
    for skill in &skills::SHIPPED {
        let declared = skill
            .text
            .lines()
            .find_map(|line| line.strip_prefix("name:"))
            .map(str::trim)
            .unwrap_or_else(|| panic!("{} has no `name:` in its frontmatter", skill.name));
        assert_eq!(
            declared, skill.name,
            "the file's frontmatter and SHIPPED disagree about what this skill is called",
        );
        // **Either line ending, because the property is what io-harness will
        // read.** `split_front_matter` strips `---\n` and `---\r\n` alike and
        // trims `\r` off every key, and the first form of this assertion took
        // only the first — which is green on macOS and Linux and red on Windows,
        // where the checkout is CRLF and `include_str!` gets it. That was a test
        // stricter than the thing it protects, and only the matrix could see it.
        //
        // The bytes are also pinned to LF in `.gitattributes`, so one release
        // ships one artifact rather than a different one per build host. That is
        // a separate concern from this assertion and neither stands in for the
        // other: remove the attribute and this still passes, which is correct,
        // because the skill still works.
        assert!(
            skill.text.starts_with("---\n") || skill.text.starts_with("---\r\n"),
            "{}'s frontmatter is not a fence io-harness will read; without one the name \
             falls back to the file stem and the description to the first prose line",
            skill.name,
        );
    }
}

/// A description is one line, and a short one.
///
/// Every skill's description is in the system prompt on **every turn**, and
/// io-harness clamps at 240 characters — so an essay here is an essay paid for
/// per turn, silently truncated where it matters most.
#[test]
fn every_description_is_one_short_sentence() {
    for skill in &skills::SHIPPED {
        let description = skill
            .text
            .lines()
            .find_map(|line| line.strip_prefix("description:"))
            .map(str::trim)
            .unwrap_or_else(|| panic!("{} has no `description:`", skill.name));

        assert!(
            !description.is_empty(),
            "{}'s description is empty, so the catalogue says nothing about it",
            skill.name,
        );
        assert!(
            description.len() < 200,
            "{}'s description is {} characters; it is sent on every turn and io-harness \
             clamps it at 240",
            skill.name,
            description.len(),
        );
    }
}

/// **F8, the half a test can hold.** Every skill tells the model to *propose*.
///
/// The criterion is about prose and is finally judged by reading it, but the
/// sabotage it names — a skill that tells the model to report the setting as
/// changed — starts by deleting the instruction to propose. This catches that.
#[test]
fn every_shipped_skill_ends_in_a_proposal_rather_than_a_claim() {
    for skill in &skills::SHIPPED {
        assert!(
            skill.text.contains("propos"),
            "{} never tells the model to propose anything, so nothing in it ends at a \
             change the operator approves",
            skill.name,
        );
    }
}

// ---------------------------------------------------------------------------
// The install
// ---------------------------------------------------------------------------

/// A home with nothing in it gets all five, and io-harness sees all five.
#[test]
fn a_fresh_home_gets_the_five_and_the_harness_discovers_them() {
    let (_dir, home) = home();

    let report = skills::install(&home);
    let dir = skills::dir(&home);

    assert_eq!(
        discovered(&dir),
        shipped_names(),
        "the directory io-harness walks does not hold exactly the five io-cli ships",
    );
    for skill in &skills::SHIPPED {
        assert_eq!(
            read(&dir.join(format!("{}.md", skill.name))),
            skill.text,
            "{} on disk is not the text in the binary",
            skill.name,
        );
    }
    assert!(
        report
            .iter()
            .any(|line| line.contains("installed 5 skills")),
        "the report does not say what it did: {report:?}",
    );
}

/// Running it again changes nothing and says nothing.
///
/// A line per run about a thing that did not happen is a report an operator stops
/// reading, which costs the lines that *do* matter — the withheld skill, the kept
/// edit — their only chance of being seen.
#[test]
fn a_second_install_with_nothing_to_do_writes_nothing_and_reports_nothing() {
    let (_dir, home) = home();
    skills::install(&home);

    let before = manifest(&home);
    let report = skills::install(&home);

    assert!(report.is_empty(), "the second run reported {report:?}");
    assert_eq!(manifest(&home), before, "the manifest was rewritten anyway");
}

/// **F2.** A name the operator already claims is never overwritten, and the
/// session still runs.
///
/// The sabotage is installing unconditionally. Under it this is the only test
/// that fails, and it fails the way the field would: two files answering to
/// `io-mcp`, `Skills::discover` returning `Error::Config`, and — because
/// `TaskContract::discover_skills` propagates that at run start — every turn of
/// that session dead before the first completion, with the palette empty and
/// nothing on screen connecting it to an upgrade.
#[test]
fn f2_a_name_the_operator_already_claims_is_withheld_and_the_session_still_runs() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    std::fs::create_dir_all(&dir).expect("the skills directory");

    let mine = dir.join("mine.md");
    let theirs = "---\nname: io-mcp\ndescription: My own take on the MCP servers.\n---\n\nMine.\n";
    std::fs::write(&mine, theirs).expect("the operator's skill");

    let report = skills::install(&home);

    assert!(
        !dir.join("io-mcp.md").exists(),
        "io-cli wrote io-mcp.md over a name the operator already claims",
    );
    assert_eq!(read(&mine), theirs, "the operator's file was touched");

    // Four written, not five, and the fifth name still resolves to their file.
    let mut names = discovered(&dir);
    names.sort();
    assert_eq!(
        names,
        shipped_names(),
        "the resolved set is not the five names, one of which is now the operator's file",
    );
    assert_eq!(
        std::fs::read_dir(&dir)
            .expect("the directory")
            .filter(|entry| entry
                .as_ref()
                .is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with("io-")))
            .count(),
        4,
        "io-cli installed a fifth file of its own",
    );

    let said = report.join("\n");
    assert!(
        said.contains("io-mcp") && said.contains("mine.md"),
        "the report names neither the withheld skill nor the file that claimed it: {report:?}",
    );
}

/// **F3.** The shared ceiling is respected before the write, not discovered after
/// it.
///
/// The sabotage is installing all five and letting discovery complain. Under it
/// only this test fails, and it fails with the harness rejecting the **entire
/// set** rather than the excess — so an operator with a working 62-skill
/// directory has none at all after upgrading.
#[test]
fn f3_the_ceiling_is_counted_before_the_write_and_the_session_still_runs() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    operator_skills(&dir, 62);

    let report = skills::install(&home);

    let names = discovered(&dir);
    assert_eq!(
        names.len(),
        io_harness::skills::MAX_SKILLS,
        "62 of the operator's plus at most two of io-cli's is the ceiling exactly",
    );
    let shipped = shipped_names();
    let ours = names
        .iter()
        .filter(|name| shipped.iter().any(|theirs| theirs == *name))
        .count();
    assert_eq!(
        ours, 2,
        "io-cli added {ours} skills into a directory with two places left",
    );

    let said = report.join("\n");
    assert!(
        said.contains("withheld 3 skills"),
        "the report does not say how many were withheld: {report:?}",
    );
    assert!(
        said.contains("64"),
        "the report does not say why they were withheld: {report:?}",
    );
}

/// **F4.** An untouched skill is brought forward; a modified one is left alone;
/// one io-cli has no record of writing is the operator's.
///
/// The three cases are set up by hand, because they are what the *second* upgrade
/// looks like and a test that only installs twice cannot reach them: a previous
/// release's text with its own hash recorded, an operator edit, and a file with
/// no manifest entry at all.
///
/// The sabotage is comparing the bytes on disk against the **shipped** text
/// instead of against the manifest. Under it only this test fails, and it fails
/// on the second upgrade rather than the first: every skill unchanged between two
/// releases would read as modified and stop being refreshed forever.
#[test]
fn f4_an_untouched_skill_is_refreshed_and_an_edited_one_is_left_byte_for_byte() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    skills::install(&home);

    // 1. A previous release's io-mcp, recorded as io-cli's own.
    let older = "---\nname: io-mcp\ndescription: What the last release said.\n---\n\nOld.\n";
    let mcp = dir.join("io-mcp.md");
    std::fs::write(&mcp, older).expect("the older text");

    // 2. An edit the operator made to io-provider, which io-cli must not touch.
    let provider = dir.join("io-provider.md");
    let edited = format!("{}\n\nAnd my own note at the end.\n", read(&provider));
    std::fs::write(&provider, &edited).expect("the operator's edit");

    // 3. An io-remember with no manifest entry at all — a file io-cli has no
    //    record of writing, whatever it is named, is the operator's.
    let remember = dir.join("io-remember.md");
    let theirs = "---\nname: io-remember\ndescription: Mine, not io-cli's.\n---\n\nMine.\n";
    std::fs::write(&remember, theirs).expect("the operator's own io-remember");

    // The manifest as those three edits leave it: io-mcp recorded at the older
    // text's hash, io-provider still at what io-cli wrote, io-remember absent.
    let recorded: Vec<String> = manifest(&home)
        .into_iter()
        .filter(|(name, _)| name != "io-remember")
        .map(|(name, hash)| {
            let hash = if name == "io-mcp" {
                skills::digest(older.as_bytes())
            } else {
                hash
            };
            format!("{name}\t{hash:016x}")
        })
        .collect();
    std::fs::write(
        home.join(".skills-manifest"),
        format!("{}\n", recorded.join("\n")),
    )
    .expect("the manifest");
    let before = manifest(&home);

    let report = skills::install(&home);

    // 1. Untouched: replaced with the new text, and the hash brought forward.
    let shipped = skills::SHIPPED
        .iter()
        .find(|skill| skill.name == "io-mcp")
        .expect("io-mcp is shipped");
    assert_eq!(
        read(&mcp),
        shipped.text,
        "a file whose bytes still hash to the manifest is io-cli's and was not refreshed",
    );
    assert_eq!(
        hash_of(&manifest(&home), "io-mcp"),
        Some(skills::digest(shipped.text.as_bytes())),
        "the manifest still records the older text io-cli no longer ships",
    );

    // 2. Edited: byte for byte as it was, and its manifest entry untouched.
    assert_eq!(
        read(&provider),
        edited,
        "io-cli overwrote an edit the operator made",
    );
    assert_eq!(
        hash_of(&manifest(&home), "io-provider"),
        hash_of(&before, "io-provider"),
        "the manifest entry for a file io-cli did not write was rewritten",
    );

    // 3. Unmanifested: the operator's, whatever it is called.
    assert_eq!(
        read(&remember),
        theirs,
        "a file io-cli has no record of writing was overwritten anyway",
    );

    // **And it says nothing about either of them.** A kept edit is a settled
    // state, not an event: the operator changed the file on purpose and it is
    // theirs from then on, so a line here would print at every start for the rest
    // of that file's life — and on stderr at every `io exec`, which a script runs
    // a great many times. `delivery.observability` in the release contract asks
    // for silence when nothing changed, and a file that is being left alone did
    // not change. `/skills` is the standing place for it, where the row reads
    // `yours`. A claimed NAME is the opposite case and is still reported — see
    // `f2_...`, which asserts that line — because that is an unresolved hazard
    // rather than a settled state.
    let said = report.join("\n");
    for kept in ["io-provider.md", "io-remember.md"] {
        assert!(
            !said.contains(kept),
            "the report names {kept} on a run that only left it alone, and will do so \
             again at every start from now on: {report:?}",
        );
    }
    assert!(
        !said.contains("io-mcp.md"),
        "io-mcp was refreshed, not kept, and the report says otherwise: {report:?}",
    );
}

/// **F5.** A disabled skill stays disabled across a restart and an upgrade.
///
/// The sabotage is keying the skip on the enabled path alone — on "is there an
/// `io-provider.md` in `skills/`". Under it only this test fails, and disabling
/// becomes a thing that undoes itself on the next launch, which is worse than not
/// offering it.
#[test]
fn f5_a_disabled_skill_is_not_resurrected_by_the_next_run() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    skills::install(&home);

    // Disabling is a move, which is what `/skills` does with a keystroke.
    let disabled = skills::disabled_dir(&home);
    std::fs::create_dir_all(&disabled).expect("the disabled directory");
    let was = dir.join("io-provider.md");
    let now = disabled.join("io-provider.md");
    let text = read(&was);
    std::fs::rename(&was, &now).expect("the move");

    // Twice: a restart, and then an upgrade that has something else to do.
    skills::install(&home);
    // **The something else is a stale file, deliberately not a deleted one.** A
    // deleted shipped skill is not reinstalled — `rm` is the documented way to be
    // rid of one, and an installer that wrote it back would make deletion undo
    // itself, which is this very test's complaint about disabling through the
    // other door. So `io-update` is left on disk with older bytes and a manifest
    // entry that matches them, which is exactly what an operator who has touched
    // nothing looks like one release later.
    let stale = dir.join("io-update.md");
    std::fs::write(&stale, "stale\n").expect("older bytes");
    let mut entries = manifest(&home);
    for entry in &mut entries {
        if entry.0 == "io-update" {
            entry.1 = skills::digest(b"stale\n");
        }
    }
    let manifest_text: String = entries
        .iter()
        .map(|(name, hash)| format!("{name}\t{hash:016x}\n"))
        .collect();
    std::fs::write(home.join(".skills-manifest"), manifest_text).expect("the manifest");
    skills::install(&home);
    assert_ne!(
        read(&stale),
        "stale\n",
        "the second run had nothing to do, so this test proves nothing about an upgrade",
    );

    assert!(
        !was.exists(),
        "io-cli wrote a fresh io-provider.md beside the one the operator disabled",
    );
    assert_eq!(read(&now), text, "the disabled file was touched");

    let mut expected: Vec<String> = shipped_names()
        .into_iter()
        .filter(|name| name != "io-provider")
        .collect();
    expected.sort();
    assert_eq!(
        discovered(&dir),
        expected,
        "the catalogue io-harness composes names the skill the operator turned off",
    );
}

/// **N3.** The manifest is not a skill and the disabled folder is not a skill.
///
/// Asserted against the real `Skills::discover`, not against io-cli's idea of the
/// directory. The sabotage is putting the manifest in `skills/` as `manifest.md`
/// — under which only this test fails, and it fails by offering the model a skill
/// called `manifest` whose description is a hash table.
#[test]
fn n3_neither_the_manifest_nor_the_disabled_folder_is_offered_as_a_skill() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    skills::install(&home);

    // A disabled skill, and a second loose file in there — the folder holds no
    // `SKILL.md`, which is the whole reason discovery walks past it.
    let disabled = skills::disabled_dir(&home);
    std::fs::create_dir_all(&disabled).expect("the disabled directory");
    std::fs::rename(dir.join("io-update.md"), disabled.join("io-update.md")).expect("the move");
    std::fs::write(disabled.join("something-else.md"), "Turned off too.\n").expect("a second one");

    let names = discovered(&dir);
    let mut expected: Vec<String> = shipped_names()
        .into_iter()
        .filter(|name| name != "io-update")
        .collect();
    expected.sort();
    assert_eq!(
        names, expected,
        "discovery over an installed home does not return exactly the enabled set",
    );

    // The manifest exists, is in the home, and is not in the walk.
    let path = skills::manifest_path(&home);
    assert!(path.is_file(), "no manifest was written at all");
    assert_eq!(
        path.parent(),
        Some(home.as_path()),
        "the manifest is inside the directory every `.md` of which is offered to the model",
    );
    assert!(
        !dir.join(skills::MANIFEST).exists(),
        "there is a manifest in the skills directory, every `.md` of which is a skill",
    );
    for name in &names {
        assert!(
            !name.contains("manifest") && name != "disabled",
            "`{name}` is in the catalogue, and it is not a skill",
        );
    }
}

/// A directory that will not discover is reported, not added to.
///
/// The set is already ambiguous, so every turn of that session is going to fail
/// until somebody moves a file. Writing five more into it can only make that
/// harder — and io-cli's own report is the one place the operator can be told
/// the harness's own sentence, because the palette shows them an empty list.
#[test]
fn a_directory_that_will_not_discover_is_reported_and_left_alone() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    std::fs::create_dir_all(&dir).expect("the skills directory");
    for name in ["one.md", "two.md"] {
        std::fs::write(dir.join(name), "---\nname: same\n---\n\nAmbiguous.\n").expect("a skill");
    }

    let report = skills::install(&home);

    assert_eq!(
        std::fs::read_dir(&dir).expect("the directory").count(),
        2,
        "io-cli wrote into a directory that was already broken",
    );
    assert!(
        !skills::manifest_path(&home).exists(),
        "a manifest was written for skills that were never installed",
    );
    assert_eq!(
        report.len(),
        1,
        "one line, and it is the harness's: {report:?}"
    );
    assert!(
        report[0].contains("same"),
        "the report does not carry io-harness's own message: {report:?}",
    );
}

/// `wrote` answers provenance from the manifest, not from the `io-` prefix.
///
/// This is what `/skills` will decide origin on, and the prefix is a courtesy
/// rather than a guarantee: an operator's own `io-thing.md` is theirs, and a
/// shipped skill they have edited is theirs too.
#[test]
fn provenance_comes_from_the_manifest_and_never_from_the_name() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    skills::install(&home);

    let mcp = dir.join("io-mcp.md");
    assert!(
        skills::wrote(&home, "io-mcp", &mcp),
        "a file io-cli just wrote does not read as io-cli's",
    );

    let theirs = dir.join("io-thing.md");
    std::fs::write(&theirs, "A skill the operator wrote with an io- name.\n").expect("their file");
    assert!(
        !skills::wrote(&home, "io-thing", &theirs),
        "a file the operator wrote reads as io-cli's because of its name",
    );

    std::fs::write(&mcp, format!("{}\nEdited.\n", read(&mcp))).expect("their edit");
    assert!(
        !skills::wrote(&home, "io-mcp", &mcp),
        "a shipped skill the operator has edited still reads as io-cli's",
    );
}

/// **A shipped skill the operator deleted is not written back.**
///
/// `rm ~/.io-cli/skills/io-mcp.md` is the documented way to be rid of one — the
/// release record says so in `delivery.migrations` and again in
/// `delivery.rollback`, and the README says it to the operator. An installer that
/// wrote the file back on the next start would make deletion a thing that undoes
/// itself, which is the same failure the disabled check prevents through the
/// other door, and the operator's only recourse would be to delete it again after
/// every launch.
///
/// The record is the manifest: io-cli knows it wrote that name, and the file is
/// gone, so it was taken away on purpose. **A name with no record at all is a
/// skill this version ships and an earlier one did not**, which is what keeps an
/// upgrade able to deliver new skills — asserted here too, because a fix that
/// stopped installing anything would also pass the first half.
///
/// Sabotage: drop the manifest check from the absent-file branch of `install`.
/// Under it only this test fails, on the second start rather than the first.
#[test]
fn a_deleted_shipped_skill_is_not_written_back_on_the_next_run() {
    let (_dir, home) = home();
    let dir = skills::dir(&home);
    skills::install(&home);

    let gone = dir.join("io-mcp.md");
    std::fs::remove_file(&gone).expect("the operator deletes one");

    let report = skills::install(&home);
    assert!(
        !gone.exists(),
        "io-cli wrote back a skill the operator deleted; deleting it is now a thing \
         they have to do at every start",
    );
    assert!(
        report.is_empty(),
        "and it said something about a run in which it did nothing: {report:?}",
    );

    // The other half: a name io-cli has no record of is a skill a later version
    // added, and it still installs. Simulated by taking `io-remember` out of the
    // manifest while leaving no file — which is exactly the state an upgrade that
    // ships a sixth skill is in for that sixth name.
    let entries: Vec<(String, u64)> = manifest(&home)
        .into_iter()
        .filter(|(name, _)| name != "io-remember")
        .collect();
    let text: String = entries
        .iter()
        .map(|(name, hash)| format!("{name}\t{hash:016x}\n"))
        .collect();
    std::fs::write(home.join(".skills-manifest"), text).expect("the manifest");
    std::fs::remove_file(dir.join("io-remember.md")).expect("no file for that name either");

    skills::install(&home);
    assert!(
        dir.join("io-remember.md").is_file(),
        "a name io-cli has never recorded is one a later version added, and it must \
         still install — otherwise an upgrade can never deliver a new skill",
    );
}
