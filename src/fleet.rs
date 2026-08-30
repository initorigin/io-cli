//! The fleet: what a decomposed task looks like while it is running.
//!
//! **A model over the event stream, and over what somebody else has already read
//! for it.** io-harness reports a tree as five kinds of fact, each on the run it
//! belongs to, and this assembles them back into a shape a person can read. It
//! owns no state io-harness does not report and decides nothing about the run.
//!
//! **It still reads no store; it now draws what one said.** Two of the things on
//! screen are simply not on the event stream. `EventKind::Spawned` carries a run
//! id and a goal and no name at all, so a child's *address* — the instance name it
//! is reachable at, `as` in `spawn_agent` or a derived `reviewer#42` — can only
//! come from `Store::tree_addresses`; and a message one agent sent a named sibling
//! is never emitted as an event, so it can only come from `Store::messages_for`.
//! Both arrive here as **arguments** ([`Fleet::name`], [`Fleet::traffic`]). This
//! module opens no connection, prepares no statement and holds no cursor: whoever
//! calls it owns the read, its schedule and its failure. That is what keeps every
//! row here testable without a database, and what keeps a disk read on main.rs's
//! clock rather than on the frame's.
//!
//! **`messages_for`, never `read_messages`.** The second stamps `read_at` and *is*
//! the delivery — a view that called it would consume a sibling's mail in order to
//! draw it, and the agent the message was addressed to would never see it.
//! Showing traffic must not be the same act as receiving it.
//!
//! Three properties of that stream shape everything here.
//!
//! **A spawn is the parent's event.** `EventKind::Spawned` is emitted with the
//! *parent's* `run_id` and the parent's `depth`, carrying the child's own run id —
//! so the edge is derivable, and a child sits one level below the event that
//! announced it. Every later event of that child arrives under the child's own
//! `run_id` at its own `depth`, which is how a draw, a step and an ending find
//! their row.
//!
//! **A queued child is a count and never a row.** io-harness emits `Fleet` for a
//! child that has to wait *before* it is admitted, and `Spawned` only after a slot
//! frees and a run id exists. So a waiting child has no id, no goal and nothing to
//! draw a row from — inventing a placeholder for one would put a row on screen for
//! an agent that does not exist yet, which is the fabrication F4's sabotage arm
//! adds on purpose. **It has no address either**, and cannot: `queued_agents`
//! returns `(tier, goal)`, and an address is derived from a run id that has not
//! been allocated. That limit is real and stays — a count is the whole truth about
//! a child nothing has started.
//!
//! **Tier counts are replaced, never accumulated.** Each `Fleet` carries that
//! tier's whole shape — working, queued and done as they now stand — including
//! the backlog a resumed tree reads back out of the store. Adding them up would
//! make a restart look like a doubling.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use io_harness::{AgentDef, AgentMessage, Agents, EventKind, RunEvent};

use crate::glyphs::Glyphs;
use crate::picker::fit;
use crate::theme::{Theme, Tone};

/// Where one child of the tree has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Admitted and running.
    Working,
    /// Its parent stopped waiting for it. **Still running** — a parent that stops
    /// waiting is not a parent that stops the work.
    Detached,
    /// Its run ended.
    Done,
}

impl State {
    fn word(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Detached => "detached",
            Self::Done => "done",
        }
    }
}

/// The character io-harness derives an unnamed child's address with:
/// `<role>#<run id>`.
///
/// Spelled here because the harness keeps its own `DERIVED_MARK` private, and
/// there is exactly one thing this is used for: splitting a derived address back
/// into the role it was derived from. **A copy of a constant is a thing that can
/// disagree**, so it is used only where being wrong is harmless — a mark that
/// changed would cost a role label, never a wrong address: the address is always
/// drawn whole, exactly as the store spelled it.
const DERIVED_MARK: char = '#';

/// One admitted child.
#[derive(Debug, Clone)]
pub struct Child {
    /// Its own run id, which every event of its own carries.
    ///
    /// Kept even though a named child no longer draws by it: it is what every
    /// event is keyed on, and what [`Fleet::name`] matches an address against.
    pub run_id: i64,
    /// Where it is reachable — the instance name it was spawned under, or a
    /// derived `reviewer#42` — or `None` until somebody has read one for it.
    ///
    /// **Not the role.** Several children can be the same kind of agent, and an
    /// operator attaching to one, or an agent addressing mail to one, means a
    /// particular one. This is the only field on a child that identifies *which*.
    ///
    /// `None` is honest and common: it is a child announced by an event whose
    /// address has not been read back yet, and it stays `None` for as long as
    /// that is true — see [`Fleet::name`].
    pub address: Option<String>,
    /// The `[[agent]]` definition it was spawned from, where that is knowable.
    ///
    /// Answers "what kind of agent is this", which the address deliberately does
    /// not. `None` where the roster has nothing to say — which is not a defect,
    /// see [`Fleet::name`] for the two shapes that resolve and the one that
    /// cannot.
    pub role: Option<String>,
    /// Whether the definition it was spawned from asked for its own worktree.
    ///
    /// **A property of the roster entry, not a directory.** io-harness writes a
    /// contained child's actual worktree path into its `runs` row and no query
    /// ever selects it back out, and the functions that derive one are private to
    /// the harness — so a path drawn here could only be *reconstructed*, and a
    /// reconstruction is an address that disagrees with the truth the moment
    /// either side changes. That is the same rule `DERIVED_MARK` states for the
    /// role — named without a doc link because it is private to this module and a
    /// public item may not link one: copy a constant only where being wrong is
    /// harmless. Being wrong
    /// about a directory is not harmless — an operator would `cd` into it.
    ///
    /// So this is the one bit that *is* knowable: `contract.agents` says whether
    /// the definition carries `worktree = true`, and that is what the row says.
    /// `false` where the roster cannot name this child at all, which is the same
    /// honest silence [`Child::role`] keeps.
    pub worktree: bool,
    /// Its nesting level: one below the event that announced it.
    pub depth: u32,
    /// What it was asked to do.
    pub goal: String,
    pub state: State,
    /// What it has drawn from the ceiling the whole tree shares.
    ///
    /// Per child, because `SpendDraw` is emitted on the drawing run — so the sum
    /// of these plus the root's own draws is what the tree has spent, and the
    /// status line's figure is the same arithmetic done once at the top.
    pub drawn: u64,
}

impl Child {
    /// What identifies this row to a person, role included where it is known.
    ///
    /// **The address, and `run <id>` only where there is none.** A run id is a
    /// database key: it is unique, it is stable, and it is the one thing about an
    /// agent an operator cannot use — they cannot address mail to it, and it does
    /// not tell them which of three reviewers this is. The fallback is not a
    /// lesser spelling of the same thing, it is an admission that nothing has told
    /// us the name yet, and it stays visible rather than being hidden behind a
    /// placeholder for the same reason a queued child gets no row at all.
    ///
    /// The role is drawn beside the address only when it is not the address, so a
    /// child a parent named after its own role reads `scout` and not
    /// `scout (scout)`.
    fn label(&self) -> String {
        let Some(address) = &self.address else {
            return format!("run {}", self.run_id);
        };
        match &self.role {
            Some(role) if role != address => format!("{address} ({role})"),
            _ => address.clone(),
        }
    }
}

/// One message an agent sent a named sibling, as this view holds it.
///
/// An owned four-field row rather than an `io_harness::AgentMessage`, for two
/// reasons. `AgentMessage` is `#[non_exhaustive]`, so a test in this crate could
/// not build one to assert a row against — a view type nothing outside a database
/// can construct is a view type nothing can test. And it is keyed by run id,
/// while everything on this screen is now keyed by address: the recipient's
/// address is not on the message at all (only `to_run_id` is), so somebody has to
/// resolve it, and doing that once at the boundary beats doing it per frame.
///
/// `id`, `sent_at` and `read_at` are dropped deliberately. The order rows arrive
/// in is the order they are drawn in, a wall-clock stamp would spend columns the
/// body needs, and `read_at` is a fact about *delivery* that this view must never
/// look like it participates in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// The sender's address, as it signed itself.
    pub from: String,
    /// The recipient's address, resolved by the caller from `to_run_id`.
    pub to: String,
    /// The sender's step when it sent this.
    pub step: u32,
    /// What was said.
    pub body: String,
}

impl Message {
    /// One store row, addressed to the agent at `to`.
    ///
    /// Takes the recipient's address as an argument because the row does not
    /// carry one: `AgentMessage::to_run_id` is a run id, and only
    /// `Store::tree_addresses` can turn it into a name. The caller is holding
    /// both by the time it calls this.
    pub fn received(message: &AgentMessage, to: &str) -> Self {
        Self {
            // `from_name` is the sender's ADDRESS — the instance, not the roster
            // definition it was spawned from. io-harness documents it that way and
            // the whole mailbox depends on it: mail is addressed to instances.
            from: message.from_name.clone(),
            to: to.to_string(),
            step: message.step,
            body: message.body.clone(),
        }
    }
}

/// One nesting level of the tree, as io-harness last reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub tier: u32,
    pub working: u32,
    pub queued: u32,
    pub done: u32,
}

/// The tree as the events have described it so far.
#[derive(Debug, Clone, Default)]
pub struct Fleet {
    children: Vec<Child>,
    tiers: Vec<Tier>,
    /// The traffic between them, in the order the caller read it.
    ///
    /// Held flat rather than hung off each child: a message is a fact about a
    /// *pair* of agents, the sender is frequently not a child of this tree's root
    /// at all, and the recipient may be the root itself — which has no row here,
    /// because the root is not a child.
    messages: Vec<Message>,
    /// Which row the operator has marked, as an index into [`Fleet::children`].
    ///
    /// Held independently of the rows so that "nothing is selected" is
    /// expressible — the lesson 0.7.0 paid for when a remembered row read out of
    /// the current match set became a fabricated `0`.
    selected: Option<usize>,
}

impl Fleet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether anything has happened that this can draw.
    ///
    /// Traffic counts. A view holding mail is a view with rows in it, and one that
    /// called itself empty while `rows` returned something would be hidden with
    /// its contents still there.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.tiers.is_empty() && self.messages.is_empty()
    }

    pub fn children(&self) -> &[Child] {
        &self.children
    }

    pub fn tiers(&self) -> &[Tier] {
        &self.tiers
    }

    pub fn selection(&self) -> Option<usize> {
        self.selected
    }

    /// The marked child itself, or `None` when there is nothing to mark.
    ///
    /// **This is the handle, and [`Fleet::selection`] is not.** An index into a
    /// private vector is worth exactly what the vector's current contents make it
    /// worth: it drives the highlight and nothing else, and a caller that kept one
    /// across an event would be holding a number whose meaning had moved. What a
    /// caller outside this module actually wants is the child — its `run_id` to
    /// attach to, and its `state` to know whether attaching is a sensible thing to
    /// ask for. `State::Detached` is the case that matters: a detached child is
    /// **still running**, its parent has merely stopped waiting for it, so it is
    /// the one an operator can go and look at. That decision stays with the
    /// caller; this hands over the two facts it needs to make it and refuses to
    /// make it here, because a model that decided what could be attached to would
    /// be deciding something about the run.
    pub fn selected_child(&self) -> Option<&Child> {
        self.selected.and_then(|at| self.children.get(at))
    }

    /// Every message this view is holding, oldest first.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Give the children their addresses, from `(address, run id)` pairs the
    /// caller read out of `Store::tree_addresses`, and their roles from `roster`.
    ///
    /// **It takes the data instead of reading it.** A `Store` opened in here would
    /// put a disk read on the render path, make every test of a row a test that
    /// needs a database, and give this module a second opinion about the run
    /// alongside the event stream. The caller already holds the store, already
    /// knows the root run id, and already has a schedule that is not "once a
    /// frame".
    ///
    /// **Idempotent, and safe to call with a partial answer.** A child no pair
    /// names is left exactly as it was rather than being cleared — a read that
    /// raced a spawn returns the addresses that existed when it ran, and a view
    /// that blanked the rest would flicker a name off screen and back. Naming is
    /// monotonic here: a child gets an address once and keeps it, which is also
    /// what io-harness guarantees about the address itself across a restart.
    ///
    /// The pair for the root (`ROOT_ADDRESS`) matches no child and is ignored,
    /// which is correct for the same reason the root's `Finished` matches no row:
    /// the root is not a child.
    ///
    /// **Bounded per call by the size of the *tree*, not the length of the run.**
    /// It is one pass over the pairs, each doing one scan of the children, and
    /// both of those are the number of agents that have been spawned — a number
    /// that does not grow with steps, events, tokens or messages. That is the
    /// difference from the trap `src/status.rs` records for `context_events`,
    /// where a per-event read of the whole history makes an `n`-step run cost
    /// `n(n+1)/2`. Nothing here re-walks history.
    pub fn name(&mut self, addresses: &[(String, i64)], roster: &Agents) {
        for (address, run_id) in addresses {
            let def = def_of(address, roster);
            let role = def.map(|def| def.name.clone());
            // Read off the same definition the role came from rather than looked
            // up again: two lookups of one address are two chances to disagree.
            let worktree = def.is_some_and(|def| def.worktree);
            if let Some(child) = self.child_mut(*run_id) {
                child.address = Some(address.clone());
                // Assigned rather than merged: `def_of` is a pure function of the
                // address and the roster, so a `None` here means the roster cannot
                // name this one, and keeping a stale role after an `/agents`
                // reload would be showing a definition that no longer exists.
                child.role = role;
                // The same rule, and it matters more: a stale `true` would keep
                // saying a child is contained after the definition that contained
                // it was edited away.
                child.worktree = worktree;
            }
        }
    }

    /// Replace the traffic with what the caller last read.
    ///
    /// **Replaced, never appended** — the same rule the tier counts follow, and
    /// for a sharper version of the same reason. `Store::messages_for` returns a
    /// recipient's whole mailbox every call, so a view that extended would show
    /// every message twice after the second poll and three times after the third.
    /// Replacement also means this holds no cursor of its own that could disagree
    /// with the store about what has been read.
    ///
    /// Takes the rows by value: the caller has just built them and has no use for
    /// them afterwards, so moving is the whole cost. Bounded per call by what was
    /// handed over, and this does not walk the run to work out what changed.
    ///
    /// **The caller must have used `messages_for` and not `read_messages`.** The
    /// latter stamps `read_at`, which is io-harness's delivery record: calling it
    /// to populate a view would hand a sibling's mail to the screen instead of to
    /// the agent it was addressed to, and the agent would wait forever for a
    /// message that was already marked delivered.
    pub fn traffic(&mut self, messages: Vec<Message>) {
        self.messages = messages;
    }

    /// Forget the run this describes.
    ///
    /// Called where the conversation under it changes — a turn ending, `/resume`,
    /// `/fork`, a rewind. Every fact here belongs to one tree, and a view that
    /// went on showing them would be describing a run that is no longer on
    /// screen; the same rule `Status::forget_run` holds.
    pub fn forget(&mut self) {
        self.children.clear();
        self.tiers.clear();
        // Mail included, and it is the piece most likely to be missed: a message
        // outlives the turn it was sent in — it is a row in a table, not an event
        // — so a fleet that dropped its children and kept its traffic would draw a
        // conversation between agents that are no longer on screen.
        self.messages.clear();
        self.selected = None;
    }

    /// Fold one event in.
    pub fn event(&mut self, event: &RunEvent) {
        match &event.kind {
            // Keyed by the child's own run id rather than appended blindly: a
            // resumed tree announces children it already had, and a second row
            // for one agent is a fleet that looks twice its size.
            //
            // **A guard, and where a rejected arm falls through was checked
            // rather than assumed** — 0.6.0 shipped a rewind armed by exactly
            // this shape. A `Spawned` this fleet already holds matches no arm
            // below it and lands in the `_` no-op, which is the intent; the
            // arms between are keyed on other variants and cannot take it.
            EventKind::Spawned { child_run_id, goal } if self.child(*child_run_id).is_none() => {
                self.children.push(Child {
                    run_id: *child_run_id,
                    // No address and no role: the event carries neither, and a
                    // guess made from the goal or the tier would be a name an
                    // operator could type at nothing. It gets one when somebody
                    // reads one — see `Fleet::name`.
                    address: None,
                    role: None,
                    // Not knowable from the event either: `Spawned` carries a run
                    // id and a goal, and the roster entry behind them is what says
                    // whether this one is contained.
                    worktree: false,
                    depth: event.depth + 1,
                    goal: goal.clone(),
                    state: State::Working,
                    drawn: 0,
                });
                if self.selected.is_none() {
                    self.selected = Some(0);
                }
            }
            EventKind::Fleet {
                tier,
                working,
                queued,
                done,
            } => {
                let next = Tier {
                    tier: *tier,
                    working: *working,
                    queued: *queued,
                    done: *done,
                };
                match self.tiers.iter_mut().find(|held| held.tier == *tier) {
                    Some(held) => *held = next,
                    None => {
                        self.tiers.push(next);
                        self.tiers.sort_by_key(|tier| tier.tier);
                    }
                }
            }
            EventKind::ChildDetached { child_run_id, .. } => {
                if let Some(child) = self.child_mut(*child_run_id) {
                    // Not `Done`: the child goes on running, and the whole of
                    // `background_after_secs` is that difference.
                    child.state = State::Detached;
                }
            }
            // A child's own ending, on the child's own run. The root's `Finished`
            // matches no row, which is correct: the root is not a child.
            EventKind::Finished { .. } => {
                if let Some(child) = self.child_mut(event.run_id) {
                    child.state = State::Done;
                }
            }
            EventKind::SpendDraw { tokens, .. } => {
                if let Some(child) = self.child_mut(event.run_id) {
                    child.drawn += tokens;
                }
            }
            _ => {}
        }
    }

    fn child(&self, run_id: i64) -> Option<&Child> {
        self.children.iter().find(|child| child.run_id == run_id)
    }

    fn child_mut(&mut self, run_id: i64) -> Option<&mut Child> {
        self.children
            .iter_mut()
            .find(|child| child.run_id == run_id)
    }

    /// Move the marker, if there is anything to move it over.
    pub fn move_by(&mut self, delta: isize) {
        if self.children.is_empty() {
            self.selected = None;
            return;
        }
        let last = self.children.len() - 1;
        let at = self.selected.unwrap_or(0) as isize + delta;
        self.selected = Some(at.clamp(0, last as isize) as usize);
    }

    /// The tier line: what every level is doing, queued children included.
    ///
    /// This is the only place a waiting child appears, and it appears as a
    /// number. A fleet that is queueing and a fleet that is stuck look identical
    /// without it, and no per-child row can carry it.
    pub fn summary(&self) -> String {
        if self.tiers.is_empty() {
            return "nothing has been spawned yet".to_string();
        }
        self.tiers
            .iter()
            .map(|tier| {
                format!(
                    "tier {}: {} working, {} queued, {} done",
                    tier.tier, tier.working, tier.queued, tier.done
                )
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    /// One row per admitted child, then one per message, all fitted to `width`.
    ///
    /// **The children come first and stay index-aligned with
    /// [`Fleet::children`]**, because the selection this type holds is an index
    /// into that vector — see [`Fleet::selection`] — and [`Fleet::render`] marks a
    /// row by comparing it. Messages are
    /// appended after the last child rather than interleaved under their
    /// recipients: interleaving would put rows in front of children and silently
    /// shift every index the marker uses, and a marker that lands on the wrong row
    /// is an operator attaching to the wrong agent. It also reads better — the
    /// traffic is a conversation, and a conversation is worth reading in the order
    /// it happened rather than sorted into the tree.
    ///
    /// Bounded per call by what this holds — one pass, one row each, no history
    /// walked.
    /// Rows this view would like the viewport to be.
    ///
    /// The tier line, every child, every message, and the row it draws its own
    /// elision on. A request rather than a demand —
    /// [`crate::app::App::viewport_wanted`] clamps it to what the terminal can
    /// spare, and `render` elides against whatever it is given.
    ///
    /// Counted rather than measured, because these rows are fitted to the width
    /// rather than wrapped: `rows` cuts each one, so one row is one line.
    pub fn rows_wanted(&self) -> u16 {
        let content = self
            .children
            .len()
            .saturating_add(self.messages.len())
            // The tier line.
            .saturating_add(1);
        u16::try_from(content)
            .unwrap_or(u16::MAX)
            // The head row `render` reserves.
            .saturating_add(1)
    }

    pub fn rows(&self, width: u16, glyphs: &Glyphs) -> Vec<String> {
        let room = width as usize;
        let sep = glyphs.separator.trim();
        let mut rows: Vec<String> = self
            .children
            .iter()
            .map(|child| {
                let indent = "  ".repeat(child.depth.saturating_sub(1) as usize);
                let drawn = crate::status::format_tokens(child.drawn);
                // A word, not a symbol, and so it needs no entry in [`Glyphs`]:
                // `worktree` is the same eight letters in both sets, which is the
                // test that set applies — a mark goes in it when the two sets
                // would otherwise spell one thing differently, as `arrow` does.
                // It is also the word the roster spells, so an operator reading
                // the row and an operator reading `contract.agents` are reading
                // the same term.
                let contained = if child.worktree {
                    format!("worktree {sep} ")
                } else {
                    String::new()
                };
                let head = format!(
                    "{indent}{} {sep} {} {sep} {drawn} drawn {sep} {contained}",
                    child.label(),
                    child.state.word(),
                );
                // The goal is what gets cut, because everything in front of it
                // identifies the row and it is the only part that can be long.
                let room_for_goal = room.saturating_sub(head.chars().count());
                clamp(
                    format!("{head}{}", fit(&child.goal, room_for_goal, glyphs)),
                    room,
                    glyphs,
                )
            })
            .collect();
        rows.extend(self.messages.iter().map(|message| {
            // Indented one level under the tree, because that is what it is: a
            // thing that happened inside the fleet rather than another member of
            // it. The indent is the child rows' own unit, so the two read as one
            // block whatever the terminal's width is.
            let head = format!(
                "  {} {} {} {sep} step {} {sep} ",
                message.from,
                arrow(glyphs),
                message.to,
                message.step,
            );
            // The body gives way, exactly as the goal does above, and for the same
            // reason: the addresses either side of the arrow are what make the row
            // mean anything, and a body cut to nothing still says that one agent
            // wrote to another at a known step.
            //
            // Newlines are flattened rather than trimmed away: a body is free text
            // an agent wrote, and one `\n` in it would otherwise turn a single row
            // into several and push a child off an eight-row view.
            let flat = message.body.replace(['\n', '\r'], " ");
            let room_for_body = room.saturating_sub(head.chars().count());
            clamp(
                format!("{head}{}", fit(&flat, room_for_body, glyphs)),
                room,
                glyphs,
            )
        }));
        rows
    }

    /// Draw the view: the tier line, then as many child rows as there is room for.
    ///
    /// **It takes the composer's rows rather than the whole viewport**, so the
    /// status line — where the spend is — stays on screen underneath it, and the
    /// streaming tail above it goes on moving. The viewport is eight rows and
    /// cannot grow: a taller view would mean tearing down the `Screen` and
    /// rebuilding it, which is what the wizard does before a session starts and
    /// is not something to do while a run is committing into scrollback.
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let mut lines = vec![Line::from(Span::styled(
            fit(&self.summary(), area.width as usize, &theme.glyphs),
            theme.style(Tone::Accent),
        ))];

        let rows = self.rows(area.width, &theme.glyphs);
        // **A row spent saying what is not shown, and only when something is
        // not (0.32.0).** Messages sort after every child, so they are the first
        // thing off the bottom of this view — and until this release they went
        // with no count at all, which on the surface that exists to say what a
        // fan-out is doing is the one thing it must not do silently.
        let room = area.height.saturating_sub(1) as usize;
        let visible = if rows.len() > room {
            room.saturating_sub(1)
        } else {
            room
        };
        // The window follows the marker rather than the newest row: a list that
        // scrolled itself while an operator was reading one row would take the
        // row away from them.
        let at = self.selected.unwrap_or(0);
        let first = at.saturating_sub(visible.saturating_sub(1));
        let mut marked_row = None;
        for (offset, row) in rows.iter().skip(first).take(visible).enumerate() {
            let index = first + offset;
            // A message row can never match: `selected` is only ever set to an
            // index into `children`, and `rows` puts every child before every
            // message. That is the invariant the ordering in `rows` exists to
            // hold — checked here rather than assumed, in the same spirit as the
            // fall-through note on the `Spawned` arm.
            debug_assert!(
                self.selected.is_none_or(|at| at < self.children.len()),
                "the marker must index a child, not a message row",
            );
            let tone = if Some(index) == self.selected {
                marked_row = Some(area.y + 1 + offset as u16);
                Tone::Normal
            } else {
                Tone::Muted
            };
            lines.push(Line::from(Span::styled(row.clone(), theme.style(tone))));
        }
        let drawn = visible.min(rows.len().saturating_sub(first.min(rows.len())));
        if let Some(hidden) = rows.len().checked_sub(drawn).filter(|hidden| *hidden > 0) {
            lines.push(Line::from(Span::styled(
                format!("{} {hidden} more", theme.glyphs.elision),
                theme.style(Tone::Muted),
            )));
        }
        frame.render_widget(Paragraph::new(lines), area);
        // A cursor on every frame that accepts input, which this one does: the
        // marker moves under the arrows. Parked on the marked row rather than
        // hidden, so a screen reader and a terminal's own cursor agree about
        // where the reader is. 0.6.0's gate, and one that regresses quietly.
        if let Some(y) = marked_row {
            frame.set_cursor_position(ratatui::layout::Position { x: area.x, y });
        }
    }
}

/// The `[[agent]]` definition an address was spawned from, where the address can
/// still say.
///
/// The definition itself rather than its name, because two things on the row come
/// off it — the role label and whether the child is contained — and one lookup
/// cannot disagree with itself.
///
/// Two shapes resolve and one cannot, and the one that cannot is why this returns
/// an `Option` rather than a guess:
///
/// * `reviewer#42` — an address io-harness **derived**, because the parent named
///   nothing. The part in front of the mark is the definition's own name, so the
///   role is recoverable exactly.
/// * `scout` — an address the parent **assigned** that happens to be a roster
///   name. Confirmed against the roster rather than assumed, for the reason
///   `Agents::get` documents: a name nobody registered is a misspelling, not a
///   permissive default.
/// * `left-hand` — an address the parent assigned that is nothing the roster has
///   heard of. **The role is genuinely unknown here**: the definition a spawn used
///   is not on the event stream and not in `tree_addresses`, and the nearest thing
///   to a guess — matching on the goal, or on which definitions exist — would put
///   a wrong role beside a right address. `None` is the true answer and it costs a
///   label, not a row.
///
/// Pure and bounded: one split and one `BTreeMap` lookup, no allocation on the
/// path that fails.
fn def_of<'a>(address: &str, roster: &'a Agents) -> Option<&'a AgentDef> {
    let stem = address.split(DERIVED_MARK).next().unwrap_or(address);
    roster.get(stem)
}

/// The mark between a sender and a recipient, in whichever set is in play.
///
/// **An arrow that keeps its direction when it cannot be an arrow.** `->` is not a
/// decoration standing in the same column, it is the same statement in the
/// character set the terminal admits to — which is the whole test
/// [`crate::glyphs`] sets for a mark that degrades. It is one cell wider in ASCII,
/// and nothing measures it except the row that then gives way in the body, so the
/// widths need not agree.
///
/// Chosen by the set rather than added to [`Glyphs`] as an eleventh class,
/// because the set's own test for a class is that two call sites would otherwise
/// spell one mark differently, and there is one call site. It still answers to
/// `--plain` and to a locale that does not claim UTF-8, because it is the resolved
/// set it reads and never the environment.
fn arrow(glyphs: &Glyphs) -> &'static str {
    if glyphs.name == crate::glyphs::ASCII.name {
        "->"
    } else {
        "→"
    }
}

/// The last clamp on a finished row.
///
/// The parts that give way have already given way by the time this runs, so this
/// only ever bites when the head *alone* overruns — a long assigned address, a
/// deep indent, two addresses either side of an arrow. A row that overran would
/// wrap, and a wrapped row in an eight-row view costs a whole other agent its
/// place on screen; a row that is cut costs the end of a name. The row is taken by
/// value and handed straight back when it fits, so the common case — every row on
/// a terminal wide enough — copies nothing.
fn clamp(row: String, room: usize, glyphs: &Glyphs) -> String {
    if row.chars().count() <= room {
        return row;
    }
    fit(&row, room, glyphs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyphs::{ASCII, UNICODE};
    use io_harness::AgentDef;

    /// A spawn, as io-harness emits one: on the **parent's** run, at the parent's
    /// depth, carrying the child's id.
    fn spawned(parent: i64, depth: u32, child: i64, goal: &str) -> RunEvent {
        RunEvent::at_depth(
            parent,
            1,
            depth,
            EventKind::Spawned {
                child_run_id: child,
                goal: goal.to_string(),
            },
        )
    }

    fn tier(tier: u32, working: u32, queued: u32, done: u32) -> RunEvent {
        RunEvent::new(
            1,
            1,
            EventKind::Fleet {
                tier,
                working,
                queued,
                done,
            },
        )
    }

    fn message(from: &str, to: &str, step: u32, body: &str) -> Message {
        Message {
            from: from.to_string(),
            to: to.to_string(),
            step,
            body: body.to_string(),
        }
    }

    #[test]
    fn a_named_child_draws_by_address_and_role() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "read every file under src/"));
        fleet.name(
            // The root's pair is in here exactly as `tree_addresses` returns it,
            // and must match nothing: the root is not a child.
            &[("root".to_string(), 1), ("reviewer#7".to_string(), 7)],
            &Agents::new().with(AgentDef::new("reviewer")),
        );
        let rows = fleet.rows(80, &UNICODE);
        assert_eq!(rows.len(), 1, "the root got no row: {rows:?}");
        assert!(rows[0].contains("reviewer#7"), "{:?}", rows[0]);
        assert!(
            rows[0].contains("(reviewer)"),
            "the role is beside the address: {:?}",
            rows[0]
        );
        assert!(
            !rows[0].contains("run 7"),
            "the database key is gone once there is a name: {:?}",
            rows[0]
        );
    }

    /// The address is what a parent assigned, and it is emphatically not the role:
    /// two children of one definition are two different agents to address.
    #[test]
    fn siblings_of_one_role_draw_as_different_agents() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "left half"));
        fleet.event(&spawned(1, 0, 8, "right half"));
        fleet.name(
            &[("reviewer#7".to_string(), 7), ("reviewer#8".to_string(), 8)],
            &Agents::new().with(AgentDef::new("reviewer")),
        );
        let rows = fleet.rows(80, &UNICODE);
        assert!(rows[0].contains("reviewer#7"), "{rows:?}");
        assert!(rows[1].contains("reviewer#8"), "{rows:?}");
    }

    /// An assigned address that *is* a roster name reads once, not twice.
    #[test]
    fn a_child_named_after_its_role_does_not_say_it_twice() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "look around"));
        fleet.name(
            &[("scout".to_string(), 7)],
            &Agents::new().with(AgentDef::new("scout")),
        );
        assert!(!fleet.rows(80, &UNICODE)[0].contains("scout (scout)"));
    }

    #[test]
    fn an_unnamed_child_falls_back_to_its_run_id() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "read every file under src/"));
        assert!(
            fleet.rows(80, &UNICODE)[0].contains("run 7"),
            "an event stream alone knows no names",
        );
        // A read that answered about some other run leaves this one alone rather
        // than clearing it, and the fallback is still what shows.
        fleet.name(&[("scout".to_string(), 99)], &Agents::new());
        assert_eq!(fleet.children()[0].address, None);
        assert!(fleet.rows(80, &UNICODE)[0].contains("run 7"));
    }

    /// An address the roster has never heard of keeps its address and gets no role
    /// invented for it.
    #[test]
    fn an_unknown_address_gets_no_role() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "one"));
        fleet.name(
            &[("left-hand".to_string(), 7)],
            &Agents::new().with(AgentDef::new("reviewer")),
        );
        assert_eq!(fleet.children()[0].role, None);
        let row = &fleet.rows(80, &UNICODE)[0];
        assert!(row.contains("left-hand"), "{row:?}");
        assert!(!row.contains('('), "no role was guessed: {row:?}");
    }

    /// The 0.20.0 limit, still true: a queued child has no run id, so it has no
    /// address, so it stays a number in the tier line.
    #[test]
    fn a_queued_child_stays_a_count() {
        let mut fleet = Fleet::new();
        fleet.event(&tier(1, 1, 2, 0));
        fleet.name(&[("root".to_string(), 1)], &Agents::new());
        assert!(fleet.children().is_empty());
        assert!(
            fleet.rows(80, &UNICODE).is_empty(),
            "nothing was admitted, so nothing has a row",
        );
        assert!(fleet.summary().contains("2 queued"), "{}", fleet.summary());
    }

    #[test]
    fn a_message_draws_as_its_own_row() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "one"));
        fleet.traffic(vec![message("scout#7", "reviewer#8", 3, "the file moved")]);
        let rows = fleet.rows(80, &UNICODE);
        assert_eq!(rows.len(), 2, "the child, then the message: {rows:?}");
        assert!(
            rows[1].starts_with("  "),
            "indented under the tree: {rows:?}"
        );
        assert!(rows[1].contains("scout#7 → reviewer#8"), "{:?}", rows[1]);
        assert!(rows[1].contains("step 3"), "{:?}", rows[1]);
        assert!(rows[1].contains("the file moved"), "{:?}", rows[1]);
    }

    /// Every new mark has an ASCII form that says the same thing. The arrow is the
    /// only one this release adds — the role's parentheses and the `step` label are
    /// letters and punctuation that every terminal draws.
    #[test]
    fn the_ascii_arrow_keeps_its_direction() {
        let mut fleet = Fleet::new();
        fleet.traffic(vec![message("a", "b", 1, "hello")]);
        let unicode = fleet.rows(80, &UNICODE).remove(0);
        let ascii = fleet.rows(80, &ASCII).remove(0);
        assert!(unicode.contains("a → b"), "{unicode:?}");
        assert!(ascii.contains("a -> b"), "{ascii:?}");
        assert!(ascii.is_ascii(), "the ASCII row is ASCII: {ascii:?}");
        // And the row is still assembled out of the set it was handed, separator
        // included, rather than out of literals typed here.
        assert!(ascii.contains(ASCII.separator.trim()), "{ascii:?}");
    }

    /// A child row in the ASCII set is ASCII too, address and all.
    #[test]
    fn the_ascii_child_row_is_ascii() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "read every file under src/"));
        fleet.name(
            &[("reviewer#7".to_string(), 7)],
            &Agents::new().with(AgentDef::new("reviewer")),
        );
        let row = fleet.rows(80, &ASCII).remove(0);
        assert!(row.is_ascii(), "{row:?}");
        assert!(row.contains("reviewer#7 (reviewer)"), "{row:?}");
    }

    /// Eighty columns, a long address and a long body: the body gives way, and
    /// nothing wraps.
    #[test]
    fn a_long_message_gives_way_at_eighty_columns() {
        let mut fleet = Fleet::new();
        fleet.traffic(vec![message(
            "an-unusually-long-assigned-address",
            "another-unusually-long-assigned-address",
            12,
            &"and a body far longer than any of it ".repeat(10),
        )]);
        for glyphs in [UNICODE, ASCII] {
            let row = fleet.rows(80, &glyphs).remove(0);
            assert!(row.chars().count() <= 80, "{} {row:?}", glyphs.name);
            assert!(!row.contains('\n'));
        }
        // Even where the head alone overruns, the row is cut rather than wrapped.
        let row = fleet.rows(24, &UNICODE).remove(0);
        assert!(row.chars().count() <= 24, "{row:?}");
    }

    /// A body an agent wrote can contain newlines; a row cannot.
    #[test]
    fn a_multi_line_body_stays_one_row() {
        let mut fleet = Fleet::new();
        fleet.traffic(vec![message("a", "b", 1, "first line\nsecond line")]);
        let row = fleet.rows(80, &UNICODE).remove(0);
        assert!(!row.contains('\n'), "{row:?}");
        assert!(row.contains("first line second line"), "{row:?}");
    }

    /// `messages_for` hands back the whole mailbox every call, so this replaces.
    #[test]
    fn traffic_replaces_rather_than_accumulates() {
        let mut fleet = Fleet::new();
        let mail = vec![message("a", "b", 1, "one")];
        fleet.traffic(mail.clone());
        fleet.traffic(mail);
        assert_eq!(fleet.messages().len(), 1);
        assert_eq!(fleet.rows(80, &UNICODE).len(), 1);
    }

    #[test]
    fn forget_drops_the_mail_with_the_children() {
        let mut fleet = Fleet::new();
        fleet.event(&spawned(1, 0, 7, "one"));
        fleet.traffic(vec![message("a", "b", 1, "one")]);
        fleet.forget();
        assert!(fleet.is_empty());
        assert!(
            fleet.rows(80, &UNICODE).is_empty(),
            "mail outlives its turn in the store, but not on this screen",
        );
    }

    /// What main.rs attaches with: the marked child's id, and whether it is the
    /// kind of child there is any point attaching to.
    #[test]
    fn the_selected_child_carries_its_run_id_and_state() {
        let mut fleet = Fleet::new();
        assert!(fleet.selected_child().is_none(), "nothing to mark yet");
        fleet.event(&spawned(1, 0, 7, "one"));
        fleet.event(&spawned(1, 0, 8, "two"));
        fleet.move_by(1);
        let child = fleet.selected_child().expect("a marked child");
        assert_eq!(child.run_id, 8);
        assert_eq!(child.state, State::Working);

        fleet.event(&RunEvent::new(
            1,
            2,
            EventKind::ChildDetached {
                child_run_id: 8,
                after: Some(std::time::Duration::from_secs(30)),
            },
        ));
        let child = fleet.selected_child().expect("still marked");
        assert_eq!(child.state, State::Detached, "detached is still running");
    }
}
