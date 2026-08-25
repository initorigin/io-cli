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
use std::time::Instant;

use clap::Parser;
use crossterm::event::{Event, KeyEventKind};
use io_harness::{Config, Policy, Provider, ProviderSpec, Session, Store, Templates};
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
    *report = io_cli::home::adopt().map_or_else(Vec::new, |report| report.lines());
    let config = Config::discover(&root).map_err(|error| error.to_string())?;
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
    // session with none cannot fan out: `turn_contained_bounded_observed` is the only
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
    let (skills, complaint) = commands::skills(skills_dir.as_deref());
    if let Some(complaint) = complaint {
        notices.push(complaint);
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
    if let Some(notice) = settings::deprecated_max_steps(stored.as_ref()) {
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
        },
    )
    .await?
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
    skills: io_harness::Skills,
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
    // what a turn's contract is built from since 0.14.0.
    config: Config,
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
    capabilities: io_cli::contract::Capabilities,
    skills: io_harness::Skills,
    model: String,
) -> Result<(), String> {
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
    if let Some(caps) = &containment {
        let notice = settings::contained_notice(caps, app.theme.glyphs.dash);
        app.say(Tone::Muted, notice);
    }
    // **The session no longer keeps a clock, because nothing shows one.** The
    // clock on screen belongs to the turn — it starts at zero when one starts and
    // stops where it stopped — so the reading a session-long `Instant` gave was
    // `22m12s` beside a turn six seconds old. Each turn is handed its own.
    let mut picker: Option<(Picker, Pick)> = None;

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
                            // **Not "steered".** Neither turn takes a `SteerInbox`
                            // since 0.11.0 — the flat arm gave its up for a
                            // contract and the contained arm never had one — so a
                            // word promising mid-turn redirection describes
                            // nothing this product does.
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
                // The viewport is four rows and cannot grow, so everything that
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
            Command::Submit(text) => {
                // Rebuilt every turn rather than kept, because `remembered` grows
                // as the operator answers and the harness's own `remember` dies
                // with the turn it was given in. With nothing remembered this is
                // the session's policy unchanged.
                let effective = approval::session_policy(&policy, app.posture(), app.remembered());
                turn(
                    screen,
                    inputs,
                    &mut app,
                    &provider,
                    &store,
                    &mut session,
                    &effective,
                    &config,
                    // The caps reach the turn only while the session is in
                    // contained mode, so `/contain off` is a real switch and not
                    // a label: with `None` here the turn built below is the
                    // steered turn, byte for byte.
                    contained.then_some(containment.as_ref()).flatten(),
                    &capabilities,
                    planning,
                    text,
                    // **This turn's own clock, not the session's.** What a reader
                    // wants of the row above the prompt is how long the thing in
                    // front of them has been going; a clock that had been counting
                    // since the terminal opened said `22m12s` about a turn six
                    // seconds old. Every event age inside the turn is measured
                    // from here too, which is what a tool cell's duration is a
                    // difference of.
                    Instant::now(),
                )
                .await?;
                // Anything dropped onto the prompt while the turn held the
                // session is staged now that it has let go.
                for path in app.take_queued_pictures() {
                    paste_picture(&mut app, &mut session, &provider, &policy, &path);
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
    // Whether this turn proposes a plan before it works. The operator's `/plan`,
    // and nothing else — a caps configuration decided it through 0.11.0, which
    // is how every contained turn ended up stopping for one.
    planning: bool,
    text: String,
    started: Instant,
) -> Result<(), String> {
    let (observer, mut events) = bridge::channel();
    // The one way a turn is stopped from the interface, contained or not. Both
    // arms take a contract now and neither takes a steer inbox, so `Flow::Cancel`
    // out of `Bridge::event` is what `Ctrl+C` and `Esc` set.
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

    // **The two turns this product can take, and one loop over both.** They are
    // genuinely different turns rather than one turn with a flag: only the
    // contained entry point passes a containment into io-harness's driver, so
    // only it reaches the loop that owns the spawn tool — and it takes no
    // `SteerInbox`, because no session entry point takes a caller's containment
    // and a steer inbox together. Boxed to one type so the `select!` below is
    // written once; a second loop would be a second place `Ctrl+C`, the ticker
    // and the event drain could drift.
    //
    // 0.10.0 — the contained arm is `turn_contained_bounded_observed` and carries
    // a contract io-cli built, which is what a responder, a plan gate, MCP, LSP,
    // a browser and skills are fields of. The flat arm is untouched: it is still
    // `turn_steered`, still `default_contract`, and still the only one that can
    // be steered mid-turn.
    // Taken before the future borrows the session, because it is needed inside the
    // loop and `running` holds `&mut session` for the whole of it.
    let root = session.root().to_path_buf();
    app.contained = containment.is_some();
    // Built before the future borrows it, and only for the arm that can take one:
    // the flat turn is handed `text` itself, exactly as it always was.
    // **Every turn carries one now, contained or not.** Through 0.11.0 the flat
    // arm was `turn_steered`, which builds `TaskContract::workspace` inside
    // io-harness and takes none from the caller — so its step cap was twelve,
    // fixed, and a turn that read a repository and wrote a file ended on
    // `error: step_cap_reached` with the work half done.
    //
    // `turn_bounded_observed` takes a contract, streams the model's text, and is
    // not contained. What it does not take is a steer inbox, and that costs
    // nothing this interface offered: the only thing io-cli ever sent through
    // one was an interrupt, and the observer's `Flow::Cancel` — the path a
    // contained turn has always been stopped by — ends a turn at the same step
    // boundary.
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
    let mut running: std::pin::Pin<
        Box<dyn std::future::Future<Output = io_harness::Result<io_harness::TurnResult>> + '_>,
    > = match containment {
        Some(caps) => Box::pin(session.turn_contained_bounded_observed(
            &contract, provider, store, policy, &approver, caps, &observer,
        )),
        None => Box::pin(
            session.turn_bounded_observed(&contract, provider, store, policy, &approver, &observer),
        ),
    };

    // Lives for the turn and no longer, which is half of why an idle session
    // never repaints; `App::tick` is the other half and the one a test can see.
    // `MissedTickBehavior::Delay` rather than the default: a turn that blocked the
    // loop should resume ticking from now, not fire a burst catching up on the
    // frames nobody saw.
    // Set when a turn was taken back off the screen rather than stopped: there
    // is nothing to report about a turn the session no longer shows.
    let mut undone = false;
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
                commit_viewed(screen, app, &root, policy, &event)?;
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
                                // **One path for both kinds of turn.** Neither
                                // takes a steer inbox any more — a contained turn
                                // never did, and the flat one gave its up for a
                                // contract — so the observer is what io-harness
                                // reads from the interface while a run goes, and
                                // it honours `Flow::Cancel` at the next step
                                // boundary. That is the sentence
                                // `App::interrupt_or_quit` has just put on screen.
                                canceller.store(true, std::sync::atomic::Ordering::Relaxed);
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
        // And the picture, for the same reason and the same race: a `view_image`
        // on the turn's last step is exactly the one the drain would otherwise
        // lose.
        commit_viewed(screen, app, &root, policy, &event)?;
    }
    app.finished();

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
    paint(screen, app)
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
/// Upward and never into a pane. The viewport is four rows and cannot grow, and
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
    /// One directory of the workspace, in the order `list_dir` sorted it, so a
    /// chosen index reads straight back through `complete::pick`. The rows are
    /// last components rather than paths — see `complete::rows` for why — which
    /// is exactly why the entries are carried here and not read off a label.
    Complete(Vec<io_harness::tools::Entry>),
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
