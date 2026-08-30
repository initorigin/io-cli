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
        prices: Some(io_cli::settings::PriceSettings {
            source_url: Some("https://example.invalid/models".into()),
            source: Some("the reference catalogue".into()),
            models: Some(417),
        }),
        // Written out field by field rather than as a `Default::default()`, which
        // would serialize to `{}` and satisfy the row check just as well. The
        // point is the compile error: `[app.io-cli.gates]` is the one nested
        // table whose keys are explained in README prose rather than in a row of
        // their own, so a key added to `gates::Settings` has to break something
        // to be noticed. This is that something.
        gates: Some(io_cli::gates::Settings {
            retries: Some(2),
            command: Some(vec!["cargo".into(), "test".into()]),
            expect_exit: Some(0),
            file: Some("CHANGELOG.md".into()),
            contains: Some("## [0.24.0]".into()),
            rubric: Some("the change is covered by a test that fails without it".into()),
            reviewer: Some("a-reviewing-model".into()),
            allow_self_review: Some(false),
        }),
        conversational: Some(false),
        // Written out field by field for `gates`' reason, and with the same
        // consequence: `[app.io-cli.routing]`'s two rules are sub-tables whose
        // keys are explained in README prose rather than in rows of their own, so
        // a key added to `routing::Settings` has to break something to be noticed.
        routing: Some(io_cli::routing::Settings {
            escalate_after: Some(io_cli::routing::Escalation {
                failures: Some(3),
                model: Some("a-stronger-model".into()),
            }),
            downshift_under: Some(io_cli::routing::Downshift {
                bytes: Some(2_000),
                model: Some("a-cheaper-model".into()),
            }),
        }),
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

    // 0.24.0's `6`, and it is asserted as a NUMBER rather than through a constant
    // on purpose. What a script branches on is the integer, and the one mistake
    // this release could make that a caller cannot recover from is landing the
    // gate code on a number that already meant something else. So the claim is
    // that `6` is the code immediately after `UNFINISHED`, which is where the six
    // that shipped in 0.5.0 stopped. Move any of those six and this fails; give
    // the gate a number one of them already holds and it fails too.
    assert_eq!(
        io_cli::exec::UNFINISHED + 1,
        6,
        "the gate code sits after the six that shipped in 0.5.0; a code that moved \
         one of them cannot be migrated by fixing forward",
    );
    assert!(
        readme.contains("| `6` |"),
        "the README's exit table has no row for the gate that did not pass",
    );
    assert!(
        readme.contains("does not hold up"),
        "the README should say what `6` means in the words the release uses: the \
         agent finished and the work does not hold up",
    );
    // And the half a reader most needs: nothing above it moved. A release that
    // renumbered `5` would leave every script branching on it silently wrong,
    // which is the failure the sentence exists to rule out.
    assert!(
        readme.contains("No exit code was renumbered"),
        "the README should say, where the table is, that no existing code changed \
         meaning",
    );
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

/// The same gate for `io resume`, which is 0.23.0's subcommand and has three
/// times as many flags to go stale.
///
/// The absent flag is asserted as well, and it is not symmetry for its own sake:
/// the README states in words that there is deliberately no `--sandbox` here,
/// because a resumed run already started under a boundary and widening it halfway
/// through is a widening nobody asked for at the point nobody is watching. A
/// `--sandbox` added later would leave that paragraph standing and wrong, which is
/// precisely the failure this file exists for.
#[test]
fn the_readme_documents_every_flag_io_resume_actually_takes() {
    use clap::CommandFactory;

    let readme = read("README.md");
    let cli = io_cli::cli::Cli::command();
    let resume = cli
        .get_subcommands()
        .find(|sub| sub.get_name() == "resume")
        .expect("`io resume` is a subcommand");

    let flags: Vec<String> = resume
        .get_arguments()
        .filter_map(|arg| arg.get_long().map(|long| format!("--{long}")))
        .filter(|long| long != "--help")
        .collect();

    assert!(!flags.is_empty(), "there should be flags to check");
    for flag in &flags {
        assert!(
            readme.contains(&format!("`{flag}")),
            "`io resume` takes {flag} and the README does not mention it",
        );
    }

    // Each pause's own input, because a table that lost one of these would leave
    // an operator with a run they can see and cannot decide.
    for promised in ["--list", "--answer", "--plan", "--recovery", "--goal"] {
        assert!(
            flags.iter().any(|flag| flag == promised),
            "the README documents {promised} and `io resume` no longer takes it",
        );
    }
    assert!(
        !flags.iter().any(|flag| flag == "--sandbox"),
        "the README says `io resume` takes no --sandbox, and it now does",
    );
}

/// The pause that cannot be resumed, said where an operator is deciding what to
/// do about one.
///
/// **A gate on an absence would pass on a page that says nothing**, which is why
/// this reads the section rather than sweeping the file: a turn the operator
/// interrupted is recorded `cancelled`, mapped to a *completed* run, and every
/// io-harness resume entry point hands back the original outcome having driven
/// nothing. An operator who is not told that reads a `/resume` row for such a
/// session as a row they can choose, and the release's own answer — `/fork` from
/// the turn before — is the thing they never find.
///
/// The marks are taken from [`io_cli::sessions`] rather than listed here, for the
/// reason the command table is generated: a sixth state, or a renamed one, fails
/// this test instead of leaving the README teaching a word that is no longer
/// drawn.
#[test]
fn the_readme_says_which_pause_cannot_be_resumed() {
    use io_cli::sessions::{DIED_MARK, ENDED_MARK, PLAN_MARK, QUESTION_MARK, RECOVERY_MARK};

    let readme = read("README.md");
    let (_, section) = readme
        .split_once("## When a run stops for you")
        .expect("the README should have a section about a run that stopped for a person");
    let section = section.split("\n## ").next().unwrap_or(section);

    for mark in [
        QUESTION_MARK,
        PLAN_MARK,
        RECOVERY_MARK,
        DIED_MARK,
        ENDED_MARK,
    ] {
        assert!(
            section.contains(&format!("`{mark}`")),
            "the README should name the `{mark}` mark and say what it means",
        );
    }

    // The mechanism, then the consequence, then what to do instead. Short
    // distinctive phrases rather than sentences, because the sentence around a
    // claim is what an author rewrites while leaving the claim standing.
    assert!(
        section.contains("cancelled"),
        "the README should say what io-harness records a Ctrl+C as",
    );
    assert!(
        section.contains("cannot be answered"),
        "the README should say an interrupted turn is the one pause that cannot be resumed",
    );
    assert!(
        section.contains("`/fork`"),
        "the README should offer /fork, which is what an ended turn leaves you",
    );
}

// ---------------------------------------------------------------------------
// 0.30.0 F19 — every claim a module makes about io-harness is true of the
// io-harness that is pinned.
// ---------------------------------------------------------------------------

/// The io-harness version `Cargo.lock` actually pins.
///
/// Read from the lock and never from `Cargo.toml`, for the reason the release
/// process records: the manifest states a *requirement* (`"0.71"`) and the lock
/// states the *resolution* (`0.71.0`), and a source comment citing a line number
/// is citing the file that was resolved.
fn pinned_harness() -> String {
    let lock = read("Cargo.lock");
    let at = lock
        .find("name = \"io-harness\"")
        .expect("io-harness is in the lock file");
    let rest = &lock[at..];
    let line = rest
        .lines()
        .find(|line| line.starts_with("version = "))
        .expect("the pinned version follows the name");
    line.trim_start_matches("version = ")
        .trim_matches('"')
        .to_string()
}

/// **No source file cites a line inside an io-harness this crate does not pin.**
///
/// This is the mechanical half of the doc-truth sweep, and it exists because the
/// expensive half is not mechanical at all. Nine citations of the form
/// `io-harness-0.69.0/src/config.rs:1888` were still in the tree at 0.30.0, two
/// pins after that version — each one a line number that had moved, in a file
/// whose contents had changed, presented to the next reader as a fact they could
/// go and check.
///
/// A citation into the pinned version can still be *wrong*; a citation into a
/// version that is not pinned cannot even be checked. This gate catches the second
/// kind, which is the kind that accumulates silently.
///
/// Sabotage: put back any one of the nine. It names the file and the version.
#[test]
fn f19_no_source_cites_an_io_harness_this_crate_does_not_pin() {
    let pinned = pinned_harness();
    let wanted = format!("io-harness-{pinned}/");
    let mut stale: Vec<String> = Vec::new();

    // **Recursive, and every citation on a line rather than the first.** Both were
    // holes in the first version of this gate: `find` returns one match, so a line
    // carrying two citations was checked once and the second could name any version
    // at all; and a flat `read_dir` covers `src/` only for as long as `src/` stays
    // flat, which is not a property anybody is maintaining.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut dirs = vec![repo().join("src")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("a source directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    files.sort();

    for path in &files {
        let text = std::fs::read_to_string(path).expect("a source file");
        for (number, line) in text.lines().enumerate() {
            // Only the `io-harness-<version>/` form, which is a path into a
            // vendored source tree. A bare "io-harness 0.70.0" is history — a
            // sentence about when something changed — and is not a citation.
            for (at, _) in line.match_indices("io-harness-") {
                if !line[at..].starts_with(&wanted) {
                    stale.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(repo()).unwrap_or(path).display(),
                        number + 1,
                        line.trim(),
                    ));
                }
            }
        }
    }

    assert!(
        stale.is_empty(),
        "these cite a line inside an io-harness that is not pinned (the pin is \
         {pinned}), so the line numbers are unverifiable and the surrounding \
         claims are unchecked:\n{}",
        stale.join("\n"),
    );
}

/// The markdown a reader meets, by path relative to the root.
///
/// Walked rather than listed, because a list is a second place to add a file and
/// the one that never gets updated. Dotted directories are skipped, which drops
/// `.ultraship/` — gitignored whole, the maintainer's working notes rather than a
/// reader-facing surface — and also `.github/`'s issue and pull-request
/// templates, which no criterion here covers. `target/` holds nothing shipped.
fn shipped_markdown() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else if path.extension().is_some_and(|ext| ext == "md") {
                let relative = path
                    .strip_prefix(root)
                    .expect("walked from the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                out.push((relative, text));
            }
        }
    }

    let root = repo();
    let mut out = Vec::new();
    walk(&root, &root, &mut out);
    out.sort();
    out
}

/// **F7 — no shipped document contains an unfilled placeholder.**
///
/// `SECURITY.md` forbade the public-issue fallback and then named a literal
/// `<project-contact-email>`, and `CODE_OF_CONDUCT.md` carried the same token —
/// so from 0.1.0 to 0.30.1 this project told people not to open an issue and
/// gave them nowhere else to go. The README routes every vulnerability report at
/// `SECURITY.md`, so the dead end was the only documented path.
///
/// The needle is not "an angle bracket": `docs/CONTRACT.md` legitimately writes
/// `io exec "<goal>"` and `<subcommand>`, and a gate that banned those would be
/// reverted the first time someone documented a command's arguments. It is the
/// **template** shape — an angle-bracket token naming a contact that was meant to
/// be substituted and was not.
///
/// Sabotage: put `<project-contact-email>` back into either file. Only this fails.
#[test]
fn f7_no_shipped_document_leaves_a_contact_placeholder_unfilled() {
    let mut offenders = Vec::new();

    for (path, text) in shipped_markdown() {
        for (number, line) in text.lines().enumerate() {
            // An angle-bracket token that names a contact rather than an argument.
            // `<goal>`, `<path>` and `<version>` are argument spellings and are
            // deliberately not matched.
            let mut rest = line;
            while let Some(open) = rest.find('<') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('>') else { break };
                let token = &after[..close];
                let looks_like_contact = token.contains("contact")
                    || token.contains("email")
                    || token.contains("your-");
                // A URL in angle brackets is a markdown autolink, not a placeholder.
                let is_autolink = token.starts_with("http");
                if looks_like_contact && !is_autolink {
                    offenders.push(format!("{path}:{}: <{token}>", number + 1));
                }
                rest = &after[close + 1..];
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these documents ship an unfilled contact placeholder, so a reader \
         following them reaches nobody:\n{}",
        offenders.join("\n"),
    );
}

/// Every relative link in a markdown file, as (containing file, target).
///
/// Anchors and absolute URLs are not this gate's business: `#section` is checked
/// by nothing here and `https://` is checked by nobody offline. What is left is
/// the set of links that can rot silently when a file moves — which is exactly
/// what this release does to a 2,847-line README.
fn relative_links() -> Vec<(String, String)> {
    let mut links = Vec::new();

    for (path, text) in shipped_markdown() {
        let mut rest = text.as_str();
        while let Some(open) = rest.find("](") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(')') else { break };
            let target = &after[..close];
            rest = &after[close + 1..];

            // A link target may carry a title: `](path "Title")`. Take the path.
            let target = target.split_whitespace().next().unwrap_or(target);
            if target.is_empty()
                || target.starts_with('#')
                || target.starts_with("http://")
                || target.starts_with("https://")
                || target.starts_with("mailto:")
            {
                continue;
            }
            // Drop a fragment on a file link: `guide/keys.md#moving-a-key`.
            let target = target.split('#').next().unwrap_or(target);
            if target.is_empty() {
                continue;
            }
            links.push((path.clone(), target.to_string()));
        }
    }

    links
}

/// **F4 — no relative link is dead.**
///
/// Written before the guide pages exist, so it fails until they do. A link check
/// is a few lines of `std`; adding a markdown crate to a tree held at ten direct
/// dependencies to find a missing file would be the expensive answer to the cheap
/// question.
///
/// Sabotage: point any link at a file that is not there. Only this fails.
#[test]
fn f4_every_relative_link_resolves_to_a_file_that_exists() {
    let root = repo();
    let mut dead = Vec::new();

    for (from, target) in relative_links() {
        let base = std::path::Path::new(&from)
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default();
        let resolved = root.join(&base).join(&target);
        if !resolved.exists() {
            dead.push(format!("{from} → {target}"));
        }
    }

    assert!(
        dead.is_empty(),
        "these links point at files that are not there, so a reader following \
         the documentation reaches a 404:\n{}",
        dead.join("\n"),
    );
}

/// **F4, the other half — no guide page is orphaned.**
///
/// A split that leaves a page linked from nowhere is worse than not splitting:
/// the content is gone from where it was and reachable only by someone who
/// already knows the filename. Every page must be reachable from **both**
/// indexes, because each is a way in — the README for a reader arriving at the
/// repository, `docs/CAPABILITIES.md` for one arriving at the docs directory.
///
/// The non-empty assertion is load-bearing. With no guide pages on disk the two
/// loops below iterate nothing and the test passes while proving nothing, which
/// is the vacuous-gate shape this repository has shipped four times.
///
/// Sabotage: add a guide page linked from neither index; or drop one row from
/// either index. Each fails on its own.
#[test]
fn f4_every_guide_page_is_reachable_from_both_indexes() {
    let guides: Vec<String> = shipped_markdown()
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path.starts_with("docs/guide/"))
        .collect();

    assert!(
        !guides.is_empty(),
        "no guide pages were found under docs/guide/, so the two checks below \
         would pass by iterating nothing",
    );

    let readme = read("README.md");
    let capabilities = read("docs/CAPABILITIES.md");

    let mut orphans = Vec::new();
    for guide in &guides {
        // The README sits at the root and links `docs/guide/x.md`;
        // CAPABILITIES.md sits in `docs/` and links `guide/x.md`.
        let from_readme = guide.as_str();
        let from_capabilities = guide.trim_start_matches("docs/");

        if !readme.contains(from_readme) {
            orphans.push(format!("{guide} is not linked from README.md"));
        }
        if !capabilities.contains(from_capabilities) {
            orphans.push(format!("{guide} is not linked from docs/CAPABILITIES.md"));
        }
    }

    assert!(
        orphans.is_empty(),
        "these guide pages are not reachable from an index, so the split moved \
         content somewhere nobody is pointed at:\n{}",
        orphans.join("\n"),
    );
}

/// **F7, the other half — the private report route is really named.**
///
/// The test above only proves a placeholder is absent, which an empty file also
/// satisfies. This one proves the replacement is present, so deleting the
/// reporting section to make the first test pass fails this one.
#[test]
fn f7_the_security_policy_names_a_private_route_that_is_not_a_public_issue() {
    const ADVISORY: &str = "security/advisories/new";

    let security = read("SECURITY.md");
    assert!(
        security.contains(ADVISORY),
        "SECURITY.md forbids opening a public issue, so it has to name the \
         private route that replaces it",
    );
    assert!(
        security.contains("Do not open a public issue"),
        "the policy's central instruction is missing",
    );

    let conduct = read("CODE_OF_CONDUCT.md");
    assert!(
        conduct.contains(ADVISORY),
        "CODE_OF_CONDUCT.md asks for reports to reach the maintainers privately, \
         so it has to say through what",
    );
}
