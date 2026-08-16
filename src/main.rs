//! `io` — the driver.
//!
//! Everything with a decision in it lives in the library so a test can reach it.
//! What is here is the wiring: reading the configuration, taking the terminal,
//! turning keystrokes into commands and commands into io-harness calls, and
//! giving the terminal back.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use crossterm::event::{Event, KeyEventKind};
use io_harness::{
    Anthropic, Compatible, Config, DenyAll, OpenAi, OpenRouter, Policy, Provider, ProviderSpec,
    Session, Steer, Store,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use io_cli::app::{App, Command};
use io_cli::cli::{Cli, Command as Subcommand};
use io_cli::commands::{self, Action};
use io_cli::picker::{Outcome, Picker, Row};
use io_cli::settings::{self, CliSettings};
use io_cli::term::Screen;
use io_cli::theme::{Theme, Tone};
use io_cli::wizard::{Progress, Wizard};
use io_cli::{bridge, splash, verify};

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("io: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Printed after the terminal has been restored, never into raw mode.
            eprintln!("io: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let cli = Cli::parse();
    let root = match cli.dir {
        Some(dir) => dir,
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };

    let config = Config::discover(&root).map_err(|error| error.to_string())?;
    let stored: Option<CliSettings> = config.app(settings::APP_KEY).unwrap_or_default();
    let theme = Theme::from_env(stored.and_then(|s| s.theme).as_deref());

    // Interactive-only in this release. A non-TTY stdout is detected well enough
    // to refuse rather than to half-work; `io exec` and NDJSON are 0.5.0.
    if !std::io::stdout().is_terminal() {
        return Err("io is interactive in this release and stdout is not a terminal".into());
    }

    let setup = matches!(cli.command, Some(Subcommand::Setup));
    let mut screen = Screen::attach().map_err(|error| error.to_string())?;
    let mut inputs = keyboard();

    let result = drive(
        &mut screen,
        &mut inputs,
        &root,
        config,
        theme,
        setup,
        cli.model,
    )
    .await;

    // Explicit as well as on `Drop`, so the terminal is back before anything is
    // printed about how this ended.
    screen.restore();
    result
}

#[allow(clippy::too_many_arguments)]
async fn drive(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    root: &std::path::Path,
    mut config: Config,
    mut theme: Theme,
    forced_setup: bool,
    model_override: Option<String>,
) -> Result<(), String> {
    let width = screen.width();
    screen
        .commit(&splash::lines(&theme, true, width))
        .map_err(|error| error.to_string())?;

    if forced_setup || config.provider_spec().is_none() {
        if let Some(chosen) = wizard(screen, inputs, theme).await? {
            theme = chosen;
        } else {
            return Ok(());
        }
        // Read back what was written rather than trusting what was typed: the
        // file is the source of truth from here on, and if the harness disagrees
        // with the wizard about it this is where that shows.
        config = Config::discover(root).map_err(|error| error.to_string())?;
    }

    let Some(spec) = config.provider_spec().cloned() else {
        return Err("no provider is configured; run `io setup`".into());
    };
    let spec = match model_override {
        Some(model) => with_model(spec, model),
        None => spec,
    };

    let policy = config.policy().unwrap_or_default();
    let store = store_path().ok_or("no place to keep the run store")?;
    let store = Store::open(&store).map_err(|error| error.to_string())?;
    let session = Session::open(&store, root).map_err(|error| error.to_string())?;

    // `Provider` is not dyn-compatible, so the session loop is generic and the
    // spec is matched once, here, rather than behind a trait object.
    match spec {
        ProviderSpec::OpenRouter { model, api_key } => {
            let key = key_for(api_key, "OPENROUTER_API_KEY")?;
            loop_over(
                screen,
                inputs,
                OpenRouter::new(key, &model),
                store,
                session,
                policy,
                theme,
                model,
            )
            .await
        }
        ProviderSpec::Anthropic { model, api_key } => {
            let key = key_for(api_key, "ANTHROPIC_API_KEY")?;
            loop_over(
                screen,
                inputs,
                Anthropic::new(key, &model),
                store,
                session,
                policy,
                theme,
                model,
            )
            .await
        }
        ProviderSpec::OpenAi { model, api_key } => {
            let key = key_for(api_key, "OPENAI_API_KEY")?;
            loop_over(
                screen,
                inputs,
                OpenAi::new(key, &model),
                store,
                session,
                policy,
                theme,
                model,
            )
            .await
        }
        ProviderSpec::Compatible {
            model,
            preset,
            base_url,
            api_key,
            auth,
            ..
        } => {
            let key = api_key.unwrap_or_default();
            let provider = match (preset, base_url) {
                (Some(preset), _) => {
                    Compatible::preset(&preset, key, &model).map_err(|error| error.to_string())?
                }
                (None, Some(base)) => {
                    Compatible::new(base, auth.unwrap_or(io_harness::Auth::Bearer), key, &model)
                }
                (None, None) => {
                    return Err("this provider names neither a preset nor a base URL".into())
                }
            };
            loop_over(
                screen, inputs, provider, store, session, policy, theme, model,
            )
            .await
        }
        other => Err(format!(
            "this release cannot drive a {other:?} provider yet"
        )),
    }
}

/// The interactive session.
#[allow(clippy::too_many_arguments)]
async fn loop_over<P: Provider>(
    screen: &mut Screen<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    inputs: &mut UnboundedReceiver<Event>,
    provider: P,
    store: Store,
    mut session: Session,
    policy: Policy,
    theme: Theme,
    model: String,
) -> Result<(), String> {
    let approver = DenyAll;
    let mut app = App::new(theme, model);
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
                        Pick::Model => {
                            app.status.model = label.clone();
                            app.say(
                                Tone::Muted,
                                format!(
                                    "the status line says {label}, but this release cannot swap \
                                     the provider mid-session; run `io -m {label}`"
                                ),
                            );
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
                    let rows = vec![Row::with_detail(
                        app.status.model.clone(),
                        "the model this session started with",
                    )];
                    picker = Some((Picker::new("Which model?", rows), Pick::Model));
                }
            },
            Command::Submit(text) => {
                turn(
                    screen,
                    inputs,
                    &mut app,
                    &provider,
                    &store,
                    &mut session,
                    &policy,
                    &approver,
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
    approver: &DenyAll,
    text: String,
    started: Instant,
) -> Result<(), String> {
    let (steer, inbox) = Steer::channel();
    let (observer, mut events) = bridge::channel();
    app.started();
    paint(screen, app)?;

    let mut running =
        Box::pin(session.turn_steered(text, provider, store, policy, approver, &observer, &inbox));

    let outcome = loop {
        tokio::select! {
            result = &mut running => break result,
            Some(event) = events.recv() => {
                app.event(&event);
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
                        if command == Command::Interrupt {
                            // Best effort: the turn may already have ended, in
                            // which case there is nobody left to tell.
                            let _ = steer.interrupt();
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
    while let Ok(event) = events.try_recv() {
        app.event(&event);
    }
    app.finished();

    if let Err(error) = outcome {
        app.say(Tone::Error, error.to_string());
    }
    app.status.elapsed = started.elapsed();
    paint(screen, app)
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

/// Which picker is open.
enum Pick {
    Theme,
    Model,
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

/// Keystrokes, read on a thread of their own.
///
/// crossterm's `read` blocks, and the alternative — its async `EventStream` — is
/// behind a feature that pulls in a futures stack for what one thread and a
/// channel already do.
fn keyboard() -> UnboundedReceiver<Event> {
    let (tx, rx) = unbounded_channel();
    std::thread::spawn(move || {
        while let Ok(event) = crossterm::event::read() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    rx
}

fn key_for(api_key: Option<String>, var: &str) -> Result<String, String> {
    if let Some(key) = api_key {
        return Ok(key);
    }
    match std::env::var(var) {
        Ok(key) if !key.is_empty() => Ok(key),
        _ => Err(format!(
            "no key in the configuration and ${var} is not set; run `io setup`"
        )),
    }
}

fn with_model(spec: ProviderSpec, model: String) -> ProviderSpec {
    match spec {
        ProviderSpec::OpenRouter { api_key, .. } => ProviderSpec::OpenRouter { model, api_key },
        ProviderSpec::Anthropic { api_key, .. } => ProviderSpec::Anthropic { model, api_key },
        ProviderSpec::OpenAi { api_key, .. } => ProviderSpec::OpenAi { model, api_key },
        ProviderSpec::Compatible {
            preset,
            base_url,
            api_key,
            auth,
            name,
            reference_prices,
            ..
        } => ProviderSpec::Compatible {
            model,
            preset,
            base_url,
            api_key,
            auth,
            name,
            reference_prices,
        },
        other => other,
    }
}

/// Beside the configuration file, because that is the directory this product
/// already owns and asking for a second one buys nothing.
fn store_path() -> Option<PathBuf> {
    let config = settings::user_path()?;
    Some(config.parent()?.join("runs.db"))
}
