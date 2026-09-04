//! F12 — what a bundle costs on every request, and the four kinds that cost
//! nothing at all.
//!
//! **The free tier is the half an operator does not expect.** Of the seven kinds
//! io-harness lets a bundle contribute, only skills, agents and MCP servers reach
//! a provider; hooks, prompt templates, a declared binary and a policy layer are
//! never in any request. A bundle contributing only hooks is free forever, and an
//! operator trimming their window should be told that rather than left to guess
//! it — which is why `pluginview::COSTS` is an exhaustive table with a reason
//! beside every decision, and why the first test here reads io-harness's own
//! `Plugin::contributions` and fails **by name** when the two sets differ.
//!
//! **No figure on this surface is something a mask can shave.** io-harness sends
//! a byte-identical tool catalogue on a masked turn and an unmasked one, because
//! the tool array sits ahead of the provider's cache breakpoint. The lever these
//! numbers inform is switching the bundle off, which is the verb `/plugin`
//! already offers.

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use io_cli::context::{self, Request};
use io_cli::glyphs::ASCII;
use io_cli::pluginview::{self, Listed, View};
use io_harness::config::{Config, LOCAL_FILE};
use io_harness::{TaskContract, PLUGIN_FILE};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A loaded bundle with nothing on it but what each test overrides.
///
/// Built here rather than off a disk fixture, because every test below except the
/// last one is about the *drawing* rule for a set of contribution kinds, and a
/// manifest that really declared all seven would need a user-scope file and the
/// one environment lock this binary has.
fn listed(contributions: Vec<&'static str>) -> Listed {
    Listed {
        id: "rust-review".to_string(),
        enabled: true,
        description: None,
        version: None,
        root: PathBuf::from("/repo/bundles/rust-review"),
        contributions,
        skills: None,
        templates: None,
        agents: Vec::new(),
        servers: Vec::new(),
        hooks: Vec::new(),
        bin: Vec::new(),
        layers: Vec::new(),
    }
}

fn view_of(plugin: Listed, costs: BTreeMap<String, u64>) -> View {
    View {
        plugins: vec![plugin],
        refused: Vec::new(),
        adapters: None,
        costs,
    }
}

/// The detail of the row `id` draws, at a width where nothing has to give way.
fn detail_of(view: &View, id: &str) -> String {
    pluginview::rows(view, 400, &ASCII)
        .into_iter()
        .find(|row| row.label == id)
        .and_then(|row| row.detail)
        .unwrap_or_else(|| panic!("`{id}` draws a row with a detail"))
}

/// One bundle's row, for a bundle contributing `contributions` and costing
/// whatever `costs` says.
fn drawn(contributions: Vec<&'static str>, costs: &[(&str, u64)]) -> String {
    let costs = costs
        .iter()
        .map(|(id, tokens)| ((*id).to_string(), *tokens))
        .collect();
    detail_of(&view_of(listed(contributions), costs), "rust-review")
}

/// Every kind of contribution the locked io-harness can report, read out of the
/// `present` array in its own `Plugin::contributions`.
///
/// **Read from the dependency rather than listed here**, for the reason
/// `support::harness_error_variants` is: a hand-written list of seven cannot
/// notice an eighth, and an eighth kind classified by nothing would be drawn
/// through whichever arm happens to catch it.
fn harness_contribution_kinds() -> Vec<String> {
    let source = support::harness_source("plugin.rs");
    let body = source
        .split_once("pub fn contributions(&self) -> Vec<&'static str> {")
        .expect("io-harness declares `contributions` in src/plugin.rs")
        .1;
    let body = body
        .split_once("];")
        .expect("the `present` array in `contributions` is closed")
        .0;
    body.match_indices("(\"")
        .map(|(at, _)| {
            body[at + 2..]
                .split('"')
                .next()
                .expect("a quoted wire name")
                .to_string()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The table is exhaustive, and "free" is a decision
// ---------------------------------------------------------------------------

/// Sabotage: delete one line from `pluginview::COSTS` — or, equivalently, add an
/// eighth entry to io-harness's own `present` array. Every row still renders and
/// no other test in this repository notices, because an unclassified kind falls
/// through `under_reported`'s conservative arm and simply draws a floor. This
/// fails by name, in both directions.
#[test]
fn f12_every_contribution_kind_is_classified_costing_or_free() {
    let kinds = harness_contribution_kinds();
    assert!(
        !kinds.is_empty(),
        "the reader found no contribution kinds at all, so it is asserting nothing",
    );
    let named: Vec<&str> = pluginview::COSTS.iter().map(|(kind, _, _)| *kind).collect();

    for kind in &kinds {
        assert!(
            named.contains(&kind.as_str()),
            "io-harness lets a bundle contribute `{kind}` and `pluginview::COSTS` does not say \
             what it costs, so a bundle contributing it is drawn by whichever arm happens to \
             catch it: the table names {named:?}",
        );
    }
    for name in &named {
        assert!(
            kinds.iter().any(|kind| kind == name),
            "`pluginview::COSTS` classifies `{name}`, which io-harness no longer contributes: \
             it names {kinds:?}",
        );
    }

    // A decision with no reason beside it is a default wearing a table's
    // clothes. Every row of `COSTS` has to say why, or the next release cannot
    // check the claim.
    for (name, _, why) in pluginview::COSTS {
        assert!(
            why.len() > 40,
            "`{name}` is classified with no reason worth reading: {why:?}",
        );
    }
}

/// Sabotage: make `pluginview::wire` answer `Some(Wire::Free)` for a kind it does
/// not name — `.unwrap_or(Wire::Free)` at either call site. A kind an upgraded
/// io-harness added would then be drawn as costing nothing, permanently, and this
/// is the only test that fails.
#[test]
fn f12_a_kind_the_table_does_not_name_is_never_drawn_as_free() {
    // The eighth kind, arriving through a dependency bump: unclassifiable, and
    // therefore neither free nor fully counted.
    let detail = drawn(vec!["hooks", "resources"], &[("rust-review", 0)]);
    assert!(
        !detail.contains(pluginview::FREE),
        "a kind nothing classifies was drawn as free: {detail:?}",
    );
    assert!(
        detail.contains(pluginview::AT_LEAST),
        "a bundle carrying an uncountable kind draws its figure as if it were the whole of it: \
         {detail:?}",
    );
}

// ---------------------------------------------------------------------------
// The free tier
// ---------------------------------------------------------------------------

/// Sabotage: in `pluginview::cost_word`, ask `costs` before asking `free` — swap
/// the leading `if free(..)` below the `match`. A hooks-only bundle then draws
/// `0 tokens every request` on a session that has made a request and
/// `not yet on a request` on one that has not, and the permanent fact is never
/// stated at all.
#[test]
fn f12_a_bundle_contributing_only_hooks_is_stated_free() {
    // Both sessions: one that has made no request at all, and one that has made
    // a request this bundle contributed nothing to. The claim is the same in
    // both, because it is a claim about the table rather than about the wire.
    let no_request: &[(&str, u64)] = &[];
    let a_request: &[(&str, u64)] = &[("rust-review", 0)];
    for costs in [no_request, a_request] {
        let detail = drawn(vec!["hooks"], costs);
        assert!(
            detail.contains(pluginview::FREE),
            "a bundle contributing only hooks is not stated free: {detail:?}",
        );
        assert!(
            !detail.contains("tokens"),
            "a bundle that is never on a wire was given a token figure: {detail:?}",
        );
    }

    // All four free kinds together, because the claim is about the set and not
    // about hooks in particular.
    let detail = drawn(vec!["templates", "hooks", "bin", "policy"], &[]);
    assert!(detail.contains(pluginview::FREE), "{detail:?}");
}

/// Sabotage: state `free` from the figure rather than from the table — return
/// `FREE` from `cost_word` whenever `costs.get(&plugin.id) == Some(&0)`. This
/// bundle contributes an MCP server whose tools were not on the last request, and
/// it would then be promised as free forever.
#[test]
fn f12_a_measured_zero_is_not_the_promise_that_it_is_free() {
    let detail = drawn(vec!["mcp"], &[("rust-review", 0)]);
    assert!(
        !detail.contains(pluginview::FREE),
        "a server that has offered no tool yet was promised free forever: {detail:?}",
    );
    assert!(
        detail.contains("0 tokens every request"),
        "the measured zero is not drawn as the measurement it is: {detail:?}",
    );
}

/// Sabotage: read `costs.get(..).copied().unwrap_or(0)` in `cost_word`. A bundle
/// on a session that has made no request would then be reported as free, which is
/// the same lie `servers::NOT_ON_A_REQUEST` exists to stop `/mcp` telling.
#[test]
fn f12_a_bundle_no_request_has_carried_is_not_drawn_as_a_zero() {
    let detail = drawn(vec!["skills", "mcp"], &[]);
    assert!(
        detail.contains(io_cli::servers::NOT_ON_A_REQUEST),
        "a costing bundle with no request behind it draws: {detail:?}",
    );
    assert!(!detail.contains("0 tokens"), "{detail:?}");
    assert!(!detail.contains(pluginview::FREE), "{detail:?}");
}

/// Sabotage: drop the `Wire::EveryUncounted` arm — classify `agents` as
/// `Wire::Every`. The row then presents a figure that leaves out the agent roster
/// as though it were the whole cost, and only this test says so.
#[test]
fn f12_a_figure_that_leaves_out_the_agent_roster_says_so() {
    // `context::bundle_cost` deliberately does not count agents: io-harness
    // composes the roster into the system block with no marker io-cli can locate,
    // so the honest form is a floor rather than an invented number.
    let detail = drawn(vec!["agents"], &[("rust-review", 900)]);
    assert!(
        detail.contains(pluginview::AT_LEAST),
        "a bundle whose agents are uncounted draws its floor as a total: {detail:?}",
    );
    assert!(detail.contains("900 tokens every request"), "{detail:?}");

    // A bundle with nothing uncounted in it says no such thing.
    let plain = drawn(vec!["skills"], &[("rust-review", 900)]);
    assert!(
        !plain.contains(pluginview::AT_LEAST),
        "a fully counted bundle hedges a figure that is exact: {plain:?}",
    );
}

/// Sabotage: draw the cost clause outside the `plugin.enabled` arm in
/// `pluginview::rows`. A bundle contributing none of itself to this session then
/// carries a figure or a promise about it, and this fails.
#[test]
fn f12_a_switched_off_bundle_states_no_cost_at_all() {
    let mut off = listed(vec!["hooks", "mcp"]);
    off.enabled = false;
    let costs = BTreeMap::from([("rust-review".to_string(), 4_200_u64)]);
    let detail = detail_of(&view_of(off, costs), "rust-review");

    assert!(detail.starts_with("switched off"), "{detail:?}");
    assert!(!detail.contains("tokens"), "{detail:?}");
    assert!(!detail.contains(pluginview::FREE), "{detail:?}");
    assert!(
        !detail.contains(io_cli::servers::NOT_ON_A_REQUEST),
        "{detail:?}",
    );
}

// ---------------------------------------------------------------------------
// The two surfaces cannot disagree
// ---------------------------------------------------------------------------

/// The head io-harness's `with_skill_catalog` writes, and the namespaced lines
/// under it. The `__` is io-harness's own namespace separator, applied at load.
fn system_with_catalogue() -> String {
    "You are a coding agent working in a repository.\n\nSkills available to you: a skill is a \
     folder of instructions you may read when it applies.\n- rust-review__reviewer: read a Rust \
     diff and report the defects in it, with the file and line of each one\n- \
     rust-review__formatter: run the formatter and say what it changed\n- docs-bot__writer: \
     write the documentation for a module\n- housekeeping: a skill the operator put in their \
     own directory, belonging to no bundle\n"
        .to_string()
}

/// A bundle directory contributing a skills directory and nothing else.
fn bundle(root: &Path, at: &str, name: &str) {
    let dir = root.join(at);
    std::fs::create_dir_all(dir.join("skills")).expect("the skills directory");
    std::fs::write(
        dir.join(PLUGIN_FILE),
        format!("name = \"{name}\"\nskills = \"skills\"\n"),
    )
    .expect("the manifest");
}

/// Sabotage: in `pluginview::cost_word`, look the figure up by anything but the
/// bundle's own id — `costs.values().sum()`, or the first entry of the map. The
/// two bundles here cost different amounts, so any such change draws one bundle's
/// charge on the other's row and this fails on both rows.
///
/// This is the test that stops `/plugin` and `/context` from being able to
/// disagree: the figure drawn is asserted to be `context::bundle_cost`'s own
/// answer for the same request snapshot, through `servers::per_request`, which is
/// the single spelling both surfaces use.
#[test]
fn f12_the_figure_a_row_draws_is_the_one_bundle_cost_computed() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = dir.path();
    bundle(root, "review", "rust-review");
    bundle(root, "docs", "docs-bot");
    std::fs::write(
        root.join(LOCAL_FILE),
        "[[plugin]]\npath = \"review\"\n\n[[plugin]]\npath = \"docs\"\n",
    )
    .expect("the configuration");

    // `Config::discover` reads a user-scope file where one exists, and the
    // environment is process-global — the same lock every discovery in
    // `tests/plugins.rs` is taken under.
    let config = {
        let _guard = support::env_lock();
        Config::discover(root).expect("the fixture loads")
    };
    let plugins = config.plugins();
    assert!(
        plugins.dropped().is_empty(),
        "the fixture bundles did not load: {:?}",
        plugins.dropped(),
    );

    let contract = plugins.apply_to(TaskContract::workspace("ship it", root));
    let seen = Request {
        system: system_with_catalogue(),
        ..Request::default()
    };
    let costs = context::bundle_cost(&seen, &contract);

    // Two bundles, two different charges, neither of them zero — which is what
    // makes a wrong-key lookup visible at all.
    let review = *costs.get("rust-review").expect("a row for `rust-review`");
    let docs = *costs.get("docs-bot").expect("a row for `docs-bot`");
    assert!(review > 0 && docs > 0, "{costs:?}");
    assert_ne!(
        review, docs,
        "the fixture must charge the two bundles differently or a wrong-key lookup survives it",
    );

    let view = {
        let _guard = support::env_lock();
        pluginview::view(&plugins)
    }
    .with_costs(costs.clone());

    for (id, tokens) in [("rust-review", review), ("docs-bot", docs)] {
        let detail = detail_of(&view, id);
        assert!(
            detail.contains(&io_cli::servers::per_request(tokens)),
            "`/plugin` draws {detail:?} for `{id}`, and `context::bundle_cost` says {tokens}",
        );
    }
}
