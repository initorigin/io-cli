//! The slash commands and the keybinding table.

use io_cli::commands::{self, Action, COMMANDS, KEYS};
use io_cli::keys::{Keys, Newline};
use io_cli::theme::DARK;

/// The bindings a session with no `[app.io-cli.keys]` runs under. What each of
/// these tests asserts about the defaults, `tests/keys.rs` asserts about a file
/// that has moved them.
fn defaults() -> Keys {
    Keys::default()
}

fn text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_commands_are_the_commands() {
    let names: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "/help",
            "/exit",
            "/setup",
            "/theme",
            "/model",
            "/resume",
            "/fork",
            "/expand",
            // 0.14.0 — a command and not a key. The keys are nearly all spoken
            // for, and a key is cheap to add later and expensive to take back
            // once it is in anybody's fingers.
            "/status",
            // 0.17.0 — `/status` says how full the window is and this says what
            // is in it. Beside it in the table for that reason.
            "/context",
            // 0.17.0 — a word rather than a default, because a delivered steer
            // emits no event this interface can draw.
            "/steer",
            // 0.17.0 — the other word said *to* a turn, and the only other
            // command whose effect is decided by whether one is running.
            "/compact",
            "/copy",
            "/copy diff",
            // 0.25.0 — the third thing an operator does with the work a turn has
            // just finished, and it sits beside the two that copy it: one takes
            // the answer, one takes the patch, this one makes the patch permanent.
            // It goes under **this turn** and not **inspect** for that reason, and
            // it takes that group to ten, which is the bound — so the next command
            // that would fill a group re-files one that is in the wrong group
            // rather than widening it.
            "/commit",
            "/config",
            // 0.18.0 — the other two surfaces that write a file the operator
            // keeps, and they ask the same scope question `/config` does: three
            // files, and which one is half of every decision made here.
            "/remember",
            "/memory",
            // 0.19.0 — the other half of `/memory`'s question: that one is what io
            // was told, this one is what it was taught.
            "/skills",
            "/mcp",
            "/provider",
            // 0.20.0 — the third surface that lists what the file declared and
            // goes on to change it, and the widest of the three: one directory
            // can hand over skills, templates, agents, servers, hooks and policy
            // at once. That breadth is the argument for it being visible at all.
            "/plugin",
            // 0.21.0 — the operator's own work, carried across from whatever
            // agent they were using before this one. It is the only command here
            // whose subject is a tool that is not io.
            "/import",
            "/profile",
            "/effort",
            "/contain",
            // 0.27.0 — the undo that is the size of the mistake, taking the
            // `Turn` slot `/contain` left when it was re-filed into `Session`.
            // `Turn` was ten of ten and the bound was not widened, which is the
            // fourth time this product has made that correction.
            "/undo",
            // 0.12.0 — the planning phase stopped being something
            // `[app.io-cli.containment]` switched on by accident, so it needs a
            // switch of its own.
            "/plan",
            "/fleet",
            "/image",
            "/clear",
            // 0.22.0 — the two halves of one question, and two commands because
            // they are two questions. `/cost` says what the work cost and
            // `/stats` says whether it worked; every agent that has both keeps
            // them apart. `/usage` is an alias for the first and is absent from
            // this list, which is the rule every alias here follows.
            "/cost",
            "/stats",
            // 0.27.0 — the third page about work already done, and the first that
            // can also change it. Under **inspect** because that is what its bare
            // form does, with every destructive verb behind a confirmation whose
            // row 0 does nothing; it and `/export` take that group to ten, which
            // is the bound.
            "/store",
            // 0.27.0 — the other end of the same question: `/store` is what is
            // being kept, this is how the work gets out. It takes `Inspect` to
            // ten, which is the bound.
            "/export",
            // 0.24.0 — beside `/stats` because `/stats` is the only other row
            // that says the word: that page counts how the gates went, and until
            // this release nothing in the product could say what a gate was. It
            // is under **configure** and not **inspect**, because opening it is
            // one keystroke from writing `[app.io-cli.gates]` and changing what
            // every later turn has to prove.
            "/gates",
        ],
        "the fuzzy palette is still 0.7.0; this list is written out so that adding \
         a command is a decision somebody makes rather than a line somebody adds",
    );
    for (name, what) in COMMANDS {
        assert!(name.starts_with('/'), "{name}");
        assert!(!what.is_empty(), "{name} needs a description");
    }
}

/// **A listed command that `parse` has no arm for is advertised and inert**, and
/// until 0.19.0 nothing in this file could see that: every gate here derives from
/// `COMMANDS` and `GROUPS`, which a row satisfies merely by existing. `/skills`
/// was added to the table and to the palette and resolved, silently, to the help
/// listing — the same answer `/models` gets, which is the arm that exists to tell
/// somebody they typed a command that is not real.
///
/// So the property is stated once, over the whole inventory, rather than one
/// `assert_eq!` per command in `each_command_resolves` below — a list like that
/// stops covering the next row the moment somebody forgets to extend it, which is
/// exactly the failure this test exists for. Exactly ONE listed command may
/// resolve to the help listing, and it is `/help`.
///
/// **0.25.0 rewrote how "the help listing" is recognised, because the old
/// spelling had stopped recognising it at all.** The check was one comparison —
/// `parse(name) == parse("help")` — and that was true of the `unknown` arm only
/// while the arm literally returned `help(…)`. It has not for some time: it now
/// builds a *warning notice* followed by `commands(theme)` alone, with no key
/// table, so an armless command's `Action::Print` can never equal `/help`'s. The
/// gate was passing on the one command it was allowed to pass on and blind to
/// every command it existed to catch. `/commit`'s sabotage — the row added and
/// the `parse` arm withheld — went green under it, which is how it was found.
///
/// So the property is asserted against **both** shapes an inert command can take:
/// the help listing itself, and the sentence the `unknown` arm writes. The second
/// is matched on `there is no /<word>`, which is the arm's own text and the one
/// thing a fallen-through command is guaranteed to carry.
#[test]
fn no_listed_command_falls_through_to_the_help_listing() {
    let listing = commands::parse("help", &defaults(), &DARK);
    let inert: Vec<&str> = commands::COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| {
            let typed = name.strip_prefix('/').unwrap_or(name);
            let got = commands::parse(typed, &defaults(), &DARK);
            if got == listing {
                return true;
            }
            // The `unknown` arm names the word it did not recognise, and `parse`
            // decides on the FIRST word — which is what `/copy diff` is spelled
            // out of, and what the sentence would carry if it ever fell through.
            let word = typed.split_whitespace().next().unwrap_or(typed);
            match got {
                Action::Print(lines) => text(&lines).contains(&format!("there is no /{word}")),
                _ => false,
            }
        })
        .collect();
    assert_eq!(
        inert,
        vec!["/help"],
        "these commands are listed but resolve to what an UNKNOWN command resolves \
         to — the help listing, or the notice that says the command does not exist. \
         They are advertised and inert. Give each one an arm in `commands::parse` \
         and an arm in the driver, or take the row out of `COMMANDS`."
    );
}

#[test]
fn each_command_resolves() {
    assert!(matches!(
        commands::parse("help", &defaults(), &DARK),
        Action::Print(_)
    ));
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);
    assert_eq!(commands::parse("setup", &defaults(), &DARK), Action::Setup);
    assert_eq!(commands::parse("theme", &defaults(), &DARK), Action::Theme);
    assert_eq!(commands::parse("model", &defaults(), &DARK), Action::Model);
    assert_eq!(
        commands::parse("expand", &defaults(), &DARK),
        Action::Expand
    );
    // `/copy` with no argument is the answer; `diff` and `patch` are the same
    // thing, because a reader who has just been shown a diff types the word they
    // were shown.
    assert_eq!(
        commands::parse("copy", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Answer),
    );
    assert_eq!(
        commands::parse("copy diff", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Diff),
    );
    assert_eq!(
        commands::parse("copy patch", &defaults(), &DARK),
        Action::Copy(io_cli::commands::Copied::Diff),
    );

    // 0.11.0 — `/clear` and its alias, and `/exit`, which has resolved to the
    // same action as `/quit` since 0.1.0 and was listed nowhere until now.
    assert_eq!(commands::parse("clear", &defaults(), &DARK), Action::Clear);
    assert_eq!(commands::parse("new", &defaults(), &DARK), Action::Clear);
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);

    // Arguments are tolerated; the first word decides.
    assert_eq!(
        commands::parse("model gpt-5", &defaults(), &DARK),
        Action::Model
    );
    // An empty command is help, which is what a bare `/` and Enter means.
    assert!(matches!(
        commands::parse("", &defaults(), &DARK),
        Action::Print(_)
    ));
}

/// **0.14.0 F10 — `/status` is in the command set and dispatches to its own
/// action.**
///
/// Three claims and they are separable. It is *listed*, so a reader who opens
/// `/help` or the palette finds it without being told it exists. It is a
/// *command and not a key*, which is the decision the contract records: no
/// keybinding in `KEYS` claims it, so nothing has spent one of the few that are
/// left. And it *resolves*, to an action of its own rather than to the print
/// every unknown command falls through to — which is what the driver matches on
/// to commit the page.
///
/// Sabotage: render it as a table — under which F11 fails at eighty columns; the
/// arm this test guards is the one before that, where a command that parses to
/// `Action::Print` would put the *command list* in the scrollback and look, from
/// a distance, like a surface that worked.
#[test]
fn f10_status_is_listed_as_a_command_and_resolves_to_its_own_action() {
    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/status"),
        "a surface nobody is told about is a surface nobody uses",
    );
    assert_eq!(
        commands::parse("status", &defaults(), &DARK),
        Action::Status
    );
    // Arguments are tolerated and the first word decides, as everywhere else.
    assert_eq!(
        commands::parse("status now", &defaults(), &DARK),
        Action::Status,
    );
    // A command and not a key: nothing in the key table claims it, and the
    // palette row is the whole of how it is reached.
    assert!(
        !KEYS.iter().any(|(_, what)| what.contains("status")),
        "0.14.0 decided this is a command; a key spent on it is a key taken back \
         from somebody's fingers later",
    );
    // It commits into the scrollback like `/expand`, so it must not fall through
    // to the print every unknown command lands on.
    assert_ne!(
        commands::parse("status", &defaults(), &DARK),
        commands::parse("statuss", &defaults(), &DARK),
    );
}

#[test]
fn f12_resume_says_it_answers_a_parked_run_rather_than_merely_reopening_a_session() {
    // **The sabotage this defends against is changing what `/resume` does and
    // leaving its one line saying what it used to do.** That line is the only
    // thing most operators ever read about a command: it is the palette row, it
    // is what `/help` prints, and `tests/docs.rs` copies it into the README
    // table. A description that still promised a reopen would leave the whole of
    // 0.23.0 undiscoverable from inside the product.
    let (_, said) = COMMANDS
        .iter()
        .find(|(name, _)| *name == "/resume")
        .expect("/resume is listed");
    assert!(
        said.contains("answer"),
        "0.23.0 made /resume answer what a run stopped on; the description still \
         describes 0.22.0's: {said}",
    );
    // And it is still a reopen as well — the description must not have swung the
    // other way and dropped the half that is true for the four sessions in five
    // that are waiting on nothing at all.
    assert!(
        said.contains("session"),
        "most sessions have nothing parked, and reopening one is still what the \
         command does for them: {said}",
    );
    // Unchanged: the word still resolves, `/continue` is still the same action,
    // and neither has become the help listing.
    assert_eq!(
        commands::parse("resume", &defaults(), &DARK),
        Action::Resume,
    );
    assert_eq!(
        commands::parse("continue", &defaults(), &DARK),
        Action::Resume,
    );
    // No command was added for any of this — `/resume` was extended — so the
    // inventory is the size the other gates in this file assert. The number is
    // 31 plus 0.25.0's one addition, and the two halves are named separately on
    // purpose: a total that merely went up by one would be satisfied by this
    // release growing a command and losing another.
    assert_eq!(
        COMMANDS.len(),
        36,
        "0.28.0 adds NO command — every verb it adds is a verb inside a command \
         that already exists, and `io mcp|plugin|config` are binary subcommands in \
         `src/cli.rs` rather than slash commands. A thirty-seventh here means one \
         arrived unrecorded, on top of the thirty-six 0.27.0 shipped",
    );
}

/// **A question is never a write, and this shipped as one until it was caught.**
///
/// `/config get run.max_steps` fell through to the `/config <key> <value>`
/// shorthand and was read as the key `get` with the value `run.max_steps`, so a
/// question wrote a key called `get` into the operator's file; `/config list` was
/// read as a question about a key named `list`. Found by a reviewer reading the
/// guard against the comment above it, which claimed both verbs reached
/// `manage::parse` when the guard matched only `set` and `unset`.
///
/// Sabotage: drop `get` and `list` from their guards — under which this fails on
/// the `Action` a question produces, which is the shape of the write it would
/// have become.
#[test]
fn o2_a_config_question_is_answered_and_never_written() {
    use io_cli::commands::Action;

    assert_eq!(
        commands::parse("config get run.max_steps", &defaults(), &DARK),
        Action::Config(Some(("run.max_steps".to_string(), String::new()))),
        "`/config get <key>` must be the question this surface has always answered"
    );
    assert_eq!(
        commands::parse("config list", &defaults(), &DARK),
        Action::Config(None),
        "`/config list` must open the panel, not ask about a key named `list`"
    );
    // A bare `get` is the panel rather than a question about an empty key.
    assert_eq!(
        commands::parse("config get", &defaults(), &DARK),
        Action::Config(None)
    );
    // And the two verbs that WRITE do go to the one parse.
    assert!(
        matches!(
            commands::parse("config set run.max_steps 30", &defaults(), &DARK),
            Action::Manage(_)
        ),
        "`/config set` must reach the same parse `io config set` reaches"
    );
    assert!(matches!(
        commands::parse("config unset app.io-cli.plain", &defaults(), &DARK),
        Action::Manage(_)
    ));
    // The shorthand this surface has always had is untouched.
    assert_eq!(
        commands::parse("config run.max_steps 30", &defaults(), &DARK),
        Action::Config(Some(("run.max_steps".to_string(), "30".to_string()))),
    );
}

/// The write verbs of `/mcp` and `/plugin` reach the one parse; the bare word
/// opens the panel.
#[test]
fn o2_the_management_write_verbs_reach_the_shared_parse() {
    use io_cli::commands::Action;

    for line in [
        "mcp add semlith -- semlith --store /tmp/.semlith mcp",
        "mcp edit semlith --timeout-secs 30",
        "mcp remove semlith",
        "plugin add ./bundles/rust-review",
        "plugin remove ./bundles/rust-review",
    ] {
        assert!(
            matches!(commands::parse(line, &defaults(), &DARK), Action::Manage(_)),
            "`/{line}` must reach `manage::parse`, or the slash form and `io …` can disagree"
        );
    }
    // A bare word is the panel, and `list` is the panel too: in a session the
    // answer to "list" is the surface that draws it.
    assert_eq!(commands::parse("mcp", &defaults(), &DARK), Action::Mcp);
    assert_eq!(commands::parse("mcp list", &defaults(), &DARK), Action::Mcp);
    assert_eq!(
        commands::parse("plugin", &defaults(), &DARK),
        Action::Plugin
    );
    assert_eq!(
        commands::parse("plugin list", &defaults(), &DARK),
        Action::Plugin
    );
}

/// **The plural spelling the router takes is the spelling the parse takes.**
///
/// `commands::parse` routes `/plugins install x` and `/servers add …` to
/// `manage::parse` on purpose — the thing being listed is plural, so the plural is
/// what a hand reaches for, and the comment above that arm says refusing it
/// teaches nothing. Until 0.29.0 the parse then refused both words as surfaces io
/// does not manage, so five `/plugins` verbs and four `/servers` verbs were
/// accepted by one module and refused by the next with a sentence about a surface
/// the operator had not typed.
///
/// **Neither module can see this alone**, which is why it is one test rather than
/// two: `commands::parse` returns the correct `Action::Manage` and every test of it
/// passes, while `manage::parse` is asked about a word it never claimed to take and
/// answers correctly for the question it was asked. The defect is only in the
/// join, so the join is what is driven — router first, then `tokens` and `parse`
/// over exactly what the router handed back.
///
/// Sabotage: drop the `"plugins" => "plugin"` fold from the top of `manage::parse`.
/// Under it every plural line below comes back `Err` naming the three surfaces,
/// over a line the router had already accepted.
#[test]
fn o2_the_plural_spelling_the_router_takes_is_the_spelling_the_parse_takes() {
    use io_cli::commands::Action;
    use io_cli::manage;

    for line in [
        "plugins add ./bundles/rust-review",
        "plugins install owner/repo",
        "plugins remove ./bundles/rust-review",
        "plugins search review",
        "plugins marketplace list",
        "servers add semlith -- semlith --store /tmp/.semlith mcp",
        "servers edit semlith --timeout-secs 30",
        "servers get semlith",
        "servers remove semlith",
    ] {
        let Action::Manage(routed) = commands::parse(line, &defaults(), &DARK) else {
            panic!("`/{line}` no longer reaches `manage::parse` at all");
        };
        // The whole line travels, which is what makes the two doors one door.
        assert_eq!(routed, line);
        if let Err(refusal) = manage::parse(&manage::tokens(&routed)) {
            panic!(
                "`/{line}` was routed to the one parse and refused there: {refusal}. \
                 A spelling one module admits and the next refuses is worse than a \
                 spelling neither takes."
            );
        }
    }

    // The fold is an equivalence rather than a rename: the singular reads exactly
    // the same, so nothing was moved onto a second meaning.
    assert_eq!(
        manage::parse(&manage::tokens("plugins list")),
        manage::parse(&manage::tokens("plugin list")),
    );
    assert_eq!(
        manage::parse(&manage::tokens("servers list")),
        manage::parse(&manage::tokens("mcp list")),
    );
    // And a word that is neither spelling is still refused by name, so the fold
    // did not become "anything close enough".
    let refusal = manage::parse(&manage::tokens("plugs list"))
        .expect_err("`plugs` is not a surface io manages");
    assert!(
        refusal.contains("`plugs`"),
        "the refusal no longer echoes the word that was typed: {refusal}",
    );
}

/// **The palette did not grow, and no group was re-filed.**
///
/// The other half of the assertion above, and the reason it is worth its own
/// test: `f13_no_group_is_longer_than_ten` caps a group at ten, and occupancy is
/// Session 7, Turn 10, Inspect 10, Configure 9 — **one free slot in the whole
/// product.** A release that added three commands would have had to re-file the
/// groups, which is a change to a surface nobody asked to change. Growing the
/// palette has to be a deliberate act rather than a side effect of adding a verb.
///
/// Sabotage: add a command to `COMMANDS` and put it in a group. Under it this
/// fails on the occupancy it was given, naming the group that moved.
#[test]
fn o2_the_palette_did_not_grow_and_no_group_was_refiled() {
    use io_cli::commands::{Group, GROUPS};

    let occupancy: Vec<(Group, usize)> = GROUPS
        .iter()
        .map(|(group, names)| (*group, names.len()))
        .collect();
    let total: usize = occupancy.iter().map(|(_, count)| count).sum();
    assert_eq!(
        total,
        COMMANDS.len(),
        "every command is in exactly one group, so the group sizes must sum to the inventory"
    );
    // The occupancy this release inherited and must leave alone. A written-out
    // literal rather than a bound, so a group that grew while another shrank is
    // named rather than cancelling out.
    let mut sizes: Vec<usize> = occupancy.iter().map(|(_, count)| *count).collect();
    sizes.sort_unstable();
    assert_eq!(
        sizes,
        vec![7, 9, 10, 10],
        "0.28.0 re-files no group; the occupancy is Session 7, Configure 9, Turn 10, Inspect 10, \
         and there is exactly one free slot in the product"
    );
}

/// **F1 — `/effort` is a posture, and a bare `/effort` is a question.**
///
/// The three words the command understands, plus the two ways of asking rather
/// than setting. `Reasoning::Report` is what a question resolves to and it is what
/// makes the sentence below safe to print: the driver only assigns when
/// [`io_cli::app::reasoning_of`] answers `Some`.
///
/// Sabotage: resolve an unrecognised word to the nearest level — under which this
/// fails on `/effort hgih`, and it fails by spending a turn's reasoning budget on
/// a typo.
#[test]
fn f1_effort_parses_three_levels_the_absence_and_the_question() {
    use io_cli::commands::Reasoning;
    // `parse` takes the word without its slash — the driver strips it before the
    // call, which is why every other test in this file types `continue` rather
    // than `/continue`. Written as a helper so the cases below read as the
    // operator types them.
    let parse =
        |text: &str| commands::parse(text.strip_prefix('/').unwrap_or(text), &defaults(), &DARK);

    assert_eq!(
        parse("effort low"),
        Action::Effort(Reasoning::Buy(io_harness::Effort::Low)),
    );
    assert_eq!(
        parse("effort medium"),
        Action::Effort(Reasoning::Buy(io_harness::Effort::Medium)),
    );
    assert_eq!(
        parse("effort high"),
        Action::Effort(Reasoning::Buy(io_harness::Effort::High)),
    );
    assert_eq!(parse("effort off"), Action::Effort(Reasoning::Off));
    assert_eq!(parse("effort"), Action::Effort(Reasoning::Report));
    assert_eq!(
        parse("effort hgih"),
        Action::Effort(Reasoning::Unknown("hgih".into())),
        "an unrecognised word is refused by name rather than guessed at — and \
         never quietly reported, which read as an answer and left an operator \
         paying for the level they were trying to leave",
    );
    // **Both spellings of `off`, because only one of them used to work.**
    // `Effort::FromStr` lowercases for itself, so `/effort HIGH` was fine while
    // `/effort OFF` fell past the literal match, failed to parse, and reported —
    // leaving the level exactly where it was. Two spellings of one word behaving
    // differently is the asymmetry nobody reports and everybody hits once.
    assert_eq!(parse("effort OFF"), Action::Effort(Reasoning::Off));
    assert_eq!(parse("effort None"), Action::Effort(Reasoning::Off));
    assert_eq!(
        parse("effort HIGH"),
        Action::Effort(Reasoning::Buy(io_harness::Effort::High)),
    );
}

/// **F1 — a word that is not a level says so, and says what is still in force.**
///
/// Found by both adversarial reviewers. The old sentence for a rejected word was
/// the *report* sentence, so `/effort lwo` — an operator on `high` trying to spend
/// less — answered "every turn asks for high reasoning". That reads as
/// confirmation, the typo is invisible, and the expensive level goes on being
/// bought turn after turn.
#[test]
fn f1_a_word_that_is_not_a_level_is_refused_and_names_itself() {
    use io_cli::app::reasoning_said;
    use io_cli::commands::Reasoning;

    let said = reasoning_said(
        &Reasoning::Unknown("lwo".into()),
        Some(io_harness::Effort::High),
    );
    assert!(said.contains("lwo"), "the word is quoted back: {said}");
    assert!(
        said.contains("not a reasoning level"),
        "the operator is told it was refused rather than obeyed: {said}",
    );
    assert!(
        said.contains("still asks for high"),
        "and what is still in force, since nothing changed: {said}",
    );
}

/// **F1 — a question changes nothing, and `off` is a change.**
///
/// The outer `Option` [`io_cli::app::reasoning_of`] returns is the difference
/// between the two, and it is the difference the driver's assignment turns on. A
/// `Report` that answered `Some(None)` would make every bare `/effort` silently
/// clear the level the operator set — a question with a side effect.
#[test]
fn f1_asking_what_the_effort_is_does_not_change_it() {
    use io_cli::app::reasoning_of;
    use io_cli::commands::Reasoning;

    assert_eq!(reasoning_of(&Reasoning::Report), None);
    assert_eq!(reasoning_of(&Reasoning::Off), Some(None));
    assert_eq!(
        reasoning_of(&Reasoning::Buy(io_harness::Effort::High)),
        Some(Some(io_harness::Effort::High)),
    );
}

/// **F1 — the sentence says the state the session is now in.**
///
/// Setting and reporting share one sentence because what an operator wants read
/// back is where they now are, and the absent case names the fact rather than a
/// level: "no reasoning field" is what goes on the wire, and calling it "off"
/// would suggest a setting between `low` and nothing.
#[test]
fn f1_the_effort_line_names_the_level_now_in_force() {
    use io_cli::app::reasoning_said;
    use io_cli::commands::Reasoning;

    let set = reasoning_said(
        &Reasoning::Buy(io_harness::Effort::High),
        Some(io_harness::Effort::High),
    );
    assert!(set.contains("high"), "{set}");
    assert!(
        set.contains("from here"),
        "a level set holds for later turns: {set}"
    );

    let asked = reasoning_said(&Reasoning::Report, Some(io_harness::Effort::Low));
    assert!(asked.contains("low"), "{asked}");
    assert!(
        !asked.contains("from here"),
        "a question reports what is already true rather than announcing a change: {asked}",
    );

    let none = reasoning_said(&Reasoning::Off, None);
    assert!(
        none.contains("no reasoning field"),
        "the absent case is the absence of the field, not a fourth level: {none}",
    );
}

/// The group bound was met and paid for by a re-file, not by widening it.
///
/// `src/commands.rs` pre-committed this answer when `Configure` reached nine and
/// again when `/commit` took `Turn` to ten: re-file what is in the wrong group, do
/// not widen the bound. This release is the one that had to keep it, so the keeping
/// is asserted rather than described — `f13_no_group_is_longer_than_ten` would pass
/// just as well if a later release had moved the bound to eleven.
#[test]
fn f2_effort_is_a_turn_command_and_profile_moved_to_the_session() {
    assert_eq!(
        commands::group_of("/effort"),
        Some(commands::Group::Turn),
        "`/effort` decides what the next turn buys, which is what `Turn` means",
    );
    assert_eq!(
        commands::group_of("/profile"),
        Some(commands::Group::Session),
        "`/profile` changes the overlay every later turn is built from, which is \
         not the work the turn just finished — it was misfiled, and moving it is \
         what made room rather than widening the bound",
    );
    // The bound itself, restated here so this test fails as one story: a release
    // that added `/effort` without the re-file would leave `Turn` at eleven.
    for (group, names) in commands::GROUPS {
        assert!(
            names.len() <= 10,
            "the {group:?} group holds {}; the bound is ten and this release \
             does not move it",
            names.len(),
        );
    }
}

#[test]
fn an_unknown_command_says_what_does_exist() {
    let Action::Print(lines) = commands::parse("models", &defaults(), &DARK) else {
        panic!("an unknown command should print rather than do nothing");
    };
    let printed = text(&lines);
    assert!(printed.contains("there is no /models"), "{printed:?}");
    assert!(
        printed.contains("warning"),
        "the notice should carry a word, not only a colour: {printed:?}",
    );
    for (name, _) in COMMANDS {
        assert!(printed.contains(name), "{name} is missing from {printed:?}");
    }
}

#[test]
fn help_prints_every_key_and_every_command() {
    // `KEYS` is the table for a terminal that can report `Shift+Enter`, so the
    // naming this renders with is that one. What the *other* terminal's table
    // says is F9's own question and `tests/keyboard.rs` asks it.
    let printed = text(&commands::help(&defaults(), &DARK, Newline::of(true)));
    for (key, what) in KEYS {
        assert!(printed.contains(key), "{key} is missing from /help");
        assert!(printed.contains(what), "{key}'s description is missing");
    }
    for (name, what) in COMMANDS {
        assert!(printed.contains(name), "{name} is missing from /help");
        assert!(printed.contains(what), "{name}'s description is missing");
    }
}

#[test]
fn the_key_table_covers_every_key_this_release_binds() {
    // The table is the documentation, so a key that is bound and undocumented is
    // folklore. These are the bindings `App::key` and `Composer::key` implement.
    let documented: Vec<&str> = KEYS.iter().map(|(key, _)| *key).collect();
    for key in [
        "Enter",
        "Shift+Enter",
        "Up / Down",
        "Ctrl+C",
        "Ctrl+D",
        "Ctrl+L",
        "Esc",
        // The one key in this product that changes the operator's files on
        // io-cli's own initiative rather than the agent's, which is why it is two
        // presses and why it is documented as two.
        "Esc Esc",
        "Shift+Tab",
        "Ctrl+T",
        // 0.8.0. It has a key as well as `/fleet` because the moment it is worth
        // opening is mid-turn, and a slash command cannot be typed then.
        "Ctrl+F",
        "y / a / n",
        // 0.32.0. Not a chord and not rebindable: it is `Picker::key`'s, so it
        // applies to every list in the product rather than to one surface, and it
        // is the completion key an operator arrives already expecting. Until this
        // release it fell into the catch-all and did nothing at all, which is
        // worse than a key that is not bound.
        "Tab",
        // The three prefixes 0.7.0 adds. None is a chord, and none is
        // rebindable: they are characters the composer would otherwise have
        // taken, so what they cost is a literal `/`, `@` or `!` in the one
        // position each is claimed in.
        "/",
        "@",
        "!",
    ] {
        assert!(documented.contains(&key), "{key} is bound but undocumented");
    }
    assert_eq!(
        documented.len(),
        16,
        "a key was added to the table without being added to this list, or the \
         other way round",
    );
}

/// `/attach` takes the REST of the line, not its second word.
///
/// A path may contain spaces and the `@` completion that produced it does not
/// quote, so taking one token would attach the wrong file — or nothing — for
/// exactly the paths a reader is least able to retype. The `@` itself is left on:
/// stripping it belongs to `attach::prepare`, beside the read it guards, rather
/// than in two places.
#[test]
fn attach_is_no_longer_a_command_at_all() {
    // **`/attach` went away in 0.13.1.** A picture is attached by dropping it on
    // the prompt or pasting it, which is what an operator already does in every
    // other window they talk to a model in; a command was something they had to
    // be told about first. The word is not kept as a hidden alias either: it is
    // answered by whatever answers any other word that is not a command.
    for line in ["attach @my pictures/shot.png", "attach"] {
        let Action::Print(said) = commands::parse(line, &defaults(), &DARK) else {
            panic!("{line:?} is still a command");
        };
        let text = text(&said);
        assert!(
            text.contains("there is no /attach"),
            "the answer names the word that was typed: {text}",
        );
        assert!(
            text.contains("/image"),
            "and lists what there is instead: {text}",
        );
        assert!(
            !text.contains("/attach "),
            "the command list does not still carry it: {text}",
        );
    }

    // And `/image` is the command that survived, with a number after it.
    assert_eq!(
        commands::parse("image 1", &defaults(), &DARK),
        Action::Image(Some(1)),
    );
}

/// 0.11.0 F8 — `/clear` starts over from an idle prompt.
///
/// The half of it that is not the driver's: every run-scoped field back to what
/// it was before anything ran, and nothing left in `Events` that belonged to the
/// conversation being ended — a tail nobody committed, a call nothing will close,
/// or the thought `/expand` would otherwise show from a conversation no longer on
/// screen.
#[test]
fn f8_clear_at_an_idle_prompt_resets_every_run_scoped_field() {
    use io_cli::app::App;

    let mut app = App::new(DARK, "anthropic/claude-sonnet-4");
    app.status.provider = Some("openrouter".into());
    app.status.tokens = Some(4_703);
    app.status.steps = Some(3);
    app.event(
        &io_harness::RunEvent::new(
            1,
            1,
            io_harness::EventKind::Reasoning {
                text: "x ".repeat(400),
                tokens: 90,
            },
        ),
        std::time::Duration::ZERO,
    );
    assert!(app.events.thought().is_some(), "the thought was held");

    assert!(app.clear_conversation(), "an idle prompt is not refused");
    assert_eq!(app.status.provider, None);
    assert_eq!(app.status.tokens, None);
    assert_eq!(app.status.steps, None);
    assert_eq!(app.events.thought(), None);
    assert!(app.events.live().is_empty());
}

/// 0.11.0 F8 — and refuses while a turn is in flight, changing nothing.
///
/// The criterion's own sabotage arm is clearing while a turn is running, which
/// orphans a live run behind a cleared screen. The driver refuses a slash command
/// during a turn too — this is the lock a test can stand on, and the two agree.
#[test]
fn f8_clear_refuses_while_a_turn_is_in_flight_and_changes_nothing() {
    use io_cli::app::App;

    let mut app = App::new(DARK, "m");
    app.status.tokens = Some(4_703);
    app.status.steps = Some(3);
    app.started();

    assert!(
        !app.clear_conversation(),
        "a live run was cleared out from under"
    );
    assert_eq!(
        app.status.tokens,
        Some(4_703),
        "the run's own facts survived"
    );
    assert_eq!(app.status.steps, Some(3));

    // **The footer, since 0.13.1.** A refusal answers the key that was just
    // pressed; it is not part of the conversation, and it used to be committed
    // into the terminal's permanent scrollback where it stayed forever.
    let said = app
        .status
        .notice
        .as_ref()
        .map(|(_, text)| text.clone())
        .unwrap_or_default();
    assert!(
        said.contains("not while a turn is running"),
        "a refusal that says nothing is a key that appears to do nothing: {said:?}",
    );
    assert!(
        app.take_pending().is_empty(),
        "a refusal does not belong in the transcript",
    );
}

/// 0.11.0 F9 — `/exit` is listed, and the palette row it is listed as leaves.
///
/// The sabotage arm is listing it without wiring the row, which is the one way a
/// listed command must not fail: advertised and inert. `palette_pick` is what the
/// driver resolves a chosen row through, so asking it is asking the driver.
#[test]
fn f9_exit_is_listed_and_its_palette_row_leaves() {
    use io_cli::commands::{palette, palette_pick, Chosen, COMMANDS};
    use io_harness::Templates;

    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/exit"),
        "`/exit` is in the inventory",
    );

    // Found by name rather than by the inventory's own index: since 0.16.0 the
    // palette is grouped, so its rows are the commands in GROUP order with a
    // heading before each group, and a position in `COMMANDS` addresses neither.
    let rows = palette(&Templates::none(), &[]);
    let index = rows
        .iter()
        .position(|row| row.label == "exit")
        .expect("`/exit` has a row");
    assert_eq!(
        palette_pick(&Templates::none(), &[], index),
        Some(Chosen::Command("/exit")),
        "the row is advertised and inert",
    );
    assert_eq!(commands::parse("exit", &defaults(), &DARK), Action::Quit);
}

/// F14 — `/image` takes the number off the marker.
#[test]
fn f14_image_parses_the_number_a_marker_carries() {
    use io_cli::commands::{parse, Action};

    let keys = io_cli::keys::Keys::default();
    assert_eq!(parse("image 2", &keys, &DARK), Action::Image(Some(2)));
    // `#2` is what is on the prompt, so it is what an operator retypes.
    assert_eq!(parse("image #2", &keys, &DARK), Action::Image(Some(2)));
    assert_eq!(parse("images 1", &keys, &DARK), Action::Image(Some(1)));
    // Nothing, or nonsense: the same answer, which names what there is.
    assert_eq!(parse("image", &keys, &DARK), Action::Image(None));
    assert_eq!(parse("image blue", &keys, &DARK), Action::Image(None));
}

// --- F13: the groups ----------------------------------------------------------

#[test]
fn f13_every_command_is_in_exactly_one_group() {
    // Asserted against COMMANDS rather than against a hand-written list, so a
    // command added later without a group fails here by name rather than
    // quietly appearing in no menu.
    use io_cli::commands::{group_of, Group, GROUPS};

    for (name, _) in COMMANDS {
        let homes: Vec<Group> = GROUPS
            .iter()
            .filter(|(_, names)| names.contains(name))
            .map(|(group, _)| *group)
            .collect();
        assert_eq!(
            homes.len(),
            1,
            "`{name}` is in {} groups; every command belongs to exactly one",
            homes.len(),
        );
        assert!(group_of(name).is_some());
    }

    // And nothing is grouped that is not a command — a stale name here would be
    // a heading over a row that does not exist.
    for (group, names) in GROUPS {
        for name in *names {
            assert!(
                COMMANDS.iter().any(|(command, _)| command == name),
                "`{name}` is grouped under {} and is not a command",
                group.title(),
            );
        }
    }
}

#[test]
fn f13_no_group_is_longer_than_ten() {
    use io_cli::commands::GROUPS;

    for (group, names) in GROUPS {
        assert!(
            names.len() <= 10,
            "the {} group holds {} commands; a group longer than ten is the \
             flat list this release replaced",
            group.title(),
            names.len(),
        );
        assert!(
            !names.is_empty(),
            "the {} group is empty, so it is a heading over nothing",
            group.title(),
        );
    }
}

#[test]
fn f13_grouped_covers_every_command_once() {
    use io_cli::commands::grouped;

    let mut seen: Vec<&str> = grouped()
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|(name, _)| name))
        .collect();
    let mut all: Vec<&str> = COMMANDS.iter().map(|(name, _)| *name).collect();
    seen.sort();
    all.sort();
    assert_eq!(seen, all, "the grouped view lost or duplicated a command");
}

// --- F15 and F17: help, and the alias -----------------------------------------

#[test]
fn f15_help_is_grouped_and_carries_every_command() {
    use io_cli::commands::{grouped, help, Group};

    let lines = help(&defaults(), &DARK, io_cli::keys::Newline::of(true));
    let text = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    for group in Group::all() {
        assert!(
            text.lines().any(|line| line.trim() == group.title()),
            "`/help` has no heading for the {} group: {text}",
            group.title(),
        );
    }
    for (name, _) in COMMANDS {
        assert!(
            text.contains(name),
            "`/help` is missing {name}, which is a command an operator can type",
        );
    }
    // And the grouping is the palette's own rather than a second arrangement.
    let order: Vec<&str> = grouped()
        .into_iter()
        .flat_map(|(_, rows)| rows.into_iter().map(|(name, _)| name))
        .collect();
    let mut at = 0usize;
    for name in order {
        let found = text[at..]
            .find(name)
            .unwrap_or_else(|| panic!("{name} is out of the grouped order in /help"));
        at += found + name.len();
    }
}

#[test]
fn f15_help_is_committed_rather_than_drawn_as_an_overlay() {
    // `/help` answers with `Action::Print`, which the driver commits into the
    // terminal's own scrollback. That is where this product puts anything worth
    // reading twice, and it is what makes help survive the next keystroke.
    match commands::parse("help", &defaults(), &DARK) {
        Action::Print(lines) => assert!(!lines.is_empty()),
        other => panic!("`/help` should be committed, not opened: {other:?}"),
    }
}

// --- 0.18.0: the two commands that reach the memory ---------------------------

/// `/remember` takes the REST of the line, and `/memory` takes nothing.
///
/// The first half is the one with a defect behind it. A line of guidance is a
/// *sentence*, and `split_whitespace().nth(1)` would remember its first word and
/// drop the rest with nothing said — the reader would see a confirmation naming a
/// real file and find one word in it. `/config`'s value arm already documents the
/// same trap for arrays with spaces in them; here what is lost is prose, which
/// nobody notices missing until a later run behaves as though it had never been
/// told.
#[test]
fn remember_keeps_the_whole_line_and_memory_takes_no_argument() {
    assert_eq!(
        commands::parse("remember run the linter before pushing", &defaults(), &DARK),
        Action::Remember("run the linter before pushing".to_string()),
    );
    // Interior spacing is the operator's; only the ends are trimmed.
    assert_eq!(
        commands::parse("remember   two  spaces   ", &defaults(), &DARK),
        Action::Remember("two  spaces".to_string()),
    );
    // The word alone is empty rather than a refusal here: the sentence saying
    // what to type belongs to the driver, which would otherwise open a picker
    // over three files it is about to write nothing into.
    assert_eq!(
        commands::parse("remember", &defaults(), &DARK),
        Action::Remember(String::new()),
    );
    assert_eq!(
        commands::parse("remember    ", &defaults(), &DARK),
        Action::Remember(String::new()),
    );

    assert_eq!(
        commands::parse("memory", &defaults(), &DARK),
        Action::Memory
    );
    // Arguments are tolerated and the first word decides, as everywhere else.
    assert_eq!(
        commands::parse("memory now", &defaults(), &DARK),
        Action::Memory,
    );

    // Neither falls through to the print an unknown command lands on, which is
    // the failure a listed-but-unwired command has: advertised and inert.
    assert!(!matches!(
        commands::parse("memory", &defaults(), &DARK),
        Action::Print(_)
    ));
    assert!(!matches!(
        commands::parse("remember x", &defaults(), &DARK),
        Action::Print(_)
    ));
}

/// Both are listed, both are in the `Configure` group, and both have a palette
/// row that resolves back to the command whole.
///
/// The sabotage arm is listing a command without wiring its row — the one way a
/// listed command must not fail. `palette_pick` is what the driver resolves a
/// chosen row through, so asking it is asking the driver.
#[test]
fn the_memory_commands_are_configure_commands_with_working_palette_rows() {
    use io_cli::commands::{group_of, palette, palette_pick, Chosen, Group};
    use io_harness::Templates;

    for name in ["/remember", "/memory"] {
        assert!(
            COMMANDS.iter().any(|(command, _)| *command == name),
            "{name} is not in the inventory, so nobody is told it exists",
        );
        // Configure, and not Inspect: both of these write. That was already the
        // reason in 0.18.0, when Inspect was also full; it is the whole reason
        // now, because 0.19.0 moved `/mcp` and `/provider` out on the same
        // argument and left room behind them.
        assert_eq!(
            group_of(name),
            Some(Group::Configure),
            "{name} writes a file the operator keeps, so it belongs with the group that writes",
        );

        let rows = palette(&Templates::none(), &[]);
        let bare = name.strip_prefix('/').expect("a command carries its slash");
        let index = rows
            .iter()
            .position(|row| row.label == bare)
            .unwrap_or_else(|| panic!("{name} has no palette row"));
        assert_eq!(
            palette_pick(&Templates::none(), &[], index),
            Some(Chosen::Command(name)),
            "{name}'s row is advertised and inert",
        );
    }
}

// --- 0.19.0: `/skills`, and the two commands that stopped being inspections ----

/// `/mcp` and `/provider` are `Configure` commands, and `/skills` is the
/// `Inspect` command that took their place.
///
/// Two claims, and the first is the one with a decision behind it. Both of those
/// commands add, edit, disable and remove entries in the configuration file, so
/// filing them under a group whose documentation says "none of it changes what a
/// turn does" was a sentence the surface did not keep. The move is asserted by
/// name rather than by counting, because the count is the *consequence* — a later
/// release that grew `Inspect` back to ten must not be able to quietly file a
/// writer under it again to balance the tables.
///
/// The sabotage arm for the second claim is listing `/skills` without wiring its
/// row: `palette_pick` is what the driver resolves a chosen row through, so a row
/// that comes back as anything but the command whole is advertised and inert.
#[test]
fn the_writers_are_configure_commands_and_skills_is_the_inspection() {
    use io_cli::commands::{group_of, palette, palette_pick, Chosen, Group, GROUPS};
    use io_harness::Templates;

    for name in ["/mcp", "/provider"] {
        assert_eq!(
            group_of(name),
            Some(Group::Configure),
            "{name} adds, edits, disables and removes entries in the file the \
             operator keeps, so it belongs with the group that writes",
        );
    }

    assert!(
        COMMANDS.iter().any(|(command, _)| *command == "/skills"),
        "a surface nobody is told about is a surface nobody uses",
    );
    assert_eq!(
        group_of("/skills"),
        Some(Group::Inspect),
        "`/skills` opens a list and writes nothing, which is what Inspect means",
    );

    // **0.27.0 spends the last seat this move freed, and that is a decision
    // rather than an oversight.** The 0.19.0 form of this assertion was
    // `inspect < 10` — the release that made the room saying it had not used all
    // of it. `/store` and `/export` take the group to exactly ten, which is the
    // bound `f13_no_group_is_longer_than_ten` enforces, so the sentence this gate
    // makes changes from *there is room left* to *the room is now gone*: the next
    // command that would belong in `Inspect` re-files one that is in the wrong
    // group rather than widening the bound, which is what `Turn` did for `/undo`
    // in this same release and what 0.19.0, 0.22.0 and 0.26.0 each did before it.
    //
    // The equality is deliberate. `<= 10` would go on passing at nine and would
    // stop saying anything the moment somebody removed a command; an exact ten
    // fails in both directions and makes the next change a decision somebody
    // records here.
    let inspect = GROUPS
        .iter()
        .find(|(group, _)| *group == Group::Inspect)
        .map(|(_, names)| names.len())
        .expect("Inspect is a group with commands in it");
    assert_eq!(
        inspect, 10,
        "Inspect holds {inspect}; 0.27.0 filled it to the bound with `/store` and \
         `/export`, so the next command here re-files rather than widens",
    );

    let rows = palette(&Templates::none(), &[]);
    let index = rows
        .iter()
        .position(|row| row.label == "skills")
        .expect("`/skills` has no palette row");
    assert_eq!(
        palette_pick(&Templates::none(), &[], index),
        Some(Chosen::Command("/skills")),
        "`/skills`'s row is advertised and inert",
    );
}

/// `Forgotten::Refused` is reported as a refusal that names why, never as a
/// removal.
///
/// The one outcome of `Store::memory_forget` that must not be collapsed into the
/// others. The note is pinned, so it is not a run's to withdraw and io-cli asks on
/// a run's behalf: it stands, unchanged, and goes on being carried into every
/// later prompt. A surface reporting that as success tells the operator their note
/// is gone while the model keeps reading it — the same failure the pin flag exists
/// to prevent one level down.
#[test]
fn a_withdrawal_that_did_not_happen_names_the_pin_and_is_never_a_success() {
    use io_cli::commands::forgotten_said;
    use io_cli::glyphs::ASCII;
    use io_cli::recall::{Forgotten, Scope};
    use io_cli::theme::Tone;

    let (tone, said) = forgotten_said(
        "build-command",
        Scope::Workspace,
        Forgotten::Refused,
        &ASCII,
    );
    // **Not a success, and since 0.32.0 not a refusal either.** `Tone::Refused`
    // means an act the permission boundary refused, and nothing here went near
    // one — this is io-cli's own bookkeeping about a note the operator pinned.
    // Spending the word `refused` on it is how the word stops meaning anything on
    // the surface that needs it. What carries the fact is the sentence, asserted
    // below, which is the same argument `forgotten_said`'s own doc makes.
    assert_ne!(tone, Tone::Success, "nothing was withdrawn: {said}");
    assert_ne!(
        tone,
        Tone::Refused,
        "a pinned note is not a permission boundary refusing an act: {said}",
    );
    assert!(
        said.contains("pinned"),
        "the reason is what makes it actionable: {said}",
    );
    assert!(said.contains("Unpin"), "and the way out is named: {said}",);
    assert!(
        !said.contains("withdrawn") && !said.contains("removed"),
        "nothing was withdrawn, so nothing may say it was: {said}",
    );

    // The other two are three different things and stay three.
    let (removed, text) = forgotten_said(
        "k",
        Scope::Global,
        Forgotten::Removed { restore: 9 },
        &ASCII,
    );
    assert_eq!(removed, Tone::Success);
    assert!(text.contains('9'), "the restore point is named: {text}");
    let (absent, text) = forgotten_said("k", Scope::Global, Forgotten::Absent, &ASCII);
    assert_ne!(
        absent,
        Tone::Success,
        "an absent key was not removed: {text}"
    );
    assert_ne!(
        absent,
        Tone::Refused,
        "and it was not refused either: {text}"
    );
}

/// A note that exists and is not pinned reports "no entry" rather than success.
///
/// `Store::memory_pin` answers `false` for *there was no such entry*, which is not
/// "the pin failed" — and a `bool` at a call site reads as "did it work". A surface
/// that believed it shows a pin the store does not hold.
#[test]
fn pinning_a_key_that_is_not_there_is_not_reported_as_a_pin() {
    use io_cli::commands::pinned_said;
    use io_cli::recall::{Pinned, Scope};
    use io_cli::theme::Tone;

    let (tone, said) = pinned_said("gone", Scope::Workspace, true, Pinned::NoEntry);
    assert_ne!(tone, Tone::Success, "{said}");
    assert!(said.contains("nothing was changed"), "{said}");

    let (tone, said) = pinned_said("k", Scope::Workspace, true, Pinned::Set);
    assert_eq!(tone, Tone::Success);
    assert!(said.contains("pinned"), "{said}");
    let (_, unpinned) = pinned_said("k", Scope::Workspace, false, Pinned::Set);
    assert!(unpinned.contains("unpinned"), "{unpinned}");
}

/// The scope rows say what committing each file MEANS, not only its name.
///
/// That is the entire difference between the three: `IO.md`, `AGENTS.md` and
/// `AGENTS.local.md` are three filenames, and which of them goes to everybody who
/// clones the repository is not knowable from any of them. A picker offering three
/// filenames would be asking a question whose answer lives somewhere else.
#[test]
fn the_remember_scope_rows_name_the_file_and_what_committing_it_means() {
    use io_cli::commands::scope_rows;
    use io_cli::glyphs::ASCII;
    use io_harness::config::Scope;

    let paths: Vec<(Scope, std::path::PathBuf)> = [Scope::Project, Scope::Local, Scope::User]
        .into_iter()
        .map(|scope| {
            (
                scope,
                std::path::PathBuf::from("/w").join(io_cli::memory::file_name(scope)),
            )
        })
        .collect();
    let rows = scope_rows(&paths, &ASCII);
    assert_eq!(rows.len(), 3);

    for (row, (scope, path)) in rows.iter().zip(&paths) {
        assert_eq!(
            row.label,
            io_cli::memory::file_name(*scope),
            "the label is the file, which is what the operator knows it by",
        );
        let detail = row.detail.clone().unwrap_or_default();
        assert!(
            detail.contains(&path.display().to_string()),
            "the row names the file it writes: {detail}",
        );
    }

    let said = |n: usize| rows[n].detail.clone().unwrap_or_default();
    assert!(
        said(0).contains("committed"),
        "AGENTS.md is the one that goes to everybody who clones: {}",
        said(0),
    );
    assert!(
        said(1).contains("never committed"),
        "AGENTS.local.md is the one that goes nowhere: {}",
        said(1),
    );
    assert!(
        said(2).contains("every project on this machine"),
        "IO.md is the one that follows the operator: {}",
        said(2),
    );
    // The consequence leads and the path follows, because the picker fits a
    // detail from the head — so on a narrow terminal the sentence that decides
    // the answer is what survives and the path is what goes.
    for row in &rows {
        let detail = row.detail.clone().unwrap_or_default();
        assert!(
            !detail.starts_with('/'),
            "the path leads, so the consequence is what a narrow terminal drops: {detail}",
        );
    }
}

/// The memory page keeps its two lists distinguishable, and never presents a cut
/// draw scan as an exact count.
#[test]
fn the_memory_page_separates_the_two_memories_and_qualifies_a_cut_draw_count() {
    use io_cli::commands::{
        memory_notes, memory_page, Held, LOOSE_MARK, PINNED_MARK, READ_MARK, UNREAD_MARK,
    };
    use io_cli::glyphs::ASCII;
    use io_cli::memory::Instruction;
    use io_cli::recall::{Caps, Remembered, Scope, View};
    use io_harness::config::Scope as FileScope;

    let files = vec![
        Instruction {
            scope: FileScope::Project,
            path: "/w/AGENTS.md".into(),
            exists: true,
            lines: 12,
            read: true,
        },
        // The case the page exists for: there, and not being read.
        Instruction {
            scope: FileScope::Local,
            path: "/w/AGENTS.local.md".into(),
            exists: true,
            lines: 3,
            read: false,
        },
        Instruction {
            scope: FileScope::User,
            path: "/home/someone/.io-cli/IO.md".into(),
            exists: false,
            lines: 0,
            read: false,
        },
    ];
    let entries = vec![
        Remembered {
            scope: Scope::Workspace,
            key: "build-command".into(),
            value: "cargo test".into(),
            kind: "fact",
            pinned: true,
            run_id: 4,
            step: 2,
            created_at: "2026-08-26 09:00".into(),
            draws: 6,
        },
        Remembered {
            scope: Scope::Global,
            key: "prefers-terse".into(),
            value: "keep answers short".into(),
            kind: "decision",
            pinned: false,
            run_id: 9,
            step: 1,
            created_at: "2026-08-26 09:01".into(),
            draws: 0,
        },
    ];

    let (rows, held) = memory_page(&files, &entries, false, &ASCII);
    assert_eq!(
        rows.len(),
        held.len(),
        "the rows and what they stand for are built in one pass and cannot differ",
    );
    // Two headings, so the two memories are never one list.
    let headings: Vec<&str> = rows
        .iter()
        .filter(|row| row.heading)
        .map(|row| row.label.as_str())
        .collect();
    assert_eq!(headings.len(), 2, "got {headings:?}");
    for (row, held) in rows.iter().zip(&held) {
        assert_eq!(row.heading, *held == Held::Nothing);
    }

    let mark = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .unwrap_or_else(|| panic!("no row for {label}"))
            .mark
            .expect("every row but a heading carries a mark")
    };
    assert_eq!(mark("AGENTS.md"), READ_MARK);
    assert_eq!(mark("AGENTS.local.md"), UNREAD_MARK);
    assert_eq!(mark("build-command"), PINNED_MARK);
    assert_eq!(mark("prefers-terse"), LOOSE_MARK);

    // A file that exists and is not read SAYS so; nothing else in this product
    // does, because io-harness skips one without a word.
    let detail = |label: &str| {
        rows.iter()
            .find(|row| row.label == label)
            .and_then(|row| row.detail.clone())
            .unwrap_or_default()
    };
    assert!(
        detail("AGENTS.local.md").contains("NOT read"),
        "{}",
        detail("AGENTS.local.md")
    );
    assert!(
        detail("IO.md").contains("not written yet"),
        "{}",
        detail("IO.md")
    );
    assert!(
        detail("AGENTS.md").starts_with("read"),
        "{}",
        detail("AGENTS.md")
    );
    // The bucket a note came from leads its detail: "is this true here, or true
    // everywhere" is the only question the two scopes exist to answer.
    assert!(detail("build-command").starts_with("workspace"));
    assert!(detail("prefers-terse").starts_with("global"));

    // An uncut scan states a count.
    assert!(
        detail("build-command").contains("6 draws"),
        "{}",
        detail("build-command")
    );
    assert!(
        !detail("build-command").contains("or more"),
        "an uncut scan is a count, not a floor: {}",
        detail("build-command"),
    );
    // A cut one states a floor, in words, on every row.
    let (cut_rows, _) = memory_page(&files, &entries, true, &ASCII);
    for label in ["build-command", "prefers-terse"] {
        let detail = cut_rows
            .iter()
            .find(|row| row.label == label)
            .and_then(|row| row.detail.clone())
            .unwrap_or_default();
        assert!(
            detail.contains("draws or more"),
            "a cut scan makes every count a lower bound: {detail}",
        );
    }

    // And the caps are stated PER SCOPE, with the real ceiling named. One number
    // is half the ceiling for a run drawing on both, and half a ceiling makes an
    // ordinary eviction read as a defect.
    let view = View {
        workspace: "/w".into(),
        entries,
        caps: [Scope::Workspace, Scope::Global]
            .map(|scope| Caps {
                scope,
                limits: io_harness::MemoryLimits {
                    max_entries: 32,
                    max_chars: 8_000,
                    max_entry_chars: 500,
                },
            })
            .to_vec(),
        trace: Vec::new(),
        draws_cut: true,
    };
    let notes = memory_notes(&view, &ASCII).join("\n");
    assert!(notes.contains("per scope"), "{notes}");
    assert!(notes.contains("workspace 32 entries"), "{notes}");
    assert!(notes.contains("global 32 entries"), "{notes}");
    assert!(
        notes.contains("64 entries"),
        "the ceiling across both scopes is the honest number: {notes}",
    );
    assert!(
        notes.contains("lower bound"),
        "a cut scan is disclosed on the page, not only on a row: {notes}",
    );
    assert!(
        notes.contains("/w"),
        "the bucket that answered is named, so an empty list can be told from the \
         wrong bucket: {notes}",
    );
}

#[test]
fn f17_usage_answers_and_is_never_listed() {
    use io_cli::commands::{group_of, palette, GROUPS};

    // **It answers `/cost`, and until 0.22.0 it answered `/status`.** The
    // argument for that was that `/status` was the closest thing this program had
    // to a spending surface — it commits the token draw, the budgets and what is
    // left of them. It stopped holding the moment a page existed whose whole
    // subject is what has been spent: an operator typing the field's own word for
    // that page and landing on the session's configuration is being answered a
    // question they did not ask.
    assert_eq!(commands::parse("usage", &defaults(), &DARK), Action::Cost);
    assert_eq!(commands::parse("cost", &defaults(), &DARK), Action::Cost);
    // And `/status` still answers itself, which is the half of this that did not
    // change and the half a careless repoint would have taken with it.
    assert_eq!(
        commands::parse("status", &defaults(), &DARK),
        Action::Status
    );

    // And it is nowhere: not in the inventory, not in a group, not a palette row.
    assert!(
        !COMMANDS.iter().any(|(name, _)| *name == "/usage"),
        "an alias earns no row of its own",
    );
    assert!(group_of("/usage").is_none());
    for (_, names) in GROUPS {
        assert!(!names.contains(&"/usage"));
    }
    let rows = palette(&io_harness::Templates::none(), &[]);
    assert!(
        !rows.iter().any(|row| row.label == "usage"),
        "a second row for one screen reads as a second screen",
    );
}

/// 0.24.0 — `/gates` is a **configure** command, resolves to its own action, and
/// its row is reachable.
///
/// Every claim here is written against the sabotage that would satisfy the
/// generic gates in this file while leaving the command broken. None of them is
/// a restatement of a gate that already covers it.
///
/// `no_listed_command_falls_through_to_the_help_listing` proves only that the
/// word is not the unknown-command listing; a `parse` arm that returned
/// `Action::Config(None)` — the surface next door, and the one a careless hand
/// reaches for — would pass it and would open the wrong screen. So the action is
/// asserted by name.
///
/// `f13_every_command_is_in_exactly_one_group` proves only that it has *a* home.
/// `Inspect` would satisfy it, and `Inspect`'s own documentation says nothing
/// under it changes what a turn does — which a surface that writes
/// `[app.io-cli.gates]` falsifies for every later turn at once. So the group is
/// asserted by name rather than left to the count, exactly as the `/mcp` and
/// `/provider` move is.
///
/// And the description is asserted to name a cost, because `tests/docs.rs` copies
/// it into the README verbatim and it is the only sentence most operators will
/// ever read about this command. A rubric is a real billed completion on every
/// gated turn; a one-liner that said "what done means" and stopped would hide
/// that behind a word that sounds free.
#[test]
fn gates_is_a_configure_command_that_resolves_to_its_own_action() {
    use io_cli::commands::{group_of, palette, palette_pick, Chosen, Group};
    use io_harness::Templates;

    assert!(
        COMMANDS.iter().any(|(name, _)| *name == "/gates"),
        "a surface nobody is told about is a surface nobody uses",
    );
    assert_eq!(
        commands::parse("gates", &defaults(), &DARK),
        Action::Gates,
        "`/gates` must open its own surface, not the one next door",
    );
    // The singular is the spelling a hand reaches for when there is exactly one
    // criterion, and it must be the same screen rather than an unknown command.
    assert_eq!(commands::parse("gate", &defaults(), &DARK), Action::Gates);
    // Arguments are tolerated and the first word decides, as everywhere else —
    // and this is the command most likely to be typed with one, because the
    // thing an operator wants to set is a command line.
    assert_eq!(
        commands::parse("gates cargo test", &defaults(), &DARK),
        Action::Gates,
    );

    assert_eq!(
        group_of("/gates"),
        Some(Group::Configure),
        "`/gates` writes [app.io-cli.gates], and Inspect promises it never writes \
         — a gate an operator did not mean to set spends a whole extra turn \
         against a real model",
    );

    let (_, said) = COMMANDS
        .iter()
        .find(|(name, _)| *name == "/gates")
        .expect("/gates is listed");
    assert!(
        said.contains("rubric"),
        "the one line most operators read must name the kind that costs money on \
         every gated turn: {said}",
    );

    let rows = palette(&Templates::none(), &[]);
    let index = rows
        .iter()
        .position(|row| row.label == "gates")
        .expect("`/gates` has no palette row");
    assert_eq!(
        palette_pick(&Templates::none(), &[], index),
        Some(Chosen::Command("/gates")),
        "`/gates`'s row is advertised and inert",
    );
}

/// **The three new commands' argued forms, which had no gate at all.**
///
/// The adversarial review found that nothing in `tests/` typed `/store`,
/// `/undo` or `/export` with an argument — so the release's most load-bearing
/// decision, that an unparseable verb must **never** fall through to something
/// destructive, was asserted nowhere.
///
/// Both refusal families exist for a stated reason and both are checked here:
///
/// - `Action::UndoNoStep` — *an operator who typed a step and got the entire run
///   undone would have lost work they never asked to lose.*
/// - `Keep::{NoId, NoDate, Unknown}` — *somebody who typed a delete and got a
///   report would believe the delete had happened.*
#[test]
fn f10_an_unparseable_verb_never_falls_through_to_something_destructive() {
    use io_cli::commands::{Keep, Taken};
    use io_cli::undo::Grain;

    // `parse` takes the word without its slash — the driver strips it before the
    // call, which is why every other test in this file types `continue` rather
    // than `/continue`. Written as a helper so the cases below read as the
    // operator types them.
    let parse =
        |text: &str| commands::parse(text.strip_prefix('/').unwrap_or(text), &defaults(), &DARK);

    // `/undo` — the one that must never become a whole-run undo.
    assert_eq!(parse("/undo"), Action::Undo(Grain::Run));
    assert_eq!(
        parse("/undo src/a.rs"),
        Action::Undo(Grain::File("src/a.rs".into())),
    );
    assert_eq!(parse("/undo step 4"), Action::Undo(Grain::Step(4)));
    for wrong in [
        "/undo step",
        "/undo step -1",
        "/undo step 3.5",
        "/undo step nine",
        "/undo step 99999999999999999999",
        "/undo step 0x10",
    ] {
        assert_eq!(
            parse(wrong),
            Action::UndoNoStep,
            "`{wrong}` must be refused by name and must never become Grain::Run",
        );
    }

    // `/store` — the verbs, and the three refusals.
    assert_eq!(parse("/store"), Action::Store(None));
    assert_eq!(parse("/store rm 7"), Action::Store(Some(Keep::Remove(7))));
    assert_eq!(parse("/store compact"), Action::Store(Some(Keep::Compact)));
    assert_eq!(
        parse("/store sweep 2026-08-01"),
        Action::Store(Some(Keep::Sweep("2026-08-01".into()))),
    );
    for wrong in [
        "/store rm",
        "/store rm all",
        "/store rm -1x",
        "/store delete",
    ] {
        assert!(
            matches!(parse(wrong), Action::Store(Some(Keep::NoId))),
            "`{wrong}` must ask for an id rather than act: {:?}",
            parse(wrong),
        );
    }
    assert!(matches!(
        parse("/store sweep"),
        Action::Store(Some(Keep::NoDate))
    ));
    // The typo that matters: a near-miss verb must not reach the page, because a
    // report where a deletion was asked for reads as a deletion that happened.
    for typo in ["/store swep 2026-08-01", "/store purge", "/store vacume"] {
        assert!(
            matches!(parse(typo), Action::Store(Some(Keep::Unknown(_)))),
            "`{typo}` must be named as unknown rather than showing the page: {:?}",
            parse(typo),
        );
    }

    // `/export` — `trace` is a word and everything else is a path.
    assert_eq!(parse("/export"), Action::Export(Taken::Conversation(None)));
    assert_eq!(
        parse("/export notes.md"),
        Action::Export(Taken::Conversation(Some("notes.md".into()))),
    );
    assert_eq!(parse("/export trace"), Action::Export(Taken::Trace(None)));
    assert_eq!(
        parse("/export trace run.txt"),
        Action::Export(Taken::Trace(Some("run.txt".into()))),
    );
    // The documented ambiguity, and its documented escape.
    assert_eq!(
        parse("/export ./trace"),
        Action::Export(Taken::Conversation(Some("./trace".into()))),
        "a file literally called `trace` is reachable as `./trace`",
    );
}

/// **F5 — the masking verbs are words on a row that already exists.**
///
/// The count and the group occupancy are asserted by
/// [`o2_the_palette_did_not_grow_and_no_group_was_refiled`], which this release
/// leaves alone — that is the whole of the claim there, and the strongest form of
/// it is a literal nobody edited. What is left to state is the half a count cannot
/// see: that no row was added for a verb. `COMMANDS` carries `/copy diff` as a row
/// of its own, so "a verb never gets a row" is not a rule this table follows on its
/// own and a reader could reasonably have added `/context withhold` beside it.
///
/// Sabotage: add `("/context withhold", "…")` to `COMMANDS` — under which this
/// fails by name, and the count and occupancy gates fail beside it, which is the
/// point: three gates for one slot because the two big groups are at the bound.
#[test]
fn f5_the_mask_verbs_take_no_row_of_their_own() {
    let context_rows: Vec<&str> = COMMANDS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| name.split_whitespace().next() == Some("/context"))
        .collect();

    assert_eq!(
        context_rows,
        vec!["/context"],
        "0.29.0 made `marketplace` words inside `/plugin` rather than a command, \
         and the mask verbs are the same decision: `Group::Turn` and \
         `Group::Inspect` are at the ten-per-group bound, so a row here is a row \
         with nowhere to be filed.",
    );
}

/// **F5 — every `/context` form parses to what it says, and the bare word is still
/// the page.**
///
/// The bare form is asserted first and by equality, because it is the one that
/// must not have moved: `/context` was a page before this release and an operator
/// who types it is asking for the page, not for a verb they have never heard of.
///
/// The refusals are the `/store` family's, for the reason that file records —
/// somebody who typed a withhold and got a report would believe the tool was
/// withheld. `withhold` with no name is refused rather than read as "withhold
/// everything"; `allow` with no name clears, which is the asymmetry
/// [`io_cli::commands::Masked::Clear`] argues for at length.
///
/// Sabotage: fold the two bare forms together — return `Masked::Clear` for a bare
/// `withhold` as well — under which only this fails, and it fails by taking every
/// tool away from a line that named none.
#[test]
fn f5_the_context_verbs_parse_and_a_bare_context_is_still_the_page() {
    use io_cli::commands::Masked;

    let parse =
        |text: &str| commands::parse(text.strip_prefix('/').unwrap_or(text), &defaults(), &DARK);

    assert_eq!(parse("/context"), Action::Context(None));

    assert_eq!(
        parse("/context withhold docx_write"),
        Action::Context(Some(Masked::Withhold("docx_write".into()))),
    );
    assert_eq!(
        parse("/context allow docx_write"),
        Action::Context(Some(Masked::Allow("docx_write".into()))),
    );
    // Bare `allow` drops the whole mask. Deliberate, and the argument is in
    // `Masked::Clear`: no file is written, the page one keystroke away lists what
    // is withheld, and the command that built the mask puts it back.
    assert_eq!(
        parse("/context allow"),
        Action::Context(Some(Masked::Clear))
    );
    // `withhold` with nothing named is NOT the mirror of that.
    assert_eq!(
        parse("/context withhold"),
        Action::Context(Some(Masked::NoTool)),
    );

    // The name keeps its case; the verb does not need to.
    assert_eq!(
        parse("/context WITHHOLD Docx_Write"),
        Action::Context(Some(Masked::Withhold("Docx_Write".into()))),
        "a verb typed in another case is the same verb, and a tool name folded to \
         lower case is a name no catalogue answers to — io-harness keeps an \
         unknown name rather than rejecting it, so the mistake would be silent",
    );

    // A near-miss verb must not reach the page.
    for typo in ["/context withold docx_write", "/context deny docx_write"] {
        assert!(
            matches!(parse(typo), Action::Context(Some(Masked::Unknown(_)))),
            "`{typo}` must be named as unknown rather than showing the page: {:?}",
            parse(typo),
        );
    }
}

/// **F8 — the masking verbs run while a turn is in flight.**
///
/// `/context` has been in `MID_TURN` since the page existed and the verbs inherit
/// it, which is the answer rather than an oversight: the mask is state this process
/// owns, it writes no file, and `contract::masking` applies it to the *next* turn's
/// contract — so a withhold typed mid-turn cannot reach the turn already running.
/// That is what makes it unlike `/config`, which is in `BARE_ONLY_MID_TURN`
/// because its argued forms write a scope file.
///
/// `/config prices.as_of` is asserted here beside them, unchanged, so a release
/// that reached for `BARE_ONLY_MID_TURN` to guard the mask cannot quietly take the
/// guard off the thing it was built for.
///
/// Sabotage: add `"/context"` to `BARE_ONLY_MID_TURN` — under which only this test
/// fails, and it fails by refusing mid-turn exactly the lever an operator reaches
/// for *because* a turn is running and calling something they would rather it did
/// not.
#[test]
fn f8_the_mask_verbs_are_admissible_mid_turn() {
    use io_cli::commands::runs_mid_turn;

    for line in [
        "context",
        "context withhold docx_write",
        "context allow docx_write",
        "context allow",
        "context withhold",
    ] {
        assert!(
            runs_mid_turn(line),
            "`/{line}` reads the session's own state and writes no file; refusing \
             it mid-turn refuses it when it is most wanted",
        );
    }

    assert!(runs_mid_turn("config"), "the bare list only reports");
    assert!(
        !runs_mid_turn("config prices.as_of"),
        "the bare-only mechanism exists for the command that descends toward a \
         write, and this release does not borrow it",
    );
}
