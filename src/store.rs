//! What the run store is holding, and the four things an operator can do about it.
//!
//! io-harness has kept every session, run, step, event, provider call, snapshot
//! and restore point in one SQLite file since this product had a home to put it
//! in — `~/.io-cli/runs.db` since 0.15.0, across twenty-six releases. Until this
//! one there was no retention policy, no rotation, no reclamation, and **no way
//! to look at it at all**. A file that only ever grows is a cost this product
//! created; this module is where it is finally answerable.
//!
//! Everything here is io-harness's arithmetic. [`Store::store_size`],
//! [`Store::session_size`], [`Store::delete_session`], [`Store::sweep_sessions`]
//! and [`Store::compact`] are all public and all uncalled anywhere else in this
//! crate. Nothing in this module computes a byte count, and nothing re-derives a
//! figure the store already returns — the one number io-cli must never invent is
//! the one an operator is about to authorise a deletion against.
//!
//! It follows the shape of [`crate::recall`] and [`crate::sessions`]: take a
//! `&Store`, return owned data, let the caller draw it. Nothing here touches a
//! terminal and nothing here reads a clock.
//!
//! # The distinction the whole module exists to make
//!
//! **A deletion does not shrink the file.** SQLite frees pages *into* the
//! database rather than out of it, so [`Store::delete_session`] moves bytes from
//! [`StoreSize::file_bytes`] into [`StoreSize::free_bytes`] and the file on disk
//! stays exactly the size it was. [`Store::compact`] — a `VACUUM` — is the only
//! reclamation available, because every store this crate has ever created was
//! created without `auto_vacuum` and `PRAGMA incremental_vacuum` therefore does
//! nothing on any existing file.
//!
//! An interface that reported a deletion as having reclaimed space would be
//! lying in precisely the way this release exists to stop, which is why
//! [`Freed`] carries both figures and why `tests/store.rs` asserts `file_bytes`
//! is **unchanged** by a removal rather than asserting it merely did not grow.
//!
//! # The four things that make a naive reading wrong
//!
//! **1. `None` and `Some(zeros)` are different answers.** [`Store::session_size`]
//! answers `None` for a session the store does not have and `Some` with zeros for
//! one that exists and holds nothing. io-harness keeps them apart on purpose —
//! `state/sessions.rs:305-309` says why: an operator sweeping a list of ids needs
//! to know which of them were already gone. [`Sized`] preserves the distinction;
//! mapping `None` to a zeroed [`SessionSize`] would tell that operator every id
//! was empty rather than that half of them no longer exist.
//!
//! **2. The sweep refuses, and the refusal is the interesting half.** A session
//! holding a run that can still be resumed is **not deleted**, and its id comes
//! back in [`Pruned::refused`]. io-harness's reasoning (`state/sessions.rs:370-378`)
//! is that a date is a policy applied to sessions nobody looked at, and a
//! crash-resumable tree that vanished because it was old is the worst outcome the
//! call could have. So a report that shows only what went tells the operator they
//! swept sessions that are still sitting there.
//!
//! **3. There is no public reader for `sessions.created_at`, which is the column
//! the sweep filters on.** `Store::sweep_sessions` selects
//! `WHERE created_at < ?1` (`state/sessions.rs:429`) and nothing in io-harness
//! returns that value — only `root` and `head_turn_id` are readable off the
//! `sessions` row. So the set a date selects **cannot be counted before the sweep
//! runs**, and the nearest substitute is worse than none: a session's earliest
//! `Turn::created_at` is always *later* than the session's own, because
//! `Session::open` writes the session row when the process starts and a turn is
//! written when the operator types. A preview built on turns therefore
//! under-states the deletion, which is the direction that costs somebody data.
//!
//! Filed as io-harness#216. Shipped around by [`Swept`]: the operator authorises
//! the **rule**, which is what the operation is, and every figure they are shown
//! afterwards is one io-harness returned. See `US-IO-CLI-0.27.0-I02`.
//!
//! **4. Memory survives a session deletion, and its recall rows do not.**
//! io-harness's entries belong to the workspace rather than to the session
//! (`state/sessions.rs:368-373`), so removing a session unlearns nothing — but
//! the session's **restore points go**, and their count is in
//! [`Pruned::restore_points`]. That is the sentence an operator needs before they
//! agree, because it is the one that bites later: they delete an old session,
//! and reach for its rewind a week afterwards.
//!
//! # What is deliberately not here
//!
//! No automatic retention, rotation, sweeping or compaction. No startup sweep, no
//! size threshold, no configuration key that turns any of it on. The roadmap's
//! own words are *none of which happens on its own*, and io-harness makes the
//! argument itself: a call that rewrites the whole database and needs the file's
//! size again in free space while it runs is one an operator makes knowingly.
//!
//! And **nothing here is reachable by a model.** io-harness's workspace tool set
//! contains nothing that can call any of these, this release adds no tool, no MCP
//! server and no skill, and `tests/dependencies.rs` asserts that each of the three
//! destructive calls is named in exactly one module — this one.

use io_harness::{Error, Pruned, SessionSize, Store, StoreSize};

/// The size of one session, or the fact that there is no such session.
///
/// Two variants and not an `Option<SessionSize>` at the call site, because the
/// two answers need different sentences and a bare `Option` invites the caller to
/// `unwrap_or_default()` — which is exactly the collapse trap 1 describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sized {
    /// The session is in the store. Zeros here mean it holds nothing, which is a
    /// fact rather than a gap.
    Holds(SessionSize),
    /// There is no session of that id. Not an error, and not an empty session.
    Absent,
}

impl Sized {
    /// The figures, when there are any.
    pub fn size(&self) -> Option<&SessionSize> {
        match self {
            Sized::Holds(size) => Some(size),
            Sized::Absent => None,
        }
    }
}

/// One row of the store page: a session and what it costs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// The session id, which is what every operation here addresses.
    pub id: i64,
    /// The workspace root the session was opened over, as the store holds it.
    pub root: String,
    /// What it costs, or that it is gone. Read per session rather than derived
    /// from a total, so a row and the operation that acts on it agree.
    pub sized: Sized,
}

/// Everything a store panel needs, and nothing it has to compute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// The file's own page arithmetic: `page_size × page_count` for
    /// [`StoreSize::file_bytes`] and `page_size × freelist_count` for
    /// [`StoreSize::free_bytes`], plus the session and run counts and the
    /// per-table breakdown. Read from [`Store::store_size`] and never assembled
    /// here.
    pub size: StoreSize,
    /// One row per session the store holds, in the order [`crate::sessions`]
    /// returns them.
    pub rows: Vec<Session>,
    /// The session scan hit [`crate::sessions::MAX_RUNS_SCANNED`], so [`View::rows`]
    /// is a prefix rather than the whole store. The totals in [`View::size`] are
    /// unaffected — they come from the file, not from the scan — which is exactly
    /// why a cut list beside a complete total has to say so.
    pub cut: bool,
}

impl View {
    /// The part of the file that is already free inside it.
    ///
    /// Named rather than left to a caller's subtraction because the interesting
    /// quantity is the one nobody thinks to ask for: bytes a deletion has already
    /// released that are still occupying disk, and that only [`compact`] returns.
    pub fn reclaimable(&self) -> u64 {
        self.size.free_bytes
    }
}

/// Read what the store is holding.
///
/// Every row is a read. This function cannot write to the store.
///
/// The session list comes from [`crate::sessions::recent`], which is the list
/// this product already shows for `/resume` — so a session an operator sees here
/// is one they can recognise there, and the scan bound is the same one. Each
/// row's size is read individually, because a total divided among rows would be
/// io-cli inventing an attribution io-harness did not make.
pub fn view(store: &Store) -> Result<View, Error> {
    let size = store.store_size()?;
    let (recent, cut) = crate::sessions::recent(store)?;
    let mut rows = Vec::with_capacity(recent.len());
    for session in recent {
        rows.push(Session {
            id: session.id,
            root: session.root,
            sized: sized(store, session.id)?,
        });
    }
    Ok(View { size, rows, cut })
}

/// What one session costs, keeping "absent" apart from "empty".
///
/// See trap 1. The mapping is the whole function and it is the point of it.
pub fn sized(store: &Store, session_id: i64) -> Result<Sized, Error> {
    Ok(match store.session_size(session_id)? {
        Some(size) => Sized::Holds(size),
        None => Sized::Absent,
    })
}

/// What removing one session did, and what it did **not** do to the file.
///
/// Both halves together, from two reads of [`Store::store_size`] taken either
/// side of the removal, because the fact worth reporting is a relationship
/// between them: rows went, `free_bytes` rose, and the file on disk did not move.
/// A caller handed only [`Pruned`] would have no way to say the third thing, and
/// the third thing is the one that surprises people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Removed {
    /// io-harness's own account of what was deleted.
    pub pruned: Pruned,
    /// The file before.
    pub before: StoreSize,
    /// The file after. [`StoreSize::file_bytes`] equals `before`'s; only
    /// [`StoreSize::free_bytes`] has moved.
    pub after: StoreSize,
}

impl Removed {
    /// Whether the file on disk changed size. Always false, and asserted rather
    /// than assumed — if it is ever true, either SQLite's behaviour or this
    /// module's understanding of it has changed and the report must not go on
    /// claiming otherwise.
    pub fn file_moved(&self) -> bool {
        self.before.file_bytes != self.after.file_bytes
    }

    /// How much more of the file is free than was free before.
    pub fn freed_into_file(&self) -> u64 {
        self.after.free_bytes.saturating_sub(self.before.free_bytes)
    }
}

/// Remove one session and everything keyed to it.
///
/// **Final. There is no undelete, and this release does not build one.** What
/// survives is io-harness's decision and is worth knowing before agreeing: the
/// agent's memory entries belong to the workspace and are untouched, while the
/// session's *recall* rows go with it because they name runs that no longer
/// exist, and so do its **restore points** — the count of which is in
/// [`Pruned::restore_points`]. An operator who removes an old session has removed
/// its rewind.
///
/// Deleting a session the store does not have succeeds and reports nothing, which
/// is io-harness's own behaviour and is not softened here.
pub fn remove(store: &Store, session_id: i64) -> Result<Removed, Error> {
    let before = store.store_size()?;
    let pruned = store.delete_session(session_id)?;
    let after = store.store_size()?;
    Ok(Removed {
        pruned,
        before,
        after,
    })
}

/// What a date sweep did.
///
/// The counts live here and **not** in a preview, because they cannot be known
/// before the sweep runs — see trap 3 and io-harness#216. This is the whole of
/// the ship-around: the operator authorises the rule, and this is the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swept {
    /// The boundary that was applied, echoed back so the report says what it is
    /// the report *of*.
    pub before_date: String,
    /// io-harness's own account: what went, and every id it refused.
    pub pruned: Pruned,
    /// The file before the sweep.
    pub before: StoreSize,
    /// The file after it. As with [`Removed`], `file_bytes` has not moved.
    pub after: StoreSize,
}

impl Swept {
    /// The sessions that were refused for still being resumable.
    ///
    /// A borrow rather than a count, because the report names them: an operator
    /// told "one was refused" has to go looking, and the id is the thing they
    /// would go looking for.
    pub fn refused(&self) -> &[i64] {
        &self.pruned.refused
    }
}

/// Remove every session created strictly before `before_date`.
///
/// `before_date` is a timestamp string compared against `sessions.created_at`,
/// which is a `strftime('%Y-%m-%dT%H:%M:%fZ')` text column — a string comparison
/// is what the storage actually does, so that is what io-harness takes rather
/// than a duration measured against a clock the store does not have. The
/// comparison is **strictly** before: a session created at exactly `before_date`
/// survives.
///
/// A session holding a resumable run is refused rather than deleted and comes
/// back in [`Swept::refused`]. That is not an error and must not be reported as
/// one.
pub fn sweep(store: &Store, before_date: &str) -> Result<Swept, Error> {
    let before = store.store_size()?;
    let pruned = store.sweep_sessions(before_date)?;
    let after = store.store_size()?;
    Ok(Swept {
        before_date: before_date.to_string(),
        pruned,
        before,
        after,
    })
}

/// What a compaction actually returned to the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Freed {
    /// The bytes the file shrank by, **measured** by io-harness as the difference
    /// between the file's size before and after — never inferred from the
    /// freelist. Zero is an honest answer for a store with nothing free, and the
    /// caller says so rather than showing an empty success.
    pub bytes: u64,
    /// The file before.
    pub before: StoreSize,
    /// The file after.
    pub after: StoreSize,
}

impl Freed {
    /// Whether anything was reclaimed at all.
    pub fn is_nothing(&self) -> bool {
        self.bytes == 0
    }
}

/// Rewrite the database without its free pages, and say how much that returned.
///
/// **This is the only reclamation available.** Every store this crate has created
/// was created without `auto_vacuum`, so `PRAGMA incremental_vacuum` does nothing
/// on any existing file, and a deletion on its own never shrinks anything.
///
/// **It is expensive and the caller must say so first.** io-harness's own
/// documentation is explicit: it rewrites the whole database, it needs free disk
/// space of roughly the file's own size while it runs, it cannot run inside a
/// transaction, and on a large store it is not quick. That is why it is a call an
/// operator makes knowingly and never something a deletion does on their behalf.
pub fn compact(store: &Store) -> Result<Freed, Error> {
    let before = store.store_size()?;
    let bytes = store.compact()?;
    let after = store.store_size()?;
    Ok(Freed {
        bytes,
        before,
        after,
    })
}

// ---------------------------------------------------------------------------
// The surfaces
// ---------------------------------------------------------------------------

/// The row every confirmation starts on: the one that does nothing.
///
/// Index 0, always, in every confirmation this module builds. A `Picker` opens
/// on its first row, so the keystroke an operator gives by reflex is the one
/// that declines — which is the whole of criterion F5 and the reason
/// `tests/store.rs` asserts the *index* rather than the presence of the row.
pub const LEAVE_IT: &str = "leave it";

/// Whether a chosen row in one of these confirmations acts.
///
/// One line, and it is in the library rather than in the driver on purpose.
/// `src/main.rs` is `[[bin]] name = "io"` and nothing under `tests/` can link it,
/// so a `index != 0` written at the three call sites would be the one decision in
/// this module that could never be tested or sabotaged — which is 0.22.0's
/// recorded lesson (*any decision worth testing has to live in the library*) and
/// 0.23.0's (*all three HIGH defects were in `src/main.rs`*).
///
/// It is also what makes criterion F5 assertable as a property rather than as a
/// label comparison: row 0 declines, whatever it is called.
pub fn acts(index: usize) -> bool {
    index != 0
}

/// The `/store` page, committed into the scrollback.
///
/// Into the scrollback and never into a pane, for the reason `/status`, `/cost`
/// and `/stats` all give: the viewport is eight rows and cannot grow, and the
/// terminal's own search, selection and copy already work on everything above
/// it. This is a page a reader wants to keep beside the sessions it accounts for.
pub fn committed(
    store: &Store,
    theme: &crate::theme::Theme,
    width: u16,
) -> Result<Vec<ratatui::text::Line<'static>>, String> {
    let view = view(store).map_err(|e| e.to_string())?;
    let mut rows: Vec<crate::page::Row> = vec![
        crate::page::Row::heading("the file".to_string()),
        crate::page::Row::fact("on disk", crate::stats::bytes(view.size.file_bytes)),
        // Named as free *inside* the file rather than as "reclaimable", because
        // the second word invites the reading that a deletion returned it. It did
        // not: this is space a `VACUUM` could return, and nothing else can.
        crate::page::Row::fact("free inside it", crate::stats::bytes(view.reclaimable())),
        crate::page::Row::fact("sessions", view.size.sessions.to_string()),
        crate::page::Row::fact("runs", view.size.runs.to_string()),
    ];

    if view.reclaimable() > 0 {
        rows.push(crate::page::Row::caveat(format!(
            "{} is already free inside the file and still occupies the disk; \
             `/store compact` is the only thing that returns it",
            crate::stats::bytes(view.reclaimable())
        )));
    }

    rows.push(crate::page::Row::Blank);
    rows.push(crate::page::Row::heading("by table".to_string()));
    if view.size.tables.is_empty() {
        rows.push(crate::page::Row::note("no tables reported".to_string()));
    }
    for (table, bytes) in &view.size.tables {
        rows.push(crate::page::Row::fact(
            table.clone(),
            crate::stats::bytes(*bytes),
        ));
    }

    rows.push(crate::page::Row::Blank);
    rows.push(crate::page::Row::heading("by session".to_string()));
    if view.rows.is_empty() {
        rows.push(crate::page::Row::note("no sessions recorded".to_string()));
    }
    for session in &view.rows {
        rows.push(crate::page::Row::fact(
            format!("{} · {}", session.id, session.root),
            session_figures(&session.sized),
        ));
    }
    if view.cut {
        rows.push(crate::page::Row::caveat(format!(
            "only the {} most recent sessions are listed; the totals above are \
             the whole file",
            view.rows.len()
        )));
    }

    rows.push(crate::page::Row::Blank);
    rows.push(crate::page::Row::note(
        "`/store rm <id>` removes one session · `/store sweep <date>` removes \
         every session created before it · `/store compact` returns the free \
         pages to the disk"
            .to_string(),
    ));

    Ok(crate::page::commit(
        "What the store holds",
        &rows,
        theme,
        width,
    ))
}

/// One session's figures, or the sentence for a session that is not there.
///
/// The two answers are different sentences rather than one with zeros in it —
/// see trap 1. An operator reading `0 turns` about a session that no longer
/// exists would go looking for an empty session.
fn session_figures(sized: &Sized) -> String {
    match sized {
        Sized::Holds(size) => format!(
            "{} · {} turn{} · {} run{}",
            crate::stats::bytes(size.bytes),
            size.turns,
            if size.turns == 1 { "" } else { "s" },
            size.runs,
            if size.runs == 1 { "" } else { "s" },
        ),
        Sized::Absent => "no such session".to_string(),
    }
}

/// The confirmation for removing one session: what it holds, and what goes with
/// it.
///
/// The title carries the figures, because a confirmation is a confirmation of
/// something specific rather than of a keystroke. The restore-point sentence is
/// here rather than in the report for the same reason: it is the fact an
/// operator needs *before* they agree, not the one they discover afterwards.
pub fn confirm_remove(id: i64, sized: &Sized) -> (String, Vec<crate::picker::Row>) {
    let title = match sized {
        Sized::Holds(size) => format!(
            "Remove session {id}? {} · {} turn{} · {} run{}",
            crate::stats::bytes(size.bytes),
            size.turns,
            if size.turns == 1 { "" } else { "s" },
            size.runs,
            if size.runs == 1 { "" } else { "s" },
        ),
        Sized::Absent => format!("There is no session {id}"),
    };
    let mut rows = vec![crate::picker::Row::with_detail(
        LEAVE_IT,
        "nothing is removed",
    )];
    if sized.size().is_some() {
        rows.push(crate::picker::Row::with_detail(
            format!("remove session {id}"),
            "final; its restore points go with it, its memory does not",
        ));
    }
    (title, rows)
}

/// The confirmation for a date sweep: the rule, and the refusal policy.
///
/// **Not a count.** See trap 3 and io-harness#216: the set a date selects cannot
/// be read before the sweep runs, and the one substitute available under-states
/// it. So the operator authorises the rule — which is what the operation is —
/// and [`swept_report`] carries the figures afterwards.
pub fn confirm_sweep(before_date: &str) -> (String, Vec<crate::picker::Row>) {
    (
        format!("Sweep every session created before {before_date}?"),
        vec![
            crate::picker::Row::with_detail(LEAVE_IT, "nothing is removed"),
            crate::picker::Row::with_detail(
                "sweep them",
                "a session still holding a resumable run is refused and named; \
                 the figures are reported once it has run",
            ),
        ],
    )
}

/// The confirmation for a compaction: what it costs while it runs.
///
/// The cost sentence is io-harness's own and is stated before the call rather
/// than discovered by a full disk: a `VACUUM` rewrites the whole database and
/// needs roughly the file's own size free while it does.
pub fn confirm_compact(size: &StoreSize) -> (String, Vec<crate::picker::Row>) {
    (
        format!(
            "Compact the store? {} free inside a {} file",
            crate::stats::bytes(size.free_bytes),
            crate::stats::bytes(size.file_bytes),
        ),
        vec![
            crate::picker::Row::with_detail(LEAVE_IT, "the file is left as it is"),
            crate::picker::Row::with_detail(
                "compact it",
                "rewrites the whole database and needs about its own size free \
                 on disk while it runs",
            ),
        ],
    )
}

/// What to say after a removal.
///
/// Three sentences, and the third is the one this module exists for: rows went,
/// the free space inside the file rose, and **the file on disk did not move**.
pub fn removed_report(removed: &Removed) -> Vec<String> {
    let pruned = &removed.pruned;
    let mut lines = vec![format!(
        "removed {} session{}: {} turn{}, {} run{}, {} row{}, {}",
        pruned.sessions,
        if pruned.sessions == 1 { "" } else { "s" },
        pruned.turns,
        if pruned.turns == 1 { "" } else { "s" },
        pruned.runs,
        if pruned.runs == 1 { "" } else { "s" },
        pruned.rows,
        if pruned.rows == 1 { "" } else { "s" },
        crate::stats::bytes(pruned.bytes),
    )];
    if pruned.restore_points > 0 {
        lines.push(format!(
            "{} restore point{} went with it; the workspace's memory did not",
            pruned.restore_points,
            if pruned.restore_points == 1 { "" } else { "s" },
        ));
    }
    lines.push(freeing_sentence(
        removed.freed_into_file(),
        removed.after.file_bytes,
        removed.file_moved(),
    ));
    lines
}

/// What to say after a sweep — including, always, what it refused.
pub fn swept_report(swept: &Swept) -> Vec<String> {
    let pruned = &swept.pruned;
    let mut lines = vec![format!(
        "swept {} session{} created before {}: {} turn{}, {} run{}, {}",
        pruned.sessions,
        if pruned.sessions == 1 { "" } else { "s" },
        swept.before_date,
        pruned.turns,
        if pruned.turns == 1 { "" } else { "s" },
        pruned.runs,
        if pruned.runs == 1 { "" } else { "s" },
        crate::stats::bytes(pruned.bytes),
    )];
    // Never conditional on there being any. A sweep that refused nothing says so,
    // because "refused none" and "we did not check" are the same silence
    // otherwise — and the refusal is the half an operator has to act on.
    if swept.refused().is_empty() {
        lines.push("nothing was refused".to_string());
    } else {
        lines.push(format!(
            "refused {}: {} still holding a resumable run",
            swept
                .refused()
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            if swept.refused().len() == 1 {
                "it is"
            } else {
                "they are"
            },
        ));
    }
    lines.push(freeing_sentence(
        swept
            .after
            .free_bytes
            .saturating_sub(swept.before.free_bytes),
        swept.after.file_bytes,
        swept.before.file_bytes != swept.after.file_bytes,
    ));
    lines
}

/// What to say after a compaction.
pub fn freed_report(freed: &Freed) -> Vec<String> {
    if freed.is_nothing() {
        return vec![format!(
            "nothing to reclaim; the file is still {}",
            crate::stats::bytes(freed.after.file_bytes)
        )];
    }
    vec![format!(
        "returned {} to the disk; the file is now {}",
        crate::stats::bytes(freed.bytes),
        crate::stats::bytes(freed.after.file_bytes),
    )]
}

/// The sentence that keeps a deletion honest.
///
/// `moved` is asserted rather than assumed. It is always false — SQLite frees
/// pages into the file — and if it is ever true then either SQLite's behaviour or
/// this module's understanding of it has changed, and the report must say the
/// surprising thing rather than go on printing the reassuring one.
fn freeing_sentence(freed_into_file: u64, file_bytes: u64, moved: bool) -> String {
    if moved {
        return format!(
            "the file is now {} — it changed size, which a deletion is not \
             supposed to do",
            crate::stats::bytes(file_bytes)
        );
    }
    format!(
        "{} more is free inside the file; the file on disk is still {} — \
         `/store compact` is what returns it",
        crate::stats::bytes(freed_into_file),
        crate::stats::bytes(file_bytes),
    )
}
