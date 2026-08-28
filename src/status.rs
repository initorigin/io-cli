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
///
/// **A fourth field since 0.17.0, and it is the one that draws no row.**
/// [`Budgets::window`] is the context window a turn's prompt is assembled inside.
/// It is here rather than loose on [`Status`] because it arrives from exactly the
/// same place at exactly the same moment as the other three — off the contract
/// the driver has just built, in the one assignment that already reads it, so
/// there is no second driver line that could be forgotten — and because it is a
/// session fact for the same reason they are: the file does not change while a
/// session runs. It is an `Option` too, but for a different question; its own doc
/// says which, and why it draws nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Budgets {
    /// `[run] max_steps`, or `[app.io-cli] max_steps` where that beat it — and
    /// `None` where the cap is still io-cli's own floor.
    pub steps: Option<u32>,
    /// `[run] max_tokens`, summed across the turn's completions.
    pub tokens: Option<u64>,
    /// `[run] max_duration_secs`, as wall time across the turn.
    pub duration: Option<Duration>,
    /// The context window one turn's prompt is assembled inside, in tokens:
    /// `[run.context]`, taking its `share` of `[run] max_tokens` where the
    /// operator set one.
    ///
    /// **An `Option` for a different reason from its three neighbours, and the
    /// difference is the point.** Those are `Option` because the ceiling itself
    /// may not exist — a turn with no time budget is not a turn budgeted at zero.
    /// A context window always exists: `TaskContract::context` is a plain
    /// `ContextBudget` and not an `Option` of one, so every contract that exists
    /// declares a window, io-harness's own `24_000` where nobody chose otherwise.
    /// `None` here therefore does not mean *no window*. It means **io-cli has not
    /// been handed a contract yet** — the state [`Budgets::default`] carries for
    /// the moments before the first turn is built.
    ///
    /// A sentinel `0` would have been the alternative and would have been worse in
    /// exactly this product's characteristic way: a number that divides is a
    /// number somebody eventually divides by. [`Status::note_context`] says
    /// nothing at all on `None`, so a driver that forgot to fill this in loses the
    /// `ctx` field outright rather than quietly reporting a share of a window
    /// nobody set — which is precisely how this field spent two releases wrong.
    ///
    /// **It draws no row in [`Status::budgets_left_of`], unlike its three
    /// neighbours.** Those are remainders an operator watches drain; this one is a
    /// denominator, and it is already on the line — `ctx N%` is the assembled
    /// section over exactly this number. A `left 18k/24k ctx` beside it would be
    /// the same fact twice, in two spellings that round differently.
    pub window: Option<u64>,
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
            // **The harness's arithmetic, asked of the harness — which is what
            // the arm this replaces only claimed to be doing.** Through 0.16.0
            // the share on the line was taken against
            // `ContextBudget::default().effective_tokens(None)`, a constant
            // `24_000` on every session in the world, under a comment asserting
            // it was io-harness's own declared budget. It was the *crate's*
            // default budget; this is *this contract's*, which is a different
            // number the moment an operator writes a `[run.context]` table.
            //
            // `max_tokens` is passed and not `None`, because `ContextBudget`
            // takes its `share` of what the run's own token budget leaves: a
            // window computed as if there were no run budget is not the window
            // the assembler had. The expression is io-harness's own — `session.rs`
            // bounds `entry_cap_chars` with exactly this pair.
            window: Some(contract.context.effective_tokens(contract.max_tokens)),
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
    /// How full the assembled context was at the last step, as a share of the
    /// window *this session's contract* declares — [`Budgets::window`].
    ///
    /// **0.16.0's version of this field was wrong from the release it was added
    /// in, and it was wrong silently.** It divided by
    /// `ContextBudget::default().effective_tokens(None)` — a flat `24_000` — so an
    /// operator who set `[run.context] max_tokens = 8000` was shown a share of a
    /// window three times the one they had, and nothing on the screen could
    /// disagree with it. Both halves of the fix are elsewhere:
    /// [`Budgets::in_force`] is the denominator, and [`Status::note_context_from`]
    /// is the numerator.
    ///
    /// **And it was blank for exactly the period it would have been useful.** The
    /// only source was `EventKind::Compacted`, which io-harness emits when a fold
    /// happens and never otherwise — so a session whose context never filled up
    /// showed no `ctx` field at all, and the first number an operator ever saw was
    /// one taken after the section had just been cut down. The per-step estimate
    /// was durable in the harness store the whole time, which is the ponytail note
    /// that stood here for two releases and is now cashed in.
    ///
    /// `None` before the first step of the first turn, which stays the honest
    /// answer: nothing has been assembled, and a session that has assembled
    /// nothing has not assembled zero tokens — the rule `tokens` and `spend` are
    /// held to. The *window* is known from the moment a contract exists, and
    /// [`committed`] says it there rather than leaving the page blank.
    ///
    /// Cleared by [`Status::forget_run`] and deliberately still so. The share is
    /// an observation the undone run made about its own ledger; a rewind or a
    /// `/resume` puts a different conversation on screen, whose section this
    /// number never measured. The window beside it is not cleared, because the
    /// file did not change.
    pub context: Option<u8>,
    /// How this run's commands are contained: the mode asked for and the backend
    /// that actually answered on this host.
    ///
    /// Both, always. io-harness's own documentation is explicit that a surface
    /// showing the mode alone is reading an intention — `workspace-write` reaching
    /// a portable floor means resource caps and nothing else.
    pub containment: Option<String>,
    /// The branch the working tree is on, or `None` where there is no answer.
    ///
    /// **A fact about the checkout, so neither [`Status::forget_run`] nor
    /// [`Status::start_run`] clears it** — which puts it beside [`Status::policy`]
    /// and [`Status::budgets`] rather than beside [`Status::containment`], the
    /// field it otherwise most resembles. The containment word describes how *one
    /// run's* commands were held and dies with that run; a branch describes the
    /// directory the operator is standing in, and that directory does not change
    /// because a turn ended, because `/clear` started a new conversation, or
    /// because `/resume` put another one on screen. A line that blanked it there
    /// would be erasing a true fact about the operator's own checkout at the exact
    /// moment they opened a fresh conversation to work in it.
    ///
    /// Set two ways, and they answer different halves of one question.
    /// [`Status::note_branch`] takes it from a `git_branch` call the moment the
    /// agent makes one, so a branch io-cli's own agent created is on screen before
    /// the turn it created it in has finished. The driver re-reads
    /// [`crate::repo::branch`] at the turn boundary, which is what keeps a branch
    /// changed by anything *else* — a shell in another pane, a colleague's script
    /// — from standing here forever. The event arrives before the tool's result is
    /// known and a name that already exists is refused, so the second source is
    /// also the one that corrects the first.
    ///
    /// `None` draws **nothing at all** — not `none`, not an empty label. io-cli is
    /// run in plenty of directories that were never a checkout, and the rule this
    /// line holds everywhere is that an absent fact is absent rather than zero.
    pub branch: Option<String>,
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
    /// Where this turn's verification gate stands, in one word.
    ///
    /// **A word and never a mark**, for the reason [`WORDS`] is a list of words
    /// and [`SPINNER`] is explicitly not a state: the standing has to survive
    /// `--plain`, `NO_COLOR` and the ASCII glyph set, and a tick that means
    /// *passed* only in green is a fact a monochrome terminal does not carry at
    /// all. It is the choice 0.23.0 made for the resume marks, made again.
    ///
    /// **A `String` this file never interprets, and that is the decision rather
    /// than the shortcut it looks like.** The words are io-harness's own —
    /// `GateOutcome::as_str` spells the three outcomes a gate can end on, and the
    /// driver supplies the one the harness has no outcome for, a criterion that
    /// is being evaluated right now. Holding the gate's own type here would make
    /// this line the second place that decides what a standing is called;
    /// matching on the word to pick a tone would make it the third, and would
    /// leave a literal in this file that a rename on the other side walks
    /// silently past. So the word arrives composed and is drawn as it arrives —
    /// the same rule the containment word is held to, and the reason no backend
    /// name is written into this module either.
    ///
    /// `None` is a session with no criterion configured, which is nearly all of
    /// them, and it draws **nothing at all** — not `none`, not an empty label,
    /// not a zero. A turn nobody asked to verify has not failed verification.
    ///
    /// Cleared by [`Status::forget_run`] and by [`Status::start_run`]: a standing
    /// is an account of the turn that was gated and of no other turn.
    pub gate: Option<String>,
    /// Which attempt that standing was taken on, where more than one was made.
    ///
    /// **Absent below two, which is the absent-rather-than-zero rule sharpened by
    /// one.** Every gate that ran at all ran once, so `attempt 1` would be on
    /// every gated line in the world and would tell an operator nothing — the
    /// argument [`Budgets::in_force`] already makes about io-cli's own step
    /// floor, which is a cap on every turn and therefore a ceiling nobody chose.
    /// What is worth a field is that the turn is being *retried*, and that starts
    /// at the second attempt.
    ///
    /// `None` where the driver has no number to report, which is every session
    /// with no gate and the first attempt of every session with one. The
    /// rendering is [`Status::gate_field`]'s, so the threshold is decided once.
    pub gate_attempt: Option<u32>,
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
    /// Tokens, never money — and the money is [`Status::cost`], one field down.
    ///
    /// This one is the tree's draw against the tree's ceiling, which io-harness
    /// expresses in tokens and enforces in tokens. A currency figure here would be
    /// a conversion of a ceiling nobody set in currency.
    ///
    /// **The sentence that stood here until 0.22.0 said something stronger and it
    /// had stopped being true.** It read: a figure with a currency in front of it
    /// would be one this interface invented, because the crate has no price
    /// telemetry. The crate has had price telemetry since io-harness 0.18.0 —
    /// `provider_calls` records the whole token split per call and `pricing`
    /// derives money from it — and io-cli read none of it for four releases while
    /// this comment explained why it could not.
    pub spend: Option<(u64, Option<u64>)>,
    /// What this run has cost, in micro-units, or `None` for no answer.
    ///
    /// **Absent rather than zero, and there are three different ways to have no
    /// answer** — no price table configured, a table that prices none of the
    /// models this run used, or a run that has made no provider call yet. All
    /// three draw nothing, because a `$0` on this line would say the run was free,
    /// and none of the three means that. `/cost` is where the three are told
    /// apart, because there is room there to say which.
    ///
    /// The run's own, not the session's, unlike [`Status::tokens`] beside it. What
    /// an operator watches while a turn runs is what the turn is costing; the
    /// session total is a keystroke away on `/cost`, which carries both.
    pub cost: Option<u64>,
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
    /// Prompts the operator finished while a turn had the session, still waiting
    /// to run.
    ///
    /// **A session fact and not a run fact, so [`Status::forget_run`] leaves it
    /// standing — and the reason is sharper here than it is for `planning` or
    /// `budgets`.** The queue itself is `App::prompts`; `forget_run` takes
    /// `&mut Status` and can reach nothing else. Clearing the count here would
    /// therefore not empty the queue, only stop saying how deep it is — and the
    /// prompts would still fire, a turn each, out of a session whose line had
    /// just finished claiming there were none. That is worse than the hole it
    /// would be closing. `/resume`, `/fork` and a rewind change which
    /// conversation is on screen and drop nothing the operator typed;
    /// `App::forget_queued_prompts` is the one thing that drops them, and it
    /// moves this number by moving the queue rather than by contradicting it.
    ///
    /// Zero renders as **nothing at all**, the rule `bg N`, `spend` and `tokens`
    /// are held to: a session nobody typed ahead of has not queued zero prompts.
    /// It is what keeps this field off the overwhelming majority of lines — a
    /// queue exists only between an `Enter` pressed mid-turn and the end of the
    /// turn after it.
    pub queued_prompts: usize,
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
            cost: None,
            context: None,
            containment: None,
            branch: None,
            spend: None,
            plan: None,
            planning: false,
            gate: None,
            gate_attempt: None,
            jobs: 0,
            mcp: (0, 0),
            lsp: 0,
            browser: None,
            working: false,
            notice: None,
            elapsed: Duration::ZERO,
            plain: false,
            budgets: Budgets::default(),
            queued_prompts: 0,
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
    /// The model, the posture, plain mode, the session's age, the branch the tree
    /// is on and the depth of the prompt queue are not run facts and are left
    /// alone — the session is the same session either way, and the queue in
    /// particular is not even reachable from here: see [`Status::queued_prompts`]
    /// for why blanking the count would leave the line contradicting prompts that
    /// are still going to run. [`Status::branch`] is the sharpest of them:
    /// changing which conversation is on screen does not check out another
    /// branch, so clearing it here would blank a fact that is still true.
    pub fn forget_run(&mut self) {
        self.tokens = None;
        // **The money goes with the tokens it was derived from.** `start_run`
        // clears this too, and clearing it there alone is not enough: `/clear`,
        // `/resume`, `/fork` and a rewind all reach here *without* starting a run,
        // so a session cleared at an idle prompt would have gone on drawing the
        // previous conversation's cost beside a blank token count — a figure with
        // a currency in front of it attributed to a session that has spent
        // nothing, which is the invented number this field exists not to print.
        self.cost = None;
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
        // **A standing belongs to the turn that was gated, and this is the field
        // where keeping it would be worst.** Every other run fact left behind is
        // a stale *number*; this one is a verdict on work that is no longer on
        // the screen, so a `/resume` onto a conversation nobody ever gated would
        // inherit `gate passed` and read as verified. The attempt count goes with
        // it rather than separately: a retry count with no standing beside it is
        // a number about nothing.
        self.gate = None;
        self.gate_attempt = None;
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
        // Back to "no answer" rather than to zero, and it is the same distinction
        // `run_tokens` above is held to: a turn that has not called a provider yet
        // has not cost zero, it has not cost anything anybody can report.
        self.cost = None;
        // **Here as well as in `forget_run`, because `forget_run` does not run
        // between two ordinary turns.** That method is reached by `/resume`,
        // `/fork`, `/clear` and a rewind; the common case — the operator types a
        // second prompt — reaches only this one, and a standing left behind by it
        // would have the line asserting turn one's verdict over the whole of turn
        // two, which is the per-turn field this codebase has already shipped
        // stale once.
        //
        // **A gate retry IS affected, and the honest note is worth more than the
        // reassuring one.** An earlier draft of this comment said the retries
        // happen inside io-harness's own turn under a single `start_run`. They do
        // not: io-cli drives the retry as a fresh turn through the driver's own
        // queue, so it arrives here and the standing is cleared for the duration
        // of the very turn it explains. What stays on screen is the scrollback
        // record, which is permanent and says what failed; the footer field
        // reappears when the retry is judged in its turn. Keeping the word across
        // the chain would mean not clearing it here, and that is the same door the
        // stale-per-turn defect above came through.
        self.gate = None;
        self.gate_attempt = None;
    }

    /// Take the branch from a `git_branch` call, if that is what this event is.
    ///
    /// **The whole event rather than its parts, and the filter is here rather than
    /// at the call site**, which is the shape [`Status::note_context_from`] already
    /// has: the driver hands over every event it routes and cannot pick the wrong
    /// one, and the name being matched lives next to the field it sets. The name is
    /// io-harness's own constant and is never spelled here as a literal, so a
    /// rename on that side is a compile error rather than a field that silently
    /// stops updating.
    ///
    /// `target` is what io-harness made the subject of the call, which for this
    /// tool is its `name` argument — the branch being created and moved onto. An
    /// empty one sets nothing: the field's absence means "no answer", and an empty
    /// string dressed as a branch name is the one thing it must never hold.
    ///
    /// **Optimistic, on purpose, and corrected at the turn boundary.** io-harness
    /// documents `ToolCall` as emitted before the result is known, and this tool
    /// refuses a name that already exists — so this is the branch the agent asked
    /// for and not yet the branch git granted. The driver's re-read of
    /// [`crate::repo::branch`] when the turn ends is what settles it, and until
    /// then being one refused call ahead is worth being right for the whole of the
    /// turn in the ordinary case.
    pub fn note_branch(&mut self, event: &io_harness::RunEvent) {
        let io_harness::EventKind::ToolCall { name, target } = &event.kind else {
            return;
        };
        // **`target == name` is a call that named nothing, not a branch called
        // `git_branch`.** io-harness picks a call's subject from the first
        // conventional argument it carries and falls back to the tool's own name
        // when it carries none, so an announcement with no `name` argument
        // arrives here reading `git_branch` — and recording that would put a
        // branch nobody has on the status line, the `/status` page and every
        // commit block, from a call that failed. Trimmed as well as compared,
        // because whitespace is not a branch either.
        if name != io_harness::tools::GIT_BRANCH_TOOL || target == name || target.trim().is_empty()
        {
            return;
        }
        self.branch = Some(target.clone());
    }

    /// The branch, drawn, or nothing at all.
    ///
    /// **One method, reached by both renderers**, which is the shape
    /// [`Status::budgets_left`], [`Status::queued_left`], [`Status::cost_field`]
    /// and [`Status::gate_field`] already have and for the reason written where
    /// they are: this file has shipped a field into one renderer twice — 0.8.0's
    /// spend and 0.12.0's planning phase — and both times it was green in a unit
    /// test and nowhere on screen, because the binary draws the footer on every
    /// terminal seven rows or taller.
    ///
    /// `git:main`, spelled the way [`Status::policy`] and [`Status::provider`] are
    /// spelled on [`Status::fields`]. A bare `main` would be a word with no owner
    /// sitting beside a containment word and a posture, and a reader who does not
    /// already know the branch cannot tell which of the three it is. Four cells
    /// buys that, and four cells is what the shortest honest prefix costs — the
    /// same reason a detached head is seven characters of object id in
    /// [`crate::repo`] rather than forty.
    pub fn branch_field(&self) -> Option<String> {
        self.branch.as_ref().map(|branch| format!("git:{branch}"))
    }

    /// Set [`Status::cost`] from what this run has actually called, priced by
    /// `table`.
    ///
    /// **Read from the store rather than accumulated off the event stream, and
    /// that is forced rather than chosen.** `EventKind::Step` carries a scalar
    /// token count, and a price needs the split — fresh prompt against cache read
    /// against cache write against completion — which lives only on the
    /// `provider_calls` row. Counting the scalar at an input rate would over-report
    /// every cached turn and every reasoning turn, which is to say most of them.
    ///
    /// The cost shape, stated: one indexed read of one run's calls, on the events
    /// that can change the answer and no others — the same place and the same
    /// cadence as [`Status::note_context_from`], which has read the store per step
    /// since 0.16.0.
    ///
    /// Silent on every failure. A store that cannot be read is a status line with
    /// no cost field, which is what an operator with no prices already sees; a
    /// notice about a failed read of a decorative field would be worse than the
    /// field's absence.
    pub fn note_cost_from(
        &mut self,
        store: &io_harness::Store,
        run_id: i64,
        table: &io_harness::pricing::PriceTable,
    ) {
        let Ok(calls) = store.provider_calls(run_id) else {
            return;
        };
        let total = crate::cost::Total::of(&calls, table);
        // **Nothing priced is not zero priced.** A run whose models are all
        // outside the table has a real cost that this program cannot state, and
        // stating it as `$0` would be the invented number the whole of `/cost` is
        // built to avoid.
        self.cost = (total.micros > 0).then_some(total.micros);
    }

    /// Set [`Status::context`] from an observation section of `est_tokens`.
    ///
    /// **The denominator is [`Budgets::window`] and never a budget built here, and
    /// that is the whole of F10.** Everything about how the window is arrived at —
    /// io-harness's default, a `[run.context]` table, the `share` it takes of
    /// `[run] max_tokens` — is already resolved by the time a `TaskContract`
    /// exists, and `Budgets::in_force` reads it off that contract. A second
    /// resolution here would be a second answer to a settled question, and the two
    /// would disagree the first time a layer moved. That is not a hypothetical:
    /// the arm this replaces resolved it a second time, got `24_000`, and said so
    /// under a comment claiming it had asked the harness.
    ///
    /// With no window — no contract has reached this `Status` yet, see
    /// [`Budgets::window`] — the field is left exactly as it was. Losing the `ctx`
    /// field is the loud failure; a share of an invented denominator is the quiet
    /// one, and quiet is what this whole method exists to end.
    ///
    /// Clamped to `100` because an estimate can exceed the ceiling it is measured
    /// against: `ContextBudget` bounds what the assembler *aims* for and the fold
    /// is what enforces it, so a section briefly over its window is ordinary.
    /// `ctx 137%` would read as a bug in this line rather than as the pressure it
    /// actually is.
    pub fn note_context(&mut self, est_tokens: u64) {
        let Some(window) = self.budgets.window.filter(|window| *window > 0) else {
            return;
        };
        let share = (est_tokens as f64 / window as f64 * 100.0).round();
        self.context = Some(share.clamp(0.0, 100.0) as u8);
    }

    /// The same share, taken from the durable trace once a step has landed.
    ///
    /// **NOT what `ctx N%` reports, and the reason is a live run.** This reads
    /// `ContextEvent::assembled` — the observation section alone, which is what
    /// `ContextBudget` bounds and what `Compacted::after_tokens` reports. It is a
    /// coherent quantity and it was the field's numerator until the binary was
    /// driven for real: `/context` totalled **4,363 tokens of 24,000** while the
    /// status line one keystroke away said **`ctx 0%`**, because the page measures
    /// the whole request and this measures the ledger inside it. Two surfaces
    /// disagreeing about a word, on one screen, is worse than either number is
    /// useful — and the percentage is what makes an operator open the page, so the
    /// page is the one that must be right.
    ///
    /// So the field now reports what the page totals, from the same snapshot, and
    /// this is kept for the section number the page still shows. It answers "how
    /// full is the part a fold would shrink"; `ctx N%` answers "how full is the
    /// window", which is the question an overflow is the answer to.
    ///
    /// **Anchored on `Step`, for the reason `main.rs`'s `commit_edits` is.**
    /// io-harness documents `Step` as emitted once the step has been committed to
    /// the store, so the row is there to be read; a read at `ToolCall` is a read
    /// of a row that may not exist yet, and the two events are one line apart in a
    /// transcript, which is what would keep that invisible until it was a bug.
    /// One read per step, not one per event.
    ///
    /// ponytail: `Store::context_events` returns the whole run each call, so a run
    /// of `n` steps reads `n(n+1)/2` rows across its life — five-column rows
    /// behind an indexed `run_id`, and a turn is tens of steps rather than
    /// thousands. The upgrade, if that ever stops being true, is a `LIMIT 1`
    /// accessor in the harness and not a copy of the number kept here, which could
    /// disagree with the trace it was taken from.
    ///
    /// A read that fails leaves the field as it was and says nothing. This is a
    /// decoration on a status line: a run whose work succeeded is not one to
    /// interrupt because a trace could not be re-read, and unlike a diff there is
    /// nothing here worth spending a scrollback row to apologise for.
    /// The share `ctx N%` reports: the whole request, over the window.
    ///
    /// **The same numbers `/context` puts on its page, from the same snapshot, so
    /// the two cannot disagree.** They did, and a live run is what found it: the
    /// page totalled 4,363 tokens of 24,000 while the line said `ctx 0%`. The
    /// percentage is what makes an operator open the page; a percentage that
    /// contradicts the page is worse than no percentage at all.
    ///
    /// Taken from the request rather than the trace because that is what the
    /// window bounds — an overflow is refused on the whole request, not on the
    /// observations inside it — and because a request is what a page can show.
    /// Where no request has been seen yet, the caller falls back to
    /// [`Status::note_context_from`], which reads the section the trace records.
    pub fn note_context_request(
        &mut self,
        seen: &crate::context::Request,
        contract: &io_harness::TaskContract,
        remaining: Option<u64>,
    ) {
        let window = crate::context::window(contract, remaining);
        if window == 0 {
            return;
        }
        let total = crate::context::total(&crate::context::sections(seen, contract));
        let share = (total as f64 / window as f64 * 100.0).round();
        self.context = Some(share.clamp(0.0, 100.0) as u8);
    }

    pub fn note_context_from(&mut self, store: &io_harness::Store, event: &io_harness::RunEvent) {
        let io_harness::EventKind::Step { .. } = &event.kind else {
            return;
        };
        let Ok(events) = store.context_events(event.run_id) else {
            return;
        };
        if let Some(est) = events
            .iter()
            .rev()
            .find(|recorded| recorded.kind == "assembled")
            .and_then(|recorded| recorded.est_tokens)
        {
            self.note_context(est);
        }
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

    /// What is waiting behind this turn, or nothing at all when nothing is.
    ///
    /// **One method feeding both renderers, for exactly the reason
    /// [`Status::budgets_left`] is one, and N3 is that lesson a third time.**
    /// [`Status::render`] takes [`Status::footer`] on any terminal seven rows or
    /// taller — which is every real terminal — so [`Status::line`] is the
    /// short-terminal fallback and the footer is what an operator is actually
    /// looking at. A depth composed separately in each would be two spellings a
    /// test could satisfy one of; a depth composed in `line` alone would be
    /// 0.12.0's planning field again, green in a unit test and nowhere on screen
    /// in a live capture.
    ///
    /// `Option` rather than a `String` that is sometimes empty, so the absence at
    /// zero is decided here and cannot be forgotten by a caller: both renderers
    /// push whatever this returns, and neither one asks about the count itself.
    pub fn queued_left(&self) -> Option<String> {
        (self.queued_prompts > 0).then(|| format!("queued {}", self.queued_prompts))
    }

    /// This run's cost, drawn, or nothing at all.
    ///
    /// **One method, reached by both renderers**, which is the shape
    /// [`Status::budgets_left`] and [`Status::queued_left`] above already have and
    /// for the reason written where they are: this file has shipped a field into
    /// one renderer twice — 0.8.0's spend, 0.12.0's planning — and both times the
    /// field was invisible on the row the binary actually draws.
    ///
    /// `None` covers all three ways of having no answer and does not distinguish
    /// them, because this line has no room to. `/cost` is the surface that tells
    /// an empty price table from an unpriced model from a run that has called
    /// nothing, and it is one keystroke away.
    pub fn cost_field(&self) -> Option<String> {
        self.cost.map(crate::cost::money)
    }

    /// Where the gate stands, drawn, or nothing at all.
    ///
    /// **One method, reached by both renderers**, which is the shape
    /// [`Status::budgets_left`], [`Status::queued_left`] and
    /// [`Status::cost_field`] already have and for the reason written where they
    /// are: this file has shipped a field into one renderer twice — 0.8.0's spend
    /// and 0.12.0's planning phase — and both times it was green in a unit test
    /// and nowhere on screen, because the binary draws the footer on every
    /// terminal seven rows or taller.
    ///
    /// `Option` rather than a `String` that is sometimes empty, so the absence is
    /// decided here once and cannot be forgotten by a caller: both renderers push
    /// whatever this returns and neither one asks about [`Status::gate`] itself.
    ///
    /// **The attempt is part of the same field rather than a second one**, because
    /// the two are one fact — `failed` and `failed for the third time` are
    /// different situations for an operator, and an `attempt 3` that could be
    /// dropped away from the word it qualifies would be a number about nothing.
    /// It costs ten cells and appears only from the second attempt, which is to
    /// say on the lines where a turn is visibly not converging.
    ///
    /// **No tone is chosen here and none is chosen by the callers.** Both draw the
    /// word in `Tone::Normal`, the tone the planning phase and the background-job
    /// count wear, because deciding between them would mean this module comparing
    /// the standing against literal outcome words that belong to io-harness — see
    /// [`Status::gate`]. Nothing is lost by it: the rule this line has held since
    /// its first release is that the word is the state and a colour only ever
    /// agrees with it, so a standing spelled out in full needs no second channel.
    pub fn gate_field(&self) -> Option<String> {
        let standing = self.gate.as_ref()?;
        Some(match self.gate_attempt {
            Some(attempt) if attempt > 1 => format!("gate {standing} attempt {attempt}"),
            _ => format!("gate {standing}"),
        })
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
        // **Left of the background-job count, and that is a decision about what
        // survives a narrow terminal rather than a tidy grouping.** `bg N` is
        // work the agent started and the transcript a row above already names it;
        // this is a keystroke io-cli took and has not run yet, and the only other
        // thing that ever said so was a footer notice the next keystroke erases.
        // Dropped, a prompt the operator watched vanish out of the composer is
        // evidenced nowhere on screen at all — which is the lost keystroke this
        // release exists to end, arrived at from the other side.
        if let Some(text) = self.queued_left() {
            fields.push(Field::new(text, Tone::Normal));
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
        // **Left of the containment word, which is where the design put it and
        // where a live run proved it has to be.** Drafted to the right of it, the
        // field never appeared: a real containment word is `workspace-write/
        // macos-sandbox-exec`, thirty-three characters, and at a hundred columns
        // beside the model, the posture, the state, the clock and the token count
        // there was nothing left — so the one field this release exists to fill
        // was the first one dropped, on the first terminal it was run in.
        //
        // **Left of the numbers, because it is not one** — and as of 0.22.0 the
        // code is finally where this sentence has always said it was. A standing
        // mode that stops the agent writing outranks what the last turn spent: if
        // a narrow terminal can hold one of them, it should hold the one that
        // explains why nothing is happening. It was pushed *after* `steps`, `tok`
        // and `ctx` for four releases, and since `fits` drops from the right that
        // meant `line` gave up the planning phase before it gave up the token
        // count — the exact inversion of the rule written directly above it. The
        // footer had the same inversion by a different mechanism, dropping its
        // whole right-hand group to keep every counter, and both are corrected in
        // the release that adds another counter to that row.
        //
        // `Normal` rather than `Muted` for the same reason the background-job
        // count is — it is a fact about what the agent may do, not a footnote
        // about what it did.
        if self.planning {
            fields.push(Field::new("planning".to_string(), Tone::Normal));
        }
        // **Immediately right of the planning phase and left of every counter,
        // which is a decision about a narrow terminal and not a grouping.** The
        // rule this row already states is that a standing mode which stops the
        // agent writing outranks what the last turn spent; a gate that has not
        // passed is the same class of fact from the other end — it is why the
        // turn is not finished, and on a retry it is why the agent is doing the
        // work a second time. If a narrow terminal can hold one of `gate failed
        // attempt 2` and `14.2k tok`, it should hold the one that explains what
        // is happening rather than the one that measures it.
        //
        // Right of `planning` rather than left, because `planning` is a standing
        // choice that holds until `/plan off` while this is an account of one
        // turn: where both are on the line and only one can survive, the one that
        // is still true after the turn ends is the one that stays.
        //
        // `Normal` rather than `Muted`, for the reason `planning` and `bg N` are:
        // it is a fact about what the turn is still doing, not a footnote about
        // what it did.
        fields.extend(self.gate_field().map(|text| Field::new(text, Tone::Normal)));
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
        // **Right of the token count it is derived from**, so the two read in the
        // order they are computed in: the tokens are what happened and the money
        // is what they came to. `Muted` like the counters beside it, because it is
        // one — a fact about what the last turn spent, which is exactly the class
        // this row's own priority rule says yields to a standing mode.
        //
        // From the same method as the footer's, four lines of which are the whole
        // lesson of 0.8.0's spend field and 0.12.0's planning field: a field
        // rendered in one of the two renderers is a field the operator never sees,
        // because the binary draws the footer on every terminal seven rows or
        // taller.
        fields.extend(self.cost_field().map(|text| Field::new(text, Tone::Muted)));
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
        // **Immediately right of the containment word, which is its nearest kin on
        // this line, and left of the plan claim.** Both are standing facts about
        // the circumstances the agent is working in rather than counters of what a
        // turn spent — one says how its commands are held, the other says which
        // checkout its writes land in — so they read together and narrow together.
        //
        // Left of `plan` rather than right of it, and that is the drop order rather
        // than a grouping: `fits` gives up fields from the right, and what the
        // agent *claims* about its own list is the field this line already names as
        // the first to go. Which branch the operator is standing on outlives the
        // claim, the turn and the conversation, so it survives one column further.
        // The cost of putting it here is that the plan claim and the `unknown`
        // diagnostic beyond it are pushed right by the field's own width plus a
        // separator, which is what they give up on a crowded line.
        //
        // `Muted` like the containment word beside it: it is a fact about where the
        // work is happening, not a bound on what may happen, which is the tone
        // `policy` and `planning` wear.
        fields.extend(
            self.branch_field()
                .map(|text| Field::new(text, Tone::Muted)),
        );
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
        // **First in the group, and that is the same ordering decision
        // `Status::fields` makes, arrived at through this row's own mechanism.**
        // The narrowing below pops counters off the right until the right-hand
        // group fits, so position in this vector *is* priority — and the standing
        // that says why the turn is not finished outranks every number beside it,
        // exactly as the planning phase outranks them in the group on the right.
        // Pushed after `steps` it would have been among the first things a
        // crowded row gave up, which is the inversion the release before this one
        // spent a live capture finding.
        //
        // Here as well as on `Status::line` and out of the same method: this is
        // the row the binary draws at an ordinary prompt, so a gate added to
        // `line` alone would be a gate no operator ever saw.
        counts.extend(self.gate_field());
        if let Some(steps) = self.steps {
            counts.push(format!("{steps} step{}", if steps == 1 { "" } else { "s" }));
        }
        if let Some(tokens) = self.tokens {
            counts.push(format!("{} tok", format_tokens(tokens)));
        }
        if let Some(context) = self.context {
            counts.push(format!("ctx {context}%"));
        }
        // Here as well as on `Status::line`, out of the same method, for the
        // reason the budgets below are — this is the row the binary draws at an
        // ordinary prompt, so a cost added to `line` alone would be a cost no
        // operator ever saw. 0.12.0's planning field, again, and 0.8.0's spend
        // field before it.
        counts.extend(self.cost_field());
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
        // **Here as well as on `Status::line`, out of the same method, for the
        // reason the budgets four lines up are here.** This is the row the binary
        // draws at an ordinary prompt, so a depth added to `line` alone would be
        // a depth no operator ever saw — 0.12.0's planning field, again. Left of
        // `bg` here too, so the two forms read the same order as well as the same
        // words. `extend` over the `Option` because absent contributes nothing,
        // which is the whole of the zero case.
        counts.extend(self.queued_left());
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
        //
        // **Held out of `counts` until after the narrowing below, because it is
        // not a counter.** It used to be pushed here, which put it last — and the
        // narrowing pops from the end, so the first thing a crowded row gave up
        // was `esc stops`: the only place the footer tells an operator how to
        // interrupt a turn, dropped exactly when the row is full because a turn is
        // running. It is appended after the loop instead, and its width is counted
        // during the loop so the arithmetic still describes the row that gets
        // drawn.
        let hint = if self.working {
            "esc stops"
        } else {
            "/ for commands"
        }
        .to_string();

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
        // **The right-hand group and not `counts`, out of the same method
        // `Status::fields` draws it from.** This group is where the row keeps the
        // facts that describe the circumstances the agent is working in — the
        // posture, the sandbox, the phase — and the branch is the last of them: it
        // says which checkout every write on this row landed in. It is also not a
        // counter, and `counts` is documented as everything countable and nothing
        // that is not.
        //
        // **What that costs, stated plainly.** The two groups yield asymmetrically:
        // when they cannot both fit, counters come off the right of `counts` until
        // the right group fits, and the right group is never dropped. So a branch
        // here is paid for by the rightmost counters — the plan claim first, then
        // the background and queue counts — on a row that is already full. That is
        // the same trade this module states for `planning` and takes deliberately:
        // a fact that is still true after the turn ends outranks a number that
        // measured it. The bill is the branch name plus four cells for `git:` and
        // three for the separator, which on an ordinary branch is under twenty
        // columns and on `main` is eleven.
        //
        // Appended after `planning` rather than inserted at the front, because the
        // group is kept or dropped whole — position inside it is reading order, not
        // priority — and pushing it last leaves the separator rule one uniform
        // check instead of moving it onto the posture arm.
        if let Some(text) = self.branch_field() {
            if !allowed.is_empty() {
                allowed.push(Span::styled(separator, muted));
            }
            allowed.push(Span::styled(text, muted));
        }
        // **When the two groups cannot both fit, the counters yield — not the
        // group.** `row` fits its right-hand group all or nothing, so a counts
        // row one character too wide took the posture, the containment word AND
        // the planning phase off the screen together and kept every counter.
        // That is the failure the comment directly above records, arriving from
        // the other direction: 0.13.1's row was `4 steps · 14.2k tok · / for
        // commands` and the group fit beside it at a hundred columns; `ctx N%`
        // made the row ten characters wider, and in the 0.21.0 live capture the
        // finished turn's row carries no right group at all.
        //
        // Which side gives is not a judgement call — this module states it
        // where `planning` is ordered on `Status::fields`: a standing mode that
        // stops the agent writing outranks what the last turn spent. So
        // counters come off the right of `counts`, rightmost first, which is
        // the order `fits` drops fields in and the order they are pushed in
        // — least load-bearing last — until the group fits. The
        // leftmost counter is never dropped: a row that had given up every
        // number would be reporting the mode by erasing the session.
        let wanted: usize = allowed.iter().map(|s| s.content.chars().count()).sum();
        // The hint is not in `counts` and is not droppable, so its width is added
        // here rather than being carried by the join.
        let hinted = hint.chars().count() + separator.chars().count();
        while !allowed.is_empty()
            && !counts.is_empty()
            && counts.join(separator).chars().count() + hinted + wanted >= room
        {
            counts.pop();
        }
        counts.push(hint);
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
///
/// **That last-resort rule is not the counts row's policy**, and a caller who
/// has a droppable left group is expected to narrow it before calling: the
/// counts row trims counters off its own right until the group fits, because
/// the group it would otherwise lose carries the planning phase.
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

    // Directly under the workspace, because it is the second half of the same
    // fact: the path says which directory, and this says which of its branches is
    // checked out. Read off `Status` rather than off disk, so opening this page
    // reads nothing and reports exactly what the footer is already showing — the
    // rule the budgets on this page are held to, arrived at from the other side.
    //
    // An absence gets a row of its own here, unlike on the status line where it
    // draws nothing at all. This surface is one fact per row with no width to
    // compete for, and every other unknown on it — the provider, the sandbox, the
    // containment — says why it is unknown rather than going missing.
    facts.push((
        "branch".into(),
        match &status.branch {
            Some(branch) => branch.clone(),
            None => format!("not known {dash} this workspace has no readable git head"),
        },
    ));

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

    // **The window comes off the contract handed in, exactly as the budgets above
    // do, and for the same reason**: this page reports what the *next* turn would
    // run under, and reading it changes nothing. The share beside it is still the
    // session's own — what has been assembled is a fact about the turns that ran,
    // however the ceiling was arrived at — which is the split
    // `budgets_left_of` is built on.
    //
    // The size is said in the empty case too. `not known until the context has
    // been folded once` was the old text, and it was a true description of a
    // defect: the number arrived only at a fold. Now the *window* is known from
    // the moment a contract exists, so a page that said nothing at all would be
    // withholding the one half it can always answer — and it is the half an
    // operator checking whether their `[run.context]` table took effect is
    // actually looking for.
    //
    // `Budgets::in_force` always fills the window: it has a contract in hand and
    // every contract declares one. The arm below is that type's `None` spelled out
    // rather than an `unwrap` — a committed page is not worth a panic — and it is
    // unreachable from here.
    let window = match Budgets::in_force(contract).window {
        Some(tokens) => format_tokens(tokens),
        None => "unknown".to_string(),
    };
    facts.push((
        "context".into(),
        match status.context {
            Some(fill) => format!("{fill}% of a {window} window"),
            None => format!("nothing assembled yet {dash} the window is {window}"),
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

    // **`Status::queued_prompts` is deliberately not a row here, and the absence
    // is a decision rather than a renderer somebody missed.** Every fact above
    // is either a standing configuration — the workspace, the layers, the caps,
    // the budgets, the rosters — or a session total that only ever climbs, so
    // each one is still a true account of this session at the moment it was
    // written. The queue is neither: it exists between an `Enter` pressed
    // mid-turn and the end of the turn after it, and this page is *committed*,
    // into a scrollback that keeps it for the rest of the session. A row reading
    // `queue: 2 waiting` is false a turn later and goes on saying it, which would
    // make it the one row on a page whose whole argument is that a status surface
    // a reader cannot trust is worse than none. The line and the footer redraw,
    // so they are where a depth that changes belongs — which is also why N3 names
    // those two renderers and not this one.
    //
    // **`Status::gate` is not a row here either, and for that same argument.** A
    // standing is the verdict on one turn: `gate failed attempt 2` committed into
    // a scrollback goes on saying so under the passing turn that follows it, and a
    // row that is false a turn later on a page whose whole claim is that it can be
    // trusted is worse than no row. What a reader wants from *this* page about a
    // gate is the criterion the next turn will be held to, which is configuration
    // and belongs beside the budgets — a row this release does not draw rather
    // than one it draws wrongly.

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
/// **The body moved to [`crate::page::folded`] in 0.22.0 and this is the name it
/// answered to here.** `crate::context` carried the same twenty lines under the
/// name `wrapped`, differing only in hard-coding the two indents this signature
/// takes as arguments, and `/cost` and `/stats` would have been the third and
/// fourth copies. The argument for folding rather than fitting is written out
/// where the code now lives.
use crate::page::folded;
