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
use crossterm::event::{Event, KeyEventKind};
use io_harness::{Config, Policy, Provider, ProviderSpec, Session, Steer, Store};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use io_cli::app::{App, Command};
use io_cli::cli::{Cli, Command as Subcommand};
use io_cli::commands::{self, Action, Copied};
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::settings::{self, CliSettings, Posture};
use io_cli::term::Screen;
use io_cli::theme::{Theme, Tone};
use io_cli::wizard::{Progress, Wizard};
use io_cli::{approval, bridge, provider, splash, verify};
use ratatui::text::{Line, Span};

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("io: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            // Printed after the terminal has been restored, never into raw mode.
            eprintln!("io: {error}");
            ExitCode::from(io_cli::exec::FAILED)
        }
    }
}

async fn run() -> Result<u8, String> {
    let cli = Cli::parse();
    let root = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };

    let config = Config::discover(&root).map_err(|error| error.to_string())?;
    let stored: Option<CliSettings> = config.app(settings::APP_KEY).unwrap_or_default();
    let theme = Theme::from_env(stored.and_then(|s| s.theme).as_deref());

    // The headless path leaves before anything takes the terminal, and before the
    // wizard can be reached: `io exec` in a container with no configuration file
    // must fail with a sentence, never sit at a prompt nobody can answer.
    if let Some(Subcommand::Exec(args)) = cli.command {
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
            .commit(&splash::lines(&theme, true, width))
            .map_err(|error| error.to_string())?;

        let chosen = wizard(&mut screen, &mut inputs, theme).await;
        // The reader goes before the screen does, and both before the next
        // attach, so nothing is holding stdin when the session's viewport is
        // placed.
        keys.stop();
        screen.restore();
        drop(screen);

        match chosen? {
            Some(chosen) => theme = chosen,
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
        screen
            .commit(&splash::lines(&theme, true, width))
            .map_err(|error| error.to_string())?;
    }

    let result = drive(&mut screen, &mut inputs, config, theme, cli.model, &root).await;

    // Explicit as well as on `Drop`, so the terminal is back before anything is
    // printed about how this ended.
    keys.stop();
    screen.restore();
    result.map(|()| io_cli::exec::OK)
}

/// The session, once there is a configuration to run it against.
async fn drive(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    config: Config,
    theme: Theme,
    model_override: Option<String>,
    root: &std::path::Path,
) -> Result<(), String> {
    let Some(spec) = config.provider_spec().cloned() else {
        return Err("no provider is configured; run `io setup`".into());
    };

    let policy = config.policy().unwrap_or_default();
    // `[app.io-cli]` again, read through the harness rather than parsed here. It
    // is read in `drive` rather than in `run` because `run` may hand control to
    // the wizard, which writes the file this then reads back.
    let stored: Option<CliSettings> = config.app(settings::APP_KEY).unwrap_or_default();
    let diff_style = settings::DiffStyle::from_setting(stored.and_then(|s| s.diff).as_deref());
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
            catalogue_spec,
            store,
            session,
            policy,
            diff_style,
            theme,
        },
    )
    .await?
}

/// The interactive session, as something [`provider::build`] can run.
struct Interactive<'a, 'b> {
    screen: &'a mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &'b mut UnboundedReceiver<Event>,
    catalogue_spec: ProviderSpec,
    store: Store,
    session: Session,
    policy: Policy,
    diff_style: settings::DiffStyle,
    theme: Theme,
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
            self.catalogue_spec,
            self.store,
            self.session,
            self.policy,
            self.diff_style,
            self.theme,
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
    spec: ProviderSpec,
    store: Store,
    mut session: Session,
    policy: Policy,
    diff_style: settings::DiffStyle,
    theme: Theme,
    model: String,
) -> Result<(), String> {
    // Built here rather than handed in, so there is one place a provider comes
    // from and `/model` cannot drift from startup.
    let mut provider = make(&model)?;
    let mut app = App::new(theme, model);
    app.set_diff_style(diff_style);
    // Asked of the session rather than threaded down from `run`, so there is one
    // answer to "which workspace is this" and it is the harness's.
    app.set_root(session.root());
    // What the file already says, read back rather than assumed. `None` means the
    // file holds a policy that is none of the three, which io-harness's own
    // configuration can express and this session must not relabel.
    app.set_posture(Posture::of(&policy.defaults));
    let started = Instant::now();
    let mut picker: Option<(Picker, Pick)> = None;

    paint(screen, &mut app)?;

    loop {
        let Some(event) = inputs.recv().await else {
            return Ok(());
        };
        let Event::Key(key) = event else {
            if let Event::Resize(width, height) = event {
                screen
                    .resize(width, height)
                    .map_err(|error| error.to_string())?;
            }
            if let Event::Paste(text) = event {
                app.composer.paste(&text);
            }
            app.status.elapsed = started.elapsed();
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
                    match kind {
                        Pick::Theme => {
                            if let Some(chosen) = Theme::by_name(&label) {
                                app.theme = chosen;
                                app.events.set_theme(chosen);
                                app.say(
                                    Tone::Muted,
                                    format!(
                                        "theme {label} for this session; `io setup` to keep it"
                                    ),
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
                        // `if let` rather than a match on `Option`: an index past
                        // the end is the row saying the list was cut, which
                        // carries no id, and closing the picker is the only
                        // sensible thing a line of prose can do when chosen.
                        Pick::Resume(ids) => {
                            if let Some(id) = ids.get(index) {
                                match io_cli::sessions::resume(&store, *id) {
                                    Ok(reopened) => {
                                        session = reopened;
                                        app.set_root(session.root());
                                        // Where they were, in the terminal's own
                                        // buffer rather than in a four-row
                                        // viewport.
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
                                }
                            }
                        }
                        Pick::Fork(ids) => {
                            if let Some(turn) = ids.get(index) {
                                match session.branch_from(&store, *turn) {
                                    Ok(()) => app.say(
                                        Tone::Success,
                                        format!(
                                            "continuing from turn {}; what came after is still \
                                             in the transcript, marked as branched away",
                                            index + 1
                                        ),
                                    ),
                                    Err(error) => app.say(
                                        Tone::Error,
                                        format!("that turn could not be branched from: {error}"),
                                    ),
                                }
                            }
                        }
                    }
                    picker = None;
                }
                Outcome::Cancelled => picker = None,
                Outcome::Idle => {}
            }
            app.status.elapsed = started.elapsed();
            paint_picker(screen, &mut app, picker.as_mut())?;
            continue;
        }

        match app.key(key) {
            Command::None => {}
            Command::Exit => return Ok(()),
            Command::Interrupt => {}
            Command::ClearViewport => {
                // The viewport, and nothing above it.
                paint(screen, &mut app)?;
            }
            Command::Transcript => commit_transcript(screen, &session, &store, &app.theme)?,
            // The first `Esc`. Nothing has changed yet; this says what the second
            // one would change, in the turn's own words, so a confirmation is a
            // confirmation of something specific rather than of a keystroke.
            Command::ArmRewind => match io_cli::rewind::preview(&session, &store) {
                Some(about) => app.say(Tone::Warning, io_cli::rewind::armed_line(&about)),
                None => app.say(Tone::Muted, "there is no turn to undo"),
            },
            // The second. This is where the operator's files change.
            Command::Rewind => match io_cli::rewind::last_turn(&mut session, &store) {
                Ok(Some(undone)) => {
                    for (tone, line) in io_cli::rewind::undone_lines(&undone) {
                        app.say(tone, line);
                    }
                }
                Ok(None) => app.say(Tone::Muted, "there is no turn to undo"),
                Err(error) => app.say(Tone::Error, format!("nothing was undone: {error}")),
            },
            Command::Slash(text) => match commands::parse(&text, &app.theme) {
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
                        let mut rows = io_cli::sessions::rows(&found, screen.width());
                        // Last, and a row rather than a notice, so a list that was
                        // cut cannot read as a complete one. It carries no id, and
                        // choosing it does nothing.
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
                        let rows = io_cli::sessions::turn_rows(&turns, screen.width());
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
                Action::Expand => {
                    let lines = expand(&session, &store, &app.theme);
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
            },
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
                    text,
                    started,
                )
                .await?;
            }
        }

        app.status.elapsed = started.elapsed();
        paint_picker(screen, &mut app, picker.as_mut())?;
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
    text: String,
    started: Instant,
) -> Result<(), String> {
    let (steer, inbox) = Steer::channel();
    let (observer, mut events) = bridge::channel();
    // The other seam, and the one that can stop the agent. `DenyAll` stood here
    // through 0.1.0 and 0.1.1, which is why the *ask before writes* posture
    // declined everything it was named for.
    let (approver, mut asks) = approval::channel();
    app.started();
    paint(screen, app)?;

    let mut running =
        Box::pin(session.turn_steered(text, provider, store, policy, &approver, &observer, &inbox));

    // Lives for the turn and no longer, which is half of why an idle session
    // never repaints; `App::tick` is the other half and the one a test can see.
    // `MissedTickBehavior::Delay` rather than the default: a turn that blocked the
    // loop should resume ticking from now, not fire a burst catching up on the
    // frames nobody saw.
    let mut ticker = tokio::time::interval(io_cli::app::TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let outcome = loop {
        tokio::select! {
            result = &mut running => break result,
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
            Some(input) = inputs.recv() => {
                match input {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        // Bound rather than tested inside the guard: `App::key`
                        // changes state, and a match guard is not a place to put
                        // something that does.
                        let command = app.key(key);
                        match command {
                            Command::Interrupt => {
                                // Best effort: the turn may already have ended, in
                                // which case there is nobody left to tell.
                                let _ = steer.interrupt();
                            }
                            // Refused with a sentence rather than dropped in
                            // silence. `/resume`, `/fork` and the rewind each move
                            // the session head this turn is about to write, and
                            // `/model` would take effect at a moment nobody could
                            // predict — so all four wait, and the operator is told
                            // that is what is happening rather than left pressing
                            // a key that appears to do nothing.
                            Command::Slash(_) => app.say(
                                Tone::Muted,
                                "not while a turn is running — Ctrl+C interrupts it first",
                            ),
                            _ => {}
                        }
                    }
                    Event::Resize(width, height) => {
                        screen.resize(width, height).map_err(|error| error.to_string())?;
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
    }
    app.finished();

    if let Err(error) = outcome {
        app.say(Tone::Error, error.to_string());
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

/// The last step's full detail, from the run's durable trace.
///
/// This is the other half of collapsing a tool cell: the screen is not the
/// archive, so the output goes to the store when it happens and comes back here
/// when somebody asks for it. Committed upward like everything else that shows
/// more of something.
fn expand(session: &Session, store: &Store, theme: &Theme) -> Vec<Line<'static>> {
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
        format!("step {} — {}", step.step, step.decision),
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
    /// Session ids, in the order the rows are drawn. The resume picker may carry
    /// one row more than there are ids — the line saying the list was cut — and
    /// an index past the end is that row, which does nothing.
    Resume(Vec<i64>),
    /// Turn ids, in the order the rows are drawn.
    Fork(Vec<i64>),
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
                // A short poll rather than a blocking read, so the flag is seen.
                match crossterm::event::poll(Duration::from_millis(40)) {
                    Ok(true) => match crossterm::event::read() {
                        Ok(event) => {
                            if tx.send(event).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    Ok(false) => {}
                    Err(_) => break,
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
