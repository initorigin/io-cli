//! The documentation is checked against the code, not against a reviewer's
//! memory.
//!
//! The keybinding table is a shipped artifact, and a table that has drifted from
//! what the keys actually do is worse than no table: it is folklore with a
//! typeface. The same constants feed `/help` and the README, and these tests fail
//! when one of them stops agreeing.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use io_cli::commands::{COMMANDS, KEYS};

/// Held by every test in this file that reads or writes an `IO_CONFIG*` variable.
///
/// The same shape `tests/wizard.rs` and `tests/home.rs` use. The environment is
/// process-wide and this binary's tests share a process, so a second writer makes
/// the first one's answer wrong intermittently — the most expensive kind of
/// failure to diagnose, and the reason this guard exists before there is a
/// second writer rather than after.
fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(name: &str) -> String {
    std::fs::read_to_string(repo().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

/// The text between two markers.
fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let start = format!("<!-- {name}:start -->");
    let end = format!("<!-- {name}:end -->");
    let from = text
        .find(&start)
        .unwrap_or_else(|| panic!("the README has no {start}"))
        + start.len();
    let to = text
        .find(&end)
        .unwrap_or_else(|| panic!("the README has no {end}"));
    &text[from..to]
}

#[test]
fn the_readme_key_table_is_the_key_table() {
    let readme = read("README.md");
    let table = section(&readme, "keys");

    for (key, what) in KEYS {
        assert!(
            table.contains(key),
            "the README's key table is missing {key}",
        );
        assert!(
            table.contains(what),
            "the README describes {key} differently from the code",
        );
    }

    let rows = table.lines().filter(|line| line.starts_with("| `")).count();
    assert_eq!(
        rows,
        KEYS.len(),
        "the README's key table has {rows} rows and the code binds {}",
        KEYS.len(),
    );
}

/// The rebinding table under the key table, which is a second table with a second
/// job: the first says what a key does, this one says what a configuration file
/// may call it and what it is bound to today.
///
/// It sits outside the `keys` markers because [`KEYS`] is not a list of actions —
/// it holds the composer's keys and an approval's letters too — so a "rebindable?"
/// column there would have been a column that is empty for five of eleven rows.
/// What replaces it is this: the row is generated from `Action` and asserted, so
/// the prose cannot name an action the code does not have, quote a default the
/// code does not bind, or offer `interrupt` as something to move.
#[test]
fn the_readme_says_which_actions_can_be_rebound_and_to_what() {
    use io_cli::keys::{Action, Keys};

    let readme = read("README.md");
    let keys = Keys::default();

    for action in Action::ALL.iter().copied() {
        let row = format!("| `{}` | `{}` |", action.name(), keys.binding(action));
        if action.rebindable() {
            assert!(
                readme.contains(&row),
                "the README should offer `{}` with its default, as `{row}`",
                action.name(),
            );
        } else {
            assert!(
                !readme.contains(&format!("| `{}` |", action.name())),
                "the README offers `{}` as rebindable and the code refuses it",
                action.name(),
            );
        }
    }
}

#[test]
fn the_readme_command_table_is_the_command_table() {
    let readme = read("README.md");
    let table = section(&readme, "commands");

    for (name, what) in COMMANDS {
        assert!(table.contains(name), "the README is missing {name}");
        assert!(
            table.contains(what),
            "the README describes {name} differently from the code",
        );
    }

    let rows = table
        .lines()
        .filter(|line| line.starts_with("| `/"))
        .count();
    assert_eq!(rows, COMMANDS.len());
}

/// The README's account of the skills this crate ships.
///
/// The names are read out of `skills/` rather than listed here, so a sixth
/// shipped skill fails this test instead of arriving undocumented — which is the
/// shape of drift the command table already has a gate for and this section did
/// not. The resolved name is the frontmatter `name:` and not the filename,
/// because that is the name io-harness addresses a skill by and therefore the
/// name an operator has to avoid claiming.
///
/// The two rules asserted under it are the ones an operator can only learn from
/// prose: a duplicate name is fatal at the harness level, and there is a ceiling
/// that rejects the whole set rather than the excess. Both are asserted on a
/// short distinctive phrase rather than on a sentence, because a sentence gets
/// reworded and a gate that fails on an improvement teaches people to delete
/// gates.
#[test]
fn the_readme_documents_every_skill_this_crate_ships() {
    let readme = read("README.md");
    let (_, section) = readme
        .split_once("## Skills")
        .expect("the README should have a section about the shipped skills");
    let section = section.split("\n## ").next().unwrap_or(section);

    let mut shipped = 0;
    for entry in std::fs::read_dir(repo().join("skills")).expect("the shipped skills directory") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|end| end.to_str()) != Some("md") {
            continue;
        }
        let body = std::fs::read_to_string(&path).expect("a skill body");
        let name = body
            .lines()
            .find_map(|line| line.strip_prefix("name:"))
            .unwrap_or_else(|| panic!("{} declares no name in its frontmatter", path.display()))
            .trim();
        assert!(
            section.contains(&format!("`{name}`")),
            "the README should name the {name} skill and say what it is for",
        );
        shipped += 1;
    }
    assert!(shipped > 0, "there should be skills to document");

    // The collision rule, in both halves: what it costs, and what io-cli does
    // about it. Half of it is worse than none — "io-cli withholds a colliding
    // skill" without the consequence reads as tidiness rather than as the
    // session-killer it avoids.
    assert!(
        section.contains("every turn of that session"),
        "the README should say what a duplicate skill name costs",
    );
    assert!(
        section.contains("frontmatter"),
        "the README should say the claimed name is the frontmatter name, not the filename",
    );
    assert!(
        section.contains("withheld"),
        "the README should say io-cli withholds rather than overwrites a claimed name",
    );

    // And the ceiling, with the number, because "a limit on skills" is not
    // actionable and 64 is.
    assert!(
        section.contains("64 skills"),
        "the README should name the 64-skill ceiling",
    );
    assert!(
        section.contains("rejects the whole set"),
        "the README should say the ceiling rejects the set rather than trimming it",
    );
}

/// The `[app.io-cli]` table in the README, sliced by the prose either side of it.
///
/// It has no `<!-- -->` markers because it is not generated — the descriptions are
/// written for a reader, not derived from a doc comment. What is asserted instead
/// is that it has a row for every key the struct actually carries, which is the
/// gate that was missing: `max_steps` was added to `CliSettings` in 0.10.0 and
/// this table never mentioned it once.
fn settings_table(readme: &str) -> &str {
    let from = readme
        .find("keys live there")
        .expect("the README introduces the [app.io-cli] table");
    let to = readme[from..]
        .find("Because the section is unvalidated")
        .expect("the prose that closes the [app.io-cli] table")
        + from;
    &readme[from..to]
}

#[test]
fn the_readme_documents_every_key_of_the_io_cli_section() {
    // Serialized rather than listed by hand, so a key added to `CliSettings`
    // fails here instead of shipping undocumented. A scalar is written as
    // `theme`; a table is `[app.io-cli.keys]` and a list of tables is
    // `[[app.io-cli.mcp]]`, and any of the three spellings satisfies the row.
    let readme = read("README.md");
    let table = settings_table(&readme);

    // Every field set, because `skip_serializing_if` drops a `None` — a default
    // value here would assert against a table with nothing in it.
    let every = io_cli::settings::CliSettings {
        theme: Some("dark".into()),
        diff: Some("unified".into()),
        glyphs: Some("ascii".into()),
        plain: Some(false),
        keys: Some(Default::default()),
        containment: Some(io_harness::Containment::new(12, 4, 2, 200_000)),
        mcp: Some(Vec::new()),
        lsp: Some(Vec::new()),
        browser: Some(io_harness::BrowserConfig::default()),
        skills: Some("/skills".into()),
        max_parallel_reads: Some(16),
        spawn_background_after_secs: Some(120),
        detached_spawns: Some(true),
    };
    let value = serde_json::to_value(&every).expect("[app.io-cli] serializes");
    let keys = value.as_object().expect("a table");

    let (mut scalars, mut tables) = (0, 0);
    for (name, held) in keys {
        let spellings = [
            format!("| `{name}` |"),
            format!("| `[app.io-cli.{name}]` |"),
            format!("| `[[app.io-cli.{name}]]` |"),
        ];
        assert!(
            spellings.iter().any(|row| table.contains(row)),
            "the README's [app.io-cli] table has no row for `{name}`",
        );
        if held.is_object() || held.is_array() {
            tables += 1;
        } else {
            scalars += 1;
        }
    }

    let rows = table.lines().filter(|line| line.starts_with("| `")).count();
    assert_eq!(
        rows,
        keys.len(),
        "the README's [app.io-cli] table has {rows} rows and the section has {}",
        keys.len(),
    );

    // The sentence above the table counts them, and a count written in prose is
    // the first half of a table to go stale — this one said "five keys and four
    // tables" over a table of five keys and five tables, for four releases.
    let words = [
        "no", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
        "Eleven", "Twelve",
    ];
    let sentence = format!(
        "{} keys live there, and {} tables",
        words[scalars],
        words[tables].to_lowercase(),
    );
    assert!(
        readme.contains(&sentence),
        "the README should say `{sentence}`",
    );
}

#[test]
fn the_readme_quotes_the_budget_fields_the_status_line_actually_draws() {
    // 0.14.0's F6. The budgets are the one part of the status line a README can
    // quote verbatim, and a quoted format that has drifted is worse than no
    // example: an operator reads it as the thing to grep their scrollback for.
    let readme = read("README.md");
    let mut status = io_cli::status::Status::new("a-model");
    status.budgets = io_cli::status::Budgets {
        steps: Some(20),
        tokens: Some(200_000),
        duration: Some(std::time::Duration::from_secs(600)),
        // The context window is a denominator rather than a remainder, so it
        // draws no budget row of its own — `ctx N%` is where it shows. `None`
        // here keeps this fixture about the three that do draw rows.
        window: None,
    };
    status.steps = Some(3);
    status.run_tokens = Some(187_600);
    status.elapsed = std::time::Duration::from_secs(330);

    let drawn = status.budgets_left();
    assert_eq!(drawn.len(), 3, "one field per budget in force: {drawn:?}");
    for text in drawn {
        assert!(
            readme.contains(&format!("`{text}`")),
            "the README should quote the budget field the line draws: `{text}`",
        );
    }

    // And a session that configured nothing draws none of them, which is the
    // half of F6 that keeps io-cli's own step floor off the line.
    let quiet = io_cli::status::Status::new("a-model");
    assert!(quiet.budgets_left().is_empty());
}

#[test]
fn no_documentation_surface_still_claims_the_old_asymmetry() {
    // Through 0.13.1 both files said an interactive session read past most of
    // `io.toml` and that `[app.io-cli.containment]` was what carried the
    // capabilities. The first stopped being true in 0.14.0 and the second in
    // 0.11.0. A claim this specific cannot be caught by reading the diff of the
    // release that falsifies it, so it is caught here.
    let readme = read("README.md");
    let example = read("docs/config.example.toml");
    for (name, text) in [
        ("README.md", &readme),
        ("docs/config.example.toml", &example),
    ] {
        for stale in [
            "Not read by an interactive session",
            "Contained turns only",
            "and on no other turn",
        ] {
            assert!(
                !text.contains(stale),
                "{name} still says {stale:?}, which has not been true since 0.14.0",
            );
        }
    }

    // Every section `Config::apply_to` carries is named in the example as
    // something that reaches both arms, so a reader looking for one finds it
    // where the old block used to deny it.
    for section in [
        "[sandbox]",
        "[run]",
        "[run.commit_identity]",
        "[instructions]",
        "[[mcp]]",
        "[[lsp]]",
        "[[agent]]",
        "[web]",
        "[memory]",
        "[browser]",
    ] {
        assert!(
            example.contains(section),
            "docs/config.example.toml should document {section}",
        );
    }

    // The two facts about that widening an operator has to be told rather than
    // left to discover: `[web]` is a capability the *vendor* exercises, so the
    // local `net` rule is not what governs it, and `[browser]` is refused in a
    // project-scoped file by io-harness itself.
    for (name, text) in [
        ("README.md", &readme),
        ("docs/config.example.toml", &example),
    ] {
        let said = text.to_lowercase();
        assert!(
            said.contains("capability") && said.contains("vendor"),
            "{name} should say `[web]` is a capability and that the vendor dials the URL",
        );
        assert!(
            said.contains("project"),
            "{name} should say where `[browser]` is refused",
        );
    }
}

/// Every comment in the crate, read for this release's version of the same
/// mistake: **a sentence saying a turn cannot be steered.**
///
/// The test above catches the 0.14.0 shape of it in two prose files. This one is
/// the 0.17.0 shape and it reads the *source*, because that is where the claim
/// lived: through io-harness 0.66 no session entry point took a caller's
/// containment and a `SteerInbox` on one call, so a dozen comments explained the
/// arms in terms of what each gave up to get the other. 0.67.0 opened
/// `turn_bounded_steered` and `turn_contained_bounded_steered`, io-cli took both,
/// and every one of those sentences became folklore with a typeface — the exact
/// failure this file exists for. A reviewer reading the release diff sees the two
/// call sites change and has no reason to open `settings.rs`; this does.
///
/// **Comment lines only, and that is the whole of the narrowing.** Two things
/// must go on saying these words and neither is a claim about today:
///
/// - the assertions that hold io-harness's *deaf* entry points to their name —
///   `turn_bounded_observed` really does take no inbox, and `tests/steer.rs` says
///   so in a `format!` — and the arm of `tests/contain.rs` that asserts
///   `"cannot be steered"` is absent from the containment notice. Both are code,
///   not comments, so neither is read here.
/// - `CHANGELOG.md`, which is a record of releases rather than a description of
///   this one: its 0.14.0 and 0.11.0 entries were true when they were written and
///   rewriting them would be falsifying the history. Its `0.17.0` section is
///   checked by `the_changelog_has_a_section_for_this_version` instead.
///
/// And the sentences explaining why `Ctrl+C` is **not** on the inbox are the
/// point of the release rather than a violation of it. They say the interrupt
/// travels as `Flow::Cancel` on the observer; none of them says a turn cannot be
/// steered, so none of the phrases below can match one.
///
/// Sabotage: put any single corrected comment back — `settings.rs`'s "neither
/// turn takes a `SteerInbox` any more", `main.rs`'s "Neither turn takes a
/// `SteerInbox`", `commands.rs`'s "fan out, or be steered", `README.md`'s "no
/// session turn takes a steer inbox". Only this test fails, and it names the file
/// and the sentence.
#[test]
fn no_comment_still_says_a_turn_cannot_be_steered() {
    // Each of these was a true sentence in some release and is a false one now.
    // Written as the words that carry the claim rather than as whole sentences,
    // because the sentence around them is what an author rewrites while leaving
    // the claim standing.
    const FALSEHOODS: &[&str] = &[
        // "no entry point takes a contract and an inbox together"
        "containment and a steer inbox together",
        "a contract and a steer inbox together",
        // "neither arm takes an inbox"
        "neither takes a steer inbox",
        "neither turn takes a `SteerInbox`",
        "Neither turn takes a `SteerInbox`",
        "takes a `SteerInbox` any more",
        "no session turn takes a steer inbox",
        "session turn takes a `SteerInbox`",
        // "a contained turn takes no inbox"
        "A contained turn takes no",
        "contained turn takes no `SteerInbox`",
        // "containment is what decides whether a turn can be steered"
        "fan out, or be steered",
        "be steered and one that can fan out",
        "either fans out or is steerable",
        "turned steering off",
        "cannot be steered",
    ];

    let mut sources = Vec::new();
    collect(&repo().join("src"), &mut sources);
    collect(&repo().join("tests"), &mut sources);

    for path in sources {
        // This file, and only this file, is allowed to write the sentences down:
        // the list above is what they are, and the doc comment names four of them
        // so the sabotage arm can be run without going to find them. A gate that
        // could not quote what it forbids would have to describe it instead, and
        // a description is the thing that drifts.
        if path.file_name().is_some_and(|name| name == "docs.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let shown = path.strip_prefix(repo()).unwrap_or(path.as_path());
        for (number, line) in text.lines().enumerate() {
            if !line.trim_start().starts_with("//") {
                continue;
            }
            for claim in FALSEHOODS {
                assert!(
                    !line.contains(claim),
                    "{}:{} still says {claim:?}. A contained turn CAN be steered \
                     since 0.17.0 — both arms are driven through the `_steered` \
                     entry points and both hold a `SteerInbox`. Containment \
                     decides fan-out and nothing else.\n  {}",
                    shown.display(),
                    number + 1,
                    line.trim(),
                );
            }
        }
    }

    // The README carries the same claim in prose rather than in comments, so the
    // whole file is read.
    let readme = read("README.md");
    for claim in FALSEHOODS {
        assert!(
            !readme.contains(claim),
            "README.md still says {claim:?}, which has not been true since 0.17.0",
        );
    }

    // And the positive half, because a gate made only of absences passes on a
    // file that says nothing at all. Somewhere the README has to state what is
    // true now, or an operator reading it learns that containment costs a steer
    // from the silence where the correction should be.
    let said = readme.to_lowercase();
    assert!(
        said.contains("steer inbox") || said.contains("/steer"),
        "the README should say a turn can be spoken to while it runs",
    );
    assert!(
        said.contains("contained turn can be steered"),
        "the README should say the containment switch no longer decides it",
    );
}

/// Every `.rs` file under `dir`, recursively.
fn collect(dir: &std::path::Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            into.push(path);
        }
    }
}

#[test]
fn the_removed_step_cap_is_announced_where_it_was_promised() {
    // F12. The key is GONE from `CliSettings` as of 0.16.0, and the notice is
    // not — because `CliSettings` carries no `deny_unknown_fields`, so a file
    // still holding the key would otherwise be ignored in silence and the
    // operator's step cap would change with no error anywhere.
    //
    // So the notice reads the RAW section now. It is built through
    // `Config::from_toml` rather than from a struct, which is the only way left
    // to express "a file that still has this key" — there is no field to set.
    let still_has_it =
        io_harness::Config::from_toml("[app.io-cli]\nmax_steps = 40\ntheme = \"dark\"\n")
            .expect("a leftover key still LOADS, which is exactly the problem");
    let notice = io_cli::settings::deprecated_max_steps(&still_has_it)
        .expect("a file that still writes the key earns the notice");
    assert!(notice.contains("[run] max_steps"), "{notice}");
    assert!(notice.contains("0.16.0"), "{notice}");
    assert!(
        notice.contains("no longer read"),
        "the notice must say the key is dead, not merely deprecated: {notice}"
    );

    for name in ["README.md", "CHANGELOG.md"] {
        let text = read(name);
        assert!(
            text.contains("`[app.io-cli] max_steps`"),
            "{name} should name the removed key",
        );
        assert!(
            text.contains("removed in 0.16.0"),
            "{name} should say when the key stopped being read",
        );
    }

    // A file that never wrote it is told nothing, which is the other half of F12
    // — a notice on a session that is not affected teaches operators to stop
    // reading notices.
    let clean = io_harness::Config::from_toml("[app.io-cli]\ntheme = \"dark\"\n").unwrap();
    assert!(io_cli::settings::deprecated_max_steps(&clean).is_none());
    let empty = io_harness::Config::from_toml("").unwrap();
    assert!(io_cli::settings::deprecated_max_steps(&empty).is_none());
}

#[test]
fn the_changelog_has_a_section_for_this_version() {
    // `release.yml` refuses to cut a Release without one, and finding that out
    // from a failed workflow is four cross-compiles too late.
    let version = env!("CARGO_PKG_VERSION");
    let changelog = read("CHANGELOG.md");
    assert!(
        changelog.contains(&format!("## [{version}]")),
        "CHANGELOG.md has no '## [{version}]' section",
    );

    let start = changelog
        .find(&format!("## [{version}]"))
        .expect("the section");
    let rest = &changelog[start..];
    let body = rest
        .split_once("\n## [")
        .map(|(body, _)| body)
        .unwrap_or(rest);
    assert!(
        body.lines().filter(|line| !line.trim().is_empty()).count() > 3,
        "the {version} section is a heading with nothing under it",
    );
}

/// The 0.19.0 section, and the one thing in it a reader cannot find out any
/// other way.
///
/// A command that changes group changes where an operator finds it: nothing
/// about `/mcp` or `/provider` was rewritten, so the only trace of the move is
/// that `/help` lists them somewhere else than it did yesterday. A release note
/// that leaves that out has documented the features and hidden the one change
/// that will make somebody think a command was removed. Pinned to `0.19.0`
/// rather than to `CARGO_PKG_VERSION`, because this is a fact about one release
/// and not a rule about every one.
#[test]
fn the_changelog_says_which_commands_changed_group() {
    let changelog = read("CHANGELOG.md");
    let start = changelog
        .find("## [0.19.0]")
        .expect("CHANGELOG.md should have a 0.19.0 section");
    let rest = &changelog[start..];
    let body = rest
        .split_once("\n## [")
        .map(|(body, _)| body)
        .unwrap_or(rest);

    for moved in ["`/mcp`", "`/provider`"] {
        assert!(
            body.contains(moved),
            "the 0.19.0 section should say {moved} changed group",
        );
    }
    assert!(
        body.contains("configure"),
        "the 0.19.0 section should say which group they moved to",
    );
}

#[test]
fn the_readme_states_what_the_checksum_does_not_defend_against() {
    // N7's own words: the README says this rather than implying more. A `curl |
    // sh` install is a trust-the-publisher model however the script is written,
    // and a page that leaves that unsaid is overselling it.
    let readme = read("README.md");
    assert!(
        readme.contains("compromised repository"),
        "the README should say what the checksum does not cover",
    );
    assert!(readme.contains("trust-the-publisher"), "{readme}");
}

/// The paragraph that says where io keeps an operator's files.
///
/// It is prose rather than a generated table, and until 0.15.0 nothing asserted
/// a word of it — which is how a repository ends up with a discovery ladder in
/// its README that no release has read since the one that wrote it. The
/// directory name is taken from [`io_cli::home::path`] and the variable from
/// io-harness, so renaming either fails here rather than leaving the README
/// naming a directory the binary no longer uses.
#[test]
fn the_readme_says_where_io_keeps_its_files() {
    let readme = read("README.md");
    let (_, section) = readme
        .split_once("### Where io keeps your things")
        .expect("the README should have a section saying where io keeps its files");
    let section = section.split("\n## ").next().unwrap_or(section);

    let home = io_cli::home::path().expect("a home directory to take the name from");
    let dir = home
        .file_name()
        .expect("the home is a directory under the operator's own")
        .to_string_lossy()
        .into_owned();
    // The last two arrived with 0.19.0, and they are here rather than in the
    // skills section's own gate because this is the paragraph an operator reads
    // when they are asking what io put on their disk. `disabled/` is a directory
    // that changes behaviour, and the manifest is a dotfile they did not create;
    // both are things to find named somewhere before finding them by accident.
    for named in [
        format!("`~/{dir}`"),
        format!("`%USERPROFILE%\\{dir}`"),
        format!("`~/{dir}/skills`"),
        format!("`~/{dir}/skills/disabled/`"),
        format!("`~/{dir}/.skills-manifest`"),
    ] {
        assert!(
            section.contains(&named),
            "the README should name {named}, which is where io-cli actually looks",
        );
    }

    // The ladder in the order io-harness walks it. Asserted as positions and not
    // as four `contains`, because a paragraph that names all four in the wrong
    // order is the way this sentence goes wrong and every `contains` passes on
    // it.
    let mut walked = 0;
    for rung in [
        "`$IO_CONFIG`",
        "`$IO_CONFIG_HOME/io.toml`",
        "`$XDG_CONFIG_HOME/io/io.toml`",
        "`%APPDATA%\\io\\io.toml`",
    ] {
        let at = section
            .find(rung)
            .unwrap_or_else(|| panic!("the README should name {rung} in the discovery order"));
        assert!(at > walked, "{rung} is out of order in the README's ladder");
        walked = at;
    }

    // N4: io-cli sets the variable in its own process environment, so every
    // child a session starts inherits it. A README that names the home and
    // leaves that out has documented the outcome and hidden the mechanism.
    let var = io_harness::config::CONFIG_HOME_VAR;
    assert!(
        section.contains(var),
        "the README should name the {var} io-cli sets to put the file there",
    );
    assert!(
        section.contains("inherit") && section.contains("nested `io`"),
        "the README should say every child a session starts inherits {var}",
    );

    // And the one act of this release that touches files io-cli did not create,
    // said where an operator upgrading will read it rather than in a changelog.
    assert!(
        section.contains("moved into the home"),
        "the README should say an existing install is moved on the first run",
    );
    assert!(
        section.contains("nothing is overwritten") && section.contains("Nothing is deleted"),
        "the README should say what the move does not do",
    );
}

#[test]
fn the_shipped_configuration_example_is_a_file_io_harness_accepts() {
    // The example is documentation that runs: if the harness stops accepting it,
    // it has stopped being an example and started being a trap.
    let text = std::fs::read_to_string(repo().join("docs/config.example.toml"))
        .expect("docs/config.example.toml");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let home = dir.path().join("home");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(&home).expect("a home");
    std::fs::create_dir_all(&workspace).expect("a workspace");
    let path = home.join("io.toml");
    std::fs::write(&path, &text).expect("write the example");

    // Read as the USER scope, which is what it documents and where widening is
    // allowed. Read as a project file it would rightly be refused.
    //
    // Under the lock from 0.15.0. This was the one `IO_CONFIG*` writer in this
    // repository's tests that took no guard, and it was harmless only while
    // nothing else in the process wrote that variable — which stopped being true
    // when `home::adopt` became a writer of it.
    let _guard = env_lock();
    std::env::set_var("IO_CONFIG", &path);
    let config = io_harness::Config::discover(&workspace);
    std::env::remove_var("IO_CONFIG");

    let config = config.expect("io-harness accepts the shipped example");
    assert!(
        config.provider_spec().is_some(),
        "the example should configure a provider",
    );
    assert!(
        config.policy().is_some(),
        "the example should configure a policy",
    );
    let settings: Option<io_cli::settings::CliSettings> =
        config.app(io_cli::settings::APP_KEY).expect("[app.io-cli]");
    assert_eq!(
        settings.and_then(|settings| settings.theme).as_deref(),
        Some("dark"),
    );
}

#[test]
fn every_documentation_file_the_repository_promises_exists() {
    for name in [
        "LICENSE",
        "NOTICE",
        "README.md",
        "CHANGELOG.md",
        "CONTRIBUTING.md",
        "CODE_OF_CONDUCT.md",
        "SECURITY.md",
        "install.sh",
        "install.ps1",
        "docs/config.example.toml",
        ".github/CODEOWNERS",
        ".github/dependabot.yml",
        ".github/PULL_REQUEST_TEMPLATE.md",
        ".github/workflows/ci.yml",
        ".github/workflows/release.yml",
    ] {
        assert!(
            repo().join(name).is_file(),
            "{name} is referred to but missing",
        );
    }
}

#[test]
fn the_notice_carries_the_attribution_and_the_licence_body_is_untouched() {
    let notice = read("NOTICE");
    assert!(
        notice.contains("Copyright 2026 Aakash Pawar (InitOrigin)"),
        "{notice}"
    );
    assert!(read("README.md").contains("Copyright 2026 Aakash Pawar (InitOrigin)"));

    // The Apache text itself is pristine: the attribution goes in NOTICE and the
    // README, never into the licence.
    let licence = read("LICENSE");
    assert!(licence.starts_with("                                 Apache License"));
    assert!(
        !licence.contains("InitOrigin"),
        "the Apache-2.0 body must be unmodified",
    );
}

#[test]
fn the_workspace_directory_is_not_shipped_inside_the_crate() {
    // `.ultraship` is planning state: tool-written maintainer notes, not part of
    // what a user installs and not a contributor-facing surface. The whole tree
    // is ignored, so the rule is one line and there is no sub-path to keep in
    // step with it. The bare entry is asserted rather than a `contains`, because
    // a narrower rule such as `.ultraship/products/*/evidence/` also contains it.
    let gitignore = read(".gitignore");
    assert!(
        gitignore.lines().any(|line| line.trim() == ".ultraship/"),
        "the planning tree must stay off the repository: {gitignore}",
    );
}

#[test]
fn the_readme_exit_table_is_the_exit_table() {
    // The exit codes are public contract from 0.5.0 onward: a script branches on
    // them and cannot be migrated by fixing forward. A table in the README that
    // drifted from the constants would be worse than no table at all.
    let readme = read("README.md");
    for (code, what) in [
        (io_cli::exec::OK, "of its own accord"),
        (io_cli::exec::FAILED, "never got that far"),
        (io_cli::exec::REFUSED, "a boundary said no"),
        (io_cli::exec::CEILING, "a ceiling was reached"),
        (io_cli::exec::PAUSED, "needing a human"),
        (io_cli::exec::UNFINISHED, "without finishing"),
    ] {
        let row = format!("| `{code}` |");
        assert!(
            readme.contains(&row),
            "the README's exit table has no row for {code}",
        );
        assert!(
            readme.contains(what),
            "the README describes {code} differently from the code: {what}",
        );
    }
}

#[test]
fn the_readme_lists_every_flag_io_exec_actually_takes() {
    use clap::CommandFactory;

    let readme = read("README.md");
    let cli = io_cli::cli::Cli::command();
    let exec = cli
        .get_subcommands()
        .find(|sub| sub.get_name() == "exec")
        .expect("`io exec` is a subcommand");

    let flags: Vec<String> = exec
        .get_arguments()
        .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
        .filter(|long| long != "--help")
        .collect();

    assert!(!flags.is_empty(), "there should be flags to check");
    for flag in &flags {
        assert!(
            readme.contains(&format!("`{flag}")),
            "`io exec` takes {flag} and the README does not mention it",
        );
    }

    // And nothing the README promises has been removed from the binary.
    for promised in ["--json", "--sandbox", "--policy", "--provider"] {
        assert!(
            flags.iter().any(|flag| flag == promised),
            "the README documents {promised} and `io exec` no longer takes it",
        );
    }
}
