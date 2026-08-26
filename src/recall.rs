//! What the agent remembers between runs, read back.
//!
//! io-harness has kept durable memory since 0.10.0 — notes an agent writes for
//! itself, keyed to a workspace, carried into the prompt of every later run over
//! that workspace. The whole API is `pub`, and until this release **io-cli had
//! never called a line of it**. An operator running this product could not see
//! what their agent believed, could not tell a note it had leaned on from one it
//! had never used, and had no way to know that the harness had quietly dropped
//! one to hold a cap.
//!
//! This module is the read. It writes nothing: no `memory_put`, no
//! `memory_write`, no `memory_pin`, no `memory_forget`. Pinning and forgetting
//! are an operator's actions and belong to their own surface; a reader that could
//! also mutate is a reader somebody eventually calls from a render pass.
//!
//! It follows the shape of [`crate::sessions`]: take a `&Store`, return owned
//! data, let the caller draw it. Nothing here touches a terminal, and nothing
//! here reads a clock — every time it reports is the `created_at` string the
//! store wrote, passed through untouched, which is the rule `tests/timing.rs`
//! enforces across this crate.
//!
//! # The four things that make a naive reading wrong
//!
//! **1. The bucket is a canonicalised path, and io-cli has to canonicalise it
//! itself.** io-harness computes the key in `memory_key()` at
//! `src/run/memory.rs:14-19` — `std::fs::canonicalize(root)`, falling back to the
//! path as given when that fails — and the function is `pub(super)`, so it cannot
//! be called from here. [`workspace_key`] reproduces it exactly, fallback
//! included. This matters more than it looks: a checkout reached through a
//! symlink (`/var` on macOS resolves to `/private/var`, a home-directory link to
//! a volume, a container bind mount) has its notes written under the *resolved*
//! path, so a lookup keyed on the path the operator typed finds an empty bucket
//! beside an agent writing a note every turn. It is correct on the developer's
//! own machine and wrong on somebody else's, which is the defect that ships
//! green.
//!
//! **2. There are two buckets.** The workspace's own, and the literal
//! [`GLOBAL_MEMORY_WORKSPACE`] — `"<global>"`, `src/state.rs:2833` — which holds
//! what the agent believes everywhere. Both are listed, and every row carries the
//! [`Scope`] it came from, because "is this true here, or true everywhere" is the
//! only question the two scopes exist to answer and a merged list destroys it.
//!
//! **3. The caps are per scope, not per run.** `src/contract.rs:376-379` states
//! it: each scope holds its own, so a run drawing on both may carry up to twice
//! `max_entries` and twice `max_chars`. [`View::caps`] is therefore a row per
//! scope rather than a number, and [`View::entries_ceiling`] is the sum — a
//! single `max_entries` presented as *the* cap is half the real ceiling, and the
//! half that makes a legitimate eviction look like a bug.
//!
//! **4. Eviction, pin-refusal and recall emit no `EventKind` at all.** io-harness
//! records all three as [`io_harness::ContextEvent`] rows deliberately —
//! `src/state.rs:2996-3002` spells out why: the question they answer
//! (*did my pin hold?*) is asked
//! afterwards by somebody reading the store, not during the run by an observer.
//! A *write* does emit `EventKind::MemoryWrote`, which is exactly what makes the
//! mistake easy: the observer stream looks like the right place because it
//! carries the neighbouring fact. An implementation reaching for it reports that
//! nothing has ever been evicted, refused or recalled, and a store where none of
//! that has happened looks identical. So [`trace`] reads
//! [`Store::context_events`] and `tests/recall.rs` asserts the stream's silence.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use io_harness::{Error, MemoryKind, MemoryLimits, Store, TaskContract, GLOBAL_MEMORY_WORKSPACE};

/// How many runs the draw-count scan will look at before giving up.
///
/// The same ceiling, for the same reason, as [`crate::sessions::MAX_RUNS_SCANNED`]:
/// io-harness exposes no public per-workspace draw count — `Store::memory_draws`
/// exists (`src/state/memory.rs:682`) and is `pub(crate)` — so the only public
/// path is [`Store::memory_recalls`], which is keyed by run. Counting how many
/// distinct runs drew on an entry therefore costs one indexed query per run, and
/// this bounds that cost.
///
/// A scan that stopped early makes every draw count a **lower bound**, which is a
/// different claim from the one an unqualified number makes. [`View::draws_cut`]
/// carries that so a caller can say so rather than letting a short count read as
/// a complete one.
pub const MAX_RUNS_SCANNED: usize = 500;

/// Which of the agent's two memories a note lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    /// This workspace only, keyed on the canonicalised root.
    Workspace,
    /// Every workspace — io-harness's [`GLOBAL_MEMORY_WORKSPACE`].
    Global,
}

impl Scope {
    /// What to call it in a line of output.
    pub fn label(self) -> &'static str {
        match self {
            Scope::Workspace => "workspace",
            Scope::Global => "global",
        }
    }
}

/// The key a workspace's durable memory is stored under.
///
/// **A reproduction of io-harness's own `memory_key`** (`src/run/memory.rs:14-19`),
/// which is `pub(super)` and so unreachable from here. Both halves are
/// load-bearing and both are copied on purpose:
///
/// - `canonicalize`, so the same directory reached by two different paths is one
///   bucket rather than two. This is the whole of trap 1 — see the module note.
/// - **the fallback to the path as given**, so a root that cannot be resolved
///   *yet* — deleted, unmounted, not created — still has memory rather than none.
///   Returning an error instead would make the notes about a workspace that has
///   moved unreadable precisely when somebody wants to read them.
///
/// If io-harness ever changes its keying, this diverges silently and the panel
/// goes empty. That is why `tests/recall.rs` drives a real turn through the
/// harness's own run loop and asserts the recall rows land under this key.
pub fn workspace_key(root: &Path) -> String {
    std::fs::canonicalize(root)
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The stored spelling of a memory kind.
///
/// `MemoryKind::as_str` is **private** in io-harness (`src/state.rs:1812`), so the
/// two words are spelled here. The enum is `#[non_exhaustive]`
/// (`src/state.rs:1800`) — the crate documents a third kind it intends to add —
/// so the wildcard arm is required rather than defensive, and it says *unknown*
/// rather than guessing `"fact"`: a kind this build cannot name is not a fact, it
/// is a row written by a newer harness than the one this binary was compiled
/// against.
pub fn kind_label(kind: MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Fact => "fact",
        MemoryKind::Decision => "decision",
        _ => "unknown",
    }
}

/// One note, as a panel shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remembered {
    /// Which of the two memories holds it.
    pub scope: Scope,
    /// The name it is recalled by, unique within its bucket.
    pub key: String,
    /// The remembered text.
    pub value: String,
    /// [`kind_label`] of the entry's kind.
    pub kind: &'static str,
    /// Whether an operator pinned it, which is what stops a run overwriting it.
    pub pinned: bool,
    /// The run that wrote it.
    pub run_id: i64,
    /// The step of that run which wrote it.
    pub step: u32,
    /// The store's own UTC write time, **as stored**. Never computed here, never
    /// turned into an age: this crate reads no clock outside `src/main.rs`.
    pub created_at: String,
    /// How many **distinct runs** have drawn on it.
    ///
    /// Distinct runs and not recall rows, matching the evidence io-harness itself
    /// evicts by (`src/state/memory.rs:639-644`): a recall row is written once per
    /// carried key per *step*, so counting rows would let one two-hundred-step run
    /// outvote fifty runs that each leaned on the note once, and would make the
    /// number monotone in age rather than in usefulness.
    ///
    /// Zero is a fact — nothing has used it — not a gap. A lower bound when
    /// [`View::draws_cut`] is set.
    pub draws: usize,
}

/// The caps in force for one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// The scope these bound.
    pub scope: Scope,
    /// What the contract carries. Taken from a [`TaskContract`] the caller passes
    /// in and never re-derived from configuration here — the setting in force is
    /// the one the turn carries, and a second answer assembled in a view would be
    /// io-cli holding an opinion about a value it does not own.
    pub limits: MemoryLimits,
}

/// What a trace row says happened to memory.
///
/// Exactly the three that are invisible anywhere else. `memory_write` and
/// `memory_forget` are trace rows too, but a write is already on the observer
/// stream as `EventKind::MemoryWrote` and a forget is the operator's own action
/// on a surface that owns it; these three have no other witness at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Happened {
    /// A note was dropped to hold the caps (`"memory_evict"`).
    Evicted,
    /// A run tried to overwrite a pinned note and was refused
    /// (`"memory_refused"`).
    Refused,
    /// Notes from earlier runs were carried into a turn's prompt
    /// (`"memory_recall"`).
    Recalled,
}

impl Happened {
    /// The kind string io-harness stores, `src/state.rs:2982-3009`.
    fn of(kind: &str) -> Option<Self> {
        match kind {
            "memory_evict" => Some(Happened::Evicted),
            "memory_refused" => Some(Happened::Refused),
            "memory_recall" => Some(Happened::Recalled),
            _ => None,
        }
    }
}

/// One thing the trace says happened to memory during a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noted {
    /// The step it belongs to.
    pub step: u32,
    /// Which of the three it is.
    pub happened: Happened,
    /// io-harness's own sentence — the key and why, for an eviction or a refusal;
    /// how many notes of how many were carried, for a recall.
    pub detail: Option<String>,
}

/// Everything a memory panel needs, and nothing it has to compute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    /// The bucket the workspace rows were read from: the **canonicalised** root.
    ///
    /// Reported rather than assumed, so a reader looking at an empty panel can
    /// see which path answered — which is the difference between "the agent has
    /// learnt nothing" and "you are looking at the wrong bucket".
    pub workspace: String,
    /// Every note in both buckets: the workspace's, then the global one, each in
    /// the order io-harness returns it (`created_at ASC, key ASC`,
    /// `src/state/memory.rs:764`).
    pub entries: Vec<Remembered>,
    /// The caps, one row per scope. See the module note, trap 3.
    pub caps: Vec<Caps>,
    /// The evictions, refusals and recalls the trace holds for the run the caller
    /// named. Empty when it named none.
    pub trace: Vec<Noted>,
    /// The draw scan hit [`MAX_RUNS_SCANNED`], so every [`Remembered::draws`] is a
    /// lower bound rather than a count.
    pub draws_cut: bool,
}

impl View {
    /// How many notes a run may carry in total.
    ///
    /// The sum across scopes, because each scope holds its own cap
    /// (`src/contract.rs:376-379`). This exists so the honest number is the easy
    /// one to reach for: quoting `max_entries` at an operator tells them half the
    /// real ceiling, and half a ceiling makes an ordinary eviction read as a
    /// defect.
    pub fn entries_ceiling(&self) -> usize {
        self.caps.iter().map(|c| c.limits.max_entries).sum()
    }

    /// How many characters of memory a run may carry in total, summed across
    /// scopes for the same reason as [`View::entries_ceiling`].
    ///
    /// `max_entry_chars` is deliberately not summed anywhere: it bounds one
    /// entry, and an entry lives in exactly one scope.
    pub fn chars_ceiling(&self) -> usize {
        self.caps.iter().map(|c| c.limits.max_chars).sum()
    }
}

/// Read the agent's durable memory for `root`.
///
/// `contract` supplies the caps — the one the session is about to run, or the one
/// it just ran. `run` names the run whose trace to read, and `None` is a view with
/// no trace rather than a view of the newest run: guessing which run an operator
/// meant would put somebody else's evictions in front of them.
///
/// Every row is a read. This function cannot write to the store and cannot reach
/// the filesystem except through [`workspace_key`], which only resolves a path.
pub fn view(
    store: &Store,
    root: &Path,
    contract: &TaskContract,
    run: Option<i64>,
) -> Result<View, Error> {
    let workspace = workspace_key(root);
    let (drawn, draws_cut) = draws(store)?;

    let mut entries = Vec::new();
    for (scope, bucket) in [
        (Scope::Workspace, workspace.as_str()),
        (Scope::Global, GLOBAL_MEMORY_WORKSPACE),
    ] {
        for entry in store.memory_list(bucket)? {
            let draws = drawn
                .get(&(bucket.to_string(), entry.key.clone()))
                .map_or(0, |runs| runs.len());
            entries.push(Remembered {
                scope,
                kind: kind_label(entry.kind),
                key: entry.key,
                value: entry.value,
                pinned: entry.pinned,
                run_id: entry.run_id,
                step: entry.step,
                created_at: entry.created_at,
                draws,
            });
        }
    }

    Ok(View {
        workspace,
        entries,
        // One row per scope rather than one number. The contract carries a single
        // `MemoryLimits`, and that is precisely the trap: the same numbers apply
        // to each scope *separately*, so the pair is what is true and a lone copy
        // of it is what is misread.
        caps: [Scope::Workspace, Scope::Global]
            .map(|scope| Caps {
                scope,
                limits: contract.memory,
            })
            .to_vec(),
        trace: match run {
            Some(id) => trace(store, id)?,
            None => Vec::new(),
        },
        draws_cut,
    })
}

/// The evictions, refusals and recalls recorded against `run_id`.
///
/// **From [`Store::context_events`], which is the only place they exist.** See the
/// module note, trap 4: none of the three emits an `EventKind`, so an
/// implementation built on the observer stream reports an empty history of a
/// store that has been evicting notes for months.
///
/// Rows of every other kind — `"assembled"`, `"reread"`, `"memory_write"`,
/// `"memory_forget"` — are dropped rather than surfaced as an unknown variant:
/// this is a memory panel, and a context-assembly row in it is noise a reader has
/// to learn to skip.
pub fn trace(store: &Store, run_id: i64) -> Result<Vec<Noted>, Error> {
    Ok(store
        .context_events(run_id)?
        .into_iter()
        .filter_map(|event| {
            Happened::of(&event.kind).map(|happened| Noted {
                step: event.step,
                happened,
                detail: event.detail,
            })
        })
        .collect())
}

/// Which runs have drawn on each `(bucket, key)`, and whether the scan was cut.
///
/// io-harness's own per-workspace answer, `Store::memory_draws`
/// (`src/state/memory.rs:682`), is `pub(crate)`. The public surface is
/// [`Store::memory_recalls`], which is keyed by run — so the count has to be
/// assembled from the other side, one indexed query per run, bounded by
/// [`MAX_RUNS_SCANNED`].
///
/// A `BTreeSet` of run ids rather than a counter because a run that carried the
/// same note on six steps writes six rows, and the number worth showing is one.
#[allow(clippy::type_complexity)]
fn draws(store: &Store) -> Result<(BTreeMap<(String, String), BTreeSet<i64>>, bool), Error> {
    let runs = store.runs()?;
    let cut = runs.len() > MAX_RUNS_SCANNED;
    let mut drawn: BTreeMap<(String, String), BTreeSet<i64>> = BTreeMap::new();
    for run in runs.into_iter().take(MAX_RUNS_SCANNED) {
        for recall in store.memory_recalls(run)? {
            drawn
                .entry((recall.workspace, recall.key))
                .or_default()
                .insert(recall.run_id);
        }
    }
    Ok((drawn, cut))
}
