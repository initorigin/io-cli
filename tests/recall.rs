//! F6, F7 and F8 — the agent's own durable memory, read back and steered.
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
//! **F7 is the other half: the operator's two levers.** The read half of
//! `src/recall.rs` writes nothing at all; `recall::pin`, `recall::forget` and
//! `recall::unforget` are the only writes, and they exist because a store the
//! agent manages alone is a store an operator cannot correct. The one fixture
//! below that calls `Store::memory_pin` directly predates them and is left
//! alone deliberately: it stages a *refused write*, which is io-harness's own
//! act, and it must keep passing whatever io-cli's wrapper does.
//!
//! **F7's sabotage is invisible from the entry list.** Forget through
//! `Store::memory_delete` instead of `Store::memory_forget` and the entry is
//! gone either way — so the tests below assert the two things only
//! `memory_forget` leaves behind: a restore point (proved by really rewinding
//! it and watching the entry come back) and the key's recall rows removed.
//!
//! **No clock.** Every time asserted on is the string the store wrote. Nothing
//! here sleeps or measures, per `tests/timing.rs`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use io_cli::recall::{self, Forgotten, Happened, Pinned, Scope};
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

    let wrote = remember(
        &store,
        &bucket,
        "test-command",
        "cargo test --lib",
        MemoryKind::Fact,
    );
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
    let view = recall::view(
        &store,
        &link,
        &contract(&link, MemoryLimits::default()),
        None,
    )
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
    let gone = tempfile::tempdir()
        .expect("a parent")
        .path()
        .join("deleted");
    assert!(!gone.exists(), "the fixture is a path that is not there");

    let bucket = recall::workspace_key(&gone);
    assert_eq!(
        bucket,
        gone.to_string_lossy(),
        "unresolvable falls back to the path as given, byte for byte",
    );

    remember(
        &store,
        &bucket,
        "why",
        "the workspace was moved",
        MemoryKind::Fact,
    );
    let view = recall::view(
        &store,
        &gone,
        &contract(&gone, MemoryLimits::default()),
        None,
    )
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

    let view = recall::view(
        &store,
        workspace.path(),
        &contract(workspace.path(), limits),
        None,
    )
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
            .map(|batch| {
                batch
                    .into_iter()
                    .map(|(k, v)| remember_call(k, v))
                    .collect()
            })
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
    remember(
        &store,
        &bucket,
        "alpha",
        "the operators value",
        MemoryKind::Decision,
    );
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
    let view =
        recall::view(&store, workspace.path(), &task, Some(second.run_id)).expect("the view reads");

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

    // --- 0.30.0 F6: the same three, through `view`, for a real run id --------
    //
    // Not a second fixture. Producing a genuine eviction, a genuine refusal and a
    // genuine recall costs two scripted turns of io-harness's own run loop, and
    // this test has already paid for them — so the criterion that `View::trace`
    // carries all three for a run that produced them is asserted here, against the
    // run that produced them.
    let ran =
        recall::view(&store, workspace.path(), &task, Some(first.run_id)).expect("the view reads");
    assert!(
        !ran.trace.is_empty(),
        "SABOTAGE: this is the arm that reddens when the caller goes back to \
         passing `None`, which is what io-cli shipped through 0.29.0 — an empty \
         trace beside a store that had been evicting, refusing and recalling all \
         along",
    );
    for expected in [Happened::Evicted, Happened::Refused, Happened::Recalled] {
        assert!(
            ran.trace.iter().any(|note| note.happened == expected),
            "the run {expected:?} something and the view does not say so; \
             `View::trace` is the only place any of the three exists, because \
             io-harness emits no `EventKind` for them",
        );
    }
    assert!(
        details(&ran.trace, Happened::Evicted)[0].contains("beta"),
        "io-harness's own sentence rides through the view and not only through \
         `trace` — a page that said a note had been dropped without naming which \
         one would be telling an operator to go and look for it",
    );

    // The shipped behaviour, stated rather than assumed: `None` really is empty,
    // so the assertion above is a test of the run id reaching the call and not of
    // the fixture being lively.
    assert!(
        recall::view(&store, workspace.path(), &task, None)
            .expect("the view reads")
            .trace
            .is_empty(),
        "`None` is a view with no trace, deliberately — which is exactly why the \
         driver passing it made the whole trace half of this module unreachable",
    );

    assert_eq!(
        [Happened::Evicted, Happened::Refused, Happened::Recalled].map(Happened::label),
        ["evicted", "refused", "recalled"],
        "each has a word, and it is `refused` rather than `failed`: the write did \
         not fail, io-harness declined it because an operator had pinned the entry \
         — which is the pin working. A reader told it failed goes looking for a \
         broken store.",
    );
}

// ---------------------------------------------------------------------------
// 0.30.0 F5 and F6 — the driver gates
//
// **The `f6_` and `f8_` tests above are 0.29.0's criteria and are not these.**
// Criteria are numbered per release, so the two tests below carry the release in
// their names and nothing else in this file does.
//
// Both criteria assert something about `src/main.rs`, which nothing under
// `tests/` links: `[[bin]] name = "io"` is a separate compilation and a test
// binary cannot call into it. The established answer in this repository is a
// driver-text gate — see `tests/contract.rs:303`, `:336`, `:369` and
// `tests/context_share.rs:526` — and it is weak in exactly one way, which is why
// the comments come off first.
// ---------------------------------------------------------------------------

/// `src/main.rs`, with every comment taken off before anything is matched.
///
/// **Copied from `tests/structure.rs:137` rather than shared, because a test
/// binary cannot import another's helper**, and the reason it exists is worth
/// repeating: 0.14.0 shipped a gate that asserted the driver contained
/// `EventKind::Dialed` and was satisfied by a *comment* naming it — a green test
/// over code that had none of it. Every patch these two gates are written against
/// carries a paragraph of prose naming the very calls they assert on, so without
/// the stripping they would pass on the prose alone.
fn driver_without_comments() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let text = std::fs::read_to_string(path).expect("the driver is readable");
    text.lines()
        .map(|line| match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **0.30.0 F5 — a forgotten agent memory is restored.**
///
/// `recall::forget` has returned a restore id since 0.29.0 and `recall::unforget`
/// has been able to spend one for just as long. Nothing in `src/` ever did:
/// `commands::forgotten_said` formatted the id into a sentence — *run 42 holds the
/// way back* — and the id then went out of scope. The operator was told there was
/// a way back and given no way to take it, which is worse than not offering one.
///
/// Counted rather than `contains`, which is this suite's recorded vacuous-gate
/// shape: one call satisfies a `contains` forever, so it could never catch the
/// site going missing again.
///
/// Sabotage: drop the call site and keep `tests/recall.rs`'s
/// `f7_forgetting_removes_the_entry_and_leaves_a_way_back`, which drives
/// `recall::unforget` for real and would stay green over a function no keystroke
/// reaches — which is precisely the tested-but-uncalled shape this product has now
/// shipped in more than one release.
#[test]
fn v0_30_0_f5_the_driver_spends_the_restore_id_rather_than_only_printing_it() {
    let text = driver_without_comments();
    // **Whitespace-collapsed before the pattern match, and that is not a
    // weakening.** `rustfmt` decides where a pattern breaks across lines from the
    // indent it happens to sit at, so a gate matching the source bytes of a
    // destructuring asserts the formatter's arithmetic as much as the code's
    // meaning — it goes red when somebody wraps an unrelated block one level
    // deeper. What is being asserted is that the driver destructures the id out of
    // `forget`'s own answer, and that survives collapsing runs of whitespace to
    // one space.
    let flat = text.split_whitespace().collect::<Vec<&str>>().join(" ");

    assert_eq!(
        text.matches("io_cli::recall::unforget(").count(),
        1,
        "exactly one keystroke puts a withdrawn note back, and it is in code \
         rather than in a sentence about code",
    );
    assert!(
        flat.contains("io_cli::recall::Forgotten::Removed { restore, } = outcome")
            || flat.contains("io_cli::recall::Forgotten::Removed { restore } = outcome"),
        "the id is the one `recall::forget` just answered with and never a run id \
         found some other way — `recall::forget`'s own note gives the two \
         plausible wrong ids and what each of them restores instead",
    );
    assert!(
        text.contains("Pick::Unforget {"),
        "and it is spent through a confirmation rather than immediately: the \
         operator asked to forget the note, so putting it back is a second \
         question and `store::acts` decides it the way it decides every other \
         one — row 0 declines",
    );
}

/// **0.30.0 F6 — `/memory` shows what the store recorded about the run.**
///
/// The library side has been right since 0.29.0 and unreachable for just as long:
/// `recall::view`'s `run` parameter is an `Option<i64>` and the one caller in
/// `src/main.rs` passed `None`, so `View::trace` was empty in production, and
/// `recall::trace`, `Happened` and `Noted` were reachable from `tests/recall.rs`
/// and from nowhere an operator could stand.
///
/// **The `None` is the shipped behaviour, so this gate is red before the driver is
/// wired and green after** — it is not a description of what the driver already
/// does. The first assertion is the criterion's named sabotage written out: put
/// `None` back and it fails on its own, without the other two.
#[test]
fn v0_30_0_f6_the_driver_reads_the_trace_of_the_run_the_session_just_ran() {
    let text = driver_without_comments();

    assert!(
        !text.contains("&opening, None)"),
        "SABOTAGE: `recall::view(.., &opening, None)` is what shipped, and it is \
         the whole defect — every eviction, refusal and recall the store holds is \
         read by a function the operator can never reach",
    );
    assert!(
        text.contains("io_cli::recall::view(&store, &root, &opening, ran)"),
        "the run id reaches the view, and it is the session's own last turn: \
         `last_run` is the same anchor `/undo`, `/export` and the gate report \
         already take, so `/memory` cannot be looking at a different run from the \
         rest of the surface",
    );
    assert!(
        text.contains("last_run(&session, &store).map(|turn| turn.run_id)"),
        "read from the session's transcript rather than carried in a variable \
         that a resumed or forked session would have left stale",
    );
    assert!(
        text.contains("io_cli::commands::trace_notes("),
        "and the rows are drawn — a `View::trace` that is populated and never \
         rendered is the same defect one layer up, and the sentence lives in \
         `io_cli::commands` because nothing under `tests/` can drive one written \
         in the driver",
    );
}

// ---------------------------------------------------------------------------
// F7 — the operator pins and forgets, and forgetting leaves a way back
// ---------------------------------------------------------------------------

/// The value of one key as the store holds it, or a panic naming the key.
fn stored(store: &Store, bucket: &str, key: &str) -> io_harness::MemoryEntry {
    store
        .memory_get(bucket, key)
        .expect("the store reads")
        .unwrap_or_else(|| panic!("`{key}` should still be in `{bucket}`"))
}

#[test]
fn f7_pinning_and_unpinning_act_on_the_bucket_the_scope_names() {
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    // The same key in both buckets. This is the only fixture that can catch a
    // wrapper which takes the scope and then ignores it — with two different
    // keys, pinning the wrong bucket is a silent no-op that every assertion
    // about the right one still passes.
    remember(&store, &bucket, "editor", "helix", MemoryKind::Decision);
    remember(
        &store,
        GLOBAL_MEMORY_WORKSPACE,
        "editor",
        "whatever is on PATH",
        MemoryKind::Decision,
    );

    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Global, "editor", true)
            .expect("the pin lands"),
        Pinned::Set,
    );
    assert!(
        stored(&store, GLOBAL_MEMORY_WORKSPACE, "editor").pinned,
        "the global note is the one that was pinned",
    );
    assert!(
        !stored(&store, &bucket, "editor").pinned,
        "SABOTAGE: route every write to the workspace bucket and this is the row \
         that moved. An operator pinning what the agent believes EVERYWHERE \
         would instead pin a same-named note in whichever checkout they happened \
         to be standing in, and the global belief would keep being overwritten \
         by every run.",
    );

    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Global, "editor", false)
            .expect("the unpin lands"),
        Pinned::Set,
    );
    assert!(
        !stored(&store, GLOBAL_MEMORY_WORKSPACE, "editor").pinned,
        "unpinning is the same lever the other way, and it is the prerequisite \
         for withdrawing a pinned note",
    );

    // "Nothing to pin" is an outcome of its own. io-harness will not invent an
    // entry to carry the pin (`src/state/memory.rs:556-558`), so a `bool` read as
    // "did it work" would leave a surface showing a pin the store does not hold.
    assert_eq!(
        recall::pin(
            &store,
            workspace.path(),
            Scope::Workspace,
            "never-written",
            true
        )
        .expect("the call succeeds even though there is nothing to pin"),
        Pinned::NoEntry,
    );
    assert!(
        store
            .memory_get(&bucket, "never-written")
            .expect("the store reads")
            .is_none(),
        "and nothing was conjured to hang the pin on",
    );
}

#[test]
fn f7_a_pinned_entry_survives_an_overwrite_and_an_unpinned_one_does_not() {
    // This is what a pin is *for*: a correction a person made must not be
    // silently replaced by the next thing the agent decides. io-harness enforces
    // it in the write itself — `WHERE memory.pinned IS NOT 1`,
    // `src/state/memory.rs:430`.
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    remember(
        &store,
        &bucket,
        "owner",
        "the platform team",
        MemoryKind::Decision,
    );
    remember(&store, &bucket, "retries", "three", MemoryKind::Fact);
    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Workspace, "owner", true)
            .expect("the operator pins the correction"),
        Pinned::Set,
    );

    // A later run rewrites both keys. Not through `remember`, which asserts the
    // write was not refused — a refusal is exactly what half of this is about.
    let run = store
        .start_run("a later run knows better", &bucket)
        .expect("a run to attribute the writes to");
    let onto_pinned = store
        .memory_write_with(
            &bucket,
            "owner",
            "whoever ran me last",
            run,
            1,
            MemoryKind::Decision,
            MemoryLimits::default(),
        )
        .expect("the write is attempted");
    let onto_plain = store
        .memory_write_with(
            &bucket,
            "retries",
            "nine",
            run,
            1,
            MemoryKind::Fact,
            MemoryLimits::default(),
        )
        .expect("the write lands");

    assert!(
        onto_pinned.refused,
        "the run was refused, and told so — an agent that believes it corrected \
         something and did not will act on the correction it thinks it made",
    );
    assert!(!onto_plain.refused, "nothing protects the unpinned note");

    assert_eq!(
        stored(&store, &bucket, "owner").value,
        "the platform team",
        "SABOTAGE: never pin, and the operator's answer is gone one turn later \
         with nothing anywhere saying so. The pin is the only lever io-cli gives \
         over a store the agent otherwise manages alone.",
    );
    assert_eq!(
        stored(&store, &bucket, "retries").value,
        "nine",
        "and the control moved, so the fixture is testing the pin rather than a \
         store that refuses every write",
    );

    let view = recall::view(
        &store,
        workspace.path(),
        &contract(workspace.path(), MemoryLimits::default()),
        None,
    )
    .expect("the view reads");
    assert!(
        view.entries
            .iter()
            .find(|e| e.key == "owner")
            .expect("still there")
            .pinned,
        "and the pin is visible to a reader, so the survivor can be told from a \
         note that simply has not been rewritten yet",
    );
}

#[test]
fn f7_a_pinned_entry_survives_eviction_at_the_cap_and_an_unpinned_one_does_not() {
    // The other half of what a pin buys, and the half that makes the cap
    // survivable: `src/state/memory.rs:736-739` skips a pinned entry when it
    // drops notes to hold the caps. It still *counts* towards them, so pinning
    // does not quietly raise the ceiling — it makes writes fail loudly instead.
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    // `plain` first, so it is also the first eviction candidate: nothing has
    // recalled either, and the candidate order falls through to `created_at ASC,
    // id ASC` (`src/state/memory.rs:653-660`). The pin is therefore the only
    // thing that can decide which of the two goes.
    remember(&store, &bucket, "plain", "an older note", MemoryKind::Fact);
    remember(
        &store,
        &bucket,
        "pinned",
        "the operators correction",
        MemoryKind::Decision,
    );
    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Workspace, "pinned", true)
            .expect("the operator pins it"),
        Pinned::Set,
    );

    let tight = MemoryLimits {
        max_entries: 2,
        ..MemoryLimits::default()
    };
    let run = store
        .start_run("one note too many", &bucket)
        .expect("a run to attribute the write to");
    let wrote = store
        .memory_write_with(
            &bucket,
            "newcomer",
            "learned this turn",
            run,
            1,
            MemoryKind::Fact,
            tight,
        )
        .expect("the write lands");

    assert!(!wrote.refused, "`newcomer` is new, so nothing refuses it");
    assert_eq!(
        wrote.evicted,
        ["plain"],
        "SABOTAGE: without the pin the store drops the oldest, least-proven note \
         and the operator's correction is the oldest note in this bucket — so an \
         unpinned correction is the FIRST thing a cap throws away",
    );

    let keys: Vec<String> = store
        .memory_list(&bucket)
        .expect("the bucket reads")
        .into_iter()
        .map(|e| e.key)
        .collect();
    assert_eq!(keys.len(), 2, "the cap of two holds");
    assert!(
        keys.contains(&"pinned".to_string()),
        "the pinned note stands"
    );
    assert!(
        keys.contains(&"newcomer".to_string()),
        "and the new one landed"
    );
    assert!(
        !keys.contains(&"plain".to_string()),
        "while the unpinned one went, which is the comparison that makes this a \
         test of the pin and not of the cap",
    );
}

#[tokio::test]
async fn f7_forgetting_removes_the_entry_and_leaves_a_way_back() {
    // The criterion this whole file's F7 half exists for. Both implementations
    // — `Store::memory_forget` and `Store::memory_delete` — leave the entry
    // gone, so *the entry being gone proves nothing*. What only the first one
    // leaves behind is a restore point and a bucket with the key's recall rows
    // removed, and those are what is asserted.
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    // Written before the turn so the turn carries it into its prompt and the key
    // accrues real recall rows. `Store::record_memory_recall` is `pub(crate)`
    // (`src/state/memory.rs:786`), so a real turn is the only way to make one —
    // which is why this reuses the `Remembering` harness above rather than
    // hand-writing a row io-harness would never have written.
    remember(
        &store,
        &bucket,
        "doomed",
        "the flaky test is in parser rs",
        MemoryKind::Fact,
    );

    let task = contract(workspace.path(), MemoryLimits::default());
    let mut session = Session::open(&store, workspace.path()).expect("a session");
    let watching = Watching::default();
    let turn = session
        .turn_bounded_observed(
            &task,
            &Remembering::with(vec![vec![("unrelated", "kept")]]),
            &store,
            &Policy::permissive(),
            &ApproveAll,
            &watching,
        )
        .await
        .expect("a scripted turn cannot fail");

    let doomed_recalls = |run: i64| {
        store
            .memory_recalls(run)
            .expect("the recalls read")
            .into_iter()
            .filter(|r| r.key == "doomed")
            .count()
    };
    assert!(
        doomed_recalls(turn.run_id) > 0,
        "the fixture is only meaningful if the turn really drew on `doomed` — \
         otherwise the cleared-rows assertion below would pass against anything",
    );

    let Forgotten::Removed { restore } =
        recall::forget(&store, workspace.path(), Scope::Workspace, "doomed")
            .expect("the withdrawal runs")
    else {
        panic!("an unpinned entry that is there is removed");
    };

    assert!(
        store
            .memory_get(&bucket, "doomed")
            .expect("the store reads")
            .is_none(),
        "gone from the bucket — which is ALSO true under the sabotage, and is \
         exactly why the next two assertions exist",
    );

    assert_eq!(
        doomed_recalls(turn.run_id),
        0,
        "SABOTAGE: `Store::memory_delete` is a bare `DELETE FROM memory` \
         (io-harness `src/state/memory.rs:863-869`) and leaves every recall row \
         standing. The evidence a withdrawn note accrued would keep voting in \
         the eviction order (`src/state/memory.rs:653-660`) on behalf of a note \
         that no longer exists, and a draw count read back for a later note of \
         the same key would carry the dead one's history.",
    );

    // The way back, driven for real — the same `rewind_run` io-cli's undo path
    // already calls (`src/rewind.rs:181`).
    assert_eq!(
        recall::unforget(&store, workspace.path(), restore).expect("the rewind runs"),
        ["doomed"],
        "SABOTAGE: `memory_delete` writes no `memory_snapshots` row (io-harness \
         `src/state/memory.rs:838-848`), so `rewind_run` finds nothing to put \
         back (`src/run.rs:749`) and this is empty. The operator who withdrew \
         the wrong note would have no way back at all — and would find that out \
         at the one moment they wanted it.",
    );

    let back = stored(&store, &bucket, "doomed");
    assert_eq!(
        back.value, "the flaky test is in parser rs",
        "byte for byte, from the restore point taken BEFORE the removal \
         (io-harness `src/state/memory.rs:838-848`)",
    );
    assert!(
        !back.pinned,
        "restored unpinned, which is not a guess: a pinned entry cannot be \
         forgotten in the first place",
    );

    // And the undoing is in the trace, where io-cli's own rewind reporting reads
    // it (`src/rewind.rs:219-220`).
    let record = &store.rewinds(restore).expect("the rewinds read")[0];
    assert_eq!(record.memory_restored, ["doomed"]);
    assert!(
        record.memory_removed.is_empty(),
        "the entry was withdrawn by the operator, not created by that run — the \
         two are opposite directions of the same rewind",
    );
    assert_eq!(
        record.undid_step, None,
        "a whole-run rewind rather than a step revert (io-harness \
         `src/state.rs:3436-3440`)",
    );
}

#[test]
fn f7_a_pinned_entry_is_refused_and_an_unknown_key_is_absent() {
    let (_keep, store) = store();
    let workspace = tempfile::tempdir().expect("a workspace");
    let bucket = recall::workspace_key(workspace.path());

    remember(
        &store,
        &bucket,
        "owner",
        "the platform team",
        MemoryKind::Decision,
    );
    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Workspace, "owner", true)
            .expect("the operator pins it"),
        Pinned::Set,
    );

    assert_eq!(
        recall::forget(&store, workspace.path(), Scope::Workspace, "owner")
            .expect("the call succeeds; the withdrawal does not"),
        Forgotten::Refused,
        "SABOTAGE: `MemoryForget::Pinned` means the harness REFUSED \
         (io-harness `src/state/memory.rs:829-831`), and it is the one outcome \
         that looks like the other two if it is folded into a `bool`. Reported \
         as success it tells an operator their note is gone while it stays in \
         the store and is carried into every later prompt — the same failure the \
         pinned flag exists to prevent one level down.",
    );
    assert_eq!(
        stored(&store, &bucket, "owner").value,
        "the platform team",
        "and the entry stands, untouched",
    );

    // Unpinning first is the way through, which is what makes naming the reason
    // worth doing: the operator learns the order of the two acts.
    assert_eq!(
        recall::pin(&store, workspace.path(), Scope::Workspace, "owner", false)
            .expect("the unpin lands"),
        Pinned::Set,
    );
    assert!(
        matches!(
            recall::forget(&store, workspace.path(), Scope::Workspace, "owner")
                .expect("the withdrawal runs"),
            Forgotten::Removed { .. },
        ),
        "unpinned, the same call withdraws it",
    );

    assert_eq!(
        recall::forget(&store, workspace.path(), Scope::Workspace, "owner")
            .expect("saying it twice is not an error"),
        Forgotten::Absent,
        "a key that is not there is its own outcome — not an error, and not a \
         second removal either",
    );
    assert_eq!(
        recall::forget(&store, workspace.path(), Scope::Workspace, "never-written")
            .expect("nor is a key that was never there"),
        Forgotten::Absent,
    );
}
