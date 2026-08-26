//! The status line: one row, always at the bottom of the viewport.
//!
//! This release fills three of its fields — the model answering, whether a turn
//! is running, and how long the session has been going. The rest of the line is
//! 0.2.0's: the policy layer in force, context pressure, spend against the tree
//! ceiling, and containment. They are named in [`Field`] now so that adding them
//! is filling in a value rather than redesigning the line.
//!
//! Its narrow form drops fields from the right rather than wrapping, because a
//! status line that becomes two lines has taken a row from the transcript and
//! stopped being a status line.
//!
//! **And since 0.14.0 there is a second, much longer form of the same facts.**
//! [`committed`] is what `/status` writes into the terminal's own scrollback: the
//! whole state at once, laid out down the page rather than across a row, because
//! nothing that has to fit in one line can carry a policy layer's rules or the
//! caps a fan-out runs under. The two forms read the same values — the budgets
//! come from [`Status::budgets_left`] on both, the backend from the same
//! `containment` field — so the row and the page cannot say different things
//! about one session.

use std::time::Duration;

use io_harness::{Containment, Policy, Session, TaskContract};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::theme::{Theme, Tone};

/// The frames of the working indicator, in the Unicode set.
///
/// Braille, because every frame is exactly one cell wide — a spinner built from
/// characters of differing width shifts the whole line right and left as it turns,
/// which is worse than not moving at all. Ten frames and a modulo; a crate for
/// this would be the beginning of the thing this product exists not to become.
///
/// It never carries a meaning of its own. The state is the word beside it, and
/// this is only the evidence that the word is still true.
///
/// Reached through [`crate::glyphs::Glyphs::spinner`] rather than named directly
/// by the renderer, so a terminal that cannot draw braille turns
/// [`crate::glyphs::ASCII_SPINNER`] instead — which is held to the same one-cell
/// rule, for the same reason.
pub const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// What the activity line calls a turn, one word per step.
///
/// **A word and never only a tone.** The list is carried here rather than
/// generated, indexed by the step count rather than by a clock or a random
/// number, so the word is chosen once per step, is stable for as long as that
/// step is, and can be stated by a test. Every entry is plain ASCII, so the
/// line reads the same under the ASCII glyph set as it does under braille.
///
/// The literal name for what is happening right now belongs to the row under
/// this one, which is [`crate::events::Events::live`]'s. Two rows because the
/// question was asked as an either-or and both answers were right.
pub const WORDS: [&str; 10] = [
    "Pondering",
    "Noodling",
    "Mulling",
    "Chewing",
    "Puzzling",
    "Wrangling",
    "Untangling",
    "Percolating",
    "Rummaging",
    "Simmering",
];

/// A field of the status line, in priority order: the first is the last to be
/// dropped when the terminal is narrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub text: String,
    pub tone: Tone,
    /// Whether this field is drawn in weight as well as in tone.
    ///
    /// One field on the line uses it — the model — because a status line where
    /// everything is emphasised is a status line where nothing is. It is a flag
    /// rather than a `Style` so that the tone tokens stay the only place a colour
    /// is decided.
    pub bold: bool,
}

impl Field {
    fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            text: text.into(),
            tone,
            bold: false,
        }
    }

    fn bold(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            bold: true,
            ..Self::new(text, tone)
        }
    }
}

/// The ceilings the operator's configuration put on a turn, as the contract
/// carries them.
///
/// **Three `Option`s and not three numbers, because two of the three genuinely
/// may not exist.** `TaskContract::max_duration` and `TaskContract::max_tokens`
/// are `Option` in io-harness itself — a turn with no time budget is not a turn
/// with a budget of zero — and a field rendered from a `0` on that side would
/// report an exhausted budget on every session whose operator never set one.
///
/// **The step cap is the odd one, and it is the reason this type exists at all
/// rather than three loose fields on [`Status`].** `TaskContract::max_steps` is
/// a plain `u32` and is therefore *always* set, so "does a step budget exist" is
/// not a question the contract answers. [`Budgets::in_force`] asks a different
/// one — whether anybody chose it — by comparing the cap against
/// [`crate::contract::MAX_STEPS`], which is io-cli's own floor and exists
/// precisely so that the step cap is not the thing that ends a turn. A line
/// reading `left 997/1000 steps` on every session would be reporting io-cli's
/// scaffolding back to the operator as a budget they set, and 0.14.0's F6 is
/// explicit that a session with no `[run]` table shows no budget field at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budgets {
    /// `[run] max_steps`, or `[app.io-cli] max_steps` where that beat it — and
    /// `None` where the cap is still io-cli's own floor.
    pub steps: Option<u32>,
    /// `[run] max_tokens`, summed across the turn's completions.
    pub tokens: Option<u64>,
    /// `[run] max_duration_secs`, as wall time across the turn.
    pub duration: Option<Duration>,
}

impl Budgets {
    /// What this turn's contract actually bounds, read off the contract and never
    /// off the file.
    ///
    /// **The contract is the one place the precedence is already resolved.**
    /// `crate::contract::configured` documents five layers — io-harness's own
    /// defaults, io-cli's step floor, `Config::apply_to`, `[sandbox]` and
    /// `[app.io-cli]` — and by the time a `TaskContract` exists, a `[run]`
    /// budget the file lowered and an `[app.io-cli] max_steps` that outranked it
    /// are the same single fact. Reading `io.toml` a second time here would be a
    /// second answer to a question already settled, and the two would drift the
    /// first time a layer moved.
    pub fn in_force(contract: &TaskContract) -> Self {
        Self {
            steps: (contract.max_steps != crate::contract::MAX_STEPS).then_some(contract.max_steps),
            tokens: contract.max_tokens,
            duration: contract.max_duration,
        }
    }
}

/// What the status line is currently saying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// The model answering. First, because it is the field a reader looks for.
    pub model: String,
    /// The provider serving the run, by io-harness's own name for it.
    ///
    /// **New in 0.11.0, and the reason it exists is a removal.** Through 0.10.0
    /// the provider was named by a `via {provider}` line committed under every
    /// prompt — the owner's complaint, and the one place in the whole product
    /// that ever said it. Taking that line away without putting the fact here
    /// would have deleted it rather than moved it, which is what
    /// `US-IO-CLI-0.11.0-I01` records.
    ///
    /// Set from `EventKind::Started`, and moved by `EventKind::FellBackTo`, which
    /// is a different provider answering the same turn.
    pub provider: Option<String>,
    /// Steps the run has taken.
    ///
    /// The other half of that removal: the count lived only in the `Finished`
    /// row's arithmetic. It climbs from `RunEvent::step` as the steps commit and
    /// is replaced by the run's own total when the run ends, for the reason
    /// `tokens` is.
    pub steps: Option<u32>,
    /// Events whose kind `crate::triage` has no disposition for.
    ///
    /// Zero on every run against the locked io-harness, so the field is absent
    /// from every line this release will ever draw against it. It stops being
    /// zero when a later harness is pinned and starts emitting something new,
    /// which is the moment somebody needs to know that the transcript is quiet
    /// *because* nobody has triaged it yet.
    pub unknown: usize,
    /// Whether a turn is running.
    pub working: bool,
    /// What io-cli has to say about the last keystroke, if anything.
    ///
    /// **The footer's last row, since 0.13.1, and it used to be the scrollback.**
    /// `stopping at the next step`, `not while a turn is running`, `press Ctrl+C
    /// again to exit`: every one of them answers a key that was just pressed, and
    /// every one of them used to be committed into the terminal's permanent
    /// record — so stopping a turn left three warning-coloured rows sitting
    /// between two answers for as long as the scrollback lived. A notice replaces
    /// the previous one and is gone at the next keystroke.
    pub notice: Option<(Tone, String)>,
    /// The permission posture in force, by its short name — or `None` before one
    /// is known, which is the wizard's first moments and nothing else.
    ///
    /// It is a *posture*, which is an `io_harness::Defaults` set, and never a flag
    /// of io-cli's own. That is what makes this field an explanation rather than a
    /// decoration: the word here is the same thing the agent is actually bounded
    /// by, and a refusal can name the rule and the layer underneath it.
    pub policy: Option<String>,
    /// How long the session has been open.
    pub elapsed: Duration,
    /// Tokens this session has spent, accumulated from the steps that reported
    /// them. `None` until one does — a session that has spent nothing yet is not
    /// a session that has spent zero, and the difference is the whole of F9.
    pub tokens: Option<u64>,
    /// Tokens the turn now running has spent.
    ///
    /// **A second counter, because the two rows answer different questions.**
    /// The footer says what the *session* has cost, which is what a spend is
    /// judged on and which must not fall back to zero every time a turn starts.
    /// The activity line says what *this turn* is costing while you watch it,
    /// beside a clock that starts at zero for the same reason — a number that
    /// only ever climbs across a long session tells you nothing about the turn
    /// in front of you.
    ///
    /// Cleared by `Status::start_run`, which is the one place a run begins.
    pub run_tokens: Option<u64>,
    /// How full the assembled context was the last time io-harness said so, as a
    /// share of the budget io-harness itself declares.
    ///
    /// `None` until a fold reports one, which is the honest answer: `Compacted` is
    /// the only event carrying an observation-section size, and between folds
    /// nothing on the event stream knows it.
    ///
    /// ponytail: derived from the last fold. The per-step estimate is durable in
    /// the harness store as `ContextEvent::est_tokens`, so a live share is one
    /// store read away if this field turns out too quiet to be useful.
    pub context: Option<u8>,
    /// How this run's commands are contained: the mode asked for and the backend
    /// that actually answered on this host.
    ///
    /// Both, always. io-harness's own documentation is explicit that a surface
    /// showing the mode alone is reading an intention — `workspace-write` reaching
    /// a portable floor means resource caps and nothing else.
    pub containment: Option<String>,
    /// Whether later turns propose a plan before they work.
    ///
    /// **Not run-scoped, and the only field on this line that is a standing
    /// choice rather than an observation.** `/plan on` holds until `/plan off`,
    /// and while it holds io-harness denies every write and every exec under a
    /// `plan-gate` layer until a proposal is approved — so it must survive
    /// [`Status::forget_run`], which every neighbouring field is cleared by. An
    /// operator watching an agent that will not write needs the reason on screen,
    /// and the turn it was set on is over by then.
    ///
    /// `false` renders as nothing at all, on the rule this line already holds:
    /// a session that has not asked to plan is not a session planning zero times.
    pub planning: bool,
    /// What this turn has drawn against the tree's shared ceiling, and what is
    /// left of it: `(drawn, remaining)`.
    ///
    /// **The field 0.2.0 named and could not fill.** `EventKind::SpendDraw` is
    /// emitted only from io-harness's contained loop, and until 0.8.0 no session
    /// turn reached that loop, so this line carried a name with no value behind
    /// it for six releases. It is reachable now for a structural reason rather
    /// than because somebody got round to it.
    ///
    /// `None` until a draw arrives — a turn that has drawn nothing is not a turn
    /// that has drawn zero, the same distinction `tokens` is held to. The inner
    /// `Option` is io-harness's own: a tree with no ceiling reports `remaining:
    /// None`, and rendering that as `0` would report a full ceiling as an
    /// exhausted one.
    ///
    /// Tokens, never money. `Containment::max_total_cost` is documented inert
    /// because the crate has no price telemetry, so a figure with a currency in
    /// front of it would be one this interface invented.
    pub spend: Option<(u64, Option<u64>)>,
    /// How many background shells this run has started and not yet finished.
    ///
    /// A `shell_start` outlives the step that launched it, which is the whole
    /// point of it and the whole problem: a run waiting on a dev server looks
    /// exactly like a run that has hung. io-harness emits five events describing
    /// a handle's life and until now nothing rendered any of them.
    ///
    /// Zero renders as **nothing at all**, not as `0`. A session that has started
    /// no background work has not started zero jobs, which is the same
    /// distinction `tokens` and `spend` are held to — and it is what keeps this
    /// field free on the overwhelming majority of lines, where it never appears.
    ///
    /// Counted from the stream rather than read from the store: `HandleStarted`
    /// opens exactly one handle and exactly one of `HandleExited`, `HandleKilled`
    /// and `HandleOrphaned` closes it, which io-harness documents as an invariant.
    /// `HandlePolled` is not an ending and must not move it.
    pub jobs: usize,
    /// How much of the agent's plan the agent says is done, as done over total.
    ///
    /// `None` until the agent writes a list, and that is the whole of F12: a
    /// session with no plan has not written a plan of nothing, so this renders as
    /// nothing at all rather than as `0/0`. Set from a `TodoWrote`'s own items,
    /// which carry the whole list on every write and are never a delta — there is
    /// nothing to read back out of the store to complete it.
    ///
    /// It is drawn as a *claim*. io-harness's own documentation is explicit that
    /// nothing verifies a plan item, so an item saying `Done` is what the agent
    /// said about its own work; a field that stated it as a fact would be the one
    /// place in this product where the plan stopped being the agent's account.
    pub plan: Option<(usize, usize)>,
    /// Whether this session runs in plain mode.
    ///
    /// It lives on the status line rather than beside it because the status line
    /// is the only surface in this product that animates — so this is the field
    /// the mode is *about*, and putting it here means there is one boolean in the
    /// session rather than two that have to agree. [`crate::app::App`] reads it
    /// back off this struct for the same reason.
    ///
    /// A separate axis from the theme's colour, and from the glyph set. A
    /// monochrome terminal is not a reason to still the indicator, and a terminal
    /// that cannot draw braille gets an ASCII spinner that turns perfectly well —
    /// `NO_COLOR` and the ASCII set are both about what can be *drawn*, and this
    /// is about whether anything should *move*.
    pub plain: bool,
    /// MCP servers that actually came up, and how many CALLS they answered.
    ///
    /// **From `EventKind::Mcp` and never from the configuration**, which is the
    /// whole value of the field: a server that is configured and a server that
    /// answered are different facts, and the one an operator is asking about is
    /// the second. A configured server that failed to start leaves this at zero,
    /// which is what it should look like.
    ///
    /// A count of servers and a count of calls, because "connected" is not the
    /// question either — a server that came up and was never useful is a server
    /// that will not help, and a count beside it is what says so.
    ///
    /// **The second number said `tools` from 0.10.0 to 0.16.0 and counted calls
    /// the whole time.** `EventKind::Mcp` carries no tool count and
    /// `io_harness::mcp` exposes no catalogue accessor, so the number this field
    /// wanted was never on the wire and it counted the thing that was. 0.16.0
    /// renames it rather than inventing the number, because `/mcp` now draws a
    /// per-server count beside it and two numbers disagreeing about one word is
    /// worse than one number with an honest label. See
    /// `US-IO-CLI-0.16.0-I01`, and `US-IO-HARNESS-0.68.0-I01` for the release
    /// that makes the original question answerable.
    pub mcp: (usize, usize),
    /// Language servers that came up for this workspace, by `LspStarted`.
    pub lsp: usize,
    /// The browser, and the last host it was asked about.
    ///
    /// `bool` is whether the navigation was **allowed**, and it is in the field
    /// because a host that was refused and a host that was visited must not read
    /// the same — an indicator that showed both as "browser: example.com" would
    /// report a blocked request as a successful one. `None` for a browser that
    /// started and has gone nowhere yet.
    pub browser: Option<(String, Option<bool>)>,
    /// The budgets the operator's configuration put on a turn.
    ///
    /// **A session fact and not a run fact**, which is why it is not cleared by
    /// [`Status::forget_run`] beside the counters it is measured against. The
    /// file does not change while a session runs, so `/resume` onto another
    /// conversation, a `/fork` away from this one and a rewind all land under the
    /// same `[run]` table they started under — and a budget blanked by any of
    /// them would leave the operator with a turn that will stop at a ceiling and
    /// nothing on screen saying which.
    ///
    /// Set by the driver from the contract it built, in the same place and the
    /// same way `planning` is: nothing here reads the configuration, and nothing
    /// here invents a default. `Budgets::default()` — no budget at all — is what
    /// every session carries until a contract says otherwise, and it draws
    /// nothing.
    pub budgets: Budgets,
    /// Which frame of the indicator is showing. Advanced by the tick, never by
    /// the clock: an indicator that read the time would be a second timer.
    frame: usize,
}

impl Status {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            provider: None,
            steps: None,
            unknown: 0,
            policy: None,
            tokens: None,
            run_tokens: None,
            context: None,
            containment: None,
            spend: None,
            plan: None,
            planning: false,
            jobs: 0,
            mcp: (0, 0),
            lsp: 0,
            browser: None,
            working: false,
            notice: None,
            elapsed: Duration::ZERO,
            plain: false,
            budgets: Budgets::default(),
            frame: 0,
        }
    }

    /// Move the indicator on one frame. Called from the tick and from nowhere
    /// else, so the animation cannot outlive the thing it is reporting on.
    pub fn advance(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Forget everything the *run* said, keeping what the *session* is.
    ///
    /// Called when the conversation under this line changes: `/resume` onto
    /// another session, `/fork` away from this one, a rewind that undoes the turn
    /// that set a field. Every field cleared here is a per-run fact — the tokens
    /// that run spent, how full its context got, how its commands were contained,
    /// how much of its plan the agent claimed — and none of them outlives the run
    /// that reported it. A line that goes on asserting them is describing a
    /// conversation that is no longer on the screen.
    ///
    /// The whole class rather than `plan` alone. `tokens`, `context` and
    /// `containment` have had the same hole since they were added and would want
    /// the same call at the same three sites, and four methods to make one moment
    /// true is three more than the moment has.
    ///
    /// Nothing is read back to replace them, though the store holds the resumed
    /// run's plan: F12 sets that field from `TodoWrote`'s own items with no store
    /// read, and absent is the honest answer until the agent writes a list in the
    /// run that is now on screen.
    ///
    /// The model, the posture, plain mode and the session's age are not run facts
    /// and are left alone — the session is the same session either way.
    pub fn forget_run(&mut self) {
        self.tokens = None;
        // Both new in 0.11.0, and both run facts. The provider is the one that
        // reads as a session fact and is not: a resumed conversation may have
        // been served by another provider entirely, and `Started` sets it again
        // on the next turn. Clearing it here rather than on `Started` is F10's
        // sabotage arm — blanking it as a run begins is exactly when it is about
        // to become true.
        self.provider = None;
        self.steps = None;
        self.context = None;
        self.containment = None;
        self.spend = None;
        self.plan = None;
        // A handle belongs to the run that started it. Leaving the count behind
        // would have `/resume`, `/fork` and a rewind assert that another run's
        // jobs are still alive — and there would be no event left that could ever
        // close them.
        self.jobs = 0;
        // A connection belongs to the run that opened it. io-harness brings MCP
        // servers, language servers and a browser up for the run and takes them
        // down with it, so a line that kept them would claim a session is wired to
        // something no process is holding open any more.
        self.mcp = (0, 0);
        self.lsp = 0;
        self.browser = None;
    }

    /// A run is starting: the clock and the run's own counter go back to zero.
    ///
    /// **The session's total does not.** The footer says what this session has
    /// cost and a spend that fell back to zero on every turn would be a spend
    /// nobody could read; the activity line says what the turn in front of you
    /// is costing, and a number that only climbs across an hour says nothing
    /// about it.
    pub fn start_run(&mut self) {
        self.elapsed = Duration::ZERO;
        self.run_tokens = None;
    }

    /// The indicator, if there is anything to indicate and anywhere to show it.
    ///
    /// `None` under `NO_COLOR`, where an animation is noise a reader cannot use —
    /// and `None` when nothing is running, because a session that spins while it
    /// waits for a prompt is lying about being busy.
    ///
    /// **`None` in plain mode, which is the whole of the animation half of F1.**
    /// This is the one gate: the frames are reached through here and nowhere
    /// else, so a mode threaded to every other surface and missed here would
    /// still turn — which is the exact shape the criterion's sabotage arm names,
    /// and the reason this method is what `tests/plain.rs` asserts on directly
    /// rather than only through the bytes it eventually produces.
    pub fn indicator(&self, theme: &Theme) -> Option<char> {
        let frames = theme.glyphs.spinner;
        if self.plain || !self.working || !theme.coloured || frames.is_empty() {
            return None;
        }
        Some(frames[self.frame % frames.len()])
    }

    /// Each budget in force, with what is left of it — and nothing at all for
    /// the ones that do not exist.
    ///
    /// **One method feeding both renderers, and that is the whole point of it
    /// being a method rather than two blocks.** `Status` is drawn two ways:
    /// [`Status::line`] is the one-row form, and [`Status::footer`] is the
    /// three-row form [`Status::render`] picks on any terminal seven rows or
    /// taller — which is to say the form the binary actually draws at an ordinary
    /// prompt, while `line` has one production caller and it is the short-terminal
    /// fallback. 0.12.0 added a field to `line` alone, asserted `line` alone, and
    /// shipped a mode that was nowhere on screen in a live capture. Composing the
    /// text once here means the two forms cannot say different things about a
    /// budget, whatever a test happens to call.
    ///
    /// **What is left is arithmetic over counters this struct already carries.**
    /// `steps` is what the run has taken, `run_tokens` is what the turn has spent
    /// — the turn's own counter and not the session's, because the token budget
    /// bounds a run — and `elapsed` is how long it has been going. A second set
    /// of counters tracking the same three numbers would be three more things
    /// that can disagree with the line above them. There is no single accessor to
    /// read instead: `EventKind::SpendDraw` carries a remainder, and io-harness
    /// emits it from the contained tree loop only, so a flat session would never
    /// see one.
    ///
    /// Saturating on all three. A budget is a ceiling the harness stops the run
    /// at, not a fence it cannot cross — the last step of a turn may finish over
    /// its token budget — and `0` left is the honest reading of that, where a
    /// wrapped subtraction would report an exhausted budget as an enormous one.
    ///
    /// **No clock is read here.** `elapsed` arrives from the driver, which is what
    /// keeps `tests/timing.rs`'s claim true and what makes the time budget's
    /// remainder a number a test can state rather than race.
    pub fn budgets_left(&self) -> Vec<String> {
        self.budgets_left_of(self.budgets)
    }

    /// The same remainders against a set of ceilings this session has not
    /// necessarily run under yet.
    ///
    /// **`/status` is the caller, and it exists so that reading the page changes
    /// nothing.** That surface reports what the *next* turn would run under, and
    /// the budgets reach [`Status::budgets`] only where a turn is built — so
    /// asking before the first turn would otherwise be told there are no ceilings
    /// while `io.toml` plainly sets three, which is the exact defect this release
    /// exists to end. The first shape of it assigned the field instead, and a
    /// read-only command that changes what is on the status line is a surprise
    /// nobody asked for: the fields appeared the moment the page was opened.
    ///
    /// What is drawn *against* the ceilings is still this session's own — the
    /// steps it has taken, the tokens it has been billed for, the time it has
    /// spent — because those are facts about the session however the ceilings
    /// were arrived at.
    pub fn budgets_left_of(&self, budgets: Budgets) -> Vec<String> {
        let mut left = Vec::new();
        if let Some(cap) = budgets.steps {
            let rest = cap.saturating_sub(self.steps.unwrap_or(0));
            left.push(format!(
                "left {rest}/{cap} step{}",
                if rest == 1 { "" } else { "s" }
            ));
        }
        if let Some(cap) = budgets.tokens {
            let rest = cap.saturating_sub(self.run_tokens.unwrap_or(0));
            left.push(format!(
                "left {}/{} tok",
                format_tokens(rest),
                format_tokens(cap)
            ));
        }
        if let Some(cap) = budgets.duration {
            // No unit word, because `format_elapsed` already spells one into
            // every answer it gives — `12s`, `4m30s`, `1h02m` — and a `left
            // 4m30s/10m00s min` would be naming the unit twice and getting it
            // wrong on the hour.
            left.push(format!(
                "left {}/{}",
                format_elapsed(cap.saturating_sub(self.elapsed)),
                format_elapsed(cap)
            ));
        }
        left
    }

    /// The fields, most important first.
    pub fn fields(&self, theme: &Theme) -> Vec<Field> {
        // The WORD is the state, and the animation is only beside it. A spinner
        // carries a meaning solely for a reader who can see it move, and this
        // line has to work in a screen reader, under `NO_COLOR` and in a log — so
        // the indicator is a prefix on the field, never the field itself.
        //
        // **The state is the one field that changes meaning rather than value**,
        // so it is the one that changes colour: a session doing work reads as
        // `working` in the accent this interface uses for anything live, and an
        // idle one reads as `ready` in the muted tone everything at rest wears.
        // The word is still the state — the colour agrees with it and never
        // carries it alone.
        let state = match (self.working, self.indicator(theme)) {
            (true, Some(frame)) => Field::new(format!("{frame} working"), Tone::Accent),
            (true, None) => Field::new("working", Tone::Accent),
            (false, _) => Field::new("ready", Tone::Muted),
        };
        // The model is what this session IS, and it is the field that survives
        // every narrowing, so it is the one thing on the line drawn in weight.
        let mut fields = vec![Field::bold(self.model.clone(), Tone::Accent)];
        // Second, and the last field to be dropped after the model. What the agent
        // is allowed to do outranks how long it has been doing it.
        if let Some(policy) = &self.policy {
            fields.push(Field::new(format!("policy:{policy}"), Tone::Normal));
        }
        fields.push(state);
        // Elapsed stays fourth: it is the field 0.1.1 exists for, and the one a
        // reader checks to answer "is this alive". Everything this release adds
        // goes to the right of it, which is the order they drop in.
        fields.push(Field::new(format_elapsed(self.elapsed), Tone::Muted));
        // **Fifth, and spelled the way the posture is.** `provider:openrouter`
        // rather than a bare name, because two bare names side by side — the
        // model and the provider — cannot be told apart by a reader who does not
        // already know which vendor sells which model. It is deliberately not
        // the word `via`: that is the string the removed line used, and F2
        // asserts it never reaches a terminal again.
        if let Some(provider) = &self.provider {
            fields.push(Field::new(format!("provider:{provider}"), Tone::Muted));
        }
        // **Immediately right of the clock, and above everything else this line
        // carries.** 0.8.0 drafted the spend field to the right of the containment
        // word and it was invisible on the first terminal it met; the lesson is
        // that field order is a decision about what survives a narrow screen. This
        // one answers the same question the clock does — is anything still
        // happening — costs four cells, and appears on no line at all unless a
        // background job is actually running.
        if self.jobs > 0 {
            fields.push(Field::new(format!("bg {}", self.jobs), Tone::Normal));
        }
        // The three connection fields, right of the background count and left of
        // everything numeric. Each is absent until an event says otherwise — zero
        // servers is not "0 servers", it is a session that has none — so on the
        // overwhelming majority of lines all three cost nothing at all.
        if self.mcp.0 > 0 {
            fields.push(Field::new(
                format!("mcp {}/{} calls", self.mcp.0, self.mcp.1),
                Tone::Normal,
            ));
        }
        if self.lsp > 0 {
            fields.push(Field::new(format!("lsp {}", self.lsp), Tone::Normal));
        }
        if let Some((host, allowed)) = &self.browser {
            // A refusal is drawn as a refusal. The whole point of carrying the
            // host is that the operator can see which one, and the whole point of
            // carrying the verdict is that a blocked host must not read like a
            // visited one.
            let (text, tone) = match allowed {
                Some(true) => (format!("web {host}"), Tone::Normal),
                Some(false) => (format!("web {host} refused"), Tone::Refused),
                None => ("web ready".to_string(), Tone::Muted),
            };
            fields.push(Field::new(text, tone));
        }
        // Beside the token count rather than anywhere else, because the two are
        // the same kind of fact — what this run has spent — and they used to sit
        // together in the `Finished` row this release removed.
        if let Some(steps) = self.steps {
            fields.push(Field::new(
                format!("{steps} step{}", if steps == 1 { "" } else { "s" }),
                Tone::Muted,
            ));
        }
        if let Some(tokens) = self.tokens {
            fields.push(Field::new(
                format!("{} tok", format_tokens(tokens)),
                Tone::Muted,
            ));
        }
        if let Some(context) = self.context {
            fields.push(Field::new(format!("ctx {context}%"), Tone::Muted));
        }
        // **Left of the containment word, which is where the design put it and
        // where a live run proved it has to be.** Drafted to the right of it, the
        // field never appeared: a real containment word is `workspace-write/
        // macos-sandbox-exec`, thirty-three characters, and at a hundred columns
        // beside the model, the posture, the state, the clock and the token count
        // there was nothing left — so the one field this release exists to fill
        // was the first one dropped, on the first terminal it was run in.
        //
        // What it outranks is the honest ordering too: the containment word says
        // how commands are sandboxed and does not change during a turn; this says
        // what the fan-out is spending, and changes every step.
        // **Left of the numbers, because it is not one.** A standing mode that
        // stops the agent writing outranks what the last turn spent: if a narrow
        // terminal can hold one of them, it should hold the one that explains why
        // nothing is happening. `Normal` rather than `Muted` for the same reason
        // the background-job count is — it is a fact about what the agent may do,
        // not a footnote about what it did.
        if self.planning {
            fields.push(Field::new("planning".to_string(), Tone::Normal));
        }
        // **Right of the counters they bound and left of the tree's own spend.**
        // A budget is read against the number beside it — `3 steps` and `left
        // 17/20 steps` are one fact split in two — so the two travel together and
        // narrow together. It is left of `spend` because `spend` is the sub-agent
        // tree's shared ceiling and appears on almost no line at all, while a
        // budget an operator wrote in `[run]` is in force on every turn of the
        // session that configured it.
        //
        // `Tone::Normal` rather than the muted tone the counters wear, for the
        // reason `planning` and the background-job count are: this is a bound on
        // what the agent may do and the explanation for a turn that is about to
        // stop, not a footnote about what it did.
        for text in self.budgets_left() {
            fields.push(Field::new(text, Tone::Normal));
        }
        if let Some((drawn, remaining)) = self.spend {
            let text = match remaining {
                Some(left) => format!(
                    "spend {}/{}",
                    format_tokens(drawn),
                    format_tokens(drawn + left)
                ),
                // No ceiling was reported, so none is stated. A `0` here would be
                // an exhausted tree; a total invented from the draw would be a
                // ceiling nobody set.
                None => format!("spend {}", format_tokens(drawn)),
            };
            fields.push(Field::new(text, Tone::Muted));
        }
        if let Some(containment) = &self.containment {
            fields.push(Field::new(containment.clone(), Tone::Muted));
        }
        // Rightmost, and so the first field to go when the terminal narrows. It is
        // the only field on this line that is not an observation — everything to
        // its left is something the harness reported happening, and this is what
        // the agent says about its own work — and the plan itself is in the
        // transcript a row above for a reader who wants more than the count.
        //
        // `claimed` rather than `done` for that reason, in the same words the
        // transcript's own plan header uses. The one-word form is what fits beside
        // six other fields at eighty columns.
        if let Some((done, total)) = self.plan {
            fields.push(Field::new(
                format!("plan {done}/{total} claimed"),
                Tone::Muted,
            ));
        }
        // Past the rightmost field, and on no line at all until a harness this
        // release has never met emits something nobody has triaged. It is a
        // diagnostic rather than an observation about the work, which is why it
        // is the first thing a narrow terminal drops.
        if self.unknown > 0 {
            fields.push(Field::new(
                format!("unknown {}", self.unknown),
                Tone::Warning,
            ));
        }
        fields
    }

    /// The line, fitted to `width` by dropping whole fields from the right.
    pub fn line(&self, width: u16, theme: &Theme) -> Line<'static> {
        let fields = self.fields(theme);
        let kept = fits(&fields, width as usize, theme);

        // Even at a width that fits nothing whole, the model is what gets shown,
        // shortened. A blank status line is worse than a truncated one.
        if kept == 0 {
            let model: String = self.model.chars().take(width as usize).collect();
            return Line::from(Span::styled(model, theme.style(Tone::Accent)));
        }
        spans(&fields[..kept], theme)
    }

    /// The footer: a rule, then two rows.
    ///
    /// **One long dot-separated run is not a status line, it is a sentence with
    /// the punctuation removed.** Eight fields in one grey stream at a hundred
    /// and ten columns has no anchor, nothing to skim to, and no way to tell what
    /// changed since the last time you looked.
    ///
    /// So it is laid out the way every well-drawn terminal footer is — helix's
    /// left/right zones, tmux's multi-row status, lipgloss's dim-label/normal-
    /// value pairs:
    ///
    /// - a rule across the terminal, which is the boundary the transcript needs
    ///   above it and the one thing that says the rows below are not output;
    /// - **row one is identity and state**: the state word with its dot, then the
    ///   model — the only bold token on either row — and the clock pushed to the
    ///   right edge;
    /// - **row two is everything countable**, dim, with the posture and the
    ///   sandbox on the right where a reader looks for what they are allowed to
    ///   do, and the keys that matter right now at the end of it.
    ///
    /// Exactly one thing is coloured and exactly one is bold. Everything else is
    /// the ordinary foreground or the muted tone, which is what makes those two
    /// mean something — the failure this replaces is a line where every field was
    /// the same grey and none of them stood out because none of them could.
    pub fn footer(&self, width: u16, theme: &Theme) -> Vec<Line<'static>> {
        let room = width as usize;
        let muted = theme.style(Tone::Muted);
        let separator = theme.glyphs.separator;

        let rule = Line::from(Span::styled(
            theme.glyphs.rule.to_string().repeat(room),
            muted,
        ));

        // The state, and its dot. A shape as well as a colour, because a colour
        // that is the only difference between `ready` and `working` is a
        // difference a monochrome terminal does not have.
        // **The state word is here only when no activity line is saying it
        // louder.** While a turn runs the row above carries a spinner, a word
        // for the turn, the clock and the tokens; repeating `working` and a
        // second spinner under it put two things on screen turning at the same
        // rate to say one fact. Idle there is no row above, and `ready` is what
        // says the session is alive and waiting.
        //
        // The dot is a shape as well as a tone, because a colour that is the
        // only difference between two states is a difference a monochrome
        // terminal does not have.
        let mut left = Vec::new();
        if !self.working {
            left.push(Span::styled("• ready", muted));
            left.push(Span::styled(separator, muted));
        }
        left.push(Span::styled(
            self.model.clone(),
            theme
                .style(Tone::Normal)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
        if let Some(provider) = &self.provider {
            left.push(Span::styled(separator, muted));
            left.push(Span::styled(provider.clone(), muted));
        }
        let identity = row(
            left,
            vec![Span::styled(format_elapsed(self.elapsed), muted)],
            room,
        );

        // Everything countable, and nothing that is not. Each is absent until
        // there is something to count, so a session that has run nothing carries
        // an almost empty row rather than a row of zeroes.
        let mut counts: Vec<String> = Vec::new();
        if let Some(steps) = self.steps {
            counts.push(format!("{steps} step{}", if steps == 1 { "" } else { "s" }));
        }
        if let Some(tokens) = self.tokens {
            counts.push(format!("{} tok", format_tokens(tokens)));
        }
        if let Some(context) = self.context {
            counts.push(format!("ctx {context}%"));
        }
        // **Here as well as on `Status::line`, from the same method, and that is
        // deliberate rather than tidy.** This is the row the binary draws at an
        // ordinary prompt — `Status::render` takes the footer on any terminal
        // seven rows or taller — so a budget added to `line` alone would be a
        // budget no operator ever saw, which is exactly what 0.12.0's planning
        // field did and what the comment in the `allowed` group below records.
        // Beside the counters rather than in the group on the right: what is left
        // of a budget is a number, and it moves every step.
        counts.extend(self.budgets_left());
        if let Some((done, total)) = self.plan {
            counts.push(format!("plan {done}/{total}"));
        }
        if self.jobs > 0 {
            counts.push(format!("bg {}", self.jobs));
        }
        if self.mcp.0 > 0 {
            counts.push(format!("mcp {}/{} calls", self.mcp.0, self.mcp.1));
        }
        if self.lsp > 0 {
            counts.push(format!("lsp {}", self.lsp));
        }
        if self.unknown > 0 {
            counts.push(format!("unknown {}", self.unknown));
        }
        // The keys that mean something at this exact moment, and only those. A
        // footer that listed every binding would be a help screen; `/help` is
        // the help screen.
        counts.push(
            if self.working {
                "esc stops"
            } else {
                "/ for commands"
            }
            .to_string(),
        );

        // What the agent is allowed to do, on the right, because that is the
        // question a reader asks of a footer when they ask anything of it.
        let mut allowed = Vec::new();
        if let Some(policy) = &self.policy {
            allowed.push(Span::styled(policy.clone(), muted));
        }
        if let Some(containment) = &self.containment {
            if !allowed.is_empty() {
                allowed.push(Span::styled(separator, muted));
            }
            allowed.push(Span::styled(containment.clone(), muted));
        }
        // **Here and not in `counts`, and a live run is what settled it.** The
        // phase is not countable, so `counts` was the wrong group — but this
        // group is the right one for a sharper reason than tidiness: while the
        // phase is on, io-harness denies every write and every exec until a plan
        // is approved. That is precisely "what the agent is allowed to do", which
        // is what this half of the row is for.
        //
        // The first cut of 0.12.0 put the field on `Status::line` alone and a
        // unit test asserting `Status::line` passed. The binary drew the footer,
        // the word was nowhere on screen, and the operator had a mode they could
        // not see — the exact failure F4 exists to prevent, in the release that
        // added F4.
        if self.planning {
            if !allowed.is_empty() {
                allowed.push(Span::styled(separator, muted));
            }
            allowed.push(Span::styled("planning", muted));
        }
        let counted = row(
            vec![Span::styled(counts.join(separator), muted)],
            allowed,
            room,
        );

        // The notice takes the counts row while it is up. It is the least
        // load-bearing of the three — the identity row says what this session is
        // and the rule is the boundary — and a notice that pushed the viewport a
        // row taller would move the prompt under the operator's hands every time
        // one appeared.
        match &self.notice {
            Some((tone, text)) => vec![
                rule,
                identity,
                row(
                    vec![Span::styled(text.clone(), theme.style(*tone))],
                    Vec::new(),
                    room,
                ),
            ],
            None => vec![rule, identity, counted],
        }
    }

    /// The activity line, or `None` when no turn is in flight.
    ///
    /// **Present for exactly the turn, and it is `working` that says so** — the
    /// same flag `App::started` sets and `App::finished` clears, whether the turn
    /// ended on an answer, an interrupt, a refusal or an error. A second source
    /// of truth here is how a line ends up spinning a clock over an idle
    /// session.
    ///
    /// Visibly the status line's sibling: the same fields, the same separator,
    /// the same drop-from-the-right rule. What it drops first is the token count
    /// and then the clock, because the word is the fact and the numbers beside it
    /// are already on the status line under it.
    pub fn activity(&self, width: u16, theme: &Theme) -> Option<Line<'static>> {
        if !self.working {
            return None;
        }
        // The word is chosen by the step, so it is stable for as long as the step
        // is and moves when the work does — no timer of its own, no randomness,
        // and a test can state which word it expects.
        let word = WORDS[self.steps.unwrap_or(0) as usize % WORDS.len()];
        // The indicator is a prefix on the word and never the word itself, for
        // the reason the status line's state field says out loud: a spinner means
        // something only to a reader who can see it move.
        let mut fields = vec![Field::new(
            match self.indicator(theme) {
                Some(frame) => format!("{frame} {word}"),
                None => word.to_string(),
            },
            Tone::Normal,
        )];
        fields.push(Field::new(format_elapsed(self.elapsed), Tone::Muted));
        // This turn's own spend, not the session's. The footer carries the
        // session total; a row about the turn in front of you carries the turn.
        if let Some(tokens) = self.run_tokens {
            fields.push(Field::new(
                format!("{} tok", format_tokens(tokens)),
                Tone::Muted,
            ));
        }

        let kept = fits(&fields, width as usize, theme);
        if kept == 0 {
            let word: String = word.chars().take(width as usize).collect();
            return Some(Line::from(Span::styled(word, theme.style(Tone::Normal))));
        }
        Some(spans(&fields[..kept], theme))
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        // Three rows draws the footer; anything less draws the one line this
        // product has always had, which still says everything in one run and is
        // what a terminal with no room left can be given.
        let lines = if area.height >= 3 {
            self.footer(area.width, theme)
        } else {
            vec![self.line(area.width, theme)]
        };
        frame.render_widget(Paragraph::new(lines), area);
    }
}

/// How many of `fields`, left to right, fit in `width`.
///
/// The drop-from-the-right rule, in one place because two lines follow it. A
/// field is kept whole or not at all: half a word is not a shorter fact, it is a
/// different one.
fn fits(fields: &[Field], width: usize, theme: &Theme) -> usize {
    // Measured off the chosen set rather than off a constant. Both sets spell
    // the separator in three cells, so the arithmetic lands on the same answer
    // either way — but a set that did not would have shifted every drop
    // decision, and this is the input that says so.
    let separator_width = theme.glyphs.separator.chars().count();
    let mut used = 0usize;
    let mut kept = 0usize;
    for field in fields {
        // Counted in characters, not bytes: the separator's middle dot is two
        // bytes and one cell, and `len()` here would reserve room that is not
        // needed and drop a field one column early.
        let extra = field.text.chars().count() + if kept == 0 { 0 } else { separator_width };
        if used + extra > width {
            break;
        }
        used += extra;
        kept += 1;
    }
    kept
}

/// One footer row: a left group, a right group, and the gap between them.
///
/// The right group is pushed to the edge by padding rather than by a fill
/// character — starship's `fill` draws a rule between the two, and a rule inside
/// a row that already sits under one is a second boundary saying the same thing.
///
/// When the two groups cannot both fit, the right one goes. It is the group a
/// reader can find elsewhere: the posture is on the wizard's own screen and in
/// the configuration, and the clock is beside the word `working` on the row that
/// says a turn is running at all.
fn row(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let measure = |spans: &[Span<'static>]| -> usize {
        spans.iter().map(|span| span.content.chars().count()).sum()
    };
    let mut spans = left;
    let used = measure(&spans);
    let wanted = measure(&right);
    // One column of breathing room between the groups, at least.
    if !right.is_empty() && used + wanted < width {
        spans.push(Span::raw(" ".repeat(width - used - wanted)));
        spans.extend(right);
    }
    Line::from(spans)
}

/// The fields as one line, separated by the theme's own separator.
fn spans(fields: &[Field], theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                theme.glyphs.separator,
                theme.style(Tone::Muted),
            ));
        }
        let style = theme.style(field.tone);
        spans.push(Span::styled(
            field.text.clone(),
            if field.bold {
                style.add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                style
            },
        ));
    }
    Line::from(spans)
}

/// `12s`, `1m12s`, `1h02m`. Never a bare number of seconds past a minute, which
/// is unreadable at the point a session has been going long enough to care.
pub fn format_elapsed(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m{:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h{:02}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// `840`, `1.5k`, `12.4k`. A running total is read for its magnitude, and six
/// digits of it are six characters of a line that has to fit in eighty columns.
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        return tokens.to_string();
    }
    format!("{:.1}k", tokens as f64 / 1_000.0)
}

/// What a `Contained` event reads as: the mode, then the backend that answered.
///
/// Never the mode alone. The two disagree often — a `workspace-write` run on a
/// host with no sandbox available reaches the portable floor — and it is the
/// second word that says what is actually enforcing anything.
pub fn format_containment(mode: &str, backend: &str) -> String {
    format!("{mode}/{backend}")
}

/// The whole session state, as the lines `/status` commits into the scrollback.
///
/// **Every field here is a value io-harness supplied, and the ones that are not
/// readable from a live object are read from the event that carries them rather
/// than reconstructed.** That is not a stylistic preference: three of these facts
/// are behind `pub(crate)` in io-harness and there is no accessor for any of
/// them. `ExecContainment` is private, so the backend that *actually* contained
/// this run comes from the `Contained` event, which io-harness emits once per run
/// and always — carrying `none` for full access rather than being absent — and it
/// reaches this struct through `App::event`. The containment `Ledger` is built
/// inside the tree run and never returned, so the draw against the tree's ceiling
/// comes from the `SpendDraw` stream this interface already routes. `McpSession`
/// and `LspSession` are private too, so what is *connected* comes from the `Mcp`
/// and `LspStarted` events and is stated beside what the contract *configured*,
/// which is the only pair that answers the question an operator is actually
/// asking — a server named in the file and a server that answered are different
/// facts.
///
/// **Nothing here is composed a second time.** The budgets are
/// [`Status::budgets_left`], which is the same method the status line and the
/// footer render, made public in this release precisely so that this surface does
/// not become a third spelling of `left 17/20 steps`. The containment word is the
/// same `mode/backend` pair [`format_containment`] built for the line. The
/// contract handed in is the contract `crate::contract::session` would build for
/// the next turn, so the configured rosters and the skills directory are the ones
/// that would actually reach it.
///
/// **It is not a table, and that is a decision about eighty columns rather than
/// about taste.** A table has a column width, a column width is decided by the
/// widest cell, and the widest cell here is a workspace path — so at eighty
/// columns a table either truncates the path or truncates everything beside it.
/// This is one fact per row, `label: value`, with nothing padded into a column
/// and nothing aligned across rows, so there is no width for anything to be
/// squeezed out of. A row too long for the terminal is **folded**, never cut: a
/// status surface that shortened the very thing a reader opened it to read would
/// be the one surface in this product that cannot be trusted.
///
/// **Plain mode needs no path of its own here.** The three switches this codebase
/// already models stay three: `NO_COLOR` is `Theme::coloured` and reaches this
/// through `Theme::style`, the glyph set is `Theme::glyphs` and is already ASCII
/// whenever `--plain` is on, and `Status::plain` governs whether anything
/// *animates* — which nothing committed into a scrollback ever does. So plain
/// mode is this same function drawn with the theme the session already carries,
/// and the only difference on screen is the rule, the separator and the dash. No
/// colour carries a meaning of its own on any row: every fact is spelled in
/// words, which is also what makes it readable in a screen reader and in a log.
#[allow(clippy::too_many_arguments)]
pub fn committed(
    status: &Status,
    session: &Session,
    policy: &Policy,
    contract: &TaskContract,
    // The caps the NEXT turn would run under, which is `None` both for a session
    // that configured no `[app.io-cli.containment]` and for one that typed
    // `/contain off`. Absence is drawn as the absence of containment rather than
    // as a missing field: a session that cannot fan out is a fact about it.
    caps: Option<&Containment>,
    theme: &Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let dash = theme.glyphs.dash;
    let rule = theme.glyphs.rule;
    let mut facts: Vec<(String, String)> = Vec::new();

    // The workspace and the conversation, both asked of the `Session` rather than
    // threaded down from the driver, so there is one answer to "which workspace
    // is this" and it is io-harness's — the same rule `App::set_root` follows.
    facts.push(("workspace".into(), session.root().display().to_string()));

    // **Where io-cli keeps what it keeps, and who decided.** The directory in
    // force rather than [`crate::home::path`]: under `$IO_CONFIG` the file is
    // somewhere io-cli did not choose, and reporting the home this crate *would*
    // have picked would be wrong in the one case this row exists for. The word
    // beside it is `Origin::word`, so `default`, `IO_CONFIG` and `IO_CONFIG_HOME`
    // are spelled by the module that decides between them and not a second time
    // here.
    facts.push((
        "home".into(),
        match crate::home::in_force() {
            Some((dir, origin)) => format!("{} {dash} {}", dir.display(), origin.word()),
            // Same shape `io_harness::config::user_path` returns for the same
            // reason: with no home directory to work from there is no answer, and
            // inventing one would name a directory nothing reads.
            None => format!("not known {dash} this process has no home directory"),
        },
    ));
    facts.push((
        "session".into(),
        match session.head() {
            Some(head) => format!("{} {dash} head at turn {head}", session.id()),
            None => format!("{} {dash} no turn has run in it yet", session.id()),
        },
    ));
    facts.push(("model".into(), status.model.clone()));
    facts.push((
        "provider".into(),
        match &status.provider {
            Some(provider) => provider.clone(),
            // Absent rather than guessed. The provider is whichever one answered,
            // and a fallback moves it mid-session — so before a turn has started
            // there is nothing true to say.
            None => {
                format!("not known until a turn has started {dash} whichever one answers")
            }
        },
    ));

    // **Every layer by name, with the acts it governs.** `Policy::layers` is a
    // public field and `Layer`, `Rule`, `Act` and `Effect` are all public, so this
    // is the harness's own stack read out rather than io-cli's summary of it. The
    // acts are deduplicated in the order they were written: a layer with forty
    // rules over two acts is two words, and forty rows here would bury the layer
    // whose one rule is the reason the agent was refused.
    if policy.layers.is_empty() {
        facts.push((
            "policy".into(),
            format!("no layer {dash} only the defaults decide"),
        ));
    }
    for layer in &policy.layers {
        let mut acts: Vec<&'static str> = Vec::new();
        // Named `entry` rather than `rule`, which is already the rule glyph in
        // this scope — two things called `rule` in one function is how a reader
        // ends up reading the wrong one.
        for entry in &layer.rules {
            let word = crate::approval::act_word(entry.act);
            if !acts.contains(&word) {
                acts.push(word);
            }
        }
        facts.push((
            format!("policy {}", layer.name),
            if acts.is_empty() {
                format!("no rule {dash} it governs nothing")
            } else {
                acts.join(", ")
            },
        ));
    }

    // **The mode asked for beside the backend that answered, and never the mode
    // alone.** The two disagree often — a `workspace-write` run on a host with no
    // sandbox available reaches the portable floor — and it is the second word
    // that says what is enforcing anything. It is `EventKind::Contained`'s pair,
    // carried on this struct by `App::event`; nothing here names a backend.
    facts.push((
        "sandbox".into(),
        match &status.containment {
            Some(word) => word.clone(),
            None => format!(
                "not known until a turn has run {dash} the mode and the backend are \
                 reported when one starts"
            ),
        },
    ));
    facts.push((
        "containment".into(),
        match caps {
            Some(caps) => format!(
                "up to {} agents, {} at once per tier, {} deep, {} tokens for the tree",
                caps.max_total_agents,
                caps.max_concurrent_agents,
                caps.max_depth,
                caps.max_total_tokens,
            ),
            None => format!(
                "not contained {dash} the next turn does the work itself and cannot fan out"
            ),
        },
    ));
    facts.push((
        "drawn".into(),
        match status.spend {
            Some((drawn, Some(left))) => format!(
                "{} of {} against the tree",
                format_tokens(drawn),
                format_tokens(drawn + left)
            ),
            // A tree with no ceiling reports no remainder, and inventing a total
            // from the draw would be a ceiling nobody set.
            Some((drawn, None)) => {
                format!("{} {dash} no ceiling was reported", format_tokens(drawn))
            }
            None => "nothing has been drawn against the tree yet".to_string(),
        },
    ));

    // The budgets, through the one method both other renderers use. A budget that
    // does not exist contributes no row there, so the empty case is said here
    // rather than left as a gap somebody has to interpret.
    let budgets = status.budgets_left_of(Budgets::in_force(contract));
    if budgets.is_empty() {
        facts.push((
            "budget".into(),
            format!("none {dash} no ceiling from `[run]` is in force"),
        ));
    }
    for text in budgets {
        facts.push(("budget".into(), text));
    }

    facts.push((
        "context".into(),
        match status.context {
            Some(fill) => format!("{fill}% of the budget io-harness declares"),
            None => "not known until the context has been folded once".to_string(),
        },
    ));

    // **What answered, beside what was configured.** Either number alone is a
    // half-answer: three configured and none connected is the state an operator
    // is trying to find, and it reads exactly like a session with none configured
    // if only the live count is shown.
    // `status.mcp.1` counts CALLS, which this sentence called tools offered until
    // 0.17.0 — the one site 0.16.0's rename missed, while the status line itself
    // has said `mcp N/M calls` since. It is doubly wrong now that `/mcp` draws a
    // real offered count from `EventKind::Mcp`'s own `tools` field: two different
    // numbers under one word is worse than either alone.
    facts.push((
        "mcp".into(),
        format!(
            "{} of {} configured connected, answering {} call{}",
            status.mcp.0,
            contract.mcp.len(),
            status.mcp.1,
            if status.mcp.1 == 1 { "" } else { "s" },
        ),
    ));
    facts.push((
        "lsp".into(),
        format!(
            "{} of {} configured started",
            status.lsp,
            contract.lsp.len()
        ),
    ));
    facts.push((
        "browser".into(),
        match (&status.browser, &contract.browser) {
            // A refusal is drawn as a refusal, for the reason the status line's
            // own field is: a blocked host must not read like a visited one.
            (Some((host, Some(true))), _) => format!("at {host}"),
            (Some((host, Some(false))), _) => format!("refused {host}"),
            (Some((_, None)), _) => "started, and has gone nowhere yet".to_string(),
            (None, Some(_)) => format!("configured {dash} not started"),
            (None, None) => "not configured".to_string(),
        },
    ));
    facts.push((
        "skills".into(),
        match &contract.skills {
            Some(dir) => dir.display().to_string(),
            None => "not configured".to_string(),
        },
    ));

    // Three of whatever the set draws a rule with, at both ends — the same edge
    // `crate::transcript` gives a committed conversation, and for the same
    // reason: this lands in a scrollback that already holds every earlier turn,
    // and a passage with no edges is one a reader cannot tell the extent of.
    let room = width as usize;
    let mut lines = vec![Line::from(Span::styled(
        format!("{rule}{rule}{rule} status"),
        theme.style(Tone::Accent),
    ))];
    for (label, value) in &facts {
        for row in folded(&format!("{label}: {value}"), room, 2, 4) {
            lines.push(Line::from(Span::styled(row, theme.style(Tone::Normal))));
        }
    }
    lines.push(Line::from(Span::styled(
        format!("{rule}{rule}{rule} status ends"),
        theme.style(Tone::Accent),
    )));
    lines
}

/// `text` as rows no wider than `width`, indented `first` and then `rest`.
///
/// **Folded and never fitted**, which is the one thing this differs in from every
/// other width-aware helper in this crate. `crate::picker::fit` and
/// [`Status::line`] shorten, because a picker row and a status line each own
/// exactly one row of a viewport that cannot grow. A committed surface owns as
/// many rows as it needs, so there is no reason left to lose a character — and
/// the characters most likely to be lost here are the tail of a workspace path
/// and the tail of a policy layer's act list, which is to say the answer.
///
/// Broken at spaces, with a hanging indent that says a row is a continuation
/// without aligning anything into a column. A word longer than the room it has —
/// a deep path, an absurd model name — is **split** rather than allowed to
/// overflow: eighty columns is a supported size, and a row that runs past it gets
/// wrapped by the terminal at a place nothing here chose.
fn folded(text: &str, width: usize, first: usize, rest: usize) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut indent = first;
    let mut row = " ".repeat(indent);
    // Content characters on this row, which is its width less the indent. Counted
    // rather than measured off `row` so the arithmetic is in one unit — the same
    // reason `fits` counts characters and not bytes.
    let mut used = 0usize;
    for word in text.split_whitespace() {
        let mut word = word;
        loop {
            // At least one cell, so a terminal narrower than the indent still
            // makes progress instead of looping forever.
            let room = width.saturating_sub(indent).max(1);
            let space = usize::from(used > 0);
            let length = word.chars().count();
            if used + space + length <= room {
                if space == 1 {
                    row.push(' ');
                }
                row.push_str(word);
                used += space + length;
                break;
            }
            if used > 0 {
                rows.push(std::mem::take(&mut row));
                indent = rest;
                row = " ".repeat(indent);
                used = 0;
                // Retried whole on the fresh row: a word is only ever split when
                // a row of its own cannot hold it.
                continue;
            }
            let head: String = word.chars().take(room).collect();
            word = &word[head.len()..];
            row.push_str(&head);
            rows.push(std::mem::take(&mut row));
            indent = rest;
            row = " ".repeat(indent);
        }
    }
    if used > 0 || rows.is_empty() {
        rows.push(row);
    }
    rows
}
