//! `io` — the driver.
//!
//! Everything with a decision in it lives in the library so a test can reach it.
//! What is here is the wiring: reading the configuration, taking the terminal,
//! turning keystrokes into commands and commands into io-harness calls, and
//! giving the terminal back.

use std::io::IsTerminal;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use io_harness::{Config, Policy, Provider, ProviderSpec, Session, Steer, Store, Templates};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use io_cli::app::{App, Command};
use io_cli::cli::{Cli, Command as Subcommand};
use io_cli::commands::{self, Action, Copied};
use io_cli::complete;
use io_cli::glyphs::Glyphs;
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::settings::{self, Posture};
use io_cli::term::Screen;
use io_cli::theme::{Theme, Tone};
use io_cli::wizard::{Progress, Wizard};
use io_cli::{approval, bridge, provider, shell, splash, verify};
use ratatui::text::{Line, Span};

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("io: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    // **The migration report is owed until somebody delivers it.** `run` fills this
    // and each arm empties it as it takes it — `io exec` on stderr, a session into
    // its scrollback — so anything still here is a report nobody has said, which is
    // to say a run that died between adopting the home and having anywhere to
    // speak. Found by running the binary twice: once with a file io-harness could
    // not parse and once with an unreadable store, an operator saw an error naming
    // a path they had never seen, one keystroke after their old directory emptied,
    // with nothing anywhere saying their install had just moved. Held here rather
    // than patched at each early return, because the next early return would not
    // know to do it.
    let mut report = Vec::new();
    match runtime.block_on(run(&mut report)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            // Printed after the terminal has been restored, never into raw mode.
            for line in report {
                eprintln!("{line}");
            }
            eprintln!("io: {error}");
            ExitCode::from(io_cli::exec::FAILED)
        }
    }
}

async fn run(report: &mut Vec<String>) -> Result<u8, String> {
    let cli = Cli::parse();
    let root = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };

    // **Before the discovery on the next line, and this is the only discovery
    // either arm reaches**, so one call here serves the session and `io exec`
    // both. Order is the whole of it: `io_harness::config::user_path` reads the
    // environment at call time, so a configuration discovered first would come
    // from the old directory while the store — derived from the file's own
    // directory by `settings::store_path` — answered from the new one, and the
    // visible symptom is a `/resume` that silently finds nothing. Empty when the
    // operator named a location themselves, which is `adopt` refusing to move
    // anybody who has already chosen.
    let adopted = io_cli::home::adopt();
    *report = adopted
        .as_ref()
        .map_or_else(Vec::new, io_cli::home::Report::lines);
    // **The skills io-cli ships go in immediately after the home is adopted and
    // long before any contract is built**, because a skill written after the
    // contract is a skill the run cannot be offered — and the run that would miss
    // them is the first one of a new install, which is exactly the session in
    // which somebody is most likely to ask for help.
    //
    // Gated on `adopt` having actually adopted, which is what the `Some` above
    // means. An operator who set `IO_CONFIG` or `IO_CONFIG_HOME` themselves has
    // chosen a home, `adopt` stands aside for them, and creating `~/.io-cli/skills`
    // anyway would do something worse than nothing: `contract::default_skills`
    // takes that directory the moment it exists, so io-cli would silently attach a
    // skills directory to a run whose operator had pointed everything else
    // somewhere else.
    //
    // It cannot fail the run. `install` returns report lines and never an error
    // for exactly that reason — a read-only directory nobody has heard of is not
    // a reason to refuse to start.
    //
    // **Into io-cli's own home, and NOT into a directory the operator chose.**
    // `[run] skills` and `[app.io-cli] skills` both beat the default, and writing
    // five files into whatever a team pointed at — a checked-out repository of
    // shared skills, say — is not something a version bump should do quietly. The
    // consequence is stated rather than hidden: where the two differ, the report
    // says so below, and `/skills` lists the directory in force rather than this
    // one.
    if adopted.is_some() {
        if let Some(home) = io_cli::home::path() {
            report.extend(io_cli::skills::install(&home));
        }
    }
    // **A refusal is not a crash, and until 0.20.0 this line made every refusal
    // look like one.** io-harness refuses a whole `Config::discover` when the
    // project-scoped file — the one a `git clone` delivers — declares `[[hook]]`,
    // because a hook runs a command on this machine. That refusal is correct and
    // io-cli does not soften it: there is no `Config` to be had, so the program
    // genuinely cannot start.
    //
    // What it can do is say so in words the operator can act on. io-harness's own
    // sentence names the key, the reason and the two files that may carry it, and
    // it is worth more than anything written here — so it is passed through
    // unreworded, with only a line above it saying which file was being read.
    // Before this, the whole thing arrived as a bare error string from a program
    // that had already exited, against a repository the operator had just cloned,
    // with nothing connecting one table in one file to the failure.
    let config =
        Config::discover(&root).map_err(|error| io_cli::configure::refusal(&root, &error))?;
    // **A profile, if one was asked for, before anything reads the configuration.**
    // Applied here rather than per arm so a session and an `io exec` run get the
    // same overlay from the same decision, and refused with io-harness's own
    // sentence — which names the profile and says it is not in this file, and is
    // more than anything written here could say.
    let config = match &cli.profile {
        Some(name) => io_cli::configure::with_profile(&config, name)?,
        None => config,
    };
    // The notice this read produces is dropped *here* and only here: `run` may
    // hand control to the wizard, which rewrites the very file this just failed
    // to read, so a complaint raised now could be about a file that no longer
    // exists in that state by the time there is a session to say it in. The read
    // in `drive` happens after the wizard and is the one that discloses.
    let (stored, _) = settings::stored(&config);
    // Plain mode is decided ONCE, and by a function in the library rather than by
    // an expression written here: nothing under `tests/` can link this binary, so
    // a decision made in this file is one no test drives and no sabotage can make
    // fail. It has to be settled at this point rather than later because the glyph
    // set on the next line is chosen from it, and that set is never re-derived.
    let plain = settings::plain(cli.plain, stored.as_ref());
    // The glyph set is chosen ONCE, here, and every later resolution is handed
    // the same one. `Theme::resolve` takes it as a required argument rather than
    // defaulting it for exactly this reason: a theme is re-resolved three times
    // as a session runs — after the wizard, on `/theme`, and inside the wizard's
    // own theme step — and a default at any of those would silently discard the
    // set startup chose the moment somebody arrowed a list.
    let glyphs = Glyphs::from_env(plain, stored.as_ref().and_then(|s| s.glyphs.as_deref()));
    let theme = Theme::from_env(stored.and_then(|s| s.theme).as_deref(), glyphs);

    // The headless path leaves before anything takes the terminal, and before the
    // wizard can be reached: `io exec` in a container with no configuration file
    // must fail with a sentence, never sit at a prompt nobody can answer.
    if let Some(Subcommand::Exec(args)) = cli.command {
        // **stderr, one line each, never stdout.** `io exec --json` writes NDJSON
        // on stdout and a line of prose in that stream breaks every machine
        // reading it — the session's scrollback and this are the same lines said
        // in the two places a run can be watched from. Drained rather than read,
        // because `main` says whatever is left and a report said twice is a report
        // an operator stops reading.
        for line in report.drain(..) {
            eprintln!("{line}");
        }
        return io_cli::exec::main(args, config, root, cli.model).await;
    }

    // A session draws, so it needs a terminal to draw on, and saying so is better
    // than half-working. The check sits AFTER the subcommand is known rather than
    // before it, because `io exec` is the answer to a non-TTY stdout rather than a
    // victim of it.
    if !std::io::stdout().is_terminal() {
        return Err(
            "io is interactive and stdout is not a terminal; use `io exec \"<goal>\"` instead"
                .into(),
        );
    }

    let setup = matches!(cli.command, Some(Subcommand::Setup));
    let mut config = config;
    let mut theme = theme;

    // The wizard runs on a viewport of its own, and a tall one. Its screens are
    // pickers, a picker draws `height - 1` rows, and at the session's four that
    // was three visible options — unusable for a four-hundred-model list, and for
    // the theme step, which shares its space with a live sample, it left no rows
    // for the picker at all. The two phases are separated by a natural boundary
    // where nothing is streaming, so giving each the viewport it needs costs
    // nothing that matters.
    if setup || config.provider_spec().is_none() {
        // Attach BEFORE any reader exists: the viewport's cursor query reads its
        // answer off stdin and a reader would take it first.
        let mut screen =
            Screen::attach_with(io_cli::term::WIZARD_VIEWPORT_HEIGHT).map_err(|e| e.to_string())?;
        let (keys, mut inputs) = Keyboard::start(&screen);
        let width = screen.width();
        screen
            .commit(&splash::lines(
                &theme,
                true,
                width,
                &splash::About::default(),
            ))
            .map_err(|error| error.to_string())?;

        let chosen = wizard(&mut screen, &mut inputs, theme).await;
        // The reader goes before the screen does, and both before the next
        // attach, so nothing is holding stdin when the session's viewport is
        // placed.
        keys.stop();
        screen.restore();
        drop(screen);

        match chosen? {
            // Resolved rather than assigned, for the same reason the stored theme
            // above is: what the user picked is a preference, and `NO_COLOR`
            // outranks a preference wherever it came from. The wizard already
            // resolves its own, so this is the second lock on the same door —
            // cheap, and the door is the one a first run walks through.
            Some(chosen) => theme = Theme::from_env(Some(chosen.name), glyphs),
            // Nothing was written and the user said so. Leaving is the whole
            // answer; starting a session against no configuration is not.
            None => return Ok(io_cli::exec::OK),
        }
        // Read back what was written rather than trusting what was typed: the
        // file is the source of truth from here on, and if the harness disagrees
        // with the wizard about it this is where that shows.
        config = Config::discover(&root).map_err(|error| error.to_string())?;
    }

    let mut screen = Screen::attach().map_err(|error| error.to_string())?;
    let (keys, mut inputs) = Keyboard::start(&screen);
    if !setup && config.provider_spec().is_some() {
        let width = screen.width();
        // What an operator has to know at the first prompt and cannot read off
        // an abbreviation they have not learned yet: where the turn is going,
        // what it may do when it gets there, and which directory it is about.
        let about = splash::About {
            model: cli.model.clone().or_else(|| {
                config
                    .provider_spec()
                    .map(|spec| io_cli::provider::model_of(spec).to_string())
            }),
            policy: settings::Posture::of(&config.policy().unwrap_or_default().defaults)
                .map(|posture| posture.short().to_string()),
            workspace: Some(root.display().to_string()),
        };
        screen
            .commit(&splash::lines(&theme, true, width, &about))
            .map_err(|error| error.to_string())?;
    }

    let result = drive(
        &mut screen,
        &mut inputs,
        config,
        theme,
        cli.model,
        // **The NAME, not just the overlay it produced.** The configuration above
        // already carries `--profile`, and from 0.18.0 the session re-reads the
        // file at every turn boundary — a fresh `Config::discover` with no
        // overlay on it. Without the name to re-apply, a flag that says *for this
        // run* would quietly stop meaning anything after the first prompt.
        cli.profile,
        plain,
        // Taken, not borrowed: from here the session owns the report and `main` has
        // nothing left to say on its behalf.
        std::mem::take(report),
        &root,
    )
    .await;

    // Explicit as well as on `Drop`, so the terminal is back before anything is
    // printed about how this ended.
    keys.stop();
    screen.restore();
    result.map(|()| io_cli::exec::OK)
}

/// The session, once there is a configuration to run it against.
#[allow(clippy::too_many_arguments)]
async fn drive(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    config: Config,
    theme: Theme,
    model_override: Option<String>,
    // The `--profile` this run was started with, by name. `config` already
    // carries its overlay; the name is what re-applies it after the reload at
    // each turn boundary, which goes back to the file and knows nothing about a
    // flag. `/profile` replaces it for the rest of the session.
    profile: Option<String>,
    // Threaded down from `run` rather than read out of `config` again here, the
    // way `diff_style` below is. `--plain` is a flag, the flag outranks the file,
    // and a second read of the file at this depth would be a second answer to a
    // question already settled — one that silently drops the flag.
    plain: bool,
    // What `home::adopt` did, carried down from `run` rather than asked for again
    // here: `adopt` moves files, so a second call would be a second migration, and
    // by the time there is an `App` to say this in the environment already names
    // the home — there would be nothing left to report.
    report: Vec<String>,
    root: &std::path::Path,
) -> Result<(), String> {
    let Some(spec) = config.provider_spec().cloned() else {
        return Err("no provider is configured; run `io setup`".into());
    };

    let policy = config.policy().unwrap_or_default();
    // `[app.io-cli]` again, read through the harness rather than parsed here. It
    // is read in `drive` rather than in `run` because `run` may hand control to
    // the wizard, which writes the file this then reads back.
    //
    // **This is the read that discloses.** A section io-harness could not parse
    // comes back here as a sentence rather than as silence, and the session
    // starts on the defaults with that sentence in its scrollback — see
    // [`settings::stored`] for why the old `.unwrap_or_default()` was a defect
    // and not a shortcut.
    let (stored, complaint) = settings::stored(&config);
    let diff_style =
        settings::DiffStyle::from_setting(stored.as_ref().and_then(|s| s.diff.as_deref()));
    // The keys are resolved once, here, and handed down. Every notice the file
    // earned — a key that could not be read, a name that is no action, an
    // attempt on `Ctrl+C` — is carried with the section's own complaint, so the
    // session says everything it ignored in one place at the start rather than
    // leaving the operator to discover it by pressing something.
    let (keys, mut notices) =
        io_cli::keys::Keys::resolve(stored.as_ref().and_then(|s| s.keys.as_ref()));
    if let Some(complaint) = complaint {
        notices.insert(0, complaint);
    }
    // The prompt templates, discovered ONCE and here — not per keystroke into the
    // palette, which filters on every character typed. A `[run] templates` that
    // could not be walked comes back as an empty set *and* a sentence, and the
    // sentence joins the notices rather than being dropped: a palette with no
    // templates in it looks exactly the same whether none were configured or the
    // directory is missing, which is the difference this notice is the only
    // carrier of.
    let (templates, complaint) = commands::templates(&config);
    if let Some(complaint) = complaint {
        notices.push(complaint);
    }
    // The caps the fleet needs, read once and cloned out of the settings. A
    // session with none cannot fan out: `turn_contained_bounded_steered` is the only
    // entry point that reaches io-harness's spawn loop, and it is the caps that
    // decide whether this session takes it.
    let containment = settings::containment(stored.as_ref()).cloned();
    let capabilities = io_cli::contract::Capabilities::stored(stored.as_ref());
    // The agent's own skills, walked once beside the templates and for the same
    // reasons — the palette filters on every character typed, and a directory
    // that would not walk has to say so or it reads as one nobody configured.
    // Resolved rather than read off `[app.io-cli]`: the palette must list the same
    // directory the turn hands the agent, or a skill the model can use is one the
    // operator cannot see in `/`.
    let skills_dir = io_cli::contract::skills_dir(&config, &capabilities, root.to_path_buf());
    // **The bundles are part of the inventory, not an addition to it.** io-harness
    // merges every declared bundle's skills into the catalogue the model is given,
    // so a palette listing only the operator's own directory lists less than the
    // model was offered — which is the gap 0.20.0 shipped.
    //
    // A home io-cli cannot locate costs provenance and nothing else: `home` is
    // only ever asked whether io-cli's own manifest wrote a file, so an empty one
    // makes every row read as the operator's, which is the safe direction. It
    // never reaches a path that is joined to, so this cannot resolve anything
    // against the working directory.
    let home = io_cli::home::path().unwrap_or_default();
    let bundles = bundle_skills(&config);
    let (skills, complaint) = commands::skills(&home, skills_dir.as_deref(), &bundles);
    if let Some(complaint) = complaint {
        notices.push(complaint);
    }
    // **The one case where io-cli wrote its skills somewhere the run will not
    // read**, said once, here, where the resolved directory is finally known.
    // Silence would leave an operator with a startup line saying five skills were
    // installed and a model that has never heard of them.
    if let (Some(home), Some(in_force)) = (io_cli::home::path(), skills_dir.as_deref()) {
        let ours = io_cli::skills::dir(&home);
        if ours.is_dir() && ours != in_force {
            notices.push(format!(
                "the skills in force are in {} — io's own are in {} and are not being read",
                in_force.display(),
                ours.display(),
            ));
        }
    }
    // A server named in both `[[mcp]]` and `[[app.io-cli.mcp]]` is reconciled on
    // every turn and reported here, once. The file does not change while the
    // session runs, so the sentence is the same on the fiftieth turn as on the
    // first — and a warning that repeats is one an operator learns to read past.
    notices.extend(io_cli::contract::server_notices(&config, &capabilities));
    // Said only by a file that actually wrote `[app.io-cli] max_steps`, which is
    // why the answer comes from the field and not from the cap this session ended
    // up with — every session has one of those. The key keeps winning until
    // 0.16.0; this is the one line that says where it went.
    if let Some(notice) = settings::deprecated_max_steps(&config) {
        notices.push(notice);
    }
    let store = settings::store_path().ok_or("no place to keep the run store")?;
    let store = Store::open(&store).map_err(|error| error.to_string())?;
    let session = Session::open(&store, root).map_err(|error| error.to_string())?;

    // Kept whole before the match consumes it, because `/model` needs a
    // `ProviderSpec` to ask `verify::catalogue` what this endpoint serves — the
    // same call the wizard's model step makes, rather than a second one.
    let catalogue_spec = spec.clone();

    // The provider is built by `provider::build`, which is the only match on
    // `ProviderSpec` that constructs one — the interactive session and `io exec`
    // both arrive there rather than each keeping a copy of it. `Provider` is not
    // dyn-compatible, so what comes back from that call is not a provider: it is
    // this session, run from inside the arm that built one.
    provider::build(
        spec,
        model_override,
        Interactive {
            screen,
            inputs,
            config,
            catalogue_spec,
            store,
            session,
            policy,
            diff_style,
            keys,
            notices,
            report,
            templates,
            theme,
            plain,
            containment,
            capabilities,
            skills,
            profile,
        },
    )
    .await?
}

/// The `/skills` view over the directory the RUN reads.
///
/// Resolved through `contract::skills_dir` on every call rather than held: it is
/// derived from the configuration, the configuration is re-discovered at each
/// turn boundary, and a surface holding a directory decided at startup would go
/// on listing one an operator has since pointed away from. The home is a separate
/// argument because the manifest — which is what decides whose a file is — lives
/// there and not in the skills directory.
fn skills_view(
    config: &Config,
    capabilities: &io_cli::contract::Capabilities,
    root: &std::path::Path,
) -> io_cli::skillview::View {
    let Some(home) = io_cli::home::path() else {
        return io_cli::skillview::View::default();
    };
    let bundles = bundle_skills(config);
    match io_cli::contract::skills_dir(config, capabilities, root.to_path_buf()) {
        Some(dir) => io_cli::skillview::view(&home, &dir, &bundles),
        // **Still the bundles, with no directory of the operator's own.** A home
        // that has never made `skills/` is the ordinary case, and it is exactly
        // the case where every skill in front of the model came from a bundle —
        // so returning an empty view here would blank the surface precisely when
        // it is the only listing there is.
        None => io_cli::skillview::view_of_bundles(&bundles),
    }
}

/// The import plan as picker rows: one per item, then the row that writes.
///
/// **The accepted mark rides the LABEL, never the detail.** A narrow terminal
/// drops the detail column first — the 0.16.0 lesson — and an operator who cannot
/// see which items are switched on is an operator about to write something they
/// did not choose. On this surface that is the whole safety property, so it goes
/// in the column that survives.
///
/// The last row is the only one that writes anything, and it says how many items
/// it will write. At zero accepted it still reads honestly rather than being
/// hidden: a row that says it will write nothing is how an operator confirms they
/// meant to decline everything.
/// Write an accepted import plan, and the model with it when an endpoint was
/// chosen. Answers with the lines the surface should commit, in order.
///
/// **A free function returning lines, rather than an arm that records as it
/// goes.** The write happens from two places — straight off the review surface,
/// and after the endpoint question a model item forces — and the one thing that
/// must not differ between them is what gets written. Returning the lines instead
/// of holding `App` keeps both callers on one implementation.
///
/// The model is written last and separately because it is the only item whose
/// destination the plan could not resolve: `[[provider]]` names a vendor and a
/// model, a foreign configuration names only the model, so the vendor is a
/// question and never an inference.
fn import_written(
    chosen: &[io_cli::import::Item],
    root: &std::path::Path,
    endpoint: Option<io_cli::providers::Endpoint<'_>>,
) -> Vec<(Tone, String)> {
    let mut lines: Vec<(Tone, String)> = io_cli::import::apply(chosen, root)
        .lines()
        .into_iter()
        .map(|line| (Tone::Muted, line))
        .collect();
    let Some(endpoint) = endpoint else {
        return lines;
    };
    let Some(item) = chosen
        .iter()
        .find(|item| item.kind == io_cli::import::Kind::Model)
    else {
        return lines;
    };
    match item.provider_edit(endpoint) {
        None => lines.push((
            Tone::Refused,
            "the model could not be written: the item names none".to_string(),
        )),
        Some(edit) => {
            // The user scope, the same file the rest of the import wrote to, so a
            // provider and the servers it will talk to do not end up in two files
            // with different lifetimes.
            match io_cli::configure::write(root, io_harness::config::Scope::User, &[edit]) {
                // **A fallback, and the sentence says so.** `providers::add`
                // appends, and the front of a `[[provider]]` array is the entry a
                // run uses — so on any configuration that already had a provider,
                // which is every operator this feature was written for, the
                // imported model is the LAST link and does not answer anything
                // until the ones before it fail. Saying "in force" would be true
                // only on an empty chain, which is the one case the fixtures
                // build. The operator is told where it landed and what makes it
                // answer.
                Ok(()) => lines.push((
                    Tone::Success,
                    format!(
                        "{} joined the provider chain as its last fallback; `/provider` promotes \
                         it if you want it to answer",
                        item.model().unwrap_or("the imported model"),
                    ),
                )),
                Err(error) => {
                    lines.push((Tone::Refused, format!("the model was not written: {error}")))
                }
            }
        }
    }
    lines
}

/// Whether accepting this item causes anything to be written.
///
/// **Not `Kind::writes`, and the difference is a real one.** That answers for
/// `import::apply`, which reports a model as carried because a foreign
/// configuration names a model and never a vendor. The driver goes on to ask for
/// the vendor and write the entry, so from this surface's seat a model item does
/// write. Everything else that writes nothing has `Destination::Nowhere` — an
/// allowlist, a name already claimed, a set over the ceiling — and those are
/// findings io is reporting rather than choices it is offering.
fn import_writes(item: &io_cli::import::Item) -> bool {
    item.kind == io_cli::import::Kind::Model
        || !matches!(item.to, io_cli::import::Destination::Nowhere)
}

fn import_rows(
    items: &[io_cli::import::Item],
    accepted: &[bool],
    root: &std::path::Path,
) -> Vec<io_cli::picker::Row> {
    let mut rows: Vec<io_cli::picker::Row> = items
        .iter()
        .zip(accepted)
        .map(|(item, on)| {
            io_cli::picker::Row::marked(
                // **A row that can never write is not drawn as a checkbox.** An
                // allowlist io cannot express, a skill whose name is already
                // claimed, a set over the ceiling — these are things io found and
                // is telling the operator about. Giving them a box to tick invites
                // ticking one and being told "write the 1 item switched on above"
                // by a surface that then writes nothing.
                if !import_writes(item) {
                    "(i)"
                } else if *on {
                    "[x]"
                } else {
                    "[ ]"
                },
                format!("{} · {}", item.kind.word(), item.says),
                // **Both ends, because this surface promises both.** The source
                // alone left the destination the one fact never on screen — on the
                // one surface whose whole safety property is that nothing is
                // written the operator has not read first.
                match item.to.path(root) {
                    Some(to) => format!("{} → {}", item.from.display(), to.display()),
                    None => item.from.display().to_string(),
                },
            )
        })
        .collect();
    let count = items
        .iter()
        .zip(accepted)
        .filter(|(item, on)| **on && import_writes(item))
        .count();
    rows.push(io_cli::picker::Row::new(match count {
        0 => "write nothing and close".to_string(),
        1 => "write the 1 item switched on above".to_string(),
        many => format!("write the {many} items switched on above"),
    }));
    rows
}

/// The `(id, directory)` pair for every loaded bundle that declares skills.
///
/// **Read off `pluginview` rather than off `Plugins`, and that is not a
/// shortcut.** `Plugins::skill_dirs` is `pub(crate)` in io-harness, but
/// `Plugin::id` and `Plugin::skills_dir` are both public and `pluginview::view`
/// already folds them into exactly this pair for the `/plugin` surface. Building
/// it a second time from the plugin list would be a second answer to one
/// question, and the two could drift — which is the whole reason `/plugin` and
/// `/skills` must agree about which bundle contributed what.
///
/// **The order is the declaration order, and it matters.** It is the order
/// `TaskContract::discover_skills` folds the directories in, so a surface listing
/// them in this order lists them the way the model will be offered them.
///
/// A bundle that declares no skills directory is absent rather than present and
/// empty: it contributed nothing here, and a row saying so would be a row about
/// the absence of a thing the operator never asked for.
fn bundle_skills(config: &Config) -> Vec<(String, std::path::PathBuf)> {
    io_cli::pluginview::view(config)
        .plugins
        .into_iter()
        .filter_map(|listed| listed.skills.map(|dir| (listed.id, dir)))
        .collect()
}

/// The interactive session, as something [`provider::build`] can run.
struct Interactive<'a, 'b> {
    screen: &'a mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &'b mut UnboundedReceiver<Event>,
    /// The whole configuration, carried down to the one place a turn's contract
    /// is built rather than re-read there. Every applicable section of it reaches
    /// an interactive turn through `contract::session` from 0.14.0, which is what
    /// this field exists for; the values already read out of it above — the
    /// policy, `[app.io-cli]`, the templates — are read once and stay read once.
    config: Config,
    catalogue_spec: ProviderSpec,
    store: Store,
    session: Session,
    policy: Policy,
    diff_style: settings::DiffStyle,
    keys: io_cli::keys::Keys,
    /// What `[app.io-cli]` and `[run] templates` earned themselves, in the order
    /// they will be said.
    notices: Vec<String>,
    /// What `home::adopt` did on the way in, in the order it did it. Empty when
    /// the operator named their own location, and one line long — the home — on
    /// every run that had nothing to move.
    report: Vec<String>,
    /// What `[run] templates` points at, walked once at startup. Empty when
    /// nothing is configured and empty when the walk failed — the notice above is
    /// what tells those two apart.
    templates: Templates,
    theme: Theme,
    plain: bool,
    /// The caps a fan-out runs under, from `[app.io-cli.containment]`. `None`
    /// means the session cannot fan out at all, which is every session that
    /// configures nothing.
    containment: Option<io_harness::Containment>,
    /// What `[app.io-cli]` asked a turn's contract to carry, and the strongest of
    /// the layers `io_cli::contract::configured` documents: it is applied after
    /// `Config::apply_to`, so `[app.io-cli] max_steps` beats a `[run] max_steps`
    /// and a `[[app.io-cli.mcp]]` beats a `[[mcp]]` of the same id. It reaches
    /// every turn, contained or not — the coupling that made that untrue was
    /// removed in 0.11.0, when the flat arm moved onto an entry point that takes
    /// a caller's contract. 0.12.0 is a different change: it is where the plan
    /// gate stopped riding containment.
    capabilities: io_cli::contract::Capabilities,
    /// What io-harness discovered in the configured skills directory, walked once
    /// at startup. Empty when nothing is configured and empty when the walk
    /// failed — the notice above is what tells those two apart.
    ///
    /// **Rows rather than an `io_harness::Skills`, because bundles.** The harness
    /// merges every declared bundle's directory into the catalogue the model gets,
    /// and `Skills` has a private field, no public constructor and a `pub(crate)`
    /// `merged` — so the value describing what the run is actually offered is one
    /// io-cli cannot build. It carries the rows it can build instead, which also
    /// carry the origin the palette draws.
    skills: Vec<io_cli::skillview::Listed>,
    /// The named profile in force, or `None`. Carried as a name because the
    /// turn-boundary reload discovers the file afresh and would otherwise drop
    /// the overlay it produced.
    profile: Option<String>,
}

impl provider::WithProvider for Interactive<'_, '_> {
    type Out = Result<(), String>;

    async fn call<P: Provider>(
        self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out {
        loop_over(
            self.screen,
            self.inputs,
            make,
            self.config,
            self.catalogue_spec,
            self.store,
            self.session,
            self.policy,
            self.diff_style,
            self.keys,
            self.notices,
            self.report,
            self.templates,
            self.theme,
            self.plain,
            self.containment,
            self.capabilities,
            self.skills,
            self.profile,
            model,
        )
        .await
    }
}

/// The interactive session.
#[allow(clippy::too_many_arguments)]
async fn loop_over<P: Provider, F: Fn(&str) -> Result<P, String>>(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    make: F,
    // Held for the whole session and handed to every turn, because the file is
    // what a turn's contract is built from since 0.14.0. **Mutable since
    // 0.16.0**: `/config` writes the file the session is reading, and a turn
    // built from the configuration as it was at startup would contradict the
    // surface that just said the value changed.
    mut config: Config,
    spec: ProviderSpec,
    store: Store,
    mut session: Session,
    policy: Policy,
    diff_style: settings::DiffStyle,
    keys: io_cli::keys::Keys,
    notices: Vec<String>,
    report: Vec<String>,
    templates: Templates,
    theme: Theme,
    plain: bool,
    containment: Option<io_harness::Containment>,
    // Mutable for the reason `config` is, and it is the half a reload forgets:
    // this is derived from `config` ONCE at startup, so refreshing only the
    // `Config` would leave every `[app.io-cli]` answer stale while the rest of
    // the session moved on.
    mut capabilities: io_cli::contract::Capabilities,
    // Mutable from 0.19.0, and for a reason that did not exist before it: this is
    // the list the `/` palette offers, walked once at startup, and until this
    // release the directory behind it only ever changed out of band. `/skills`
    // now turns one off and on from inside the session, so a list that stayed put
    // would go on offering a skill the model's catalogue no longer has — and
    // `read_skill` would refuse it by name.
    mut skills: Vec<io_cli::skillview::Listed>,
    // The named profile in force. `--profile` seeds it and `/profile` replaces
    // it; it is re-applied after every turn-boundary reload, which goes back to
    // the file and knows nothing about either.
    mut profile: Option<String>,
    model: String,
) -> Result<(), String> {
    // Every request the session makes goes past this, and it is the only way
    // io-cli can say what is in the model's window: io-harness enumerates none
    // of it — the composer is private and the event announcing a composed prompt
    // carries a byte count with no text — while the request it hands the caller
    // carries the system block, the tool catalogue and the messages as public
    // fields. The MAKER is wrapped rather than the provider it makes, so a
    // `/model` switch keeps reporting rather than quietly reverting to a
    // provider nothing is watching.
    let seen = io_cli::context::Seen::default();
    let make = io_cli::provider::watching(make, seen.clone());
    // Built here rather than handed in, so there is one place a provider comes
    // from and `/model` cannot drift from startup.
    let mut provider = make(&model)?;
    let mut app = App::new(theme, model);
    app.set_diff_style(diff_style);
    // Before anything is drawn and before the first keystroke, so the keys are
    // never briefly the defaults, and so the notices are the first thing in the
    // scrollback rather than something that appears under an answer.
    app.set_keys(keys);
    // **Committed, not said, and through 0.13.1 this was `App::say` — which
    // replaces.** A startup notice is not the answer to a keystroke: nobody has
    // pressed anything yet, and the footer's line is gone at the first key that
    // is. Worse than misplaced, `say` *replaces*: six things can put a sentence
    // in this list — a section io-harness could not read, a keybinding naming no
    // action, a templates directory that would not walk, a skills directory that
    // would not either, a server named in both scopes, and this release's
    // `max_steps` deprecation — and saying them in a loop shows the last one and
    // silently drops every earlier one. Two of those six are new in 0.14.0, so a
    // file with several things wrong with it is exactly the file that lost the
    // most. What the session refused has to survive until the operator reads it,
    // so each takes a row of its own in the scrollback — which is what the
    // comment above has claimed since it was written.
    // **Recorded, not said, and first.** What `home::adopt` did is a fact about
    // where this install's files now are: a line naming each file it moved, and on
    // an ordinary run the one line saying where they live. `say` would put it on
    // the footer's row, where the first keystroke replaces it — and a migration
    // that happened once, on an upgrade, would be gone before the operator who has
    // to know about it pressed anything. It goes above the notices because it says
    // which directory the file those notices are about was read from.
    for line in report {
        app.record(Tone::Muted, line);
    }
    for notice in notices {
        app.record(Tone::Warning, notice);
    }
    // Said once, before anything is drawn or any turn starts. Everything the mode
    // governs — the indicator, and the state words that go to scrollback in its
    // place — is downstream of this one call.
    app.set_plain(plain);
    // Asked of the session rather than threaded down from `run`, so there is one
    // answer to "which workspace is this" and it is the harness's.
    app.set_root(session.root());
    // **The ceilings are on the line before the first prompt, not after the first
    // turn.** This release's whole claim is that the file an operator wrote is the
    // session they get, and a session that showed no budget until a turn had
    // already spent against one would be answering that question late — the
    // moment to learn a conversation is capped at forty steps is before typing
    // into it, not afterwards.
    //
    // Built from the same builder every turn uses, and bound `opening` so that
    // `tests/contract.rs` can name all three call sites and still fail a fourth:
    // recomposing the ceilings from `Config` here instead would be a second answer
    // to the precedence question F1 exists to keep single, and it would drift the
    // first time a layer moved. The goal is empty because nothing runs this one.
    let (answerer, _opening_questions) = io_cli::intent::channel();
    let opening = io_cli::contract::session(
        String::new(),
        session.root().to_path_buf(),
        &config,
        &capabilities,
        std::sync::Arc::new(answerer),
        None,
    );
    app.status.budgets = io_cli::status::Budgets::in_force(&opening);
    // What the file already says, read back rather than assumed. `None` means the
    // file holds a policy that is none of the three, which io-harness's own
    // configuration can express and this session must not relabel.
    app.set_posture(Posture::of(&policy.defaults));
    // Said once, before the first prompt, and only where there is something to
    // say. A contained turn is a different turn — it is the only one that reaches
    // io-harness's spawn loop — and a session that silently switched into it
    // would be one whose agents started costing tokens with nothing said.
    // `contained` starts true because configuring caps is the asking; `/contain
    // off` is how a turn is taken back.
    let mut contained = containment.is_some();
    // **Off, and off is not a missing feature.** Registering a plan gate is the
    // whole condition for io-harness's planning phase, and while it is on every
    // write and every exec is denied until somebody approves a proposal. Through
    // 0.10.0 and 0.11.0 this rode `[app.io-cli.containment]`, so configuring a
    // fan-out silently made every turn stop and plan first. It is the operator's
    // switch now, and nothing turns it on but `/plan on`.
    let mut planning = false;
    // One `/compact` typed at an idle prompt, spent by the next turn that starts.
    let mut fold_next = false;
    // **The file, held so it can be read again — which through 0.17.0 it never
    // was.** io-harness composes the instruction files inside `Config::discover`
    // and stores the result privately; there is no `Config::reload`, so a
    // `Config` is exactly as old as the call that made it. A repository whose
    // `AGENTS.md` changed mid-session therefore reached no turn at all, and
    // 0.18.0 adds `/remember`, which writes those very files — turning a
    // papered-over annoyance into a surface that would lie about its own effect.
    //
    // Built from the `Config` the driver already holds rather than by
    // discovering again here, which is [`io_cli::reload::Configuration::new`]'s
    // own rule: `run` applies `--profile` before anything reads the
    // configuration, and a fresh discovery inside the constructor would silently
    // drop that overlay.
    //
    // `settings::stored` is a pure function of that same value, so asking it a
    // second time is the same answer rather than a second one — and the notice
    // it can carry has already been disclosed by `drive`, which is where that
    // read is documented as *the read that discloses*.
    let (settings_in_force, _) = settings::stored(&config);
    let mut configuration = io_cli::reload::Configuration::new(
        session.root().to_path_buf(),
        config.clone(),
        settings_in_force,
    );
    if let Some(caps) = &containment {
        let notice = settings::contained_notice(caps, app.theme.glyphs.dash);
        app.say(Tone::Muted, notice);
    }
    // **The session no longer keeps a clock, because nothing shows one.** The
    // clock on screen belongs to the turn — it starts at zero when one starts and
    // stops where it stopped — so the reading a session-long `Instant` gave was
    // `22m12s` beside a turn six seconds old. Each turn is handed its own.
    let mut picker: Option<(Picker, Pick)> = None;

    // **The import offer, made once to everybody and never twice.**
    //
    // The gate is a marker in io's own home and NOT `provider_spec().is_none()`,
    // which is what the wizard uses: that condition is only ever true for an
    // operator who has configured nothing, so an offer behind it would reach
    // nobody who upgraded into this release — every existing operator would be
    // excluded from the one feature written for them.
    //
    // The marker is written here, as the offer is *made*, so declining costs one
    // keystroke and is remembered. `Esc` closes the picker into the session rather
    // than out of the program, which is the other half of the same promise: the
    // wizard's cancel returns `None` and `main` turns that into an exit, and an
    // operator who did not want to import has not said they did not want to run.
    //
    // Nothing is drawn when nothing was found, because a surface that opens to say
    // "no other agent is installed" is a surface charging every first run for a
    // question that had no answer.
    // **Painted first, and this ordering is load-bearing rather than tidy.**
    // Detection reads and decodes `~/.claude.json`, which is tens of megabytes on
    // a real install, and walks `~/.claude/plugins` to depth eight following
    // symlinks. Doing that above the first paint would hold a blank terminal for
    // the length of that work on every operator's first launch after upgrading —
    // the one launch where a session that looks hung is least explicable.
    paint(screen, &mut app)?;

    if !io_cli::home::import_offered() {
        if let Some(home) = io_cli::home::path() {
            let found = io_cli::import::detect(
                &io_cli::home::expand(std::path::Path::new("~")),
                session.root(),
            );
            let items = io_cli::import::plan(&found, &home, io_harness::config::Scope::User);
            if !items.is_empty() {
                for source in &found {
                    app.record(Tone::Muted, source.says());
                }
                app.record(
                    Tone::Muted,
                    "io found work you have already done in another agent. Nothing is written \
                     until you switch an item on and choose the last row; `Esc` leaves it all \
                     alone and `/import` opens this again."
                        .to_string(),
                );
                let accepted = vec![false; items.len()];
                picker = Some((
                    Picker::new("Import", import_rows(&items, &accepted, session.root())),
                    Pick::Import { items, accepted },
                ));
            }
            // Written whatever happened above, including when nothing was found:
            // the question has been asked and answered, and a machine that grows a
            // `~/.claude` next week should not be interrupted for it mid-session.
            if let Err(error) = io_cli::home::mark_import_offered() {
                app.record(
                    Tone::Muted,
                    format!("io could not record that it offered to import: {error}"),
                );
            }
        }
    }

    paint(screen, &mut app)?;

    loop {
        let Some(event) = inputs.recv().await else {
            return Ok(());
        };
        let Event::Key(key) = event else {
            if let Event::Resize(width, height) = event {
                // 0.13.0 — the palette is drawn in the viewport the session
                // already has, so a resize under an open palette is a resize like
                // any other and there is nothing to take back. Up to 0.12.0 this
                // arm closed the palette and re-placed the viewport, because the
                // palette was the one surface that had grown it.
                screen
                    .resize(width, height)
                    .map_err(|error| error.to_string())?;
            }
            if let Event::Paste(text) = event {
                match app.paste(&text, picker.is_some()) {
                    io_cli::app::Pasted::Picture(paths) => {
                        for path in paths {
                            paste_picture(&mut app, &mut session, &provider, &policy, &path);
                        }
                    }
                    io_cli::app::Pasted::Text | io_cli::app::Pasted::Refused => {}
                }
            }
            paint(screen, &mut app)?;
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // A picker owns the keyboard while it is open, which is what makes it a
        // modal overlay rather than a suggestion.
        if let Some((open, kind)) = picker.as_mut() {
            match open.key(key) {
                Outcome::Chosen(index) => {
                    let label = open.rows()[index].label.clone();
                    // Every other surface closes on a choice, and the assignment
                    // below is unconditional. A completion that descends is the
                    // one that replaces itself instead, so its next picker is
                    // built here — while `kind` still borrows `picker` — and
                    // installed the moment that borrow ends.
                    let mut descended = None;
                    match kind {
                        Pick::Theme => {
                            if let Some(chosen) = Theme::by_name(&label) {
                                // Resolved, not assigned — the third and last
                                // place a theme reaches a session, and the one
                                // that would otherwise let `/theme` bring colour
                                // back into a run the environment asked to be
                                // uncoloured. The file lost to `NO_COLOR`
                                // already, and 0.6.0 made the wizard's picker
                                // lose to it too; a mid-session picker that still
                                // won would make the variable mean *until you
                                // touch anything*.
                                let applied = Theme::from_env(Some(chosen.name), app.theme.glyphs);
                                app.theme = applied;
                                app.events.set_theme(applied);
                                // The sentence has to say which of the two
                                // happened. Under the variable the choice is
                                // real but invisible, and reporting it the same
                                // way would describe a change the operator
                                // cannot see anywhere on their screen.
                                app.say(
                                    Tone::Muted,
                                    if applied.coloured {
                                        format!(
                                            "theme {label} for this session; `io setup` to keep it"
                                        )
                                    } else {
                                        format!(
                                            "theme {label} chosen; NO_COLOR is set, so this session stays uncoloured"
                                        )
                                    },
                                );
                            }
                        }
                        // The switch is the provider and nothing else. The
                        // conversation is held by the `Session`, which is not
                        // involved, so there is no context to lose — that is a
                        // property of io-harness taking the provider per turn
                        // rather than per session, not something this does
                        // carefully.
                        Pick::Model => match make(&label) {
                            Ok(built) => {
                                provider = built;
                                app.status.model = label.clone();
                                app.say(Tone::Muted, format!("{label}, from the next turn"));
                            }
                            Err(error) => app.say(
                                Tone::Error,
                                format!("{label} could not be reached: {error}"),
                            ),
                        },
                        // The index resolves in the library, not here. This file
                        // is `[[bin]] name = "io"` and nothing links it, so an
                        // `ids.get(index)` written inline was a lookup no
                        // sabotage could reach — `sessions::pick` is the same
                        // line where a test can stand on it.
                        Pick::Resume(ids) => match io_cli::sessions::pick(ids, index) {
                            Some(id) => match io_cli::sessions::resume(&store, id) {
                                Ok(reopened) => {
                                    session = reopened;
                                    app.set_root(session.root());
                                    // The tokens, the context, the containment
                                    // and the plan were facts about the run just
                                    // left. Carrying them across would leave the
                                    // line describing a conversation that is no
                                    // longer on screen.
                                    app.status.forget_run();
                                    app.forget_fleet();
                                    // A window that outlives its conversation
                                    // describes a turn the operator has left.
                                    seen.forget();
                                    // Where they were, in the terminal's own
                                    // buffer rather than in a four-row viewport.
                                    commit_transcript(screen, &session, &store, &app.theme)?;
                                    app.say(
                                        Tone::Success,
                                        format!("resumed {}", session.root().display()),
                                    );
                                }
                                Err(error) => app.say(
                                    Tone::Error,
                                    format!("that session could not be reopened: {error}"),
                                ),
                            },
                            // The cut note, chosen. It carries no id, and until
                            // 0.7.0 it was last and effectively unreachable — but
                            // the filter ranks it against the session rows, so a
                            // query can put it under the marker and `Enter` on it
                            // closed the picker and did nothing, with nothing
                            // said. A picker that vanishes after a choice and
                            // leaves no trace is indistinguishable from a resume
                            // that failed, so the row answers for itself.
                            None => app.say(
                                Tone::Muted,
                                "that row is the note, not a session — older sessions are \
                                 not listed",
                            ),
                        },
                        // The turn number in the sentence comes back with the id
                        // rather than being computed here a second time: it is
                        // the same index, and the operator is being told which
                        // turn they are now continuing from.
                        Pick::Fork(ids) => match io_cli::sessions::pick_turn(ids, index) {
                            Some((turn, number)) => match session.branch_from(&store, turn) {
                                Ok(()) => {
                                    app.status.forget_run();
                                    app.forget_fleet();
                                    // A window that outlives its conversation
                                    // describes a turn the operator has left.
                                    seen.forget();
                                    app.say(
                                        Tone::Success,
                                        format!(
                                            "continuing from turn {number}; what came after is \
                                             still in the transcript, marked as branched away"
                                        ),
                                    )
                                }
                                Err(error) => app.say(
                                    Tone::Error,
                                    format!("that turn could not be branched from: {error}"),
                                ),
                            },
                            // Unreachable rather than impossible: `/fork` has no
                            // note row, so every row is a turn. Said rather than
                            // swallowed for the reason above — a choice that
                            // silently does nothing is the one outcome an
                            // operator cannot tell from a failure.
                            None => {
                                app.say(Tone::Muted, "that row is not a turn in this conversation")
                            }
                        },
                        // Typed, not run. The command goes into the prompt and
                        // the operator presses `Enter` on it themselves, so the
                        // submit path below — `strip_prefix('/')`, then
                        // `commands::parse` — stays the only way a command is
                        // dispatched. A template follows the same rule, which is
                        // why one arm answers both. `None` for the same reason
                        // `Pick::Resume` uses an `if let`: an index with nothing
                        // behind it puts nothing in the prompt rather than a
                        // guess.
                        // A chosen setting FILLS the composer rather than
                        // writing anything, which is the palette's own idiom and
                        // the right one here: the operator sees the key they are
                        // about to change, types the value themselves, and
                        // presses Enter. A picker that wrote on a keystroke would
                        // change a file on the way past.
                        // **Choosing an item switches it on; only the last row
                        // writes.** That separation is the release's central
                        // promise and it is enforced here rather than in
                        // `import::apply`: an operator who backs out with `Esc`
                        // at any point has changed no file, and one who reaches
                        // the write row has seen every destination first.
                        //
                        // Rebuilt through `descended` rather than `picker`, for
                        // the reason the `Pick::SkillToggle` note above gives —
                        // the end of this match closes the surface
                        // unconditionally, so a picker assigned here would be
                        // built and thrown away. `selecting` puts the cursor back
                        // on the row just toggled, so a run of items can be
                        // switched on without hunting for the place each time.
                        Pick::Import { items, accepted } => {
                            if index < items.len() {
                                // A row that can never write is a finding io is
                                // reporting, not a choice it is offering, so a
                                // press on it changes nothing rather than arming a
                                // box that means nothing. The surface draws those
                                // rows with `(i)` instead of a checkbox.
                                if import_writes(&items[index]) {
                                    accepted[index] = !accepted[index];
                                }
                                descended = Some((
                                    Picker::new(
                                        "Import",
                                        import_rows(items, accepted, session.root()),
                                    )
                                    .selecting(index),
                                    Pick::Import {
                                        items: items.clone(),
                                        accepted: accepted.clone(),
                                    },
                                ));
                            } else {
                                let chosen: Vec<io_cli::import::Item> = items
                                    .iter()
                                    .zip(accepted.iter())
                                    .filter(|(_, on)| **on)
                                    .map(|(item, _)| item.clone())
                                    .collect();
                                match (chosen.is_empty(), io_cli::home::path()) {
                                    (true, _) => app.record(
                                        Tone::Muted,
                                        "nothing was imported and nothing was written; \
                                         `/import` offers the same list again"
                                            .to_string(),
                                    ),
                                    (false, None) => app.record(
                                        Tone::Refused,
                                        "io has no home directory of its own, so there is \
                                         nowhere to import into"
                                            .to_string(),
                                    ),
                                    (false, Some(_)) => {
                                        // **A model forces one more question
                                        // before anything is written.** A foreign
                                        // configuration names a model and never a
                                        // vendor, and `[[provider]]` needs both —
                                        // so which endpoint answers for it is the
                                        // operator's to say, not io-cli's to
                                        // guess. Guessing would write a provider
                                        // that resolves, authenticates against the
                                        // wrong account and fails on the first
                                        // turn.
                                        if chosen
                                            .iter()
                                            .any(|item| item.kind == io_cli::import::Kind::Model)
                                        {
                                            descended = Some((
                                                Picker::new(
                                                    "Which provider answers for that model?",
                                                    vec![
                                                        Row::new("OpenRouter"),
                                                        Row::new("Anthropic"),
                                                        Row::new("OpenAI"),
                                                        Row::new(
                                                            "write everything else and leave \
                                                             the model alone",
                                                        ),
                                                    ],
                                                ),
                                                Pick::ImportModel(chosen),
                                            ));
                                        } else {
                                            // Every destination, named. That list
                                            // is what makes an import undoable by
                                            // hand, which is the only undo there
                                            // is.
                                            for (tone, line) in
                                                import_written(&chosen, session.root(), None)
                                            {
                                                app.record(tone, line);
                                            }
                                        }
                                        // The file on disk has moved under the
                                        // session, so the session reads it again
                                        // rather than trusting what it just wrote
                                        // — the same round trip every other writer
                                        // here makes, and the place a refusal by
                                        // io-harness would finally show.
                                        match io_cli::configure::reload(session.root()) {
                                            Ok((fresh, stored)) => {
                                                capabilities =
                                                    io_cli::contract::Capabilities::stored(
                                                        stored.as_ref(),
                                                    );
                                                config = fresh;
                                            }
                                            Err(error) => app.record(
                                                Tone::Error,
                                                format!(
                                                    "the import was written but the \
                                                     configuration would not read back: {error}"
                                                ),
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                        // The endpoint question, answered. The last row declines
                        // it, and declining writes everything else rather than
                        // abandoning the import — the model was one item among
                        // several and the operator already accepted the others.
                        Pick::ImportModel(chosen) => {
                            let endpoint = match index {
                                0 => Some(io_cli::providers::Endpoint::OpenRouter),
                                1 => Some(io_cli::providers::Endpoint::Anthropic),
                                2 => Some(io_cli::providers::Endpoint::OpenAi),
                                _ => None,
                            };
                            for (tone, line) in import_written(chosen, session.root(), endpoint) {
                                app.record(tone, line);
                            }
                            match io_cli::configure::reload(session.root()) {
                                Ok((fresh, stored)) => {
                                    capabilities =
                                        io_cli::contract::Capabilities::stored(stored.as_ref());
                                    config = fresh;
                                }
                                Err(error) => app.record(
                                    Tone::Error,
                                    format!(
                                        "the import was written but the configuration would \
                                         not read back: {error}"
                                    ),
                                ),
                            }
                        }
                        Pick::Mcp => {
                            let list = io_cli::servers::servers(&config, &app.servers);
                            if let Some(server) = list.get(index) {
                                app.record(
                                    Tone::Muted,
                                    format!(
                                        "{} — {} · {} · configured in the {} scope. \
                                         A change takes effect on the next turn.",
                                        server.id,
                                        server.state.word(),
                                        server.transport,
                                        server.decided.word(),
                                    ),
                                );
                                // **The position is read out of the file's own
                                // bytes, never from this row's index.** A row here
                                // is a merged, filtered view across three scopes;
                                // the `[[mcp]]` array it would be spliced into is
                                // a different list entirely. Handing one list's
                                // index to the other is precisely the silent wrong
                                // delete 0.20.0 shipped in `pluginview::rows`, and
                                // `servers::At` exists so it cannot be spelled.
                                match io_cli::servers::declared_at(server) {
                                    None => app.record(
                                        Tone::Muted,
                                        "no configuration file in force declares this server, \
                                         so there is nothing here to change"
                                            .to_string(),
                                    ),
                                    Some(at) => {
                                        descended = Some((
                                            Picker::new(
                                                format!("{}?", server.id),
                                                vec![
                                                    Row::new("leave it as it is"),
                                                    Row::new("remove this server"),
                                                ],
                                            ),
                                            Pick::McpRemove {
                                                id: server.id.clone(),
                                                at,
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                        // Row 0 is "leave it", the default that does nothing —
                        // the shape `/skills` and `/plugin` both already use.
                        Pick::McpRemove { id, at } => {
                            if index == 1 {
                                let edit = io_cli::servers::remove(at);
                                match io_cli::configure::write(session.root(), at.scope, &[edit]) {
                                    Ok(()) => {
                                        match io_cli::configure::reload(session.root()) {
                                            Ok((fresh, stored)) => {
                                                capabilities =
                                                    io_cli::contract::Capabilities::stored(
                                                        stored.as_ref(),
                                                    );
                                                config = fresh;
                                            }
                                            Err(error) => app.record(
                                                Tone::Error,
                                                format!(
                                                    "{id} was removed but the configuration \
                                                     would not read back: {error}"
                                                ),
                                            ),
                                        }
                                        app.record(
                                            Tone::Success,
                                            format!(
                                                "{id} is no longer configured; the next turn \
                                                 talks to it no more",
                                            ),
                                        );
                                    }
                                    Err(refusal) => app.record(Tone::Refused, refusal),
                                }
                            }
                        }
                        // Says what the skill is and offers the one change there
                        // is. The view is read again rather than carried: between
                        // the row being drawn and this keystroke the operator may
                        // have moved the file themselves, and a carried row would
                        // then name a path that is no longer there.
                        Pick::PluginEntry {
                            id,
                            bundle,
                            action_at,
                        } => {
                            // Every row but one is a fact to read.
                            if index == *action_at {
                                match io_cli::pluginview::declared_at(session.root(), bundle) {
                                    Some((scope, at)) => {
                                        descended = Some((
                                            Picker::new(
                                                format!("Stop loading {id}?"),
                                                vec![
                                                    Row::new("leave it".to_string()),
                                                    Row::with_detail(
                                                        "stop loading it".to_string(),
                                                        format!(
                                                            "removes `plugin[{at}]` from the {} \
                                                             scope; the directory is left alone",
                                                            io_cli::configure::Decided::File {
                                                                scope,
                                                                path: Default::default(),
                                                            }
                                                            .word()
                                                        ),
                                                    ),
                                                ],
                                            ),
                                            Pick::PluginRemove {
                                                id: id.clone(),
                                                scope,
                                                index: at,
                                            },
                                        ));
                                    }
                                    // **Said, not guessed at.** No file names this
                                    // path, so there is no entry to remove — and
                                    // removing whichever entry happened to sit at
                                    // some other index would take a bundle the
                                    // operator never mentioned, silently.
                                    None => app.record(
                                        Tone::Refused,
                                        format!(
                                            "no `[[plugin]]` entry in any scope names {}; \
                                             nothing was removed",
                                            bundle.display()
                                        ),
                                    ),
                                }
                            }
                        }
                        // `at` and not `index`: the outer `index` is the row the
                        // operator chose, and the entry's own position is a
                        // different number entirely. Binding both to one name is
                        // how a confirmation removes the wrong bundle.
                        Pick::PluginRemove {
                            id,
                            scope,
                            index: at,
                        } => {
                            // Row 0 is "leave it", which is the default and does
                            // nothing — the same shape `/skills`' toggle uses.
                            if index == 1 {
                                let edit = io_cli::pluginview::remove(*at);
                                match io_cli::configure::write(session.root(), *scope, &[edit]) {
                                    Ok(()) => {
                                        // The directory is untouched on purpose:
                                        // this surface edits a configuration file,
                                        // and deleting an operator's directory
                                        // because they stopped loading it is not a
                                        // thing a list should do.
                                        app.record(
                                            Tone::Success,
                                            format!(
                                                "{id} is no longer declared; its directory is \
                                                 untouched, and the change is in force from the \
                                                 next turn",
                                            ),
                                        );
                                    }
                                    Err(refusal) => app.record(Tone::Refused, refusal),
                                }
                            }
                        }
                        Pick::Plugins(view) => {
                            // **Answered from the reading the rows were drawn
                            // from, and `/skills` does the opposite on purpose.**
                            // There, a row names a file the operator may have
                            // moved between two keystrokes, so the directory is
                            // read again and a row that moved is refused. Here the
                            // rows describe what is *loaded in this session* — the
                            // bundle a turn taken right now would actually use —
                            // and re-reading would answer a different question
                            // than the one the operator asked by choosing a row.
                            //
                            // The split is `pluginview::rows`'s own: loaded first,
                            // refused after, no headings, so the index is direct.
                            if let Some(plugin) = view.plugins.get(index) {
                                // `descended`, not `picker`: the assignment at the
                                // end of this match installs it, while `kind`
                                // still borrows the one being replaced.
                                //
                                // **A pane rather than a line in the scrollback,
                                // and `/mcp` above does the opposite for a good
                                // reason that does not hold here.** A server is
                                // one row of facts, so a sentence says all of it.
                                // A bundle is several *lists* — its agents, its
                                // servers, its policy layers — and a sentence
                                // holding three lists is a sentence nobody reads.
                                let mut rows = io_cli::pluginview::detail(
                                    plugin,
                                    screen.width(),
                                    &app.theme.glyphs,
                                );
                                // **The action's index is taken before the row is
                                // pushed, never worked out afterwards.** Every
                                // other index in this surface addresses a list
                                // somewhere else, and the whole class of defect
                                // here is a row number being read against the
                                // wrong list — so this one is the length of what
                                // was already there, which cannot be wrong.
                                let action_at = rows.len();
                                rows.push(Row::with_detail(
                                    "stop loading this bundle".to_string(),
                                    "removes its `[[plugin]]` entry".to_string(),
                                ));
                                descended = Some((
                                    Picker::new(plugin.id.clone(), rows),
                                    Pick::PluginEntry {
                                        id: plugin.id.clone(),
                                        bundle: plugin.root.clone(),
                                        action_at,
                                    },
                                ));
                            } else if let Some(refused) =
                                view.refused.get(index - view.plugins.len())
                            {
                                // **`Tone::Refused` and io-harness's own sentence,
                                // re-worded by nobody.** This is the one an
                                // operator will actually hit — a bundle declared
                                // in the project file that contributes hooks or
                                // servers is refused whole — and the harness's
                                // message is the only text that names both files
                                // it could move to instead. `record` rather than
                                // `say`: a refusal explains a boundary and outlives
                                // the keystroke that earned it.
                                app.record(
                                    Tone::Refused,
                                    format!(
                                        "{} ({}): {}",
                                        refused.id,
                                        refused.path.display(),
                                        refused.error,
                                    ),
                                );
                            }
                        }
                        Pick::Skills(drawn) => {
                            // **Located by what the row said, not by where it
                            // sat.** The list is read again — the operator may
                            // have moved a file in another pane between drawing
                            // the rows and choosing one — and an index carried
                            // across two different readings of a directory names
                            // whichever skill happens to be in that position now.
                            // Getting a *different* skill than the row you read,
                            // silently, is worse than being told it moved.
                            let drawn = drawn.get(index).cloned();
                            let view = skills_view(&config, &capabilities, session.root());
                            let found = drawn.as_ref().and_then(|(name, path)| {
                                view.skills
                                    .iter()
                                    .find(|skill| &skill.name == name && &skill.path == path)
                            });
                            if found.is_none() && drawn.is_some() {
                                app.record(
                                    Tone::Muted,
                                    "that skill is not where it was a moment ago; \
                                     open `/skills` again",
                                );
                            }
                            if let Some(skill) = found {
                                app.record(
                                    Tone::Muted,
                                    format!(
                                        "{} — {} · {} · {} · {}",
                                        skill.name,
                                        skill.description,
                                        skill.origin.word(),
                                        if skill.enabled { "enabled" } else { "disabled" },
                                        skill.path.display(),
                                    ),
                                );
                                let verb = if skill.enabled {
                                    "turn it off"
                                } else {
                                    "turn it back on"
                                };
                                // `descended`, not `picker`: the assignment at
                                // the end of this match is unconditional, so a
                                // second surface opened here would be built and
                                // then thrown away. The same replace-in-place
                                // `Pick::Complete` and `Pick::Remembered` use.
                                descended = Some((
                                    Picker::new(
                                        format!("{}?", skill.name),
                                        vec![Row::new(verb), Row::new("leave it as it is")],
                                    ),
                                    Pick::SkillToggle {
                                        name: skill.name.clone(),
                                        path: skill.path.clone(),
                                        enabled: skill.enabled,
                                    },
                                ));
                            }
                        }
                        // **A rename, and only ever a rename.** A copy would leave
                        // one name resolving in both directories, and two skills
                        // answering to one name is an `Err` from `Skills::discover`
                        // that io-harness propagates at run start — every turn of
                        // the session dead. So the failure of a move is said, and
                        // nothing is written twice.
                        Pick::SkillToggle {
                            name,
                            path,
                            enabled,
                        } => {
                            if index != 0 {
                                app.record(Tone::Muted, format!("{name} is unchanged"));
                            } else {
                                // **The bundle list goes to the move, not just to
                                // the listing.** A row drawn from a bundle is
                                // refused inside `disable`/`enable` rather than
                                // being filtered out here, because a guard at the
                                // call site only guards the call site: the
                                // destination is computed from the file's own
                                // parent, so a bundle path reaching the move
                                // creates `disabled/` inside somebody else's
                                // bundle and takes their file into it.
                                let bundles = bundle_skills(&config);
                                let moved = if *enabled {
                                    io_cli::skillview::disable(path, &bundles)
                                } else {
                                    io_cli::skillview::enable(path, &bundles)
                                };
                                match moved {
                                    Ok(to) => {
                                        // **The palette's list is walked once at
                                        // startup and this is the only thing in
                                        // the product that changes the directory
                                        // under it.** Leave it and `/` goes on
                                        // offering a skill the model's catalogue
                                        // no longer has, whose `read_skill` the
                                        // harness refuses by name — or hides one
                                        // that has just come back.
                                        skills = io_cli::commands::skills(
                                            &io_cli::home::path().unwrap_or_default(),
                                            io_cli::contract::skills_dir(
                                                &config,
                                                &capabilities,
                                                session.root().to_path_buf(),
                                            )
                                            .as_deref(),
                                            // Re-read rather than carried: the
                                            // operator may have added or removed a
                                            // `[[plugin]]` entry through `/plugin`
                                            // since this session started, and a
                                            // carried list would go on offering a
                                            // bundle's skills after the bundle was
                                            // removed.
                                            &bundle_skills(&config),
                                        )
                                        .0;
                                        app.record(
                                            Tone::Muted,
                                            format!(
                                                "{name} is now {} — {}. The next turn is composed \
                                                 from the directory as it is then, so this is in \
                                                 force immediately.",
                                                if *enabled { "off" } else { "on" },
                                                to.display(),
                                            ),
                                        );
                                    }
                                    Err(why) => app.record(Tone::Refused, why),
                                }
                            }
                        }
                        // Says what the link is, and where it points. The chain
                        // is arranged through `/config`, which is the one writer
                        // this release gives the file.
                        Pick::Provider => {
                            let chain = io_cli::providers::chain(&config);
                            if let Some(entry) = chain.get(index) {
                                let place = if entry.index == 0 {
                                    "used".to_string()
                                } else {
                                    format!("fallback {}", entry.index)
                                };
                                app.record(
                                    Tone::Muted,
                                    format!(
                                        "{} · {} · {} · {}{}",
                                        entry.kind,
                                        entry.model,
                                        place,
                                        entry.credential.word(),
                                        entry
                                            .endpoint
                                            .as_deref()
                                            .map(|e| format!(" · {e}"))
                                            .unwrap_or_default(),
                                    ),
                                );
                                // The chain's order IS which model answers — the
                                // first `[[provider]]` entry is the provider and
                                // the rest are its fallbacks — so promoting is not
                                // cosmetic reordering, it is the switch. Read from
                                // the file rather than from `entry.index`, which
                                // counts the merged chain.
                                match io_cli::providers::declared_at(&config, entry) {
                                    None => app.record(
                                        Tone::Muted,
                                        "no configuration file in force declares this link, so \
                                         there is nothing here to change"
                                            .to_string(),
                                    ),
                                    Some(at) => {
                                        let first = entry.index == 0;
                                        let mut rows = vec![Row::new("leave it as it is")];
                                        if !first {
                                            rows.push(Row::new("make this the provider in force"));
                                        }
                                        rows.push(Row::new("remove this link from the chain"));
                                        descended = Some((
                                            Picker::new(format!("{}?", entry.model), rows),
                                            Pick::ProviderVerb {
                                                label: entry.model.clone(),
                                                at,
                                                first,
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                        // Rows are "leave it", then promote where it is offered,
                        // then remove — so the index of `remove` moves with
                        // `first`. Computed from the same flag the rows were built
                        // from rather than hard-coded, because those two drifting
                        // apart is how a list removes the thing it offered to
                        // promote.
                        Pick::ProviderVerb { label, at, first } => {
                            let promote = if *first { usize::MAX } else { 1 };
                            let remove = if *first { 1 } else { 2 };
                            let edit = if index == promote {
                                io_cli::providers::promote(at)
                            } else if index == remove {
                                Some(io_cli::providers::remove(at))
                            } else {
                                None
                            };
                            if let Some(edit) = edit {
                                match io_cli::configure::write(session.root(), at.scope, &[edit]) {
                                    Ok(()) => {
                                        match io_cli::configure::reload(session.root()) {
                                            Ok((fresh, stored)) => {
                                                capabilities =
                                                    io_cli::contract::Capabilities::stored(
                                                        stored.as_ref(),
                                                    );
                                                config = fresh;
                                            }
                                            Err(error) => app.record(
                                                Tone::Error,
                                                format!(
                                                    "the chain was written but the configuration \
                                                     would not read back: {error}"
                                                ),
                                            ),
                                        }
                                        app.record(
                                            Tone::Success,
                                            if index == promote {
                                                format!(
                                                    "{label} answers from the next turn; what \
                                                     was in force is now its fallback"
                                                )
                                            } else {
                                                format!(
                                                    "{label} is out of the chain from the next \
                                                     turn"
                                                )
                                            },
                                        );
                                    }
                                    Err(refusal) => app.record(Tone::Refused, refusal),
                                }
                            }
                        }
                        Pick::Profile(names) => {
                            if let Some(name) = names.get(index) {
                                match io_cli::configure::with_profile(&config, name) {
                                    Ok(overlaid) => {
                                        // The same both-halves rule a write
                                        // follows: a profile can carry
                                        // `[app.io-cli]` keys too.
                                        let (stored, _) = io_cli::settings::stored(&overlaid);
                                        capabilities =
                                            io_cli::contract::Capabilities::stored(stored.as_ref());
                                        config = overlaid;
                                        // **The name, kept.** The overlay above
                                        // is in force until the next turn
                                        // boundary re-reads the file, which
                                        // discovers it afresh and knows nothing
                                        // about a profile. Without this the
                                        // sentence below would be true for
                                        // exactly as long as nobody pressed
                                        // Enter.
                                        profile = Some(name.clone());
                                        app.record(
                                            Tone::Success,
                                            format!(
                                                "profile `{name}` is in force from the next \
                                                 turn; nothing was written"
                                            ),
                                        );
                                    }
                                    Err(refusal) => app.record(Tone::Error, refusal),
                                }
                            }
                        }
                        Pick::Config(paths) => {
                            if let Some(key) = paths.get(index) {
                                app.composer.set(&format!("/config {key} "));
                            }
                        }
                        Pick::ConfigScope { key, value, paths } => {
                            // An index past the end puts nothing in the file,
                            // which is the same answer every other picker arm
                            // gives a row it cannot resolve.
                            if let Some((scope, _)) = paths.get(index) {
                                {
                                    let root = session.root().to_path_buf();
                                    let edits =
                                        [io_cli::edit::Edit::set(key.clone(), value.clone())];
                                    match io_cli::configure::write(&root, *scope, &edits) {
                                        Ok(()) => {
                                            // Both halves, or the next turn runs
                                            // on what the file said at startup.
                                            match io_cli::configure::reload(&root) {
                                                Ok((fresh, stored)) => {
                                                    capabilities =
                                                        io_cli::contract::Capabilities::stored(
                                                            stored.as_ref(),
                                                        );
                                                    config = fresh;
                                                    app.record(
                                                        Tone::Success,
                                                        format!(
                                                            "{key} = {value}, written to the {} \
                                                             scope and in force from the next turn",
                                                            io_cli::configure::Decided::File {
                                                                scope: *scope,
                                                                path: Default::default(),
                                                            }
                                                            .word()
                                                        ),
                                                    );
                                                }
                                                Err(error) => app.record(Tone::Error, error),
                                            }
                                        }
                                        // io-harness's own sentence, re-worded by
                                        // nobody. `record` and not `say`: a
                                        // refusal explains a boundary and outlives
                                        // the keystroke that earned it.
                                        Err(refusal) => app.record(Tone::Refused, refusal),
                                    }
                                }
                            }
                        }
                        // The line goes into the file the row names, and nothing
                        // was written until now. An index past the end writes
                        // nothing, which is the answer every other picker arm
                        // gives a row it cannot resolve.
                        Pick::RememberScope { line, paths } => {
                            if let Some((scope, _)) = paths.get(index) {
                                let root = session.root().to_path_buf();
                                match io_cli::memory::remember(&root, *scope, line) {
                                    Ok(at) => {
                                        app.record(
                                            Tone::Success,
                                            format!("remembered in {}", at.display()),
                                        );
                                        // **Writing the file is not enough, and
                                        // this is where that is settled.**
                                        // `read_instructions` joins each name in
                                        // `[instructions] files` to the discovery
                                        // root and the default list is exactly
                                        // `["AGENTS.md"]`, so `AGENTS.local.md`
                                        // is read only where a file names it and
                                        // `IO.md` — which lives in io-cli's home,
                                        // not the workspace — is not reachable by
                                        // a bare name at all. A `/remember` that
                                        // wrote one of those two and stopped
                                        // would be a surface reporting a change
                                        // no run will ever see.
                                        match io_cli::memory::install(&root) {
                                            // It changed the configuration, so it
                                            // is said. A write into `io.toml` on
                                            // the operator's behalf is not
                                            // something to do silently.
                                            Ok(true) => app.record(
                                                Tone::Muted,
                                                "`[instructions] files` now names all three, so \
                                                 the next turn reads them",
                                            ),
                                            // Already exactly right. Not one byte
                                            // written and nothing to report — this
                                            // is reached from a command an
                                            // operator types repeatedly.
                                            Ok(false) => {}
                                            Err(error) => app.record(Tone::Error, error),
                                        }
                                    }
                                    // The line is the operator's own and the
                                    // refusal explains a boundary, so it outlives
                                    // the keystroke that earned it.
                                    Err(refusal) => app.record(Tone::Refused, refusal),
                                }
                            }
                        }
                        // The memory page. Every row stands for something the
                        // parallel `held` vector names — see `commands::Held` —
                        // rather than for a label read back, which the picker's
                        // own fitter may have shortened.
                        Pick::Memory { held } => match held.get(index) {
                            // An instruction file says what it is. There is
                            // nothing to open: the file is markdown an operator
                            // edits in their own editor, and `/remember` is the
                            // one writer this product gives it.
                            Some(io_cli::commands::Held::File(file)) => app.record(
                                Tone::Muted,
                                io_cli::commands::instruction_said(file, &app.theme.glyphs),
                            ),
                            // A note has two verbs on it and a picker has one
                            // Enter, so the row descends into them — the same
                            // replace-in-place `Pick::Complete` uses, and the
                            // same two-step `/config` takes to reach a scope.
                            Some(io_cli::commands::Held::Note { scope, key, pinned }) => {
                                descended = Some((
                                    Picker::new(
                                        format!("{key}?"),
                                        io_cli::commands::verb_rows(*pinned),
                                    ),
                                    Pick::Remembered {
                                        scope: *scope,
                                        key: key.clone(),
                                        verbs: io_cli::commands::Verb::of(*pinned).to_vec(),
                                    },
                                ));
                            }
                            // A heading, or an index past the end. A heading
                            // cannot be under the marker — `Picker::refilter`
                            // admits one only while nothing is typed and nothing
                            // can be chosen while one is there — so this is the
                            // structural answer rather than a case an operator
                            // reaches.
                            Some(io_cli::commands::Held::Nothing) | None => {}
                        },
                        // The verb, applied. Both wrappers report through
                        // `io_cli::commands`, so what an outcome is called is a
                        // decision a test can stand on rather than a string in a
                        // file nothing links.
                        Pick::Remembered { scope, key, verbs } => {
                            if let Some(verb) = verbs.get(index) {
                                let root = session.root().to_path_buf();
                                match verb {
                                    io_cli::commands::Verb::Pin | io_cli::commands::Verb::Unpin => {
                                        let want = matches!(verb, io_cli::commands::Verb::Pin);
                                        match io_cli::recall::pin(&store, &root, *scope, key, want)
                                        {
                                            Ok(outcome) => {
                                                let (tone, said) = io_cli::commands::pinned_said(
                                                    key, *scope, want, outcome,
                                                );
                                                app.record(tone, said);
                                            }
                                            Err(error) => app.record(
                                                Tone::Error,
                                                format!("{key} was not changed: {error}"),
                                            ),
                                        }
                                    }
                                    io_cli::commands::Verb::Forget => {
                                        match io_cli::recall::forget(&store, &root, *scope, key) {
                                            Ok(outcome) => {
                                                let (tone, said) = io_cli::commands::forgotten_said(
                                                    key,
                                                    *scope,
                                                    outcome,
                                                    &app.theme.glyphs,
                                                );
                                                app.record(tone, said);
                                            }
                                            Err(error) => app.record(
                                                Tone::Error,
                                                format!("{key} was not withdrawn: {error}"),
                                            ),
                                        }
                                    }
                                }
                            }
                        }
                        Pick::Palette => match commands::palette_pick(&templates, &skills, index) {
                            Some(commands::Chosen::Command(command)) => app.composer.set(command),
                            // The rendered template, in the prompt and not on the
                            // wire. A template is a starting point for a goal
                            // rather than the goal itself, so the operator reads
                            // it, edits it and presses `Enter` themselves — the
                            // same rule the commands follow, for the same reason.
                            //
                            // A render that fails is said rather than swallowed.
                            // The one way it fails in this release is a
                            // placeholder with no argument, and the sentence is
                            // io-harness's own: it names the template, the
                            // placeholder and what to do about it, which is more
                            // than anything written here could.
                            Some(commands::Chosen::Template(name)) => {
                                match commands::expand(&templates, &name) {
                                    Ok(prompt) => app.composer.set(&prompt),
                                    Err(error) => app.say(Tone::Error, error),
                                }
                            }
                            // A skill goes into the prompt by NAME and nothing
                            // more. The body is the agent's to read — io-harness
                            // hands it the catalogue and it opens the file under
                            // the run's own policy — so a picker that pasted the
                            // instructions in would be this crate holding a copy
                            // of a skill, which is the one thing it must not do.
                            Some(commands::Chosen::Skill(name)) => {
                                app.composer.set(&commands::invoke_skill(&name));
                            }
                            None => {}
                        },
                        // A directory replaces the picker with the level below
                        // it, which is the whole of "one directory at a time":
                        // nothing walks a tree, and the operator pays for the
                        // level they asked to see. A file goes into the prompt
                        // as the path relative to the session root, at the
                        // cursor and beside whatever was already typed — the
                        // `@` never reached the composer, so what is inserted is
                        // the path and not a marker the model would have to
                        // learn. `None` is the row saying the list was cut.
                        //
                        // `paste` rather than the palette's `set`: `set`
                        // replaces the prompt, and a path belongs beside what
                        // was already typed rather than instead of it. A path
                        // long enough to be collapsed to a placeholder is still
                        // the path on the wire — `Composer::text` is what puts a
                        // paste back, and it does it on the submit path.
                        Pick::Complete(entries) => match complete::pick(entries, index) {
                            Some(complete::Picked::Insert(path)) => app.composer.paste(&path),
                            Some(complete::Picked::Descend(dir)) => {
                                // Read again rather than carried: the posture can
                                // have moved since the picker opened, and what is
                                // offered has to be what the next turn may read.
                                let effective = approval::session_policy(
                                    &policy,
                                    app.posture(),
                                    app.remembered(),
                                );
                                match completion(
                                    session.root(),
                                    &effective,
                                    &dir,
                                    &app.theme.glyphs,
                                ) {
                                    Ok(Some(open)) => descended = Some(open),
                                    Ok(None) => app
                                        .say(Tone::Muted, format!("nothing to complete in {dir}")),
                                    Err(error) => app.say(Tone::Error, error),
                                }
                            }
                            // The cut note is a row like any other now that a
                            // query can rank it, and it stands for no entry. A
                            // picker that closed on a choice and said nothing
                            // would be indistinguishable from a completion that
                            // failed — the same answer `/resume` gives its own
                            // note row.
                            None => app.say(
                                Tone::Muted,
                                "that row is the note, not a file — the rest of the \
                                 listing is not shown",
                            ),
                        },
                    }
                    // `None` in every arm but the descent, so this closes the
                    // picker exactly as it always did and replaces it in the one
                    // case that asked for it.
                    picker = descended;
                }
                Outcome::Cancelled => picker = None,
                Outcome::Idle => {}
            }
            paint_picker(screen, &mut app, picker.as_mut())?;
            continue;
        }

        // `/` at an empty prompt opens the palette, in front of the session
        // rather than inside it: the keystroke never reaches the composer, which
        // is what makes backing out leave the prompt exactly as it was, and
        // `App` goes on treating `/` as the ordinary character a hand-typed
        // `/theme` needs it to be. The condition is `commands::opens_palette`
        // rather than a test written here, because nothing can reach this file.
        if commands::opens_palette(key, app.composer.is_empty(), app.armed()) {
            let rows = commands::palette(&templates, &skills);
            // **0.13.0 — a repaint, and nothing else.** Up to 0.12.0 this grew the
            // viewport to hold every row and shrank it again on the way out, which
            // put a terminal round trip on the keystroke: `Screen::replace` asks
            // the terminal where its cursor is (`ESC[6n`) and takes the stdin lock
            // to read the answer, twice per visit to the palette. On a terminal
            // that does not answer, that is the two-second wait `term.rs` records
            // — on `/`, which is the fastest thing an operator does.
            //
            // What it costs is the whole-list view: the picker draws the rows the
            // session's viewport has and scrolls and filters for the rest, which
            // is what `/model` already does against four hundred models. That is
            // the trade the release contract records, and the fallback if it turns
            // out wrong is *not* to restore the round trip.
            picker = Some((Picker::new("Which command?", rows), Pick::Palette));
            paint_picker(screen, &mut app, picker.as_mut())?;
            continue;
        }

        // `@` at a word boundary opens the completion picker, in front of the
        // session for the same two reasons `/` is: the keystroke never reaches
        // the composer, so `Esc` leaves the prompt exactly as it was, and `App`
        // goes on treating `@` as the ordinary character an address needs it to
        // be. The condition is `complete::opens` rather than a test written here.
        //
        // The policy is the one this session is running under — the file's, with
        // the posture `Shift+Tab` chose folded in, and the rules the operator has
        // already answered `a` to. Built the same way the turn below builds it,
        // so what the picker offers and what the agent may read cannot differ.
        if complete::opens(key, &app.composer.text(), app.armed()) {
            let effective = approval::session_policy(&policy, app.posture(), app.remembered());
            match completion(session.root(), &effective, "", &app.theme.glyphs) {
                Ok(Some(open)) => picker = Some(open),
                // An empty root, or one the policy reads as empty. Said rather
                // than opened onto nothing, the same way `/resume` declines.
                Ok(None) => app.say(Tone::Muted, "nothing in this workspace to complete"),
                Err(error) => app.say(Tone::Error, error),
            }
            paint_picker(screen, &mut app, picker.as_mut())?;
            continue;
        }

        match app.key(key) {
            Command::None => {}
            Command::Exit => return Ok(()),
            // Nothing is running at an idle prompt, so there is nothing to stop.
            Command::Interrupt | Command::Abandon => {}
            Command::ClearViewport => {
                // The viewport, and nothing above it.
                paint(screen, &mut app)?;
            }
            Command::Transcript => commit_transcript(screen, &session, &store, &app.theme)?,
            Command::Attach(run_id) => {
                watch_child(screen, &mut app, &store, inputs, run_id).await?;
            }
            // The first `Esc`. Nothing has changed yet; this says what the second
            // one would change, in the turn's own words, so a confirmation is a
            // confirmation of something specific rather than of a keystroke.
            Command::ArmRewind => match io_cli::rewind::preview(&session, &store) {
                Some(about) => app.say(
                    Tone::Warning,
                    io_cli::rewind::armed_line(&about, &app.theme.glyphs),
                ),
                None => app.say(Tone::Muted, "there is no turn to undo"),
            },
            // The second. This is where the operator's files change.
            Command::Rewind => match io_cli::rewind::last_turn(&mut session, &store) {
                Ok(Some(undone)) => {
                    // The undone turn is where those numbers came from.
                    app.status.forget_run();
                    app.forget_fleet();
                    // As above: the last request belonged to the undone turn.
                    seen.forget();
                    for (tone, line) in io_cli::rewind::undone_lines(&undone, &app.theme.glyphs) {
                        app.say(tone, line);
                    }
                }
                Ok(None) => app.say(Tone::Muted, "there is no turn to undo"),
                Err(error) => app.say(Tone::Error, format!("nothing was undone: {error}")),
            },
            Command::Slash(text) => match commands::parse(&text, app.keys(), &app.theme) {
                Action::Print(lines) => {
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                Action::Quit => return Ok(()),
                Action::Setup => {
                    app.say(
                        Tone::Muted,
                        "run `io setup` from the shell to change the configuration",
                    );
                }
                Action::Mcp => {
                    let list = io_cli::servers::servers(&config, &app.servers);
                    if list.is_empty() {
                        // The "not configured" shape this product uses
                        // everywhere: an empty section is not an error.
                        app.record(
                            Tone::Muted,
                            "no MCP servers are configured; `/config` writes them",
                        );
                    } else {
                        picker = Some((
                            Picker::new("MCP servers", io_cli::servers::rows(&list)),
                            Pick::Mcp,
                        ));
                    }
                }
                // **Both directories, and a failed discovery is drawn rather than
                // swallowed.** `view` reads the enabled set through the same
                // `Skills::discover` the run makes, so what this lists is what the
                // model is offered — and when that call fails the harness's own
                // sentence goes into the scrollback, which is more than io-cli
                // knows how to say: on a duplicate name it names both files. The
                // rows still open afterwards, because the disabled set comes from
                // a directory discovery never looks at and is exactly where the
                // operator's next move probably is.
                Action::Skills => {
                    let view = skills_view(&config, &capabilities, session.root());
                    if let Some(sentence) = &view.failed {
                        app.record(Tone::Refused, sentence.clone());
                    }
                    // **This is the loudest thing this surface says, and it is
                    // the only place it can be said.** A bundle that declares a
                    // skills directory which is not on disk is not a cosmetic
                    // problem: `Plugin::skills_dir` does no existence check, the
                    // harness's `discover_skills` walks it with `?` before the
                    // first completion, and every turn of the session therefore
                    // ends with an error naming a path the operator never chose
                    // and never typed. Nothing else in io-cli connects that error
                    // to the bundle that caused it. So the sentence names the
                    // bundle, carries the harness's own words for the failure,
                    // and says plainly what it costs — and it is drawn even when
                    // the rows below it are fine, because the rows being fine is
                    // exactly what makes the dead session baffling.
                    for (id, sentence) in &view.bundles_failed {
                        app.record(
                            Tone::Refused,
                            format!(
                                "the {id} bundle contributes no skills: {sentence}. Every turn of \
                                 this session ends on that error until the directory exists or the \
                                 bundle's `[[plugin]]` entry is removed — `/plugin` removes it.",
                            ),
                        );
                    }
                    if view.skills.is_empty() {
                        if view.failed.is_none() {
                            // **Not "on the next start".** `install` already ran
                            // at the top of this one, so an empty list means it
                            // was skipped or could not write — and promising a
                            // fix on the next start promises a thing that will
                            // happen again exactly as it just did. Name the
                            // directory instead, which is the fact the operator
                            // can act on.
                            app.record(
                                Tone::Muted,
                                match io_cli::contract::skills_dir(
                                    &config,
                                    &capabilities,
                                    session.root().to_path_buf(),
                                ) {
                                    Some(dir) => {
                                        format!("no skills in {}", dir.display())
                                    }
                                    None => "no skills directory is in force".to_string(),
                                },
                            );
                        }
                    } else {
                        picker = Some((
                            Picker::new(
                                "Skills",
                                io_cli::skillview::rows(
                                    &view.skills,
                                    screen.width(),
                                    &app.theme.glyphs,
                                ),
                            ),
                            Pick::Skills(
                                view.skills
                                    .iter()
                                    .map(|skill| (skill.name.clone(), skill.path.clone()))
                                    .collect(),
                            ),
                        ));
                    }
                }
                Action::Plugin => {
                    // **Read fresh, every time, and that is io-harness's design
                    // rather than a choice made here.** `Config::plugins()`
                    // re-walks every declared directory on each call, so a bundle
                    // the operator has just edited on disk is reflected without a
                    // reload — and a bundle they have just broken shows up as a
                    // refusal in the same breath.
                    let view = io_cli::pluginview::view(&config);
                    // **The same call `/skills` makes, deliberately.** A bundle
                    // whose declared skills directory is absent ends every turn of
                    // the session, and both surfaces have to say so — but saying
                    // it twice from two implementations is how two surfaces come
                    // to disagree about one bundle. `skillview` owns the question
                    // "what did this bundle actually contribute", so `/plugin`
                    // asks it rather than answering it, and the two can only ever
                    // agree. `Plugins::dropped` cannot cover this: the bundle
                    // loaded fine, and it is the directory it names that is gone.
                    for (id, sentence) in
                        io_cli::skillview::view_of_bundles(&bundle_skills(&config)).bundles_failed
                    {
                        app.record(
                            Tone::Refused,
                            format!(
                                "the {id} bundle contributes no skills: {sentence}. Every turn of \
                                 this session ends on that error until the directory exists or \
                                 this bundle is removed below.",
                            ),
                        );
                    }
                    if view.is_empty() {
                        // Naming the file rather than the concept: an operator who
                        // has never declared a bundle needs to know where the
                        // entry goes, and one who has needs to know io-cli read
                        // the file they meant.
                        app.record(
                            Tone::Muted,
                            "no capability bundles are declared; add one with a \
                             `[[plugin]]` entry naming its directory"
                                .to_string(),
                        );
                    } else {
                        picker = Some((
                            Picker::new(
                                "Plugins",
                                io_cli::pluginview::rows(&view, screen.width(), &app.theme.glyphs),
                            ),
                            Pick::Plugins(view),
                        ));
                    }
                }
                // **Everything is shown before anything is written, and the
                // default for every item is no.** The plan is built whole here —
                // detection, translation and destination — and drawn into the
                // scrollback before the picker opens, so an operator who presses
                // `Esc` has still *seen* what io found. The picker then turns
                // items on one at a time; nothing is written until the last row is
                // chosen, which is why a cancelled import is never a partial one.
                Action::Import => match io_cli::home::path() {
                    None => app.record(
                        Tone::Refused,
                        "io has no home directory of its own, so there is nowhere to import into"
                            .to_string(),
                    ),
                    Some(home) => {
                        // The operator's home, not io's: `~/.claude` and
                        // `~/.codex` sit beside `~/.io-cli`, not inside it.
                        let found = io_cli::import::detect(
                            &io_cli::home::expand(std::path::Path::new("~")),
                            session.root(),
                        );
                        for source in &found {
                            app.record(Tone::Muted, source.says());
                        }
                        // **The user scope, and it is not a default chosen for
                        // convenience.** It is the one configuration file that is
                        // never committed, and an import writes absolute paths and
                        // an operator's accumulated instructions — neither of
                        // which belongs in a checkout somebody else clones.
                        let items =
                            io_cli::import::plan(&found, &home, io_harness::config::Scope::User);
                        if items.is_empty() {
                            app.record(
                                Tone::Muted,
                                "nothing to import: no other agent's configuration was found. \
                                 `AGENTS.md` in a repository needs no import — io-harness has \
                                 discovered it with no configuration at all since its 0.45.0."
                                    .to_string(),
                            );
                        } else {
                            let accepted = vec![false; items.len()];
                            picker = Some((
                                Picker::new(
                                    "Import",
                                    import_rows(&items, &accepted, session.root()),
                                ),
                                Pick::Import { items, accepted },
                            ));
                        }
                    }
                },
                Action::Provider => {
                    let chain = io_cli::providers::chain(&config);
                    if chain.is_empty() {
                        app.record(Tone::Muted, "no provider is configured; run `io setup`");
                    } else {
                        picker = Some((
                            Picker::new(
                                "Providers, in the order they are tried",
                                io_cli::providers::rows(&chain),
                            ),
                            Pick::Provider,
                        ));
                    }
                }
                Action::Profile => {
                    let names = io_cli::configure::profiles(&config);
                    if names.is_empty() {
                        // The "not configured" shape again: an absent section is
                        // not an error, and `[profile.<name>]` is io-harness's
                        // own spelling rather than something to explain here.
                        app.record(
                            Tone::Muted,
                            "this configuration declares no `[profile.<name>]` sections",
                        );
                    } else {
                        let rows: Vec<Row> =
                            names.iter().map(|name| Row::new(name.clone())).collect();
                        picker = Some((Picker::new("Which profile?", rows), Pick::Profile(names)));
                    }
                }
                Action::Config(None) => {
                    let settings = io_cli::configure::settings(&config);
                    let paths: Vec<String> = settings.iter().map(|s| s.path.clone()).collect();
                    picker = Some((
                        Picker::new("Which setting?", io_cli::configure::rows(&settings)),
                        Pick::Config(paths),
                    ));
                }
                // A key with no value is a question. Naming what is in force and
                // which file decided it is the answer, and nothing is written.
                Action::Config(Some((key, value))) if value.is_empty() => {
                    let setting = io_cli::configure::setting(&config, &key);
                    let what = setting.value.as_deref().unwrap_or("not set");
                    app.record(
                        Tone::Muted,
                        format!("{key} is {what} ({})", setting.decided.word()),
                    );
                }
                Action::Config(Some((key, value))) => {
                    // Every scope, whether or not its file exists yet — writing a
                    // key into a scope for the first time is how this is used.
                    let root = session.root().to_path_buf();
                    let paths: Vec<(io_harness::config::Scope, std::path::PathBuf)> = [
                        io_harness::config::Scope::User,
                        io_harness::config::Scope::Project,
                        io_harness::config::Scope::Local,
                    ]
                    .into_iter()
                    .filter_map(|scope| {
                        io_cli::configure::scope_path(&root, scope).map(|p| (scope, p))
                    })
                    .collect();

                    let rows: Vec<Row> = paths
                        .iter()
                        .map(|(scope, path)| {
                            let word = io_cli::configure::Decided::File {
                                scope: *scope,
                                path: path.clone(),
                            };
                            Row::with_detail(word.word().to_string(), path.display().to_string())
                        })
                        .collect();

                    picker = Some((
                        Picker::new(format!("Write {key} where?"), rows),
                        Pick::ConfigScope { key, value, paths },
                    ));
                }
                // A line typed with nothing after the word. Answered with what to
                // type rather than by opening a picker over three files it would
                // write nothing into — asking which file a blank line goes in is
                // a question with no useful answer.
                Action::Remember(line) if line.is_empty() => app.say(
                    Tone::Muted,
                    "say what to remember: pick the row, then type the line after it",
                ),
                // **Which scope is half the decision, and this product has three
                // of them** — the same sentence `/config`'s write arm carries, and
                // a sharper one here: the three files differ only in who else
                // reads them, so a guess is a guess about whether a private note
                // is committed. So the same shape: the line waits in the `Pick`
                // and nothing is written until a row is chosen.
                Action::Remember(line) => {
                    let root = session.root().to_path_buf();
                    // Every scope that has a path on this machine. The user scope
                    // has none where there is no home to speak of, which is
                    // `memory::path`'s own answer and not something to invent one
                    // around.
                    let paths: Vec<(io_harness::config::Scope, std::path::PathBuf)> = [
                        io_harness::config::Scope::Project,
                        io_harness::config::Scope::Local,
                        io_harness::config::Scope::User,
                    ]
                    .into_iter()
                    .filter_map(|scope| io_cli::memory::path(&root, scope).map(|p| (scope, p)))
                    .collect();
                    let rows = io_cli::commands::scope_rows(&paths, &app.theme.glyphs);
                    picker = Some((
                        Picker::new("Remember it where?", rows),
                        Pick::RememberScope { line, paths },
                    ));
                }
                // **One page, two lists, and they are two different memories.**
                // The instruction files a person writes, and the notes the agent
                // wrote for itself. Every row is built by `io_cli::commands`
                // rather than here, because nothing under `tests/` links this
                // file — a mark or a sentence written in the driver is one no
                // sabotage can reach.
                Action::Memory => {
                    let root = session.root().to_path_buf();
                    // Read off the configuration the session is running on, so a
                    // `[instructions] files` that replaced the list shows here as
                    // a file that is not read rather than being argued away.
                    let files = io_cli::memory::view(&root, &config);
                    // The caps come off `opening` — the contract built at startup
                    // from the same builder every turn uses — rather than from a
                    // fifth `contract::session` call, which `tests/contract.rs`
                    // counts and fails at five. It is the same reason `/compact`
                    // reads `opening.compaction`.
                    let remembered = io_cli::recall::view(&store, &root, &opening, None);
                    // A store that will not answer loses the second list and
                    // keeps the first. The two halves have nothing to do with
                    // each other — one is markdown on disk — and dropping the
                    // page over the half that failed would hide the instruction
                    // files from the operator who came to check them.
                    let (entries, cut) = match &remembered {
                        Ok(view) => {
                            for note in io_cli::commands::memory_notes(view, &app.theme.glyphs) {
                                app.record(Tone::Muted, note);
                            }
                            (view.entries.clone(), view.draws_cut)
                        }
                        Err(error) => {
                            app.record(
                                Tone::Error,
                                format!("the agent's own memory could not be read: {error}"),
                            );
                            (Vec::new(), false)
                        }
                    };
                    let (rows, held) =
                        io_cli::commands::memory_page(&files, &entries, cut, &app.theme.glyphs);
                    picker = Some((
                        Picker::new("What io remembers", rows),
                        Pick::Memory { held },
                    ));
                }
                Action::Theme => {
                    picker = Some((
                        Picker::new(
                            "Which theme?",
                            io_cli::theme::THEMES
                                .iter()
                                .map(|theme| Row::new(theme.name))
                                .collect(),
                        ),
                        Pick::Theme,
                    ));
                }
                Action::Model => {
                    // The provider's live catalogue, through the same call the
                    // wizard's model step makes. A catalogue that cannot be read
                    // offers the configured model and says why, exactly as the
                    // wizard does — an empty picker would be a dead end where the
                    // session is still perfectly able to continue.
                    let models = verify::catalogue(&spec).await;
                    let rows = if models.is_empty() {
                        app.say(
                            Tone::Muted,
                            "the catalogue could not be read; only the configured model is offered",
                        );
                        vec![Row::with_detail(
                            app.status.model.clone(),
                            "the model this session is using",
                        )]
                    } else {
                        models.iter().map(Row::new).collect()
                    };
                    let at = models
                        .iter()
                        .position(|name| *name == app.status.model)
                        .unwrap_or(0);
                    picker = Some((Picker::new("Which model?", rows).selecting(at), Pick::Model));
                }
                Action::Resume => match io_cli::sessions::recent(&store) {
                    Ok((found, _)) if found.is_empty() => {
                        app.say(Tone::Muted, "no session in this store has run a turn yet")
                    }
                    Ok((found, cut)) => {
                        let ids: Vec<i64> = found.iter().map(|session| session.id).collect();
                        let mut rows =
                            io_cli::sessions::rows(&found, screen.width(), &app.theme.glyphs);
                        // Last, and a row rather than a notice, so a list that was
                        // cut cannot read as a complete one. It carries no id, so
                        // `sessions::pick` answers `None` for it and the arm above
                        // says so — being last stopped being protection the moment
                        // the picker started ranking rows by what is typed.
                        if let Some(note) = io_cli::sessions::cut_note(cut, rows.len()) {
                            rows.push(Row::new(note));
                        }
                        picker = Some((
                            Picker::new("Resume which session?", rows),
                            Pick::Resume(ids),
                        ));
                    }
                    Err(error) => app.say(
                        Tone::Muted,
                        format!("the sessions could not be read: {error}"),
                    ),
                },
                Action::Fork => match session.history(&store) {
                    Ok(turns) if turns.is_empty() => app.say(
                        Tone::Muted,
                        "this conversation has no turn to fork from yet",
                    ),
                    Ok(turns) => {
                        let ids: Vec<i64> = turns.iter().map(|turn| turn.id).collect();
                        let rows =
                            io_cli::sessions::turn_rows(&turns, screen.width(), &app.theme.glyphs);
                        let at = ids.len().saturating_sub(1);
                        picker = Some((
                            Picker::new("Continue from which turn?", rows).selecting(at),
                            Pick::Fork(ids),
                        ));
                    }
                    Err(error) => app.say(
                        Tone::Muted,
                        format!("this conversation could not be read: {error}"),
                    ),
                },
                // At an idle prompt. Mid-turn the key is the way in, since the
                // driver refuses a slash command while a run is in flight.
                Action::Fleet => app.toggle_fleet(),
                // The policy the NEXT turn would run under, built exactly the way
                // the completion above and the turn below build it — so what may
                // be attached and what the agent may read are the same set by
                // construction rather than by two agreeing lists.
                Action::Contain(want) => match (&containment, want) {
                    // Nothing to switch. Said as the configuration gap it is,
                    // with the key that closes it, rather than as a refusal —
                    // the caps are what the fan-out runs under and there is no
                    // safe default for somebody else's token ceiling.
                    (None, _) => app.record(
                        Tone::Muted,
                        "no [app.io-cli.containment] in the configuration, so a turn here \
                         cannot fan out. Set max_total_agents, max_concurrent_agents, \
                         max_depth and max_total_tokens to turn it on.",
                    ),
                    (Some(caps), None) => {
                        let where_it_is = if contained {
                            settings::contained_notice(caps, app.theme.glyphs.dash)
                        } else {
                            // **Not "steered", and since 0.17.0 for the opposite
                            // reason.** Both arms hold a `SteerInbox` now, so
                            // steering is what this turn and a contained one have
                            // in common — naming it here would offer as a
                            // consolation something the other mode has too. The
                            // one difference either way is the fan-out, so that
                            // is the only thing this sentence names.
                            "not contained — this turn does the work itself and cannot fan out"
                                .to_string()
                        };
                        app.record(Tone::Muted, where_it_is);
                    }
                    (Some(caps), Some(true)) => {
                        contained = true;
                        let notice = settings::contained_notice(caps, app.theme.glyphs.dash);
                        app.record(Tone::Muted, notice);
                    }
                    (Some(_), Some(false)) => {
                        contained = false;
                        app.record(
                            Tone::Muted,
                            "not contained from the next turn — it does the work itself and \
                             cannot fan out",
                        );
                    }
                },
                // The same three answers `/contain` gives, and the same rule:
                // nothing reports, and only an explicit word switches. Both take
                // effect from the NEXT turn, because the contract a running turn
                // is under was built when it started.
                Action::Plan(asked) => {
                    let said = match asked {
                        None if planning => {
                            "planning — a turn proposes a plan and waits for you before it \
                             writes anything. `/plan off` to let it work straight away"
                        }
                        None => {
                            "working — a turn starts on the job. `/plan on` to have it propose \
                             a plan first and wait for you"
                        }
                        Some(true) => {
                            planning = true;
                            "planning from the next turn — it proposes a plan and every write \
                             and every command is denied until you approve it"
                        }
                        Some(false) => {
                            planning = false;
                            "working from the next turn — no plan is proposed and nothing waits \
                             on you before it starts"
                        }
                    };
                    // The line says it once; the status line keeps saying it. A
                    // mode that outlives the turn it was set on has to be
                    // readable from the screen rather than from memory.
                    app.status.planning = planning;
                    // **A mode report is a record, not a notice.** It outlives
                    // the keystroke that asked for it — the mode is in force
                    // until something changes it — and it carries the sentence
                    // saying how to change it, which does not fit the one row a
                    // notice has. `App::say` is for what answers a key and is
                    // gone at the next one.
                    app.record(Tone::Muted, said);
                }
                // The picture, drawn now, at the bottom. It cannot open the row
                // it was announced on: that row is in the terminal's scrollback,
                // which nothing here reaches.
                Action::Image(which) => {
                    let total = app.images();
                    match which.and_then(|n| app.image(n).map(|path| (n, path.to_string()))) {
                        Some((number, path)) => {
                            let effective =
                                approval::session_policy(&policy, app.posture(), app.remembered());
                            match io_cli::attach::prepare(
                                session.root(),
                                &effective,
                                provider.accepts_images(),
                                &path,
                            ) {
                                Ok(staged) => {
                                    let (drawable, graphics) = forms(&app);
                                    let drawn = io_cli::picture::render(
                                        &staged.bytes,
                                        &staged.path,
                                        staged.media_type,
                                        drawable,
                                        graphics,
                                        screen.width(),
                                    );
                                    let caption = app.theme.notice(
                                        Tone::Muted,
                                        io_cli::picture::caption(
                                            number,
                                            &staged.path,
                                            staged.media_type,
                                            staged.bytes.len(),
                                        ),
                                    );
                                    commit_drawn(screen, &mut app, drawn, Some(caption))?;
                                }
                                Err(error) => app.say(Tone::Error, error),
                            }
                        }
                        None if total == 0 => {
                            app.say(Tone::Muted, "no image has been attached in this session")
                        }
                        None => app.say(
                            Tone::Muted,
                            format!("say which one: /image 1 to /image {total}"),
                        ),
                    }
                }
                Action::Expand => {
                    let lines = expand(&session, &store, &app.theme, app.events.thought());
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                // **Committed upward, exactly as `/expand` and `Ctrl+T` are.**
                // The viewport is eight rows and cannot grow, so everything that
                // shows more of something writes into the terminal's own
                // scrollback — one answer to "show me more" rather than three.
                //
                // The contract is built by the same call the next turn would
                // build it with, so the configured rosters and the skills
                // directory on this page are the ones that would actually reach
                // it rather than a second reading of the file. A responder is
                // required to build one and this contract runs nothing, so the
                // channel is opened and dropped — the same shape
                // `contract::server_notices` already uses to ask a question only
                // `Config::apply_to` can answer. The plan gate is `None` here
                // and deliberately: registering one turns io-harness's planning
                // phase on, and reading the state must not change it.
                Action::Status => {
                    // Bound as `reading` and not as `contract`, deliberately:
                    // `tests/plan.rs` finds the turn's builder by the binding
                    // name and asserts the plan-gate argument on *that* call is
                    // the operator's switch. This one is not a turn's contract
                    // and must not be the call that gate is read off — a second
                    // binding of the same name would hand the assertion the
                    // wrong argument list, and it would go green on a `None`
                    // that means something else entirely.
                    let (answerer, _questions) = io_cli::intent::channel();
                    let reading = io_cli::contract::session(
                        String::new(),
                        session.root().to_path_buf(),
                        &config,
                        &capabilities,
                        std::sync::Arc::new(answerer),
                        None,
                    );
                    let lines = io_cli::status::committed(
                        &app.status,
                        &session,
                        &policy,
                        &reading,
                        // What the NEXT turn would run under, which is why
                        // `/contain off` reads as not contained here: the caps
                        // are only in force on a turn that takes the contained
                        // entry point.
                        containment.as_ref().filter(|_| contained),
                        &app.theme,
                        screen.width(),
                    );
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                // The sibling of `Action::Status` above and wired the same way,
                // with one difference worth stating: this arm holds no decision
                // at all. Every sentence, every figure and every caveat is
                // `io_cli::cost::committed`, because nothing under `tests/` can
                // link this file — so a format string here would be a format
                // string no test could ever read.
                Action::Cost => {
                    // Asked of the configuration again rather than threaded down
                    // from the driver's own binding, for the reason stated where
                    // `settings_in_force` is: `settings::stored` is a pure
                    // function of the config, so a second call cannot disagree
                    // with the first, and a threaded copy could go stale behind a
                    // `/config` write that reloaded one and not the other.
                    let (settings, _) = settings::stored(&config);
                    let table = io_cli::cost::table(&config);
                    let provenance = io_cli::cost::Provenance::of(&config, settings.as_ref());
                    let lines = io_cli::cost::committed(
                        &store,
                        &table,
                        &provenance,
                        last_run(&session, &store).map(|turn| turn.run_id),
                        Some(session.id()),
                        &app.theme,
                        screen.width(),
                    )?;
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                Action::Stats => {
                    let lines =
                        io_cli::stats::committed(&store, &app.theme, screen.width())?;
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                // Reached only at an idle prompt: while a turn runs the driver's
                // own key handler answers `/steer` before `parse` is ever called,
                // because that is where the inbox lives. So this arm is the
                // honest "nothing to steer" and it says what the command would
                // have done, rather than leaving a registered command looking
                // broken to the operator who just found it in the palette.
                Action::Steer => app.say(
                    Tone::Muted,
                    "nothing is running — /steer sends what is queued into a turn that is already \
                     working, and it is read at that turn's next step",
                ),
                // Reached at an idle prompt, where `/compact` is not a refusal:
                // there is no turn to steer, so the request rides the NEXT turn's
                // contract as `TaskContract::fold_now` and folds at that turn's
                // first step. While a turn runs, the driver's own key handler
                // answers the word before `parse` is called, the way `/steer` is.
                Action::Compact => {
                    let dash = app.theme.glyphs.dash;
                    // Read off `opening` — built at startup from the same builder
                    // every turn uses — rather than from a fifth
                    // `contract::session` call, which `tests/contract.rs` counts
                    // and fails at five. Nothing in `[run]` or `[app.io-cli]` can
                    // move `compaction` between two turns of one session.
                    let said = io_cli::compact::Said::asked(opening.compaction, false);
                    fold_next = said == io_cli::compact::Said::Armed;
                    app.say(Tone::Muted, said.line(dash));
                }
                Action::Context => {
                    // `reading` for the same reason the arm above binds it that
                    // way — `tests/plan.rs` reads the plan-gate argument off a
                    // binding called `contract`, and this is not a turn's.
                    let (answerer, _questions) = io_cli::intent::channel();
                    let reading = io_cli::contract::session(
                        String::new(),
                        session.root().to_path_buf(),
                        &config,
                        &capabilities,
                        std::sync::Arc::new(answerer),
                        None,
                    );
                    let lines = io_cli::context::committed(
                        seen.latest().as_ref(),
                        &reading,
                        reading.max_tokens,
                        &app.theme,
                        screen.width(),
                    );
                    screen.commit(&lines).map_err(|error| error.to_string())?;
                }
                Action::Copy(what) => {
                    let (payload, said) = to_copy(&session, &store, what);
                    match payload {
                        Some(payload) => {
                            screen
                                .escape(&io_cli::clipboard::sequence(&payload))
                                .map_err(|error| error.to_string())?;
                            // What was sent and how big it was — never "copied".
                            // Nothing answers an OSC 52 write, so a success
                            // message would be a claim this product cannot make.
                            app.say(Tone::Muted, io_cli::clipboard::describe(&payload));
                        }
                        None => app.say(Tone::Muted, said),
                    }
                }
                Action::Transcript => commit_transcript(screen, &session, &store, &app.theme)?,
                // **Start over, and only when it is safe.** The refusal is
                // `App::clear_conversation`'s, which is where a test can reach
                // it; the second lock is structural and one loop down, where a
                // slash command typed during a turn is already refused with the
                // same sentence.
                Action::Clear => {
                    if app.clear_conversation() {
                        // **The window belonged to the conversation being
                        // discarded.** `Seen::forget`'s own doc names `/clear`
                        // first of the three sites that must call it, and this was
                        // the one that did not — `/resume`, `/fork` and the rewind
                        // all do, beside their `forget_run`. Without it `/context`
                        // draws the whole of a conversation the operator has just
                        // thrown away, on a session with no turns in it, while the
                        // `ctx` field beside it is blank because `forget_run` did
                        // clear that. Two surfaces disagreeing about the same
                        // fact, which is what this release exists to stop.
                        seen.forget();
                        let root = session.root().to_path_buf();
                        match Session::open(&store, &root) {
                            Ok(fresh) => {
                                session = fresh;
                                // The screen only. The terminal's scrollback is
                                // the terminal's, and the conversation this ends
                                // is in the store and still listed by `/resume`
                                // — so nothing here destroys anything, which is
                                // what makes clearing the screen a display
                                // decision rather than a deletion.
                                screen
                                    .escape("\x1b[H\x1b[2J")
                                    .map_err(|error| error.to_string())?;
                                // Placed again against the screen it now has:
                                // the old viewport's origin was a row on a
                                // screen that no longer exists.
                                replace_viewport(screen, io_cli::term::VIEWPORT_HEIGHT)?;
                                // **The session opens again, banner and all.**
                                // A cleared screen with one grey sentence on it
                                // is not a fresh start, it is an empty room; the
                                // card is what a first prompt has above it, and
                                // a new conversation is a first prompt.
                                let width = screen.width();
                                let about = splash::About {
                                    model: Some(app.status.model.clone()),
                                    policy: app.posture().map(|p| p.short().to_string()),
                                    workspace: Some(root.display().to_string()),
                                };
                                screen
                                    .commit(&splash::lines(&app.theme, true, width, &about))
                                    .map_err(|error| error.to_string())?;
                                let dash = app.theme.glyphs.dash;
                                app.record(
                                    Tone::Muted,
                                    format!(
                                        "new conversation {dash} the last one is still in /resume"
                                    ),
                                );
                            }
                            Err(error) => app.say(
                                Tone::Error,
                                format!("a new conversation could not be started: {error}"),
                            ),
                        }
                    }
                }
            },
            // The operator's own line, in the operator's own shell. It reaches
            // this arm and no other: `App::compose` is the only thing that builds
            // a `Command::Shell`, so nothing io-harness drives can get here, and
            // `tests/dependencies.rs` asserts that rather than trusting it.
            //
            // Committed through `Screen::commit`, the same call `/expand` and
            // `Action::Print` make, and nothing else happens to the terminal:
            // the viewport is not handed over, not restored and not rebuilt, so
            // its inline origin cannot go stale. `io_cli::shell` is where that
            // constraint is argued.
            //
            // Not written to the run's trace, and there is nothing here that
            // could — the store is not touched. The agent did not run this.
            Command::Shell(line) => {
                let ran = shell::run(&line);
                let lines = shell::lines(&line, &ran, &app.theme);
                screen.commit(&lines).map_err(|error| error.to_string())?;
            }
            // **A prompt, and then whatever queued behind it while it ran.** The
            // loop is what makes three queued lines three turns: each goes
            // through `turn` on its own, so each gets its own echo, its own
            // answer under it, its own clock and its own `Ctrl+C`. Joining them
            // into one prompt would be one run answering everything in one
            // breath, with no boundary an operator could stop it at — which is
            // the shape queueing them was meant to avoid.
            //
            // Prompts typed *during* a queued turn queue behind the rest, because
            // `App::next_queued_prompt` is asked again at the bottom of every
            // pass rather than the queue being drained into a list up front.
            Command::Submit(text) => {
                let mut next = Some(text);
                while let Some(text) = next.take() {
                    // Rebuilt every turn rather than kept, because `remembered`
                    // grows as the operator answers and the harness's own
                    // `remember` dies with the turn it was given in. With nothing
                    // remembered this is the session's policy unchanged.
                    //
                    // Inside the loop for the same reason it was ever rebuilt: a
                    // queued turn runs under what the operator has allowed by the
                    // time it starts, not by the time they typed it.
                    //
                    // **And this is the turn boundary, which is where the file is
                    // read again.** Here rather than at the keystroke that opened
                    // a surface, because a turn is the unit a configuration is in
                    // force *for*: the contract is built inside `turn` from
                    // `config`, and anything read after that call is read by a
                    // turn already running under the old one. Inside the queue
                    // loop for exactly the reason `effective` is — three queued
                    // prompts are three turns, and each runs under the file as it
                    // is when it starts.
                    //
                    // **A refusal is recorded and not said.** The 0.13.1 rule:
                    // `App::say` answers a keystroke and is gone at the next one,
                    // and this answers no keystroke. It says the configuration on
                    // disk is unreadable and this turn is running on the last one
                    // that was — which outlives the keystroke and belongs to the
                    // conversation. `Configuration::refresh` compares against the
                    // text it last handed back, so a file left broken for six
                    // prompts says it once rather than six times.
                    if let Some(refusal) = configuration.refresh() {
                        app.record(Tone::Refused, refusal);
                    }
                    // ponytail: the fresh pair is copied out rather than every
                    // reader below being moved onto `configuration.config()`.
                    // One `Config` clone per turn against a dozen call sites and
                    // two picker arms that assign `config` themselves; move them
                    // if a turn ever starts often enough for the clone to show.
                    config = configuration.config().clone();
                    // **The overlay, put back.** `configure::reload` discovers the
                    // file and nothing else, so whatever `--profile` or `/profile`
                    // chose is not in what came back — and a flag that says *for
                    // this run* would have quietly stopped meaning anything after
                    // the first prompt. Re-applied by name, with io-harness's own
                    // refusal if the file no longer declares it; the name is
                    // dropped at that point, because a profile edited away is a
                    // thing to say once rather than to fail on every turn.
                    if let Some(name) = profile.clone() {
                        match io_cli::configure::with_profile(&config, &name) {
                            Ok(overlaid) => config = overlaid,
                            Err(refusal) => {
                                profile = None;
                                app.record(Tone::Refused, refusal);
                            }
                        }
                    }
                    // Both halves, or the turn below runs on every `[app.io-cli]`
                    // answer as it was at session start while the rest of the
                    // session has moved on — the asymmetry `configure::reload`'s
                    // own doc comment exists to stop. Derived from the overlaid
                    // `config` rather than from `Configuration::settings`, because
                    // a profile body can carry `[app.io-cli]` keys too.
                    let (in_force, _) = settings::stored(&config);
                    capabilities = io_cli::contract::Capabilities::stored(in_force.as_ref());
                    // **The palette's inventory is re-walked here, and before
                    // 0.21.0 nothing needed it to be.** The list was read once at
                    // startup and reassigned in exactly one place, the `/skills`
                    // toggle — which was complete while the only skills in it came
                    // from one directory the operator changed only through that
                    // toggle. Bundles broke that: `/plugin` can stop loading a
                    // bundle and `/import` can add skills, both mid-session, and
                    // neither goes anywhere near that arm. Left alone, `/` went on
                    // offering a name whose file the turn's catalogue no longer
                    // has, and the model is asked for a skill it will be refused.
                    //
                    // Done at the turn boundary rather than at each writer,
                    // because this is where `config` is already fresh and it
                    // bounds the staleness for every way the set can change,
                    // including an operator editing `io.toml` in another window.
                    // The sentence is dropped on purpose: it was said once at
                    // startup and both `/skills` and `/plugin` say it on demand.
                    // Committing it again every turn is the shape 0.19.0 already
                    // rejected once — a line that repeats for the life of the
                    // file it is about stops being read.
                    skills = commands::skills(
                        &io_cli::home::path().unwrap_or_default(),
                        io_cli::contract::skills_dir(
                            &config,
                            &capabilities,
                            session.root().to_path_buf(),
                        )
                        .as_deref(),
                        &bundle_skills(&config),
                    )
                    .0;
                    let effective =
                        approval::session_policy(&policy, app.posture(), app.remembered());
                    let stopped = turn(
                        screen,
                        inputs,
                        &mut app,
                        &provider,
                        &store,
                        &mut session,
                        &effective,
                        &config,
                        // The caps reach the turn only while the session is in
                        // contained mode, so `/contain off` is a real switch and
                        // not a label: with `None` here the turn built below is
                        // the uncontained turn, byte for byte. Both arms are
                        // steered, so that is not the word for the difference.
                        contained.then_some(containment.as_ref()).flatten(),
                        &capabilities,
                        &seen,
                        planning,
                        // Taken rather than read: one request, one turn. A queue
                        // of three prompts must not fold three times.
                        std::mem::take(&mut fold_next),
                        text,
                        // **This turn's own clock, not the session's.** What a
                        // reader wants of the row above the prompt is how long the
                        // thing in front of them has been going; a clock that had
                        // been counting since the terminal opened said `22m12s`
                        // about a turn six seconds old. Every event age inside the
                        // turn is measured from here too, which is what a tool
                        // cell's duration is a difference of.
                        //
                        // Read once per pass, so a queued turn is timed from the
                        // moment it starts rather than from the prompt that ran
                        // ahead of it.
                        Instant::now(),
                    )
                    .await?;
                    // Anything dropped onto the prompt while the turn held the
                    // session is staged now that it has let go.
                    for path in app.take_queued_pictures() {
                        paste_picture(&mut app, &mut session, &provider, &policy, &path);
                    }
                    // The stop key stops the session, not just the step in front
                    // of the operator. Firing the queue here would turn one press
                    // into three more turns against a conversation they had just
                    // decided to steer somewhere else.
                    if stopped {
                        let dropped = app.forget_queued_prompts();
                        if dropped > 0 {
                            let dash = app.theme.glyphs.dash;
                            app.say(
                                Tone::Muted,
                                format!("{dropped} queued {dash} dropped with the stopped turn"),
                            );
                        }
                    }
                    next = app.next_queued_prompt();
                }
            }
        }

        paint_picker(screen, &mut app, picker.as_mut())?;
    }
}

/// What an attachment commits into the scrollback: the picture, then the sentence.
///
/// The picture first and the sentence under it, because the sentence is the part
/// a reader can find again by scrolling and the picture is the part they are
/// looking at now. Under `--plain`, `NO_COLOR` or the ASCII glyph set there is no
/// picture and the file is named instead — see [`io_cli::picture::drawable`],
/// which is the single expression all three suppressions go through.
///
/// A file that will not decode is NOT an error here. io-harness has already
/// accepted it for the wire, so the agent will see it; what failed is this
/// crate's ability to show the operator the same thing, and saying so beats
/// refusing an attachment that is going to work.
/// Stage a pasted picture and put its marker on the prompt.
///
/// **The whole of how an image is attached since 0.13.1.** An operator drags the
/// file onto the prompt, or copies it and presses paste; the terminal delivers
/// the path; and this is what turns that into `[Image #1]` and a staged
/// attachment. There is no command: a command is something a reader has to be
/// told about first, and dragging a picture into a window is not.
fn paste_picture<P: Provider>(
    app: &mut App,
    session: &mut Session,
    provider: &P,
    policy: &Policy,
    path: &str,
) {
    // Already on this prompt: a repeat paste is a request to change what is on
    // screen — the marker or the path it stands for — and never a second copy of
    // the same file on the same turn.
    if app.composer.attached(path) {
        app.composer.attach("", path);
        return;
    }
    // A file dropped alongside pictures that is not one: it is a path, and a
    // path on the prompt is what it was before this release too.
    if io_harness::Media::source_type_for(path).is_none() {
        app.composer.paste(path);
        return;
    }
    let effective = approval::session_policy(policy, app.posture(), app.remembered());
    match io_cli::attach::prepare(session.root(), &effective, provider.accepts_images(), path) {
        Ok(staged) => {
            let number = app.attached(&staged.path);
            let note = io_cli::attach::staged_note(&staged, number);
            app.composer
                .attach(&format!("[Image #{number}]"), &staged.path);
            session.attach([staged.media]);
            app.say(Tone::Muted, note);
        }
        Err(error) => app.say(Tone::Error, error),
    }
}

fn commit_drawn(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    drawn: io_cli::picture::Drawn,
    after: Option<ratatui::text::Line<'static>>,
) -> Result<(), String> {
    match drawn {
        io_cli::picture::Drawn::Lines(mut lines) => {
            lines.extend(after);
            app.picture(lines);
        }
        io_cli::picture::Drawn::Graphics { payload, rows } => {
            // Anything already queued goes FIRST. A raw commit writes straight to
            // the terminal, so a picture emitted before the lines that precede it
            // would land above its own context — and scrollback cannot be
            // reordered afterwards.
            let pending = app.take_pending();
            if !pending.is_empty() {
                screen.commit(&pending).map_err(|error| error.to_string())?;
            }
            screen
                .commit_raw(&payload, rows)
                .map_err(|error| error.to_string())?;
            app.picture(after.into_iter().collect());
        }
    }
    Ok(())
}

/// How this session may draw a picture: in cells, and under which graphics
/// protocol — if any — it is the real image instead.
fn forms(app: &App) -> (bool, io_cli::term::Graphics) {
    let drawable = io_cli::picture::drawable(app.theme.coloured, app.plain(), &app.theme.glyphs);
    // A graphics escape is only ever sent where cells would also have been drawn:
    // `--plain`, `NO_COLOR` and the ASCII glyph set are each a reason a reader
    // wants no picture at all, and a protocol the terminal happens to speak does
    // not override any of them.
    if drawable {
        (true, io_cli::term::graphics())
    } else {
        (false, io_cli::term::Graphics::None)
    }
}

/// **The agent's own look, committed where it looked.**
///
/// A wrapper over [`io_cli::attach::viewed`], which is where every decision
/// lives. Nothing is decided here, deliberately: `src/main.rs` is linked by no
/// integration test, so a branch written here could not be sabotaged and would
/// not be covered.
fn commit_viewed(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    root: &std::path::Path,
    policy: &Policy,
    event: &io_harness::RunEvent,
) -> Result<(), String> {
    let (drawable, graphics) = forms(app);
    let width = screen.width();
    match io_cli::attach::viewed(root, policy, event, drawable, graphics, width) {
        Some(drawn) => commit_drawn(screen, app, drawn, None),
        None => Ok(()),
    }
}

/// One turn, with the keyboard live throughout so `Ctrl+C` can reach it.
///
/// Returns whether the operator asked this turn to stop — the one thing about a
/// turn the caller cannot see from outside it, and the thing the caller now has
/// to decide about, because a prompt queue that fired after a stop would make the
/// stop key start three more turns.
#[allow(clippy::too_many_arguments)]
async fn turn<P: Provider>(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    app: &mut App,
    provider: &P,
    store: &Store,
    session: &mut Session,
    policy: &Policy,
    // What the operator's file asks of this turn. Beside the policy because it is
    // the other half of the same answer: the policy is the boundary the harness
    // enforces, and this is every ceiling, budget, roster and capability the same
    // file set — none of which reached an interactive turn before 0.14.0.
    config: &Config,
    containment: Option<&io_harness::Containment>,
    capabilities: &io_cli::contract::Capabilities,
    // The last request this session's provider was handed. `/context` reads it to
    // say what is in the window, and `ctx N%` reads it so the two say the same
    // thing — see `note_context`, and the live run that found them disagreeing.
    seen: &io_cli::context::Seen,
    // Whether this turn proposes a plan before it works. The operator's `/plan`,
    // and nothing else — a caps configuration decided it through 0.11.0, which
    // is how every contained turn ended up stopping for one.
    planning: bool,
    // Whether this turn folds its history at its first step — the `/compact` an
    // operator typed at the idle prompt before it. io-harness reads `fold_now`
    // once, before the first step assembles its first request, and consumes it
    // with `mem::take`, so a contract reused for every turn would not fold every
    // turn; io-cli builds a fresh one anyway.
    fold: bool,
    text: String,
    started: Instant,
) -> Result<bool, String> {
    let (observer, mut events) = bridge::channel();
    // The one way a turn is stopped from the interface, contained or not. Both
    // arms take a contract **and** a steer inbox since 0.17.0, and the stop key
    // stayed here rather than moving onto `Steer::interrupt`: `Flow::Cancel` out
    // of `Bridge::event` is what `Ctrl+C` and `Esc` set, on either arm.
    let canceller = observer.canceller();
    // The other seam, and the one that can stop the agent. `DenyAll` stood here
    // through 0.1.0 and 0.1.1, which is why the *ask before writes* posture
    // declined everything it was named for.
    let (approver, mut asks) = approval::channel();
    // The third seam, and the one that answers rather than authorizes. It reaches
    // the run only through the contract, so on a flat turn the answerer is built
    // and never spoken to — which is exactly what a session with no responder
    // does today: the question pauses the run instead.
    let (answerer, mut questions) = io_cli::intent::channel();
    // The fourth, and the only one that can stop the work before it starts.
    // Registering it is what turns io-harness's planning phase on, so it is put
    // on the contract only where the contract itself reaches the run.
    let (gate, mut plans) = io_cli::plan::channel();
    app.started();
    paint(screen, app)?;

    // **The fifth seam, and the only one that speaks to a turn already in
    // flight.** Every other channel above is something the run asks *this*
    // interface for — an approval, a plan, an answer. This one goes the other
    // way: `Steer::say` puts the operator's words into the run's own ledger at
    // the next step boundary, so the step after it is composed with them present.
    //
    // Three things about it that decide the code below:
    //
    // - `SteerInbox` is not `Clone` and holds a `RefCell`, so it is `!Sync` and a
    //   future borrowing it is not `Send`. That costs nothing here — the boxed
    //   future below has never had a `Send` bound and is driven on this task —
    //   but it is why the inbox stays a local and only the `Steer` half could
    //   ever be handed anywhere else.
    // - A delivered steer emits **no observer event**. There is no
    //   `EventKind::Steered`; io-harness records a `steered` row in the run's
    //   context trace instead. So nothing that arrives through `bridge` can
    //   confirm delivery, and nothing this interface says may claim it — see the
    //   `/steer` arm in the loop, which says *sent*, and the drain after the loop,
    //   which is the one thing here that can honestly say *not* delivered.
    // - Only the root is steered. `extras_for` in io-harness returns no extras
    //   below depth zero, so on the contained arm a spawned child never hears the
    //   operator; the agent that reads the message is the one that spawned it.
    let (steer, inbox) = Steer::channel();

    // **The two turns this product can take, and one loop over both.** They are
    // genuinely different turns rather than one turn with a flag: only the
    // contained entry point passes a containment into io-harness's driver, so
    // only it reaches the loop that owns the spawn tool. Boxed to one type so the
    // `select!` below is written once; a second loop would be a second place
    // `Ctrl+C`, the ticker and the event drain could drift.
    //
    // 0.17.0 — both arms take a contract **and** an inbox, which through 0.66 was
    // a choice. `turn_bounded_observed` and `turn_contained_bounded_observed` had
    // no parameter for a steer inbox, so a session that wanted its own contract
    // gave up steering to get one; io-cli made that trade in 0.11.0 and paid for
    // it with a turn nobody could correct. io-harness 0.67.0 opened both, and
    // `turn_bounded_steered` / `turn_contained_bounded_steered` are positionally
    // the same two calls with `&inbox` appended — which is why the change that
    // gives an operator their voice back mid-turn is one argument on each arm.
    // Taken before the future borrows the session, because it is needed inside the
    // loop and `running` holds `&mut session` for the whole of it.
    let root = session.root().to_path_buf();
    app.contained = containment.is_some();
    // Built before the future borrows it, and for both arms alike.
    // **Every turn carries one now, contained or not.** Through 0.11.0 the flat
    // arm was `turn_steered`, which builds `TaskContract::workspace` inside
    // io-harness and takes none from the caller — so its step cap was twelve,
    // fixed, and a turn that read a repository and wrote a file ended on
    // `error: step_cap_reached` with the work half done.
    //
    // `turn_bounded_steered` takes a contract, streams the model's text, is not
    // contained, and reads the inbox above at every step boundary.
    // **Neither of the two seams rides containment any more.** The responder is
    // unconditional: io-harness resolves it inside the tool dispatch on any run,
    // so a question asked on an ordinary turn reaches the person watching instead
    // of pausing the run with nobody offered it. The plan gate is the operator's
    // switch, because registering one is what turns io-harness's planning phase
    // ON — attached to every turn, every turn stopped for a plan, which a real
    // run showed within a minute of 0.10.0 doing it.
    let contract = io_cli::contract::session(
        text.clone(),
        root.clone(),
        config,
        capabilities,
        std::sync::Arc::new(answerer),
        planning.then(|| std::sync::Arc::new(gate) as std::sync::Arc<dyn io_harness::PlanGate>),
    );
    // **The idle half of `/compact`.** Set here rather than as a parameter of the
    // builder: `contract::session` has three other callers that build a contract
    // nothing runs — the startup reading and the two reporting pages — and a
    // seventh parameter would be three `false`s of pure noise plus a signature
    // break, against one call on the single contract that is actually a turn.
    // `fold_now: false` is `TaskContract::workspace`'s own default, so the
    // field-for-field gate is unmoved either way.
    let contract = if fold {
        contract.with_fold_now(true)
    } else {
        contract
    };
    // Set while a fold has been asked for and no `Compacted` event has arrived.
    // A one-shot: io-harness spends the request whether or not it folds, so what
    // this guards is the report and never a retry.
    let mut folding = fold;
    // **Read off the contract that is about to run, and never recomposed from the
    // configuration.** The ceilings on the line have to be the ceilings in force,
    // and the only thing that knows the whole order of precedence — the floor,
    // the file, then `[app.io-cli]` — is the contract this call just built. Asking
    // `Config` a second time here would be a second answer to a question F1 exists
    // to make sure has one.
    //
    // It follows that the fields appear once a turn has been built rather than at
    // the very first idle prompt, which is the honest cost of not duplicating the
    // precedence: a session that has run nothing has not yet composed a contract
    // to read them from. They then persist, because `Status::forget_run` does not
    // clear them — the file does not change while a session runs.
    app.status.budgets = io_cli::status::Budgets::in_force(&contract);
    // **The observer composition, and it is the spine of 0.20.0.** Through 0.19.0
    // this was one value — `Bridge`, the channel the interface draws from. Three
    // things this release wants are all downstream of that single seam, so the
    // seam is where they arrive:
    //
    // 1. `[[hook]]` tables run, because io-harness's `Hooks` *is* an `Observer`
    //    and running the operator's hooks is nothing more than putting it here.
    //    io-cli spawns no process of its own for this — the subprocess, its
    //    timeout, its kill and its drained stdout are all io-harness's, which is
    //    what keeps the one-spawning-module rule in `tests/dependencies.rs` true.
    // 2. `Fanout` exists because io-harness has no combinator to do it with.
    //    `Broadcast` is *not* a tee — it is a store-writing decorator over exactly
    //    one inner observer — and the only three `impl Observer` in the whole
    //    crate are `Hooks`, `Ignore` and `Broadcast`. So io-cli folds `Flow`
    //    itself, and folds it so `Cancel` wins over `Continue` whatever the order.
    // 3. `Broadcast` wraps the result for a different reason than the fan-out
    //    exists: it is the **only** writer of the `run_events` table, and that
    //    table is what `Attach` reads. Without this wrapper a detached child can
    //    be selected and attached to and will show nothing, forever, and look
    //    like a quiet child rather than a missing write.
    //
    // The order is deliberate: `Broadcast` outermost, so an event is durable
    // before either the interface or a hook is told about it, and a hook that
    // cancels the turn cannot leave a gap in the sequence a reader is following.
    let hooks = io_cli::contract::hooks(config, session.root());
    let mut observers: Vec<&dyn io_harness::Observer> = vec![&observer];
    if let Some(hooks) = &hooks {
        observers.push(hooks);
    }
    let fanout = io_cli::fanout::Fanout::new(observers);
    // A second connection to the same file, which is what io-harness's own
    // documentation asks for rather than tolerates: `Observer` is `Send + Sync`,
    // `rusqlite::Connection` is `Send` and not `Sync`, so the run's borrowed
    // `&Store` cannot live inside one. `Store::open` has set `journal_mode = WAL`
    // and a busy timeout since 0.12.0 precisely so a second handle works. It is a
    // spectator — it writes events and never takes a run lease — so it is not the
    // two-drivers-over-one-file shape that has produced `DatabaseBusy` here.
    //
    // `None` where the store cannot be opened a second time: attach is the thing
    // that stops working, and a turn that runs without a durable event stream is
    // strictly better than a turn that refuses to start.
    let durable = io_cli::settings::store_path().and_then(|path| Store::open(&path).ok());
    let broadcast = durable.map(|store| io_harness::Broadcast::new(store, &fanout));
    let watcher: &dyn io_harness::Observer = match &broadcast {
        Some(broadcast) => broadcast,
        None => &fanout,
    };
    let mut running: std::pin::Pin<
        Box<dyn std::future::Future<Output = io_harness::Result<io_harness::TurnResult>> + '_>,
    > = match containment {
        Some(caps) => Box::pin(session.turn_contained_bounded_steered(
            &contract, provider, store, policy, &approver, caps, watcher, &inbox,
        )),
        None => Box::pin(session.turn_bounded_steered(
            &contract, provider, store, policy, &approver, watcher, &inbox,
        )),
    };

    // Lives for the turn and no longer, which is half of why an idle session
    // never repaints; `App::tick` is the other half and the one a test can see.
    // `MissedTickBehavior::Delay` rather than the default: a turn that blocked the
    // loop should resume ticking from now, not fire a burst catching up on the
    // frames nobody saw.
    // Set when a turn was taken back off the screen rather than stopped: there
    // is nothing to report about a turn the session no longer shows.
    let mut undone = false;
    // Set by either stop key, and read by the caller. Both arms below are the
    // operator asking for this turn to end: the first press cancels at a step
    // boundary and the second drops the future, and neither is visible in what
    // io-harness returns — a cancelled run comes back as an ordinary result.
    let mut stopped = false;
    let mut ticker = tokio::time::interval(io_cli::app::TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // `None` is the turn the operator abandoned. Every other arm breaks with what
    // io-harness returned, error included.
    let outcome = loop {
        tokio::select! {
            result = &mut running => break Some(result),
            _ = ticker.tick() => {
                if app.tick(started.elapsed()) {
                    paint(screen, app)?;
                }
            }
            Some(event) = events.recv() => {
                // One clock read, used for both. `main.rs` is the only module in
                // `src/` allowed one, which is what keeps a tool cell's duration
                // assertable without a test ever measuring anything.
                let at = started.elapsed();
                app.status.elapsed = at;
                app.event(&event, at);
                commit_edits(app, store, &event, screen.width());
                // The live half of `ctx N%`. Anchored on a step rather than on
                // every event, for the reason `commit_edits` above it is: the
                // assembly is written once a step and reading it per event would
                // be one store query per token.
                //
                // The REQUEST first, so the line and `/context` cannot disagree —
                // a live run caught them saying 0% and 4,363-of-24,000 about the
                // same turn. The trace is the fallback for the window between a
                // step landing and the first completion call being seen.
                note_context(app, store, &event, seen, &contract);
                note_cost(app, store, config, event.run_id);
                note_fleet(app, store, &event, &contract);
                commit_viewed(screen, app, &root, policy, &event)?;
                commit_fold(app, store, &event, &mut folding);
                paint(screen, app)?;
            }
            Some(ask) = asks.recv() => {
                // The run is now stopped inside `Approver::decide_in_context` and
                // stays there until the overlay answers. The loop keeps turning,
                // which is what leaves `Ctrl+C` reachable while a question is up.
                app.open_approval(ask);
                app.status.elapsed = started.elapsed();
                paint(screen, app)?;
            }
            Some(proposed) = plans.recv() => {
                // The run is stopped inside `PlanGate::review`, and while it is
                // io-harness's own policy denies every write and every exec — so
                // the workspace behind this overlay cannot change while it is up.
                app.open_plan(proposed);
                app.status.elapsed = started.elapsed();
                paint(screen, app)?;
            }
            Some(asked) = questions.recv() => {
                // The same shape one seam over: the run is stopped inside
                // `Responder::answer` and the loop keeps turning. It arrives on
                // ANY turn — the responder is on the one contract both arms are
                // handed, since 0.12.0. (This comment said "only a contained
                // turn" until 0.13.0, which was true of 0.11.0 and of nothing
                // since.)
                app.open_intent(asked);
                app.status.elapsed = started.elapsed();
                paint(screen, app)?;
            }
            Some(input) = inputs.recv() => {
                match input {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Bound rather than tested inside the guard: `App::key`
                        // changes state, and a match guard is not a place to put
                        // something that does.
                        let command = app.key(key);
                        match command {
                            Command::Interrupt => {
                                // **One path for both kinds of turn, and it is
                                // the observer's.** Both arms now hold a
                                // `SteerInbox` and `Steer::interrupt` would also
                                // end the turn at a step boundary — and this key
                                // is deliberately not routed through it. The two
                                // paths end in the same `RunOutcome::Cancelled`
                                // but they are recorded by different code in
                                // io-harness, and moving the one key this product
                                // refuses to let a configuration file rebind onto
                                // a different mechanism buys an operator nothing
                                // they can see. `Flow::Cancel` is honoured at the
                                // next step boundary, which is the sentence
                                // `App::interrupt_or_quit` has just put on screen.
                                canceller.store(true, std::sync::atomic::Ordering::Relaxed);
                                stopped = true;
                            }
                            // **The second press, and it does not wait.** A
                            // cancel is honoured at a step boundary, and a step
                            // inside a slow tool call or a wide fan-out can be
                            // seconds away — which is a key that reads as
                            // ignored. Breaking here drops the turn's future,
                            // which ends it where it stands.
                            //
                            // What that costs is the run's own record: io-harness
                            // closes a run it cancelled and closes nothing for a
                            // run that was dropped. So it is the second press and
                            // never the first, and `App::finished` below still
                            // commits whatever streamed before it.
                            Command::Abandon => {
                                canceller.store(true, std::sync::atomic::Ordering::Relaxed);
                                stopped = true;
                                // **A turn that had done nothing is taken back
                                // whole.** `App::undoable` is what decides that
                                // — no step, nothing streamed, nothing on screen
                                // but the echo of the prompt — and what it buys
                                // is the session the operator had a moment
                                // before: the goal line comes off the screen and
                                // the prompt goes back in the composer, ready to
                                // edit or to send again.
                                //
                                // Only what is still on screen. Rows that have
                                // scrolled past the top belong to the terminal's
                                // scrollback, which nothing here reaches, so an
                                // echo that long is left where it is.
                                if app.undoable() && app.turn_rows() <= screen.erasable() {
                                    let (rows, _) = app.undo_turn();
                                    // The viewport is placed again at the rows
                                    // that came back, which asks the terminal
                                    // where its cursor is — so nothing may be
                                    // reading stdin while it lands.
                                    let _parked = io_cli::stdin::placing();
                                    let _ = screen.rewind(rows);
                                    drop(_parked);
                                    undone = true;
                                }

                                // Nothing is constructed to stand in for a result
                                // io-harness never returned. `TurnResult` is
                                // `#[non_exhaustive]` and could not be anyway,
                                // and a fabricated outcome would be this
                                // interface reporting a run it did not observe.
                                break None;
                            }
                            // **`/steer` — what is queued goes to the turn that is
                            // still running, and only because the operator asked.**
                            //
                            // The open question this release had to answer was
                            // whether a line typed mid-turn should reach the
                            // agent by itself. It must not, and the reason is the
                            // second note where the inbox is built: a delivered
                            // steer emits no event, so an interface cannot show
                            // that the agent heard it. A
                            // line sent by default would leave the screen with no
                            // echo, no cell and no confirmation — the same shape
                            // as the keystroke 0.16.0 lost, which `App::compose`
                            // has just been fixed to stop losing. A queue is
                            // visible state a surface can draw; a steer is not.
                            // And `Steer::say` has no undo: an operator writing a
                            // note to themselves while an agent works would be
                            // changing what it does, once per stray sentence.
                            //
                            // So the queue keeps its promise — three lines are
                            // three turns — and this is the one word that spends
                            // it differently. It is a slash command rather than a
                            // key because `App::compose` already lets `/` through
                            // mid-turn on purpose, so nothing in the library had
                            // to learn a new mode for it.
                            //
                            // **Said, never claimed delivered.** io-harness takes
                            // the message at the next step boundary and records a
                            // `steered` row in the run's own trace; no
                            // `RunEvent` carries it, so `Ok` here means the
                            // channel accepted the words and nothing more. The
                            // sentence says exactly that. What *is* certain is
                            // the negative, and the drain after this loop is
                            // where it gets said.
                            // **`/compact` mid-turn.** `Steer::fold` lands at the
                            // turn's next step boundary — the same promise
                            // `/steer` makes, for the same reason: a tool call in
                            // flight is not a safe place to change the
                            // conversation out from under the model.
                            //
                            // **Asked, never reported folded.** io-harness names
                            // four conditions under which an accepted request
                            // folds nothing, and the request is spent under all
                            // four, so the only thing that may say a fold happened
                            // is `EventKind::Compacted`.
                            // The first word, so `/compact …` reaches the arm that answers it
                            // rather than the mid-turn refusal below, which
                            // would tell the operator to interrupt the turn
                            // first — the opposite of what this command does.
                            Command::Slash(ref line)
                                if line.split_whitespace().next() == Some("compact") => {
                                let dash = app.theme.glyphs.dash;
                                let said = io_cli::compact::Said::asked(contract.compaction, true);
                                if said == io_cli::compact::Said::Sent {
                                    match steer.fold() {
                                        Ok(()) => {
                                            folding = true;
                                            app.say(Tone::Muted, said.line(dash));
                                        }
                                        // Unreachable while `inbox` is alive, and
                                        // said rather than swallowed anyway.
                                        Err(error) => app.say(
                                            Tone::Warning,
                                            format!("nothing is listening {dash} {error}"),
                                        ),
                                    }
                                } else {
                                    app.say(Tone::Muted, said.line(dash));
                                }
                            }
                            // The first word, so `/steer …` reaches the arm that answers it
                            // rather than the mid-turn refusal below, which
                            // would tell the operator to interrupt the turn
                            // first — the opposite of what this command does.
                            Command::Slash(ref line)
                                if line.split_whitespace().next() == Some("steer") => {
                                let dash = app.theme.glyphs.dash;
                                let mut sent = 0usize;
                                // The summary below is skipped when this is set,
                                // because a count is not the answer to a refusal
                                // — and `App::say` keeps one notice, so a
                                // summary written over the error would be the
                                // interface reporting success on the one path
                                // where there was none.
                                let mut refused = false;
                                // Each on its own, in the order they were typed:
                                // io-harness pushes one `Observation` per message
                                // and the model reads them in that order. Joining
                                // them would be one paragraph the operator never
                                // wrote.
                                while let Some(waiting) = app.next_queued_prompt() {
                                    if let Err(error) = steer.say(waiting.clone()) {
                                        // Unreachable while `inbox` is alive —
                                        // and said rather than swallowed anyway,
                                        // because the one thing worse than a
                                        // correction that arrives late is one
                                        // that reports success and goes nowhere.
                                        app.queue_prompt(waiting);
                                        app.say(
                                            Tone::Warning,
                                            format!("nothing is listening {dash} {error}"),
                                        );
                                        refused = true;
                                        break;
                                    }
                                    // Into the transcript, not the footer. A
                                    // steered line becomes an observation in the
                                    // run's ledger, so it is part of the
                                    // conversation rather than about it — and it
                                    // is the only part that will never get an
                                    // echo of its own, because it is not a turn.
                                    app.record(
                                        Tone::Muted,
                                        format!("[mid-turn] {}", waiting.trim()),
                                    );
                                    sent += 1;
                                }
                                if !refused {
                                    app.say(
                                        Tone::Muted,
                                        match sent {
                                            0 => format!(
                                                "nothing queued {dash} type a line first, then \
                                                 /steer"
                                            ),
                                            1 => format!(
                                                "sent {dash} the turn reads it at its next step"
                                            ),
                                            many => format!(
                                                "{many} sent {dash} the turn reads them at its \
                                                 next step"
                                            ),
                                        },
                                    );
                                }
                            }
                            // Refused with a sentence rather than dropped in
                            // silence. `/resume`, `/fork` and the rewind each move
                            // the session head this turn is about to write, and
                            // `/model` would take effect at a moment nobody could
                            // predict — so all four wait, and the operator is told
                            // that is what is happening rather than left pressing
                            // a key that appears to do nothing.
                            //
                            // A `!` line waits for a different reason and the
                            // same sentence covers it: `shell::run` blocks until
                            // the child exits, and blocking here is blocking the
                            // select loop — the turn's events would stop being
                            // drained, the ticker would stop, and `Ctrl+C` would
                            // be unreadable for as long as the command took. So
                            // the block is confined to the idle prompt, where
                            // there is no turn for it to stall.
                            Command::Slash(_) | Command::Shell(_) => {
                                let dash = app.theme.glyphs.dash;
                                app.say(
                                    Tone::Muted,
                                    format!(
                                        "not while a turn is running {dash} Ctrl+C interrupts it \
                                         first"
                                    ),
                                )
                            }
                            // **`Command::Submit` no longer arrives here, and
                            // that is the fix.** Through 0.16.0 a prompt typed
                            // mid-turn reached this arm and was dropped without a
                            // word — the worst shape a lost keystroke can take,
                            // because the composer had already emptied and there
                            // was nothing left to press `Enter` on again.
                            // `App::compose` now queues it instead, so what falls
                            // through here is only what this loop has always
                            // ignored. The guard is in the library rather than in
                            // this arm on purpose: nothing in `src/main.rs` is
                            // linked by an integration test, so a branch written
                            // here could not be sabotaged and would not be
                            // covered — `tests/queue.rs` asserts the queueing
                            // where a test can reach it.
                            _ => {}
                        }
                    }
                    Event::Resize(width, height) => {
                        screen.resize(width, height).map_err(|error| error.to_string())?;
                    }
                    // A turn in flight is not a reason to drop what the operator
                    // pasted. Typing already reaches the composer here; a paste
                    // that did not would be the same keystroke treated two ways.
                    // No picker can be open on this path — one owns the keyboard
                    // before a turn starts — and the approval is refused inside
                    // `App::paste`.
                    Event::Paste(text) => {
                        match app.paste(&text, false) {
                            // The turn holds the session and staging needs it,
                            // so a picture dropped mid-turn waits rather than
                            // being dropped or half-attached. It is staged the
                            // moment the turn lets go, which is the next thing
                            // the driver does.
                            io_cli::app::Pasted::Picture(paths) => {
                                for path in paths {
                                    app.queue_picture(&path);
                                }
                                app.say(
                                    Tone::Muted,
                                    "picture held until this turn ends, then attached",
                                );
                            }
                            io_cli::app::Pasted::Text | io_cli::app::Pasted::Refused => {}
                        }
                    }
                    _ => {}
                }
                app.status.elapsed = started.elapsed();
                paint(screen, app)?;
            }
        }
    };

    // Drain whatever the run emitted between its last event and its return, or
    // the tail of a turn is lost.
    // One age for the whole drain. These events all arrived while the loop was
    // not looking, and inventing distinct ages for them would be fiction.
    let at = started.elapsed();
    let width = screen.width();
    while let Ok(event) = events.try_recv() {
        app.event(&event, at);
        // The diff belongs here too. Without it a `Step { changed: true }` still
        // queued when the turn's future won the select loses its diff silently —
        // and the last step of a turn is exactly the one that loses that race,
        // so the edit a reader most wants to see is the one that vanishes.
        commit_edits(app, store, &event, width);
        // And on the drain, for the same race the two lines above it are here
        // for: the last step of a turn is exactly the one whose event the select
        // loop loses to the turn's own return.
        note_context(app, store, &event, seen, &contract);
        // And on the drain too, for the same race: the last step of a turn is the
        // one whose event the select loop loses, and it is also the step that
        // makes the largest single difference to what the turn cost.
        note_cost(app, store, config, event.run_id);
        note_fleet(app, store, &event, &contract);
        // And the picture, for the same reason and the same race: a `view_image`
        // on the turn's last step is exactly the one the drain would otherwise
        // lose.
        commit_viewed(screen, app, &root, policy, &event)?;
        commit_fold(app, store, &event, &mut folding);
    }
    app.finished();

    // **The one delivery fact this interface can state, and it is the negative
    // one.** io-harness emits no event for a message it delivered, so there is
    // nothing to read on the way in; what is left in the inbox after the turn has
    // returned, though, is exactly what no step got to. `SteerInbox::pending` is
    // public for this — a caller draining an inbox it is no longer handing to a
    // turn, rather than discovering later that the operator's last words went
    // with the channel.
    //
    // Into the transcript, because the words are the operator's and the sentence
    // is about a run that has already ended — a footer notice would be gone at
    // the next keystroke, and it would fight with `stopped` below.
    //
    // **And put back, unless the operator stopped the turn.** `/steer` REMOVES
    // each line from the queue to send it, and the send cannot fail while the
    // inbox is alive — so a turn that reaches its last step before the next
    // boundary consumed the queue, delivered nothing, and left the operator with
    // three prompts' worth of work in a sentence. In the release whose whole
    // headline is that a mid-turn prompt is no longer destroyed.
    //
    // The window is not narrow either: `/steer` is typed exactly when the agent
    // looks close to done, which is exactly when there is no boundary left.
    //
    // So they go back to the FRONT of the queue, in order, and the driver's drain
    // runs them as the next turns — which is what the queue promised in the first
    // place. A turn the operator STOPPED keeps the old behaviour and drops them,
    // because one press of the stop key must not start another turn; the earlier
    // comment claimed a composer was waiting to catch them, and nothing ever put
    // them in it.
    let lost = inbox.pending().messages;
    for line in &lost {
        app.record(
            Tone::Muted,
            format!("[mid-turn, not delivered] {}", line.trim()),
        );
    }
    if !lost.is_empty() {
        if stopped {
            app.record(
                Tone::Muted,
                format!(
                    "{} went with the turn you stopped",
                    if lost.len() == 1 {
                        "it".to_string()
                    } else {
                        format!("all {} of them", lost.len())
                    }
                ),
            );
        } else {
            let waiting = app.requeue_prompts(lost);
            app.record(
                Tone::Muted,
                format!(
                    "back in the queue {} {waiting} waiting",
                    app.theme.glyphs.dash
                ),
            );
        }
    }

    // **The fold that was asked for and never announced.** A conversation shorter
    // than `Compaction::keep_recent` has no prefix a paragraph could stand in for,
    // and an interrupt at the same boundary wins — io-harness reports neither,
    // because there is nothing to report. The request is spent under both, so this
    // is the end of the story rather than a retry, and it says nothing folded
    // rather than that a fold happened.
    //
    // `record` and not `say`, for the reason the drain above it gives: a footer
    // notice is gone at the next keystroke and would fight with the stop line.
    if folding {
        let dash = app.theme.glyphs.dash;
        app.record(
            Tone::Muted,
            io_cli::compact::Said::unfolded(contract.compaction, stopped).line(dash),
        );
    }

    match outcome {
        // The operator's sentence in front of the harness's own line — see
        // `io_cli::failure`. A provider that will not take an image says so in the
        // vocabulary of a routing layer, and "HTTP 404" is not something anybody
        // can act on.
        Some(Err(error)) => app.record(Tone::Error, io_cli::failure::said(&error)),
        // Abandoned. The run's own record is whatever io-harness had written by
        // the time the future was dropped, and saying so is the honest line: the
        // work above is real and the turn did not finish.
        None if undone => {}
        None => app.say(Tone::Muted, "stopped"),
        Some(Ok(_)) => {}
    }
    app.status.elapsed = started.elapsed();
    paint(screen, app)?;
    Ok(stopped)
}

/// Draw what a step changed, by asking the store what it recorded.
///
/// **Anchored on `Step` and never on `ToolCall`.** io-harness documents
/// `ToolCall` as *a tool was invoked, before its result is known*, while `Step`
/// is emitted after the step has been committed to the store — so a read at the
/// tool call is a read of a row that may not be there yet, and the two events
/// are one line apart in a transcript, which is what would make that invisible
/// until it was a bug.
///
/// This is the first time this product reads the durable trace while a run is
/// live. It is safe because it happens on the drain half of the select, where
/// the turn future is suspended, and because every `Store` call is synchronous —
/// nothing this function does is held across an await, so there is no shape here
/// that can deadlock against the turn holding the same `&Store`.
///
/// A read that fails degrades to a line saying so. A run whose work succeeded is
/// not a run to panic over because the trace could not be re-read, and silence
/// would say the step changed nothing.
/// Keep `ctx N%` saying what `/context` would say.
///
/// **One quantity, one source, because a live run found two.** The page totals
/// the request against the window the contract declares; the field divided the
/// observation section by the same window and read `0%` where the page read
/// eighteen. Both numbers were defensible and the pair was not: the percentage is
/// what makes an operator open the page, so it has to be the page's own number.
///
/// The trace is the fallback rather than the answer. A step lands before the
/// completion call that follows it is snapshotted, so on the very first step
/// there is no request to measure and the section the trace records is the only
/// number there is — better than a blank field, and it converges the moment a
/// request has been seen.
/// Give the fleet the two facts the event stream does not carry.
///
/// **On `Step` and on nothing else, and the reason is a race rather than a
/// preference.** io-harness documents `Step` as emitted once the step has been
/// committed to the store, so the spawn row is there to be read. The obvious
/// place — `Spawned`, the event that announces the child — is a read of a row
/// that may not exist yet, and it fails the way races fail: green on a quiet
/// laptop, intermittent on a loaded CI machine. `src/status.rs` already records
/// this rule for the context read; this is the same rule applied to addresses.
///
/// **Naming is cheap and unconditional; traffic is not and is conditional.**
/// `tree_addresses` costs one query returning one row per agent — a number that
/// grows with the size of the fleet and not with the length of the run — and it
/// is what makes a child draw as `reviewer` rather than `run 41`, which is worth
/// having on screen whether or not the fleet pane is open, because the scrollback
/// keeps it.
///
/// The mailbox is the opposite shape. `Store::messages_for` returns a
/// recipient's **whole** mailbox every call, so polling it once a step would
/// re-read every message already read — `n(n+1)/2` reads over an `n`-message
/// run, which is exactly the trap `src/status.rs` names for `context_events`.
/// So it is read only while the pane that shows it is open. That is not a
/// half-measure: a message an operator cannot see has nowhere to arrive, and the
/// pane is refreshed by the next step the moment they open it.
fn note_fleet(
    app: &mut App,
    store: &Store,
    event: &io_harness::RunEvent,
    contract: &io_harness::TaskContract,
) {
    if !matches!(event.kind, io_harness::EventKind::Step { .. }) {
        return;
    }
    // A run with no children has an empty tree and nothing to name. Asking the
    // store anyway would be one query per step of every ordinary turn, which is
    // the overwhelming majority of them.
    if app.fleet.is_empty() {
        return;
    }
    let Ok(root) = store.run_root(event.run_id) else {
        return;
    };
    if let Ok(addresses) = store.tree_addresses(root) {
        app.fleet.name(&addresses, &contract.agents);
    }
    if !app.fleet_open() {
        return;
    }
    // One mailbox per child that has an address, and a child without one has no
    // name to show a message under yet. Bounded by the fleet, and re-read whole
    // because `Fleet::traffic` replaces rather than appends — `messages_for` is
    // the non-consuming reader, so this never takes delivery of a sibling's mail.
    let mut traffic: Vec<io_cli::fleet::Message> = Vec::new();
    for child in app.fleet.children() {
        let Some(address) = child.address.as_deref() else {
            continue;
        };
        if let Ok(messages) = store.messages_for(child.run_id) {
            traffic.extend(
                messages
                    .iter()
                    .map(|message| io_cli::fleet::Message::received(message, address)),
            );
        }
    }
    app.fleet.traffic(traffic);
}

fn note_context(
    app: &mut App,
    store: &Store,
    event: &io_harness::RunEvent,
    seen: &io_cli::context::Seen,
    contract: &io_harness::TaskContract,
) {
    if let Some(request) = seen.latest() {
        // **What is LEFT of the run budget, not all of it.** io-harness assembles
        // against the unspent remainder — a run low on budget gets a smaller
        // window, down to the floor — and `context::window`'s own doc says so.
        // Passing the flat maximum reports a window the turn can no longer afford
        // and a share several times too small, which under-reports pressure
        // exactly when there is pressure. Only moves when `[run] max_tokens` is
        // set; with none, the harness's own expression is flat too.
        let remaining = contract
            .max_tokens
            .map(|cap| cap.saturating_sub(app.status.run_tokens.unwrap_or(0)));
        app.status
            .note_context_request(&request, contract, remaining);
    } else {
        app.status.note_context_from(store, event);
    }
}

/// Set the status line's cost field from what this run has actually called.
///
/// Beside [`note_context`] and on the same events, because the two are the same
/// shape of fact: both are read from the store rather than accumulated off the
/// stream, both change only when a step lands, and both are decorative enough
/// that a failed read is a missing field rather than a notice.
///
/// **The table is asked of the configuration on every call rather than held.** A
/// `/config` write or a price refresh mid-session changes what a turn costs, and a
/// table captured when the session opened would go on reporting the old rate for
/// the rest of it — which is the one thing a figure with a currency in front of it
/// must not do.
fn note_cost(app: &mut App, store: &Store, config: &io_harness::Config, run_id: i64) {
    let table = io_cli::cost::table(config);
    app.status.note_cost_from(store, run_id, &table);
}

/// Report a fold that was asked for — once, and only from the event that
/// announces it.
///
/// io-harness emits `Compacted` the moment a fold lands and nothing at all when
/// one does not, so this is the whole of what may say a fold happened. It runs on
/// both event paths because the last step of a turn is exactly the one whose
/// event the select loop loses to the turn's own return.
fn commit_fold(app: &mut App, store: &Store, event: &io_harness::RunEvent, folding: &mut bool) {
    if !*folding {
        return;
    }
    let Some(said) = io_cli::compact::Said::folded(store, event) else {
        return;
    };
    *folding = false;
    let dash = app.theme.glyphs.dash;
    app.say(Tone::Muted, said.line(dash));
}

fn commit_edits(app: &mut App, store: &Store, event: &io_harness::RunEvent, width: u16) {
    let io_harness::EventKind::Step { changed: true, .. } = &event.kind else {
        return;
    };
    match store.edits(event.run_id) {
        Ok(edits) => app.edits(&io_cli::diff::for_step(edits, event.step), width),
        Err(error) => app.say(
            Tone::Muted,
            format!("the diff for this step could not be read: {error}"),
        ),
    }
}

/// Put the whole conversation back into the terminal's own scrollback.
///
/// Upward and never into a pane. The viewport is eight rows and cannot grow, and
/// there is no alternate screen in this product — so the place a reader reads
/// something long is the terminal's own buffer, where its search, its selection
/// and tmux copy-mode already work. A failure to read the store says so and
/// changes nothing else: a session is not worth ending over a transcript.
fn commit_transcript(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    session: &Session,
    store: &Store,
    theme: &Theme,
) -> Result<(), String> {
    let lines = match session.transcript(store) {
        Ok(transcript) => io_cli::transcript::lines(&transcript, theme),
        Err(error) => vec![theme.notice(
            Tone::Muted,
            format!("the transcript could not be read: {error}"),
        )],
    };
    screen.commit(&lines).map_err(|error| error.to_string())
}

/// The last run of this session, if it has had one.
fn last_run(session: &Session, store: &Store) -> Option<io_harness::TranscriptTurn> {
    session
        .transcript(store)
        .ok()?
        .turns
        .into_iter()
        .rfind(|turn| turn.on_path)
}

/// What `/expand` commits: the thought that did not fit, then the step detail.
///
/// Two sources, because they are two different kinds of "more". The step's
/// output is in the durable trace and is read back from it; the model's thinking
/// is in neither the trace nor the next prompt — io-harness does not store it —
/// so the only copy of a fitted thought is the one [`Events`](io_cli::events::Events) kept, and this is
/// where it is spent.
fn expand(
    session: &Session,
    store: &Store,
    theme: &Theme,
    thought: Option<&str>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(thought) = thought {
        lines.push(theme.notice(Tone::Muted, "the thought, in full"));
        lines.extend(
            thought.lines().map(|line| {
                Line::from(Span::styled(format!("  {line}"), theme.style(Tone::Muted)))
            }),
        );
        lines.push(Line::from(""));
    }
    lines.extend(step_detail(session, store, theme));
    lines
}

/// The last step's full detail, from the run's durable trace.
///
/// This is the other half of collapsing a tool cell: the screen is not the
/// archive, so the output goes to the store when it happens and comes back here
/// when somebody asks for it. Committed upward like everything else that shows
/// more of something.
fn step_detail(session: &Session, store: &Store, theme: &Theme) -> Vec<Line<'static>> {
    let Some(turn) = last_run(session, store) else {
        return vec![theme.notice(Tone::Muted, "nothing has run in this session yet")];
    };
    let steps = match store.steps(turn.run_id) {
        Ok(steps) => steps,
        Err(error) => {
            return vec![theme.notice(Tone::Muted, format!("the trace could not be read: {error}"))]
        }
    };
    let Some(step) = steps.last() else {
        return vec![theme.notice(Tone::Muted, "that turn recorded no steps")];
    };
    if step.result.trim().is_empty() {
        return vec![theme.notice(
            Tone::Muted,
            format!("step {} recorded no detail", step.step),
        )];
    }

    let mut lines = vec![theme.notice(
        Tone::Muted,
        format!("step {} {} {}", step.step, theme.glyphs.dash, step.decision),
    )];
    lines.extend(
        step.result
            .lines()
            .map(|line| Line::from(Span::styled(format!("  {line}"), theme.style(Tone::Muted)))),
    );
    lines.push(Line::from(""));
    lines
}

/// What `/copy` should put on the clipboard, or why there is nothing to put.
fn to_copy(session: &Session, store: &Store, what: Copied) -> (Option<String>, String) {
    let Some(turn) = last_run(session, store) else {
        return (None, "nothing has run in this session yet".into());
    };
    match what {
        Copied::Answer => match turn.reply {
            Some(reply) if !reply.trim().is_empty() => (Some(reply), String::new()),
            // A turn that stopped on a ceiling, a refusal or an interrupt has no
            // closing message, and inventing one would misreport the ending.
            _ => (None, "that turn ended without an answer to copy".into()),
        },
        Copied::Diff => match store.patch(turn.run_id) {
            Ok(patch) if !patch.trim().is_empty() => (Some(patch), String::new()),
            Ok(_) => (None, "that turn changed no files".into()),
            Err(error) => (None, format!("the patch could not be read: {error}")),
        },
    }
}

/// The first-run wizard. Returns the theme chosen, or `None` if it was abandoned.
async fn wizard(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    theme: Theme,
) -> Result<Option<Theme>, String> {
    let mut wizard = Wizard::new(theme);
    loop {
        screen
            .draw(|frame| wizard.render(frame, frame.area()))
            .map_err(|error| error.to_string())?;
        if wizard.done() {
            return Ok(Some(wizard.theme()));
        }

        let Some(event) = inputs.recv().await else {
            // Only the keyboard going away ends the wizard here. Every other
            // event is the wizard's own business — see `Wizard::event`, which is
            // where a paste used to fall through to "the user left".
            return Ok(None);
        };
        if let Event::Resize(width, height) = event {
            screen
                .resize(width, height)
                .map_err(|error| error.to_string())?;
        }

        match wizard.event(&event) {
            Progress::Idle => {}
            Progress::Commit(lines) => {
                screen.commit(&lines).map_err(|error| error.to_string())?;
            }
            Progress::Cancelled => {
                screen
                    .commit(&[wizard.theme().notice(Tone::Muted, "nothing was written")])
                    .map_err(|error| error.to_string())?;
                return Ok(None);
            }
            Progress::Verify(spec) => {
                // Drawn before the call, so the screen says what it is waiting on.
                screen
                    .draw(|frame| wizard.render(frame, frame.area()))
                    .map_err(|error| error.to_string())?;
                match verify::credential(&spec).await {
                    Ok(()) => {
                        if let Progress::Catalogue(spec) = wizard.verified() {
                            let models = verify::catalogue(&spec).await;
                            wizard.catalogue(models);
                        }
                    }
                    Err(message) => {
                        wizard.rejected(message);
                    }
                }
            }
            Progress::Catalogue(spec) => {
                let models = verify::catalogue(&spec).await;
                wizard.catalogue(models);
            }
            Progress::Write(path, contents) => {
                settings::write(&path, &contents)
                    .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                let theme = wizard.theme();
                screen
                    .commit(&[
                        theme.notice(Tone::Success, format!("wrote {}", path.display())),
                        ratatui::text::Line::from(""),
                    ])
                    .map_err(|error| error.to_string())?;
                return Ok(Some(theme));
            }
        }
    }
}

/// Which picker is open, and what its rows point at.
///
/// A `Picker` row carries a label and nothing else, so a surface whose rows stand
/// for database ids keeps them here, parallel to the rows, and reads the id back
/// by index. Matching a label back to a session would be matching a prompt, and
/// two sessions can begin with the same prompt.
enum Pick {
    Theme,
    Model,
    /// Session ids, in the order the rows are drawn, read back through
    /// `sessions::pick`. The resume picker may carry one row more than there are
    /// ids — the line saying the list was cut — and an index past the end is that
    /// row, which the arm answers with a sentence.
    Resume(Vec<i64>),
    /// Turn ids, in the order the rows are drawn, read back through
    /// `sessions::pick_turn` — which also hands back the turn number the row was
    /// drawn with, so the sentence and the branch cannot disagree.
    Fork(Vec<i64>),
    /// The slash palette. Its rows are `commands::palette()`, which is the
    /// command inventory and then the templates, in that order, so the chosen
    /// index reads straight back through `commands::palette_pick` — no list is
    /// carried here because the rows already address the thing they stand for.
    Palette,
    /// The import plan, and which of its items are switched on.
    ///
    /// **The two vectors are the same length and are indexed together**, which is
    /// the arrangement this codebase has already been bitten by once: in 0.20.0 an
    /// index from one list was handed to a function expecting another's and
    /// removed the wrong entry. They are safe here for a reason that must hold
    /// whenever this arm is edited — `import_rows` builds one row per item in
    /// order and appends exactly one row after them, so a chosen index below
    /// `items.len()` addresses the item at that index and the single index equal
    /// to it is the write row. Nothing filters, so no row index is ever a
    /// different list's position.
    ///
    /// Carried rather than recomputed between keystrokes, because recomputing the
    /// plan would re-read the filesystem underneath the operator and could change
    /// the row count while they are choosing.
    Import {
        items: Vec<io_cli::import::Item>,
        accepted: Vec<bool>,
    },
    /// The accepted items, held while the operator says which endpoint answers
    /// for an imported model. Nothing has been written when this is open: the
    /// question comes before the write, so declining it still writes the rest.
    ImportModel(Vec<io_cli::import::Item>),
    /// One MCP server, and where in which file it is declared. The position is a
    /// `servers::At`, read from that file's own bytes — never this surface's row
    /// index, which addresses a merged view across three scopes.
    McpRemove {
        id: String,
        at: io_cli::servers::At,
    },
    /// One link of the provider chain, and where it is declared. `first` decides
    /// whether promoting it is offered at all: a control that is drawn and does
    /// nothing is worse than one that is absent.
    ProviderVerb {
        label: String,
        at: io_cli::providers::At,
        first: bool,
    },
    /// One directory of the workspace, in the order `list_dir` sorted it, so a
    /// chosen index reads straight back through `complete::pick`. The rows are
    /// last components rather than paths — see `complete::rows` for why — which
    /// is exactly why the entries are carried here and not read off a label.
    Complete(Vec<io_harness::tools::Entry>),
    /// The settings surface, in the order `configure::rows` drew them, so a
    /// chosen index reads straight back against this list. The paths are carried
    /// rather than read off a label because a label is a rendered string and the
    /// key is what a write addresses.
    Config(Vec<String>),
    /// The MCP servers, in the order `servers::rows` drew them. Read-only in
    /// this release: the write verbs are reached through `/config`, and the two
    /// the roadmap named that io-harness cannot express are not offered at all.
    Mcp,
    /// The provider chain, in the order `providers::rows` drew it — which is
    /// the order a turn tries it.
    Provider,
    /// Every skill, in the order `skillview::rows` drew them, carried as the
    /// `(name, path)` each row stood for.
    ///
    /// **Carried rather than re-derived by index**, which is the rule every other
    /// list-bearing variant here follows. The view IS read again when a row is
    /// chosen — the directory can change between two keystrokes — and an index
    /// held across two readings of it addresses whichever skill is in that
    /// position now. So the pair says which skill the operator actually read, and
    /// a row that is no longer there is answered with a sentence.
    Skills(Vec<(String, std::path::PathBuf)>),
    /// One skill, and the move that decides whether the model is offered it.
    ///
    /// **Two steps rather than one, and for the reason [`Pick::ConfigScope`]
    /// gives**: this is a picker that would otherwise change a file on the way
    /// past. `/mcp`'s own arm says the same thing about why it opens no editor.
    /// So the list says what a skill is, and the change is a second, named answer.
    SkillToggle {
        name: String,
        path: std::path::PathBuf,
        enabled: bool,
    },
    /// The capability bundles this configuration declared, loaded and refused.
    ///
    /// **The whole view rather than a list of ids, and for the same reason
    /// [`Pick::Skills`] carries pairs**: descending into a bundle has to show
    /// what that bundle contributed, and re-reading `Config::plugins()` to answer
    /// a keystroke would be a second reading of directories that can change
    /// between two of them — so the rows an operator chose from and the detail
    /// they descend into come from one reading.
    ///
    /// A refused bundle is in here too, and choosing one shows io-harness's
    /// sentence rather than a detail pane: there is nothing to descend into,
    /// because the bundle contributed nothing at all.
    Plugins(io_cli::pluginview::View),
    /// One bundle's contributions, and the one thing that can be done about it.
    ///
    /// `bundle` is the resolved directory, which is the only thing a row on screen
    /// and an entry in a file genuinely share — `Config::plugins()` reports neither
    /// the scope that declared a bundle nor its position in that file, so the path
    /// is what `pluginview::declared_at` matches on to find the entry.
    ///
    /// `action_at` is where the removal row sits, recorded by the code that built
    /// the rows rather than recomputed here.
    PluginEntry {
        id: String,
        bundle: std::path::PathBuf,
        action_at: usize,
    },
    /// The second half of removing a bundle: which entry, and are you sure.
    ///
    /// **Two steps rather than one, for the reason [`Pick::SkillToggle`] gives** —
    /// this is a picker that would otherwise change a file on the way past. It
    /// carries the scope and index `declared_at` resolved, so the confirmation acts
    /// on the entry that was found rather than searching again against a file the
    /// operator may have edited in between.
    PluginRemove {
        id: String,
        scope: io_harness::config::Scope,
        index: usize,
    },
    /// The named profiles a file declares, in the order `configure::profiles`
    /// sorted them.
    Profile(Vec<String>),
    /// Which file a change goes into, and the change it is waiting on.
    ///
    /// Two steps rather than one because *which scope* is half the decision and
    /// this product has three of them — a write that guessed would put an
    /// operator's credential in the file a repository ships.
    ConfigScope {
        key: String,
        value: String,
        paths: Vec<(io_harness::config::Scope, std::path::PathBuf)>,
    },
    /// Which memory file a remembered line goes into, and the line waiting on it.
    ///
    /// Two steps for the reason [`Pick::ConfigScope`] has two, and a sharper one:
    /// the three files differ *only* in who else reads them — a repository, a
    /// checkout, this machine — so a write that guessed would be a guess about
    /// whether a private note is committed.
    RememberScope {
        line: String,
        paths: Vec<(io_harness::config::Scope, std::path::PathBuf)>,
    },
    /// The memory page, in the order `commands::memory_page` drew it.
    ///
    /// The rows are headings, instruction files and notes interleaved, so a
    /// position in either underlying list addresses neither. `held` is the
    /// parallel vector that function returns beside the rows — same order, same
    /// length, built in the same pass — and it is what a chosen index reads back
    /// against.
    Memory {
        held: Vec<io_cli::commands::Held>,
    },
    /// One note, and the verbs offered on it.
    ///
    /// The bucket is carried rather than re-derived, because a key can exist in
    /// both and `recall::pin` and `recall::forget` each take one: a verb applied
    /// to the wrong bucket either does nothing or acts on a same-named note the
    /// operator was not looking at.
    Remembered {
        scope: io_cli::recall::Scope,
        key: String,
        verbs: Vec<io_cli::commands::Verb>,
    },
}

/// The completion picker over one directory of the workspace, or `None` when the
/// policy leaves nothing in it to offer.
///
/// Both the `@` that opens it and the descent that replaces it come through
/// here, so the bound, the cut note and the title are applied in one place
/// rather than in two that could drift. Every decision it makes is a library
/// call; what is here is the wiring, which is all this file may hold.
fn completion(
    root: &std::path::Path,
    policy: &Policy,
    dir: &str,
    glyphs: &io_cli::glyphs::Glyphs,
) -> Result<Option<(Picker, Pick)>, String> {
    let (found, cut) = complete::entries(root, policy, dir)?;
    if found.is_empty() {
        return Ok(None);
    }
    let mut rows = complete::rows(&found);
    // Last, and a row rather than a notice, so a list that was cut cannot read as
    // a complete one — the shape `/resume` already uses. It stands for no entry,
    // and choosing it does nothing.
    if let Some(note) = complete::cut_note(cut, rows.len()) {
        rows.push(Row::new(note));
    }
    Ok(Some((
        Picker::new(complete::title(dir, glyphs), rows),
        Pick::Complete(found),
    )))
}

/// How often an attached child's events are asked for.
///
/// io-harness is explicit that `Attach` is a poll and that there is no push, so
/// this number is the whole of the latency an operator sees. A tenth of a second
/// is under the threshold at which a terminal reads as live, and the query behind
/// it is one indexed read of rows after a cursor — not a scan — so the cost of
/// asking and finding nothing is close to nothing.
const ATTACH_POLL: Duration = Duration::from_millis(100);

/// Watch a child that is already running, until it stops or the operator leaves.
///
/// **A mode rather than a background poll, and the idle loop's own shape is the
/// argument.** That loop blocks on `inputs.recv()` and has no tick — there is
/// nowhere for a periodic read to live without giving every idle session a timer
/// it does not need. Watching a child is also inherently something an operator is
/// *doing*: they picked a row and asked to see it. So this borrows the loop for
/// as long as they want it and gives it back on `Esc`.
///
/// **`from_now`, not from the beginning.** A detached child may have been running
/// for minutes and its earlier steps are in the trace already; replaying them
/// into the scrollback would bury the thing the operator opened this to see. The
/// cursor starts where they asked.
///
/// The events are fed to the same `App::event` the live stream uses, so a child's
/// step draws exactly as it would have if its parent had waited for it. That is
/// the property worth having: attaching changes when an operator sees a run, not
/// what it looks like.
async fn watch_child(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    store: &Store,
    inputs: &mut UnboundedReceiver<Event>,
    run_id: i64,
) -> Result<(), String> {
    // **The watch's own clock, and the alternative would be a worse lie.** The
    // `at` an event carries into `App::event` is an age, used to say how long a
    // tool call took. A detached child has been running since before this watch
    // began — possibly for minutes — so no clock this session holds gives its
    // steps their true ages. Timing from the moment of attaching at least
    // measures something real: how long the operator has been watching. The
    // driver's own `Instant` is not in scope here and reaching for it would put
    // the same wrong number behind a more convincing name.
    let watching = Instant::now();
    // **Ask whether there is anything left to watch, before opening a watch that
    // can never end.** `Attach::from_now()` sets the cursor to the store's current
    // position and does not look at the run — so attaching to a child that has
    // already finished succeeds, polls forever, and prints nothing. That is the
    // ordinary case rather than an exotic one: a detached child finishes on its
    // own schedule, its `Finished` reaches nobody once the parent's turn has
    // ended, so the fleet row still reads `detached` long after the child stopped.
    // The operator selects exactly that row, because it is the one the pane
    // invites them to select.
    match store.run_status(run_id) {
        Ok(Some(io_harness::RunStatus::Running)) => {}
        Ok(Some(status)) => {
            app.record(
                Tone::Muted,
                format!(
                    "run {run_id} is no longer running ({}); there is nothing left to watch",
                    format!("{status:?}").to_lowercase()
                ),
            );
            return Ok(());
        }
        // No row, or a store that cannot answer. Neither is a reason to open a
        // watch that would show nothing; both are worth saying.
        Ok(None) => {
            app.record(Tone::Refused, format!("run {run_id} is not in the store"));
            return Ok(());
        }
        Err(error) => {
            app.record(Tone::Refused, format!("cannot watch run {run_id}: {error}"));
            return Ok(());
        }
    }
    let mut attach = match io_harness::Attach::to(store, run_id).from_now() {
        Ok(attach) => attach,
        Err(error) => {
            // The store is readable — the session is running out of it — so this
            // is the run being unknown to it, which is a fact worth saying rather
            // than a silence to interpret.
            app.record(Tone::Refused, format!("cannot watch run {run_id}: {error}"));
            return Ok(());
        }
    };
    app.record(
        Tone::Muted,
        format!("watching run {run_id} — Esc to stop watching"),
    );
    paint(screen, app)?;
    loop {
        match attach.poll() {
            Ok(events) => {
                let mut drawn = false;
                for event in &events {
                    // **`watched`, not `event`.** This run's tokens are not this
                    // session's spend and its tree is not this turn's fan-out;
                    // `App::watched` draws the lines and folds nothing into the
                    // status line or the fleet.
                    app.watched(event, watching.elapsed());
                    drawn = true;
                    // The child is finished when it says so. Leaving on its own
                    // `Finished` rather than on a status query keeps this reading
                    // one stream instead of two sources that can disagree — and
                    // the rest of the batch is drawn first rather than abandoned
                    // mid-loop, so a `HandleKilled` sitting behind the `Finished`
                    // is not lost.
                    if matches!(event.kind, io_harness::EventKind::Finished { .. })
                        && event.run_id == run_id
                    {
                        app.record(Tone::Muted, format!("run {run_id} finished"));
                        paint(screen, app)?;
                        return Ok(());
                    }
                }
                if drawn {
                    paint(screen, app)?;
                }
            }
            Err(error) => {
                app.record(Tone::Refused, format!("stopped watching: {error}"));
                paint(screen, app)?;
                return Ok(());
            }
        }
        // **One deadline per poll, and the loop waits out the whole of it.**
        // Re-entering `timeout` with a fresh `ATTACH_POLL` after every key would
        // busy-spin: an auto-repeating key or a dragged window edge returns
        // immediately, and the loop would go straight back to `attach.poll()` —
        // tens of store reads a second instead of ten, on the connection the
        // driver is also using.
        let deadline = tokio::time::Instant::now() + ATTACH_POLL;
        loop {
            match tokio::time::timeout_at(deadline, inputs.recv()).await {
                // Both stop keys, and `Ctrl+C` is here because it is the one key
                // this product refuses to let a configuration file rebind. An
                // operator who wants out of anything presses it; a watch it did
                // nothing to would be the only surface in the interface where
                // that is false.
                Ok(Some(Event::Key(key)))
                    if key.kind == KeyEventKind::Press
                        && (key.code == KeyCode::Esc
                            || (key.code == KeyCode::Char('c')
                                && key.modifiers.contains(KeyModifiers::CONTROL))) =>
                {
                    app.record(Tone::Muted, format!("stopped watching run {run_id}"));
                    paint(screen, app)?;
                    return Ok(());
                }
                // A resize while watching is a resize, and the other two loops in
                // this file already say so. Dropping it leaves the `Screen`
                // believing in a width the terminal no longer has, and every row
                // drawn after the watch ends writes past the edge.
                Ok(Some(Event::Resize(width, height))) => {
                    screen
                        .resize(width, height)
                        .map_err(|error| error.to_string())?;
                    paint(screen, app)?;
                }
                // The channel closing is the terminal going away, which the caller
                // handles by returning; there is nothing to watch for any more.
                Ok(None) => return Ok(()),
                // Any other key is ignored on purpose: this is a window onto a run
                // the operator is not driving, so a key that did something here
                // would be a key that did it to the wrong run.
                Ok(Some(_)) => {}
                // The deadline, which is the only way out of this inner loop.
                Err(_) => break,
            }
        }
    }
}

fn paint(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<(), String> {
    paint_picker(screen, app, None)
}

fn paint_picker(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    picker: Option<&mut (Picker, Pick)>,
) -> Result<(), String> {
    let pending = app.take_pending();
    if !pending.is_empty() {
        screen.commit(&pending).map_err(|error| error.to_string())?;
    }
    // **The prompt takes the rows it needs, and gives them back.** Only with no
    // picker open and only at an idle prompt — `App::viewport_wanted` returns the
    // fixed height in every other case — because re-placing the viewport
    // re-queries the cursor, and doing that under a streaming turn would land the
    // viewport somewhere the output underneath it has already moved past.
    if picker.is_none() {
        let wanted = app.viewport_wanted(screen.width(), screen.terminal_rows());
        if wanted != screen.rows() {
            // A failure here leaves the session's own height in place — see
            // `Screen::replace` — so a terminal that will not answer keeps a
            // usable prompt rather than losing one over a row.
            let _ = replace_viewport(screen, wanted);
        }
    }
    let theme = app.theme;
    screen
        .draw(|frame| match picker {
            Some((open, _)) => open.render(frame, frame.area(), &theme),
            None => app.render(frame, frame.area()),
        })
        .map_err(|error| error.to_string())
}

/// A keyboard reader that can be stopped.
///
/// Stopping matters, and it is not a nicety. Placing an inline viewport asks the
/// terminal where its cursor is and reads the answer back **off stdin** — and a
/// thread sitting in `crossterm::event::read()` will consume that answer first,
/// so the query times out and the program refuses to start. A reader must
/// therefore never be running while a `Screen` is being attached.
///
/// That is why the thread polls rather than blocking in `read()`: a thread parked
/// in `read()` cannot be told to stand down, and there is no synthetic event to
/// wake it with.
struct Keyboard {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Re-place the viewport at `height`, with nothing reading stdin while it lands.
///
/// The lock is [`io_cli::stdin`]'s, which is also where the reason it has to be
/// fair is written down: without the reader standing aside, this call waits for a
/// scheduling accident rather than for one poll.
fn replace_viewport(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    height: u16,
) -> Result<(), String> {
    let _parked = io_cli::stdin::placing();
    screen.replace(height).map_err(|error| error.to_string())
}

impl Keyboard {
    /// Start reading.
    ///
    /// Takes the attached screen as a witness rather than as a parameter it uses.
    /// The rule it encodes is the one this cost a broken binary to learn: a
    /// reader must not exist while a viewport is being placed, and a signature
    /// that cannot be satisfied before the attach is a stronger guarantee than a
    /// comment saying so.
    fn start(
        _attached: &Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    ) -> (Self, UnboundedReceiver<Event>) {
        let (tx, rx) = unbounded_channel();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                // Parked while a viewport is being placed. Placing one asks the
                // terminal where its cursor is and reads the answer off stdin,
                // and this thread would take it first — which is the same defect
                // the `Keyboard::start` signature exists to prevent at the two
                // boundaries where the reader can simply be stopped. Here it
                // cannot: the palette re-places the viewport in the middle of a
                // session, and the channel this reader owns is what the session
                // is waiting on.
                // One call, because the poll and the read are one critical
                // section — and because it is the call that stands aside for a
                // placement. A reader that took the lock unconditionally and
                // released it at the bottom of the loop would take it again
                // before the waiter was ever scheduled, which is the freeze
                // 0.13.1 exists to end. `io_cli::stdin` holds the whole rule.
                match io_cli::stdin::next_event() {
                    Ok(Some(event)) => {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                    // Nothing typed this interval, or a placement wanted the
                    // terminal. Both mean the same thing here: go round again.
                    Ok(None) => {}
                    Err(io_cli::stdin::Broken) => break,
                }
            }
        });
        (
            Self {
                stop,
                thread: Some(thread),
            },
            rx,
        )
    }

    /// Stop reading and wait for the thread to leave stdin alone.
    ///
    /// Joined rather than detached: the point is to know the reader is gone
    /// before the next viewport is placed.
    fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
