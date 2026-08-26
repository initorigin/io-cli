//! F6 and F8 — the agent's own durable memory, read back.
//!
//! io-harness has kept durable memory since 0.10.0 and **io-cli has never once
//! read it**. Everything asserted here is therefore a first reading, and the two
//! criteria are shaped around the four ways a first reading of this particular
//! subsystem is wrong while looking right:
//!
//! 1. **The bucket is a canonicalised path.** io-harness keys a workspace's
//!    memory on `std::fs::canonicalize(root)` — `src/run/memory.rs:14-19`, and
//!    the function is `pub(super)`, so io-cli cannot call it and has to
//!    reproduce it. Key on the root as given and the panel is empty beside an
//!    agent writing a note every turn, **and only when the checkout is reached
//!    through a symlink**, which is the defect that ships green. `/var` on macOS
//!    is a symlink to `/private/var`, so a plain `tempdir()` already exercises
//!    it; the explicit symlink test below does not rely on that being true of
//!    the runner.
//! 2. **There are two buckets.** The workspace's own, and the literal
//!    `GLOBAL_MEMORY_WORKSPACE`. A view showing one shows about half of what the
//!    agent knows and says nothing about the other half.
//! 3. **The caps are per scope.** `src/contract.rs:376-379` — each scope holds
//!    its own, so a run drawing on both may carry up to twice `max_entries`. One
//!    number reported as *the* cap is half the real ceiling.
//! 4. **Eviction, refusal and recall emit no `EventKind` at all.** io-harness
//!    records them as `ContextEvent` rows on purpose (`src/state.rs:2996-3002`).
//!    An implementation reaching for the observer stream reports that none has
//!    ever happened, which is indistinguishable from a healthy store. The real
//!    run below watches the stream and asserts the *absence*, so that sabotage
//!    has a test that fails it.
//!
//! **The fixtures are real rows, never hand-built structs.** `MemoryEntry` could
//! be constructed field by field, and a test that did would assert that this
//! crate agrees with itself about a shape it invented. Every entry here is
//! written through `Store::memory_write_with`, and the eviction, the refusal and
//! the recalls in the last test are produced by an actual turn of the harness's
//! own run loop rather than recorded by hand — which is the only way to know that
//! the rows io-cli reads are the rows io-harness writes.
//!
//! **`Store::memory_pin` appears once, in a fixture.** A refused write needs a
//! pinned entry and nothing else produces one. `src/recall.rs` itself writes
//! nothing at all; pinning is a later task's, and this is the operator's action
//! staged so the refusal can happen.
//!
//! **No clock.** Every time asserted on is the string the store wrote. Nothing
//! here sleeps or measures, per `tests/timing.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use io_cli::recall::{self, Happened, Scope};
use io_harness::provider::{CompletionRequest, CompletionResponse, ToolCall};
use io_harness::{
    ApproveAll, Flow, MemoryKind, MemoryLimits, Observer, Policy, Provider, RunEvent, Session,
    Store, TaskContract, GLOBAL_MEMORY_WORKSPACE,
};

/// A store on a real temporary path, and the directory that keeps it alive.
///
/// On disk rather than `Store::memory()`, because the whole of F6's first trap is
/// about paths and a fixture that never touches the filesystem is a fixture that
/// cannot see it.
fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().expect("a temp dir for the store");
    let store = Store::open(dir.path().join("io.db")).expect("a store opens on a temp path");
    (dir, store)
}

/// Write one entry into `bucket` through io-harness's own public write path.
///
/// Returns the run that wrote it, because F6 asserts the attribution and a run id
/// invented here would not be one the store knows.
fn remember(store: &Store, bucket: &str, key: &str, value: &str, kind: MemoryKind) -> i64 {
    let run = store
        .start_run("fixture", bucket)
        .expect("a run to attribute the note to");
    let wrote = store
        .memory_write_with(bucket, key, value, run, 3, kind, MemoryLimits::default())
        .expect("the write lands");
    assert!(!wrote.refused, "{key} is not pinned, so nothing refuses it");
    run
}

/// The detail of every trace row of one kind, in the order the store returned
/// them. A free function rather than a closure over `trace`, so the borrow the
/// returned strings carry is the slice's and not an inferred one.
fn details(trace: &[recall::Noted], what: Happened) -> Vec<&str> {
    trace
        .iter()
        .filter(|note| note.happened == what)
        .map(|note| note.detail.as_deref().unwrap_or(""))
        .collect()
}

/// A contract carrying `limits`, rooted at `root`.
///
/// The caps come off a contract the caller passes in and are never read out of a
/// configuration file here: the numbers in force are the ones the turn carries,
/// and a second answer assembled from `io.toml` would be io-cli holding an opinion
/// about a value it does not own.
fn contract(root: &Path, limits: MemoryLimits) -> TaskContract {
    TaskContract::workspace("read the memory back", root).with_memory_limits(limits)
}

// ---------------------------------------------------------------------------
// F6 — every entry in both buckets, and the bucket is the canonicalised root
// ---------------------------------------------------------------------------

#[test]
fn f6_lists_both_buckets_and_keeps_them_apart() {
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    let wrote = remember(&store, &bucket, "test-command", "cargo test --lib", MemoryKind::Fact);
    remember(
        &store,
        GLOBAL_MEMORY_WORKSPACE,
        "package-manager",
        "pnpm",
        MemoryKind::Decision,
    );

    let view = recall::view(
        &store,
        workspace.path(),
        &contract(workspace.path(), MemoryLimits::default()),
        None,
    )
    .expect("the view reads");

    let keys: Vec<(&str, Scope)> = view
        .entries
        .iter()
        .map(|e| (e.key.as_str(), e.scope))
        .collect();
    assert_eq!(
        keys,
        vec![
            ("test-command", Scope::Workspace),
            ("package-manager", Scope::Global),
        ],
        "both buckets, and each row still says which one it came from — a merged \
         list cannot answer 'is this true everywhere or only here', which is the \
         one question the two scopes exist to separate",
    );

    let entry = &view.entries[0];
    assert_eq!(entry.kind, "fact");
    assert_eq!(entry.value, "cargo test --lib");
    assert!(!entry.pinned);
    assert_eq!(
        (entry.run_id, entry.step),
        (wrote, 3),
        "the attribution the store holds, so a stale note is traceable to the step \
         that wrote it",
    );
    assert!(
        !entry.created_at.is_empty(),
        "the stored string, passed through — io-cli may not compute a time",
    );
    assert_eq!(
        entry.draws, 0,
        "nothing has recalled it yet, and zero is a fact rather than a gap",
    );

    assert_eq!(
        view.entries[1].kind, "decision",
        "the two kinds are spelled here because `MemoryKind::as_str` is private \
         in io-harness (src/state.rs:1812)",
    );
}

#[cfg(unix)]
#[test]
fn f6_the_workspace_bucket_is_the_canonicalised_root() {
    // The defect this is the whole test for: a checkout reached through a symlink
    // — `~/src` pointing at `/Volumes/work/src`, a macOS `/var` temp path, a
    // container bind mount — writes its memory under the resolved path, because
    // io-harness canonicalises at `src/run/memory.rs:15`. io-cli keying on the
    // path the operator typed finds an empty bucket and reports, confidently,
    // that the agent has learnt nothing.
    let (_keep, store) = store();
    let real = tempfile::tempdir().expect("the real workspace");
    let parent = tempfile::tempdir().expect("somewhere to hang the link");
    let link = parent.path().join("checkout");
    std::os::unix::fs::symlink(real.path(), &link).expect("a symlink");

    assert_eq!(
        recall::workspace_key(&link),
        recall::workspace_key(real.path()),
        "one directory reached two ways is one bucket, which is exactly what \
         io-harness's `memory_key` promises",
    );
    assert_ne!(
        recall::workspace_key(&link),
        link.to_string_lossy(),
        "and the key is NOT the path as given — if these were equal this test \
         would pass against the sabotage it exists to catch",
    );

    // Written where io-harness would write it: under the resolved path.
    remember(
        &store,
        &recall::workspace_key(real.path()),
        "build",
        "cargo build --release",
        MemoryKind::Fact,
    );

    // Read through the symlink, which is all the operator ever has.
    let view = recall::view(&store, &link, &contract(&link, MemoryLimits::default()), None)
        .expect("the view reads");
    assert_eq!(
        view.entries.len(),
        1,
        "SABOTAGE: key the lookup on the root as given instead of canonicalising \
         it and this is 0 — an empty panel beside an agent writing memory every \
         turn",
    );
    assert_eq!(view.entries[0].key, "build");
    assert_eq!(
        view.workspace,
        recall::workspace_key(real.path()),
        "and the view names the bucket it actually read, so a reader can see \
         which path answered",
    );
}

#[test]
fn f6_a_root_that_cannot_be_canonicalised_falls_back_to_the_path_as_given() {
    // io-harness's fallback, reproduced deliberately (`src/run/memory.rs:16`): a
    // root that cannot be resolved yet should still have memory rather than none.
    // Dropping the fallback and returning an error would make every note written
    // against a since-deleted workspace unreadable, which is the case a reader
    // most wants to look at.
    let (_keep, store) = store();
    let gone = tempfile::tempdir().expect("a parent").path().join("deleted");
    assert!(!gone.exists(), "the fixture is a path that is not there");

    let bucket = recall::workspace_key(&gone);
    assert_eq!(
        bucket,
        gone.to_string_lossy(),
        "unresolvable falls back to the path as given, byte for byte",
    );

    remember(&store, &bucket, "why", "the workspace was moved", MemoryKind::Fact);
    let view = recall::view(&store, &gone, &contract(&gone, MemoryLimits::default()), None)
        .expect("the view reads");
    assert_eq!(view.entries.len(), 1, "the fallback bucket is readable");
}

// ---------------------------------------------------------------------------
// F8 — the caps, per scope
// ---------------------------------------------------------------------------

#[test]
fn f8_the_caps_are_named_per_scope_and_the_ceiling_is_both() {
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let limits = MemoryLimits {
        max_entries: 4,
        max_chars: 900,
        max_entry_chars: 120,
    };

    let view = recall::view(&store, workspace.path(), &contract(workspace.path(), limits), None)
        .expect("the view reads");

    assert_eq!(
        view.caps
            .iter()
            .map(|c| (c.scope, c.limits.max_entries))
            .collect::<Vec<_>>(),
        vec![(Scope::Workspace, 4), (Scope::Global, 4)],
        "one row per scope, because each scope holds its own \
         (io-harness src/contract.rs:376-379)",
    );
    assert_eq!(
        view.entries_ceiling(),
        8,
        "SABOTAGE: report `max_entries` as the ceiling and a reader is told 4 \
         when a run may carry 8 — half the real number, and the half that makes \
         an eviction look like a bug",
    );
    assert_eq!(view.chars_ceiling(), 1_800);
    assert_eq!(
        view.caps[0].limits.max_entry_chars, 120,
        "the per-entry cap is per entry and is NOT doubled by there being two \
         scopes",
    );
}

// ---------------------------------------------------------------------------
// F8 — an eviction, a refusal and a recall, from a turn that really happened
// ---------------------------------------------------------------------------

/// A provider that plays one batch of `remember` calls per turn.
///
/// Written here rather than reused from `tests/support`, whose `Scripted` only
/// writes files: the three trace rows this file is about are produced by the
/// `remember` tool and by nothing else, so the script has to be able to call it.
struct Remembering {
    batches: Mutex<std::collections::VecDeque<Vec<ToolCall>>>,
}

impl Remembering {
    fn with(batches: Vec<Vec<(&str, &str)>>) -> Self {
        let batches = batches
            .into_iter()
            .map(|batch| batch.into_iter().map(|(k, v)| remember_call(k, v)).collect())
            .collect();
        Self {
            batches: Mutex::new(batches),
        }
    }
}

/// One `remember` tool call. The keys and values below are bare identifiers, so
/// they need no JSON escaping and this stays four lines instead of importing an
/// encoder.
fn remember_call(key: &str, value: &str) -> ToolCall {
    ToolCall {
        name: io_harness::tools::REMEMBER_TOOL.to_string(),
        arguments: format!("{{\"key\":\"{key}\",\"value\":\"{value}\"}}")
            .parse()
            .expect("assembled as JSON, so it parses as JSON"),
    }
}

impl Provider for Remembering {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        let calls: Vec<ToolCall> = self
            .batches
            .lock()
            .expect("the script is not poisoned")
            .pop_front()
            .unwrap_or_default();
        Ok(CompletionResponse {
            // Text only once the batch is empty, so the loop stops for the
            // ordinary reason and not for a contrived one.
            text: calls.is_empty().then(|| "done".to_string()),
            tool_calls: calls,
            ..Default::default()
        })
    }
}

/// Every event the run loop announced, so the absence of a memory event can be
/// asserted rather than assumed.
#[derive(Clone, Default)]
struct Watching {
    seen: Arc<Mutex<Vec<String>>>,
}

impl Observer for Watching {
    fn event(&self, event: &RunEvent) -> Flow {
        self.seen
            .lock()
            .expect("not poisoned")
            .push(format!("{:?}", event.kind));
        Flow::Continue
    }
}

#[tokio::test]
async fn f8_eviction_refusal_and_recall_are_read_from_the_trace() {
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    // Two notes at the cap. `alpha` is pinned by the operator, which is what makes
    // the run's overwrite a refusal rather than a write.
    remember(&store, &bucket, "alpha", "the operators value", MemoryKind::Decision);
    remember(&store, &bucket, "beta", "something older", MemoryKind::Fact);
    store
        .memory_pin(&bucket, "alpha", true)
        .expect("the operator pins it");

    let limits = MemoryLimits {
        max_entries: 2,
        ..MemoryLimits::default()
    };
    let task = contract(workspace.path(), limits);
    let mut session = Session::open(&store, workspace.path()).expect("a session");
    let watching = Watching::default();

    // Turn one: overwrite the pinned note (refused), then write a third note,
    // which puts the bucket over the cap of two and evicts `beta`.
    let first = session
        .turn_bounded_observed(
            &task,
            &Remembering::with(vec![vec![
                ("alpha", "the runs value"),
                ("gamma", "learned this turn"),
            ]]),
            &store,
            &Policy::permissive(),
            &ApproveAll,
            &watching,
        )
        .await
        .expect("a scripted turn cannot fail");

    // Turn two: a second run, so the notes carried into both are drawn on twice.
    let second = session
        .turn_bounded_observed(
            &task,
            &Remembering::with(vec![vec![("delta", "learned next turn")]]),
            &store,
            &Policy::permissive(),
            &ApproveAll,
            &watching,
        )
        .await
        .expect("a scripted turn cannot fail");

    // --- the three kinds, read from the trace -----------------------------
    let trace = recall::trace(&store, first.run_id).expect("the trace reads");
    assert_eq!(
        details(&trace, Happened::Refused).len(),
        1,
        "the pinned note was not overwritten, and the refusal is a row: an agent \
         that believes it corrected something and did not will act on the \
         correction it thinks it made",
    );
    assert!(details(&trace, Happened::Refused)[0].contains("alpha"));
    assert_eq!(
        details(&trace, Happened::Evicted).len(),
        1,
        "a third note under a cap of two drops one, and which one it dropped is \
         the whole reason to record it",
    );
    assert!(details(&trace, Happened::Evicted)[0].contains("beta"));
    assert!(
        !details(&trace, Happened::Recalled).is_empty(),
        "the turn carried notes from earlier runs into its prompt",
    );

    // --- the sabotage: the observer stream knows nothing about any of it ---
    let seen = watching.seen.lock().expect("not poisoned").clone();
    assert!(
        seen.iter().any(|k| k.contains("MemoryWrote")),
        "the stream does carry the write, so this assertion is about what the \
         stream omits rather than about an observer that never fired",
    );
    for forbidden in ["Evict", "Refus", "Recall"] {
        assert!(
            !seen.iter().any(|k| k.contains(forbidden)),
            "SABOTAGE: io-harness emits no EventKind for {forbidden} \
             (src/state.rs:2996-3002), so an implementation reading evictions off \
             the observer stream reports that none has ever happened and looks \
             perfectly healthy. These must come from Store::context_events.",
        );
    }

    // --- draws: distinct runs, not rows -----------------------------------
    assert_ne!(first.run_id, second.run_id, "two turns, two runs");
    let view = recall::view(&store, workspace.path(), &task, Some(second.run_id))
        .expect("the view reads");

    // How many recall ROWS name `alpha`, which is the number the wrong
    // implementation reports. Each turn assembles a prompt on every step, so a
    // note carried through a two-step turn is two rows in one run.
    let rows = [first.run_id, second.run_id]
        .iter()
        .flat_map(|run| store.memory_recalls(*run).expect("the recalls read"))
        .filter(|recall| recall.key == "alpha")
        .count();
    assert!(
        rows > 2,
        "the fixture is only meaningful if some run carried `alpha` more than \
         once; it carried it {rows} time(s) in total",
    );

    let alpha = view
        .entries
        .iter()
        .find(|e| e.key == "alpha")
        .expect("pinned, so no eviction could take it");
    assert_eq!(
        alpha.draws, 2,
        "SABOTAGE: count recall rows instead of distinct runs and this reads \
         {rows}. A row is written once per carried key per step, so rows measure \
         how long a run went on for — one two-hundred-step run would outvote \
         fifty runs that each leaned on the note once, and the number would grow \
         with age rather than with usefulness. Two runs drew on it; two is the \
         answer.",
    );
    assert!(alpha.pinned, "and the pin is visible in the row");

    let delta = view
        .entries
        .iter()
        .find(|e| e.key == "delta")
        .expect("written by the second turn");
    assert_eq!(
        delta.draws, 1,
        "written during the second turn and carried into that same turn's next \
         step, so exactly one run has drawn on it — which is the distinction the \
         count is for: it is in the store as often as `alpha` is, and half as \
         proven",
    );

    assert!(
        view.entries.iter().all(|e| e.key != "beta"),
        "the evicted note is gone from the bucket — the trace row is the only \
         place it still exists, which is why the trace is read at all",
    );
    assert!(
        !view.draws_cut,
        "a handful of runs is nowhere near the scan ceiling",
    );

    // The view's trace follows the run it was given, not the newest one.
    assert!(
        view.trace.iter().any(|n| n.happened == Happened::Recalled),
        "the second run recalled too",
    );
}
