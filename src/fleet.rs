//! The fleet: what a decomposed task looks like while it is running.
//!
//! **A model over the event stream, and nothing else.** io-harness reports a
//! tree as five kinds of fact, each on the run it belongs to, and this assembles
//! them back into a shape a person can read. It owns no state io-harness does not
//! report, reads no store, and decides nothing about the run.
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
//! adds on purpose.
//!
//! **Tier counts are replaced, never accumulated.** Each `Fleet` carries that
//! tier's whole shape — working, queued and done as they now stand — including
//! the backlog a resumed tree reads back out of the store. Adding them up would
//! make a restart look like a doubling.

use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use io_harness::{EventKind, RunEvent};

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

/// One admitted child.
#[derive(Debug, Clone)]
pub struct Child {
    /// Its own run id, which every event of its own carries.
    pub run_id: i64,
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
    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.tiers.is_empty()
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

    /// Forget the run this describes.
    ///
    /// Called where the conversation under it changes — a turn ending, `/resume`,
    /// `/fork`, a rewind. Every fact here belongs to one tree, and a view that
    /// went on showing them would be describing a run that is no longer on
    /// screen; the same rule `Status::forget_run` holds.
    pub fn forget(&mut self) {
        self.children.clear();
        self.tiers.clear();
        self.selected = None;
    }

    /// Fold one event in.
    pub fn event(&mut self, event: &RunEvent) {
        match &event.kind {
            EventKind::Spawned { child_run_id, goal } => {
                // Keyed by the child's own run id rather than appended blindly:
                // a resumed tree announces children it already had, and a second
                // row for one agent is a fleet that looks twice its size.
                if self.child(*child_run_id).is_none() {
                    self.children.push(Child {
                        run_id: *child_run_id,
                        depth: event.depth + 1,
                        goal: goal.clone(),
                        state: State::Working,
                        drawn: 0,
                    });
                    if self.selected.is_none() {
                        self.selected = Some(0);
                    }
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

    /// One row per admitted child, fitted to `width`.
    pub fn rows(&self, width: u16, glyphs: &Glyphs) -> Vec<String> {
        let room = width as usize;
        self.children
            .iter()
            .map(|child| {
                let indent = "  ".repeat(child.depth.saturating_sub(1) as usize);
                let drawn = crate::status::format_tokens(child.drawn);
                let head = format!(
                    "{indent}run {} {} {} {} {drawn} drawn {} ",
                    child.run_id,
                    glyphs.separator.trim(),
                    child.state.word(),
                    glyphs.separator.trim(),
                    glyphs.separator.trim(),
                );
                // The goal is what gets cut, because everything in front of it
                // identifies the row and it is the only part that can be long.
                let room_for_goal = room.saturating_sub(head.chars().count());
                format!("{head}{}", fit(&child.goal, room_for_goal, glyphs))
            })
            .collect()
    }

    /// Draw the view: the tier line, then as many child rows as there is room for.
    ///
    /// **It takes the composer's rows rather than the whole viewport**, so the
    /// status line — where the spend is — stays on screen underneath it, and the
    /// streaming tail above it goes on moving. The viewport is four rows and
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

        let visible = area.height.saturating_sub(1) as usize;
        let rows = self.rows(area.width, &theme.glyphs);
        // The window follows the marker rather than the newest row: a list that
        // scrolled itself while an operator was reading one row would take the
        // row away from them.
        let at = self.selected.unwrap_or(0);
        let first = at.saturating_sub(visible.saturating_sub(1));
        let mut marked_row = None;
        for (offset, row) in rows.iter().skip(first).take(visible).enumerate() {
            let index = first + offset;
            let tone = if Some(index) == self.selected {
                marked_row = Some(area.y + 1 + offset as u16);
                Tone::Normal
            } else {
                Tone::Muted
            };
            lines.push(Line::from(Span::styled(row.clone(), theme.style(tone))));
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
