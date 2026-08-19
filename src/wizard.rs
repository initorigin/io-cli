//! The first-run wizard.
//!
//! Eight screens, driven by the product's own [`Picker`]
//! rather than by a prompt library — which would have been a second owner of raw
//! mode and a second aesthetic in the one flow where a first impression is formed.
//!
//! [`Picker`]: crate::picker::Picker
//!
//! Two rules run through it:
//!
//! - **Nothing is written before the confirmation screen.** The screen names the
//!   exact path and shows the exact text; the write happens on the keystroke after
//!   it and nowhere else.
//! - **The credential is never rendered.** It is masked while typed, it is
//!   summarised rather than shown, and where an environment variable already
//!   carries it the file is written without it at all.
//!
//! The steps that need the network — verifying the key, and reading the model
//! catalogue — are handed back to the driver rather than performed here, so this
//! whole flow is drivable from a test with a scripted key sequence.

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use io_harness::ProviderSpec;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tui_textarea::TextArea;

use crate::picker::{fit, fit_left, Outcome, Picker, Row};
use crate::settings::{self, Posture};
use crate::theme::{Background, Theme, Tone, THEMES};

/// The providers on offer: the variants `io_harness::ProviderSpec` declares.
/// Enumerated from the type, not invented here — a provider the harness gains is
/// a row this list gains, and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    OpenRouter,
    Anthropic,
    OpenAi,
    Compatible,
}

impl Kind {
    pub const ALL: &'static [Kind] = &[
        Kind::OpenRouter,
        Kind::Anthropic,
        Kind::OpenAi,
        Kind::Compatible,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::OpenRouter => "OpenRouter",
            Self::Anthropic => "Anthropic",
            Self::OpenAi => "OpenAI",
            Self::Compatible => "Any OpenAI-compatible endpoint",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::OpenRouter => "one key, most models",
            Self::Anthropic => "Claude, direct",
            Self::OpenAi => "GPT, direct",
            Self::Compatible => "a base URL of your own: a proxy, a gateway, a local runtime",
        }
    }

    /// The environment variable this provider reads when the file names no key.
    pub fn env_var(self) -> Option<&'static str> {
        match self {
            Self::OpenRouter => Some("OPENROUTER_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::OpenAi => Some("OPENAI_API_KEY"),
            // There is no single vendor to name a variable for.
            Self::Compatible => None,
        }
    }

    /// A model id to start from, so the picker opens on something plausible when
    /// the catalogue cannot be read.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenRouter => "anthropic/claude-sonnet-4",
            Self::Anthropic => "claude-sonnet-4",
            Self::OpenAi => "gpt-5",
            Self::Compatible => "",
        }
    }

    fn spec(
        self,
        model: String,
        api_key: Option<String>,
        base_url: Option<String>,
    ) -> ProviderSpec {
        match self {
            Self::OpenRouter => ProviderSpec::OpenRouter { model, api_key },
            Self::Anthropic => ProviderSpec::Anthropic { model, api_key },
            Self::OpenAi => ProviderSpec::OpenAi { model, api_key },
            Self::Compatible => ProviderSpec::Compatible {
                model,
                api_key,
                base_url,
                preset: None,
                auth: None,
                name: None,
                // Prices come from the reference catalogue for an endpoint that
                // states none of its own, which is what a proxy or a local runtime
                // does. Without it the status line's spend field has nothing to
                // read when 0.2.0 adds it.
                reference_prices: true,
            },
        }
    }
}

/// Which screen the wizard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Welcome,
    Provider,
    /// Only for a compatible endpoint, which has no vendor to assume.
    BaseUrl,
    Credential,
    /// Waiting for the driver to make the verification call.
    Verifying,
    Model,
    /// Typed rather than picked, when the catalogue has nothing to offer — a
    /// local runtime or a proxy serves whatever it serves and no catalogue can
    /// say what that is.
    ModelText,
    Theme,
    Posture,
    Confirm,
    Done,
    Cancelled,
}

/// What the driver has to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Nothing; the wizard changed its own state.
    Idle,
    /// Commit these lines to scrollback.
    Commit(Vec<Line<'static>>),
    /// Verify this credential against the live endpoint, then call
    /// [`Wizard::verified`] or [`Wizard::rejected`].
    Verify(ProviderSpec),
    /// Read the model catalogue, then call [`Wizard::catalogue`].
    Catalogue(ProviderSpec),
    /// Write this text to this path, mode 0600.
    Write(std::path::PathBuf, String),
    /// The user backed out. Nothing was written.
    Cancelled,
}

pub struct Wizard {
    step: Step,
    theme: Theme,
    kind: Option<Kind>,
    base_url: Option<String>,
    /// Typed by the user, or `None` when the environment already carries it.
    api_key: Option<String>,
    model: Option<String>,
    /// The theme name that will be **written down**, which is not always the name
    /// of the theme being rendered. Under `NO_COLOR` those two part company: the
    /// rendered theme is `MONO` and the written one is whichever row the picker
    /// was left on, because the file records a preference for the run that does
    /// not have the variable set.
    theme_name: String,
    posture: Posture,
    picker: Option<Picker>,
    /// The masked field the credential and the base URL are typed into.
    input: TextArea<'static>,
    /// The provider's own message from a failed verification, shown above the
    /// credential field.
    rejection: Option<String>,
    /// Whether the environment already carries a usable key for this provider.
    env_key_present: bool,
    /// Whether colour is off for this whole run.
    ///
    /// Read from the theme handed in rather than from the environment, because
    /// `MONO` is the only uncoloured theme there is and `Theme::resolve` hands
    /// it back for one reason only. Holding the answer instead of re-reading the
    /// variable also keeps the theme step drivable from a test that never has to
    /// set a process-wide variable.
    no_color: bool,
}

impl Wizard {
    pub fn new(theme: Theme) -> Self {
        Self {
            step: Step::Welcome,
            theme,
            kind: None,
            base_url: None,
            api_key: None,
            model: None,
            // Resolved again with colour forced on, which under `NO_COLOR` turns
            // `MONO` into the coloured theme this terminal would otherwise have
            // had. Seeding from `theme.name` directly would seed "mono", and
            // "mono" is a label rather than a key — `Theme::by_name` searches the
            // selectable themes and `MONO` is not among them — so it would open
            // the picker on the wrong row and, if the file were written from it,
            // record a preference no later launch could ever resolve.
            theme_name: Theme::resolve(false, Background::detect(), Some(theme.name), theme.glyphs)
                .name
                .to_string(),
            posture: Posture::Workspace,
            picker: None,
            input: masked(theme.glyphs.mask),
            rejection: None,
            env_key_present: false,
            no_color: !theme.coloured,
        }
    }

    pub fn step(&self) -> Step {
        self.step
    }

    /// The theme currently highlighted, which the sample transcript behind the
    /// theme picker re-renders in as the selection moves.
    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn done(&self) -> bool {
        matches!(self.step, Step::Done | Step::Cancelled)
    }

    /// The provider spec as it stands, or `None` before there is one.
    pub fn spec(&self) -> Option<ProviderSpec> {
        let kind = self.kind?;
        Some(
            kind.spec(
                self.model
                    .clone()
                    .unwrap_or_else(|| kind.default_model().to_string()),
                self.api_key.clone(),
                self.base_url.clone(),
            ),
        )
    }

    /// Take a terminal event.
    ///
    /// This exists because the driver used to pattern-match `Event::Key` and
    /// treat everything else as "the user left" — so a paste, which arrives as
    /// `Event::Paste` when bracketed paste is on, silently ended the wizard.
    /// Pasting a key is the single most likely thing anybody does on the
    /// credential screen. Dispatch lives here now, where a test can reach it.
    pub fn event(&mut self, event: &Event) -> Progress {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.key(*key),
            Event::Paste(text) => {
                self.paste(text);
                Progress::Idle
            }
            // A key release, a resize, a focus change, a mouse report from a
            // terminal that sends them unasked: none of these is a decision, and
            // none of them ends the wizard.
            _ => Progress::Idle,
        }
    }

    /// Take a bracketed paste into whichever field is open.
    ///
    /// Newlines are stripped rather than submitted: a key copied out of a web
    /// page often carries a trailing newline, and treating that as Enter would
    /// submit a field the user was still filling in.
    pub fn paste(&mut self, text: &str) {
        if !matches!(
            self.step,
            Step::Credential | Step::BaseUrl | Step::ModelText
        ) {
            return;
        }
        let cleaned: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        if cleaned.is_empty() {
            return;
        }
        self.input.insert_str(cleaned);
    }

    pub fn key(&mut self, key: KeyEvent) -> Progress {
        if key.code == KeyCode::Esc && self.step != Step::Credential {
            self.step = Step::Cancelled;
            return Progress::Cancelled;
        }
        match self.step {
            Step::Welcome => self.welcome(key),
            Step::Provider => self.provider(key),
            Step::BaseUrl => self.base_url(key),
            Step::Credential => self.credential(key),
            // The driver owns these; a keystroke while waiting does nothing rather
            // than queueing up behind a network call.
            Step::Verifying => Progress::Idle,
            Step::Model => self.model(key),
            Step::ModelText => self.model_text(key),
            Step::Theme => self.theme_step(key),
            Step::Posture => self.posture(key),
            Step::Confirm => self.confirm(key),
            Step::Done | Step::Cancelled => Progress::Idle,
        }
    }

    /// The driver's answer to [`Progress::Verify`]: the key works.
    pub fn verified(&mut self) -> Progress {
        self.rejection = None;
        self.step = Step::Model;
        self.picker = Some(Picker::new(
            "Which model?",
            vec![Row::new(
                self.kind
                    .map(Kind::default_model)
                    .unwrap_or_default()
                    .to_string(),
            )],
        ));
        self.spec()
            .map(Progress::Catalogue)
            .unwrap_or(Progress::Idle)
    }

    /// The driver's answer to [`Progress::Verify`]: the provider said no, in
    /// these words.
    ///
    /// Back to the credential step carrying the provider's own message, and
    /// **nothing is written** — the file is only ever written from the
    /// confirmation screen, which this never reaches.
    pub fn rejected(&mut self, message: impl Into<String>) -> Progress {
        self.rejection = Some(message.into());
        self.input = masked(self.theme.glyphs.mask);
        self.step = Step::Credential;
        Progress::Idle
    }

    /// The driver's answer to [`Progress::Catalogue`].
    pub fn catalogue(&mut self, models: Vec<String>) {
        if self.step != Step::Model {
            return;
        }
        let default = self.kind.map(Kind::default_model).unwrap_or_default();
        let mut rows: Vec<Row> = models.iter().map(|id| Row::new(id.clone())).collect();
        if rows.is_empty() && default.is_empty() {
            // Nothing to pick from and nothing to suggest. Asking the user to
            // type it is the honest answer; offering an empty row is not.
            self.input = plain();
            self.picker = None;
            self.step = Step::ModelText;
            return;
        }
        if rows.is_empty() {
            rows.push(Row::with_detail(
                default.to_string(),
                "the catalogue could not be read; this is the provider's usual default",
            ));
        }
        let opening = rows
            .iter()
            .position(|row| row.label == default)
            .unwrap_or(0);
        // The rows are swapped into the picker that is already on the screen
        // rather than a new picker being built over it. This step opens on the
        // provider's default while the catalogue request is still in flight, and
        // a four-hundred-model list is a list nobody scrolls — so the first thing
        // an operator does is type, and a wholesale replacement threw away
        // everything they had typed while they waited, with no way to tell that it
        // had been read at all.
        self.picker
            .get_or_insert_with(|| Picker::new("Which model?", Vec::new()))
            .set_rows(rows, opening);
    }

    fn welcome(&mut self, key: KeyEvent) -> Progress {
        if key.code != KeyCode::Enter {
            return Progress::Idle;
        }
        self.step = Step::Provider;
        self.picker = Some(Picker::new(
            "Which provider?",
            Kind::ALL
                .iter()
                .map(|kind| Row::with_detail(kind.label(), kind.detail()))
                .collect(),
        ));
        Progress::Commit(vec![
            Line::from(Span::styled(
                "No configuration found, so this is the first run.",
                self.theme.style(Tone::Normal),
            )),
            Line::from(Span::styled(
                "Nothing is written until the last screen says so.",
                self.theme.style(Tone::Muted),
            )),
            Line::from(""),
        ])
    }

    fn provider(&mut self, key: KeyEvent) -> Progress {
        let Some(picker) = self.picker.as_mut() else {
            return Progress::Idle;
        };
        match picker.key(key) {
            Outcome::Chosen(index) => {
                let kind = Kind::ALL[index];
                self.kind = Some(kind);
                self.env_key_present = kind
                    .env_var()
                    .map(|var| std::env::var_os(var).is_some_and(|value| !value.is_empty()))
                    .unwrap_or(false);
                self.picker = None;
                self.input = masked(self.theme.glyphs.mask);
                self.step = if kind == Kind::Compatible {
                    self.input = plain();
                    Step::BaseUrl
                } else {
                    Step::Credential
                };
                Progress::Idle
            }
            Outcome::Cancelled => {
                self.step = Step::Cancelled;
                Progress::Cancelled
            }
            Outcome::Idle => Progress::Idle,
        }
    }

    fn base_url(&mut self, key: KeyEvent) -> Progress {
        if key.code == KeyCode::Enter {
            let typed = self.input.lines().join("").trim().to_string();
            if typed.is_empty() {
                return Progress::Idle;
            }
            self.base_url = Some(typed);
            self.input = masked(self.theme.glyphs.mask);
            self.step = Step::Credential;
            return Progress::Idle;
        }
        self.input.input(key);
        Progress::Idle
    }

    fn credential(&mut self, key: KeyEvent) -> Progress {
        match key.code {
            KeyCode::Esc => {
                self.step = Step::Cancelled;
                Progress::Cancelled
            }
            KeyCode::Enter => {
                let typed = self.input.lines().join("").trim().to_string();
                if typed.is_empty() {
                    if !self.env_key_present {
                        return Progress::Idle;
                    }
                    // The environment already carries it, so the file is written
                    // without a key at all — which is the better outcome, not a
                    // fallback: a key that is never on disk cannot leak from it.
                    self.api_key = None;
                } else {
                    self.api_key = Some(typed);
                }
                self.input = masked(self.theme.glyphs.mask);
                self.step = Step::Verifying;
                self.spec().map(Progress::Verify).unwrap_or(Progress::Idle)
            }
            _ => {
                self.input.input(key);
                Progress::Idle
            }
        }
    }

    fn model(&mut self, key: KeyEvent) -> Progress {
        let Some(picker) = self.picker.as_mut() else {
            return Progress::Idle;
        };
        match picker.key(key) {
            Outcome::Chosen(index) => {
                self.model = picker.rows().get(index).map(|row| row.label.clone());
                self.enter_theme_step();
                Progress::Idle
            }
            Outcome::Cancelled => {
                self.step = Step::Cancelled;
                Progress::Cancelled
            }
            Outcome::Idle => Progress::Idle,
        }
    }

    fn model_text(&mut self, key: KeyEvent) -> Progress {
        if key.code == KeyCode::Enter {
            let typed = self.input.lines().join("").trim().to_string();
            if typed.is_empty() {
                return Progress::Idle;
            }
            self.model = Some(typed);
            self.enter_theme_step();
            return Progress::Idle;
        }
        self.input.input(key);
        Progress::Idle
    }

    /// Move on to the theme step. Two callers, so the picker is built once.
    fn enter_theme_step(&mut self) {
        self.step = Step::Theme;
        self.picker = Some(
            Picker::new(
                "Which theme?",
                THEMES
                    .iter()
                    .map(|theme| Row::with_detail(theme.name, "the sample below re-renders"))
                    .collect(),
            )
            .selecting(
                THEMES
                    .iter()
                    .position(|theme| theme.name == self.theme_name)
                    .unwrap_or(0),
            ),
        );
    }

    fn theme_step(&mut self, key: KeyEvent) -> Progress {
        let Some(picker) = self.picker.as_mut() else {
            return Progress::Idle;
        };
        let outcome = picker.key(key);
        // Read after every keystroke, not only on choice: this is the live
        // preview, and it costs nothing because the renderer is already there.
        //
        // **`selection`, never `selected`.** Running every keystroke is exactly
        // what makes the difference load-bearing: `selected` answers 0 when the
        // query admits nothing, and 0 is a real theme, so a `z` — a letter no
        // theme name carries — previewed `dark` and reassigned `theme_name`, which
        // is the string `settings::render` writes into `io.toml`. Nothing on
        // screen said so and backspacing did not undo it. A query that matches no
        // row leaves the previewed theme exactly where it was.
        if let Some(theme) = picker.selection().and_then(|index| THEMES.get(index)) {
            self.theme_name = theme.name.to_string();
            // Resolved, never assigned. A picked theme is a preference exactly as
            // the configuration file's theme is, and `NO_COLOR` outranks both —
            // so under it the preview and the session that follows stay `MONO`
            // while the name above stays the one the user picked. Assigning here
            // is how a user with the variable set used to choose a theme at this
            // screen and get a coloured session anyway.
            // The glyph set is handed straight back rather than resolved again.
            // It was chosen once at startup from the locale, the configuration
            // file and `--plain`, and none of those three is a thing the theme
            // picker can have changed — a second resolution here would quietly
            // discard a `--plain` the moment somebody arrowed down the list.
            self.theme = Theme::resolve(
                self.no_color,
                theme.background,
                Some(theme.name),
                self.theme.glyphs,
            );
        }
        match outcome {
            Outcome::Chosen(_) => {
                self.step = Step::Posture;
                self.picker = Some(Picker::new(
                    "How much should it be allowed to do?",
                    Posture::ALL
                        .iter()
                        .map(|posture| Row::with_detail(posture.label(), posture.detail()))
                        .collect(),
                ));
                Progress::Idle
            }
            Outcome::Cancelled => {
                self.step = Step::Cancelled;
                Progress::Cancelled
            }
            Outcome::Idle => Progress::Idle,
        }
    }

    fn posture(&mut self, key: KeyEvent) -> Progress {
        let Some(picker) = self.picker.as_mut() else {
            return Progress::Idle;
        };
        match picker.key(key) {
            Outcome::Chosen(index) => {
                self.posture = Posture::ALL[index];
                self.picker = None;
                self.step = Step::Confirm;
                Progress::Idle
            }
            Outcome::Cancelled => {
                self.step = Step::Cancelled;
                Progress::Cancelled
            }
            Outcome::Idle => Progress::Idle,
        }
    }

    fn confirm(&mut self, key: KeyEvent) -> Progress {
        if key.code != KeyCode::Enter {
            return Progress::Idle;
        }
        let (Some(path), Some(spec)) = (settings::user_path(), self.spec()) else {
            self.step = Step::Cancelled;
            return Progress::Cancelled;
        };
        let Ok(contents) = settings::render(&spec, self.posture, &self.theme_name) else {
            self.step = Step::Cancelled;
            return Progress::Cancelled;
        };
        self.step = Step::Done;
        Progress::Write(path, contents)
    }

    /// The lines the confirmation screen shows, unbounded.
    ///
    /// For a caller that wants the *facts* rather than the layout — what is on the
    /// screen, not how much of it a given terminal has room for.
    pub fn summary(&self) -> Vec<Line<'static>> {
        self.summary_at(usize::MAX)
    }

    /// The lines the confirmation screen shows: the exact path, and what will be
    /// in the file. Never the credential itself.
    ///
    /// **Fitted to `width`, because two of these lines are unbounded and this
    /// paragraph does not wrap.** The path comes from the environment and the
    /// model id from a provider's catalogue; either can be longer than eighty
    /// columns on an ordinary machine, and a viewport `Paragraph` answers a line
    /// it cannot fit by clipping it and saying nothing. On this screen in
    /// particular that is not a cosmetic loss: the whole promise of the wizard is
    /// that nothing is written until a screen has named the exact path, and a
    /// path silently missing its last segments names a different file.
    ///
    /// The path is shortened from the **left** ([`fit_left`]) and the model from
    /// the right ([`fit`]), which is the same split the resume picker makes and
    /// for the same reason: every configuration path on one machine shares its
    /// first segments and is identified by its last, while a model id is
    /// identified by its vendor and family, which come first.
    pub fn summary_at(&self, width: usize) -> Vec<Line<'static>> {
        let theme = self.theme;
        let glyphs = &theme.glyphs;
        // The width of the fixed text each line leads with. Written as the
        // strings themselves so the reservation cannot drift from the `format!`
        // below it.
        let lead = |prefix: &str| width.saturating_sub(prefix.chars().count());
        let path = settings::user_path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "(no configuration directory could be found)".into());
        let path = fit_left(&path, lead("This will write "), glyphs);
        let kind = self.kind.map(Kind::label).unwrap_or("(none)");
        let model = fit(
            self.model.as_deref().unwrap_or_default(),
            lead("  model       "),
            glyphs,
        );

        let credential = match (&self.api_key, self.kind.and_then(Kind::env_var)) {
            // Said as a fact about the file, with no part of the value in it.
            (Some(_), _) => "written into this file, mode 0600".to_string(),
            (None, Some(var)) => format!("read from ${var} at run time; not written"),
            (None, None) => "not written".to_string(),
        };

        vec![
            Line::from(Span::styled(
                format!("This will write {path}"),
                theme.style(Tone::Accent),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("  provider    {kind}"),
                theme.style(Tone::Normal),
            )),
            Line::from(Span::styled(
                format!("  model       {model}"),
                theme.style(Tone::Normal),
            )),
            Line::from(Span::styled(
                format!("  credential  {credential}"),
                theme.style(Tone::Normal),
            )),
            Line::from(Span::styled(
                format!("  permission  {}", self.posture.label()),
                theme.style(Tone::Normal),
            )),
            Line::from(Span::styled(
                format!("  theme       {}", self.theme_name),
                theme.style(Tone::Normal),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter to write it, Esc to leave without writing.",
                theme.style(Tone::Muted),
            )),
        ]
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // Every step sets a cursor, and this is the position a step that draws no
        // field of its own is left with. ratatui hides the terminal cursor on any
        // frame that sets no position, and a hidden cursor removes the only focus
        // indicator a screen reader has — through a whole flow whose every screen
        // is a question. Set before the match rather than in ten places inside it:
        // the steps below overwrite it when they have somewhere better to point,
        // and the last write of a frame is the one that lands, so no step can
        // forget by omission.
        //
        // It matters most where it is least visible. `render_input` places the
        // caret in the field only `if area.height > used`, and a viewport too
        // short for the field is exactly when a reader most needs telling where
        // they are; without this the cramped frame had no position at all.
        frame.set_cursor_position(ratatui::layout::Position {
            x: area.x,
            y: area.y,
        });
        let theme = self.theme;
        match self.step {
            Step::Welcome => paragraph(
                frame,
                area,
                vec![
                    Line::from(Span::styled(
                        "Welcome. Four questions and you have a working agent.",
                        theme.style(Tone::Normal),
                    )),
                    Line::from(Span::styled(
                        "Enter to begin, Esc to leave.",
                        theme.style(Tone::Muted),
                    )),
                ],
            ),
            Step::Provider | Step::Model | Step::Posture => {
                if let Some(picker) = self.picker.as_mut() {
                    picker.render(frame, area, &theme);
                }
            }
            Step::Theme => self.render_theme(frame, area),
            Step::ModelText => self.render_input(
                frame,
                area,
                "No catalogue to offer. Type the model id this endpoint serves.",
            ),
            Step::BaseUrl => self.render_input(
                frame,
                area,
                "The base URL of the endpoint, for example http://localhost:11434/v1",
            ),
            Step::Credential => {
                let prompt = match (self.env_key_present, self.kind.and_then(Kind::env_var)) {
                    (true, Some(var)) => {
                        format!("Paste an API key, or press Enter to use ${var}")
                    }
                    _ => "Paste an API key. It is not echoed.".to_string(),
                };
                self.render_input(frame, area, &prompt);
            }
            Step::Verifying => paragraph(
                frame,
                area,
                vec![Line::from(Span::styled(
                    format!(
                        "Checking the key against the provider{}",
                        theme.glyphs.ellipsis
                    ),
                    theme.style(Tone::Muted),
                ))],
            ),
            Step::Confirm => paragraph(frame, area, self.summary_at(area.width as usize)),
            Step::Done | Step::Cancelled => {}
        }
    }

    fn render_input(&mut self, frame: &mut Frame, area: Rect, prompt: &str) {
        let theme = self.theme;
        let mut lines = Vec::new();
        if let Some(rejection) = &self.rejection {
            // The provider's own words, not a generic failure. Every provider
            // reports a bad credential differently and the difference is the
            // information.
            //
            // **Cut to fit rather than wrapped, and this is the one place in the
            // wizard where that is the harder call.** A provider's rejection is
            // an arbitrary string — several of them return a whole JSON body —
            // and this paragraph does not wrap, so past eighty columns the tail
            // is clipped with nothing on screen to say so. Wrapping would keep
            // every word, but the height of the wrap is not knowable from
            // `lines.len()`, and `lines.len()` is what places the credential
            // field and the caret on the rows below. A rejection that grew the
            // block would draw the field on top of its own last line and put the
            // caret on text rather than on the input, which trades a visible loss
            // for an invisible one.
            //
            // The reservation is measured off an empty notice rather than
            // assumed, so the word `Tone::Error` prefixes and the colon and space
            // after it are counted by the code that draws them.
            let room = (area.width as usize).saturating_sub(theme.notice(Tone::Error, "").width());
            lines.push(theme.notice(Tone::Error, fit(rejection, room, &theme.glyphs)));
        }
        lines.push(Line::from(Span::styled(
            prompt.to_string(),
            theme.style(Tone::Muted),
        )));
        let used = lines.len() as u16;
        paragraph(
            frame,
            Rect {
                height: used.min(area.height),
                ..area
            },
            lines,
        );
        if area.height > used {
            frame.render_widget(
                &self.input,
                Rect {
                    y: area.y + used,
                    height: 1,
                    ..area
                },
            );
            frame.set_cursor_position(ratatui::layout::Position {
                x: area.x + self.input.cursor().1 as u16,
                y: area.y + used,
            });
        }
    }

    fn render_theme(&mut self, frame: &mut Frame, mut area: Rect) {
        let theme = self.theme;
        // Said on the screen rather than left to be discovered. Under `NO_COLOR`
        // the sample below cannot change, so a user arrowing through the list is
        // watching nothing happen and would fairly read that as broken. The
        // choice is still worth making — it is what the file records for a run
        // without the variable set — and one line is the whole difference between
        // a dead control and a deferred one.
        if self.no_color && area.height > 1 {
            paragraph(
                frame,
                Rect { height: 1, ..area },
                vec![Line::from(Span::styled(
                    "NO_COLOR is set, so this run stays uncoloured; the choice is saved for later.",
                    theme.style(Tone::Muted),
                ))],
            );
            area = Rect {
                y: area.y + 1,
                height: area.height - 1,
                ..area
            };
        }
        // The sample sits below the picker and is redrawn in whichever theme is
        // highlighted. It is the one moment of delight in the flow and it costs
        // nothing, because the renderer is already here.
        // Five: a label and the four sample lines. The label is not decoration —
        // the sample deliberately contains a refusal and a success so the palette
        // can be judged, and without a word saying what it is, a first-time user
        // reads it as their own session going wrong. One did.
        let sample_rows = 5u16;
        let picker_rows = area.height.saturating_sub(sample_rows);
        if picker_rows > 0 {
            if let Some(picker) = self.picker.as_mut() {
                picker.render(
                    frame,
                    Rect {
                        height: picker_rows,
                        ..area
                    },
                    &theme,
                );
            }
        }
        if area.height > picker_rows {
            paragraph(
                frame,
                Rect {
                    y: area.y + picker_rows,
                    height: area.height - picker_rows,
                    ..area
                },
                sample(&theme),
            );
        }
    }
}

/// The sample transcript the theme step renders behind its picker.
///
/// It opens by saying what it is. The lines below are invented — a refusal and a
/// success — chosen because they are the two states whose colours are worth
/// judging before committing to a theme, and they are exactly the two that look
/// alarming when nothing says they are a preview.
pub fn sample(theme: &Theme) -> Vec<Line<'static>> {
    // Drawn from the same set the session it is previewing will use, so the
    // sample is a sample of this terminal and not of an idealised one. A preview
    // showing marks the session that follows cannot draw is worse than no
    // preview at all.
    let glyphs = &theme.glyphs;
    let (separator, dash) = (glyphs.separator, glyphs.dash);
    vec![
        Line::from(Span::styled(
            format!("preview {dash} not your session:"),
            theme.style(Tone::Muted),
        )),
        Line::from(vec![
            Span::styled(glyphs.marker, theme.style(Tone::Accent)),
            Span::styled("make the failing test pass", theme.style(Tone::Normal)),
        ]),
        Line::from(vec![
            Span::styled(format!("  {} ", glyphs.bullet), theme.style(Tone::Muted)),
            Span::styled("exec", theme.style(Tone::Accent)),
            Span::styled(" cargo test", theme.style(Tone::Muted)),
        ]),
        theme.notice(
            Tone::Refused,
            format!("write /etc/hosts {dash} rule fs.deny, layer workspace"),
        ),
        theme.notice(
            Tone::Success,
            format!("success{separator}4 steps{separator}8,912 tok"),
        ),
    ]
}

fn paragraph(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// A one-line field whose characters are never shown.
///
/// The mask comes off the chosen glyph set rather than being written here: a
/// terminal that cannot draw a bullet shows a row of replacement boxes, and a
/// credential field whose masking is itself unreadable is the one field where a
/// reader cannot tell a rendering fault from having typed the wrong thing.
fn masked(mask: char) -> TextArea<'static> {
    let mut area = plain();
    area.set_mask_char(mask);
    area
}

fn plain() -> TextArea<'static> {
    let mut area = TextArea::default();
    area.set_cursor_line_style(ratatui::style::Style::default());
    area
}

/// Whether a key event is one the wizard treats as "leave without writing".
pub fn is_cancel(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}
