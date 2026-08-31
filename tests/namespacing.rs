//! F7 — the rendering gate. No operator-facing surface draws a name still
//! carrying io-harness's own namespace separator.
//!
//! `io_harness::NAMESPACE` is `__`. It is the join io-harness builds a bundle's
//! contributions with — `ultraship__brainstorm` — and it is load-bearing on the
//! wire: the system prompt, the tool dispatch and every event `target` carry it.
//! `crate::naming::display` translates it to `:` at the moment a name is drawn,
//! and `crate::naming::wire` translates back for a name an operator typed. This
//! file walks the **drawn output** of the six operator-facing surfaces and fails
//! when one of them draws the wire spelling.
//!
//! # What is asserted, and why it is not "no `__` anywhere"
//!
//! **Definition. An untranslated namespaced name is the literal string
//! `<bundle id><NAMESPACE><contribution>`, for a bundle id the fixture
//! declared.** That, and only that, is the leak. A blanket "the output contains
//! no `__`" would be a *different and wrong* property, and 0.32.0 shipped the
//! defect that proves it — twice over:
//!
//! - **A path is not a name.** `src/__init__.py`, `__pycache__`, `__tests__`,
//!   `__mocks__`, `__snapshots__` are ordinary files and directories, and they
//!   reach the transcript, the skill pane, the plugin pane and the status page as
//!   themselves. 0.32.0 translated every `read_skill` target and drew
//!   `read src/:init__.py` — a path that does not exist, on the one surface an
//!   operator checks to see what the agent touched.
//! - **Only the FIRST separator is the join.** A plugin id is
//!   `[a-z0-9][a-z0-9-]{0,31}` (io-harness's `check_id`), so it can never contain
//!   `__` and the first occurrence is always the join. Everything after it is the
//!   contribution's own name and is kept byte for byte:
//!   `bundle__deep__nested` is drawn `bundle:deep__nested`, which contains `__`
//!   and is correct.
//!
//! So each surface is fed both halves and both are asserted:
//!
//! 1. a contributed name is drawn in its translated spelling and **never** in its
//!    wire spelling — this is the half that fails when a surface stops
//!    translating;
//! 2. a legitimate `__` — a file path, and a contribution whose own name carries
//!    the separator — survives **intact** — this is the half that fails when
//!    somebody repairs (1) by translating everything, which is the actual bug
//!    0.32.0 shipped and the reason a `!contains("__")` gate must not be written
//!    here.
//!
//! The `bundle__deep__nested` fixture carries both halves in one assertion: the
//! drawn form is compared for equality against `bundle:deep__nested`, which is
//! false for the untranslated `bundle__deep__nested` and equally false for the
//! over-translated `bundle:deep:nested`.
//!
//! **The wire spelling is built out of `io_harness::NAMESPACE` and never out of
//! `crate::naming`**, so the gate does not measure the translation against
//! itself — the vacuity `AGENTS.md` records this repository having shipped three
//! times.
//!
//! Every assertion is over a value a surface *returned* — a `Line`'s spans
//! concatenated, a `Row`'s label and detail, a `String` a live row composed.
//! Nothing here reads source text: a source-text gate is satisfiable by a
//! comment.
//!
//! **The sabotage arm for every surface is written out at the foot of this
//! file**, one concrete source edit each — file, function and the exact change.
//! It is a `//` block rather than `//!` because an inner doc comment may only
//! precede the first item, and the arms belong beside the assertions they kill.
//!
//! `tests/marketplace.rs`'s F6 and `src/pluginview.rs`'s
//! `the_detail_view_groups_what_a_bundle_contributed` already assert this
//! property on two surfaces. This file extends their shape to the other four and
//! adds the over-translation half none of them had.

use std::path::{Path, PathBuf};
use std::time::Duration;

use io_cli::events::Events;
use io_cli::glyphs::{ASCII, UNICODE};
use io_cli::picker::Picker;
use io_cli::status::Status;
use io_cli::theme::DARK;
use io_cli::{commands, marketplace, pluginview, skillview, status};
use io_harness::config::Scope;
use io_harness::{EventKind, RunEvent, Templates, MCP_TOOL_PREFIX, NAMESPACE};

// --- what a name is, in both spellings ----------------------------------------

/// One name a bundle contributed, in the two spellings that matter.
///
/// The wire form is assembled from `io_harness::NAMESPACE` here rather than
/// asked of `crate::naming::wire`, and the drawn form is written out rather than
/// asked of `crate::naming::display`: a gate that built both operands out of the
/// code under test would pass for any translation at all, including none.
struct Name {
    /// The bundle's id. Never contains `__` — io-harness's `check_id` forbids it,
    /// which is what makes "the first occurrence is the join" true.
    bundle: &'static str,
    /// The contribution's own name, which **may** contain `__` and is kept.
    own: &'static str,
}

impl Name {
    /// `<bundle>__<own>` — what io-harness wrote, and what must never be drawn.
    fn wire(&self) -> String {
        format!("{}{NAMESPACE}{}", self.bundle, self.own)
    }

    /// `<bundle>:<own>` — what the operator must read, with `own` untouched.
    fn drawn(&self) -> String {
        format!("{}:{}", self.bundle, self.own)
    }
}

/// A bundle skill with an ordinary name.
const BRAINSTORM: Name = Name {
    bundle: "ultraship",
    own: "brainstorm",
};

/// The one from the live session that opened this release.
const USING: Name = Name {
    bundle: "ultraship",
    own: "using-ultraship",
};

/// **The over-translation fixture.** Its own name carries the separator, so only
/// the first occurrence may be rewritten. `bundle__deep__nested` drawn as
/// `bundle:deep:nested` is a name nothing resolves.
const DEEP: Name = Name {
    bundle: "bundle",
    own: "deep__nested",
};

/// An MCP tool, after `MCP_TOOL_PREFIX` has been stripped. The prefix itself ends
/// with the separator, so it is removed *before* translating — translating the
/// whole name splits at the prefix's own join and yields `mcp:github__create_issue`.
const MCP_TOOL: Name = Name {
    bundle: "github",
    own: "create_issue",
};

/// A path with a legitimate double underscore. The Python case 0.32.0 paid for.
const PY_PATH: &str = "references/__init__.py";

/// A directory with one, used as an ancestor of a bundle root so that a real
/// io-harness read produces a drawn path carrying `__`.
const JS_DIR: &str = "__snapshots__";

// --- the two halves, as assertions --------------------------------------------

/// The translated spelling is drawn and the wire spelling is not.
fn translated(drawn: &str, name: &Name, surface: &str) {
    assert!(
        drawn.contains(&name.drawn()),
        "{surface}: {} is not drawn as {}, which is the name the operator reads \
         everywhere else: {drawn:?}",
        name.wire(),
        name.drawn(),
    );
    assert!(
        !drawn.contains(&name.wire()),
        "{surface}: io-harness's own separator reached a person in {}: {drawn:?}",
        name.wire(),
    );
}

/// A double underscore that is not a join survives byte for byte.
fn survives(drawn: &str, literal: &str, surface: &str) {
    assert!(
        drawn.contains(literal),
        "{surface}: {literal:?} is a path, not a namespaced name, and translating \
         it draws something that does not exist: {drawn:?}",
    );
}

// --- transcript ----------------------------------------------------------------

fn flatten(lines: Vec<ratatui::text::Line<'static>>) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect()
}

fn call(name: &str, target: &str) -> RunEvent {
    RunEvent::new(
        1,
        1,
        EventKind::ToolCall {
            name: name.into(),
            target: target.into(),
        },
    )
}

fn step(decision: &str, tool_call: &str) -> RunEvent {
    RunEvent::new(
        1,
        1,
        EventKind::Step {
            decision: decision.into(),
            tool_call: tool_call.into(),
            tokens: 12,
            changed: false,
        },
    )
}

/// The cell one announced-then-finished tool call commits.
fn cell(tool: &str, target: &str, decision: &str) -> String {
    let mut events = Events::new(DARK);
    events.event(&call(tool, target), Duration::ZERO);
    flatten(events.event(&step(decision, tool), Duration::from_millis(400)))
}

/// **The transcript.** `Events::event` is the seam; the drawn value is the
/// `Vec<Line>` it returns, and the live row `Events::live` composes from the same
/// pending call.
///
/// Four sites in one surface: the committed `read_skill` cell (`skill_and_file`),
/// the live row that has to guess from the target's shape (`names_a_skill`), the
/// MCP cell whose prefix is stripped before the translation, and the `Started`
/// row that draws what the operator typed rather than the wire prompt.
#[test]
fn f7_the_transcript_translates_a_contributed_name_and_leaves_a_path_alone() {
    // The committed cell. io-harness's decision sentence is `read skill <label>`,
    // and the label is the skill's name plus, when the call carried one, the
    // companion file's relative path.
    let loaded = cell(
        "read_skill",
        &USING.wire(),
        &format!("read skill {}", USING.wire()),
    );
    translated(&loaded, &USING, "the read_skill cell");
    assert!(
        loaded.contains("loaded"),
        "a read that returned and said nothing more is drawn as loaded: {loaded:?}",
    );

    // A companion file. The label is two words and the second one is a path, so
    // the translation must reach the first and stop.
    let companion = cell(
        "read_skill",
        PY_PATH,
        &format!("read skill {} {PY_PATH}", BRAINSTORM.wire()),
    );
    translated(
        &companion,
        &BRAINSTORM,
        "the read_skill cell with a companion file",
    );
    survives(
        &companion,
        PY_PATH,
        "the read_skill cell with a companion file",
    );

    // And the failure sentence, which is the other shape io-harness writes.
    let failed = cell(
        "read_skill",
        &DEEP.wire(),
        &format!("skill {} read error", DEEP.wire()),
    );
    translated(&failed, &DEEP, "a read_skill that failed");

    // **The live row, before any sentence has arrived.** It is drawn off the
    // announcing event alone, so it is a second site with a second rule
    // (`names_a_skill`) and its own way to be wrong.
    let mut live = Events::new(DARK);
    live.event(&call("read_skill", &DEEP.wire()), Duration::ZERO);
    let row = live.live();
    translated(&row, &DEEP, "the live read_skill row");

    let mut live = Events::new(DARK);
    live.event(&call("read_skill", PY_PATH), Duration::ZERO);
    let row = live.live();
    survives(&row, PY_PATH, "the live read_skill row");

    // **An MCP tool.** The name is the prefix, the server and the tool; the
    // prefix ends with the separator, so it is stripped first.
    let mcp_name = format!("{MCP_TOOL_PREFIX}{}", MCP_TOOL.wire());
    let called = cell(&mcp_name, "repo=io-cli", "opened issue 42");
    translated(&called, &MCP_TOOL, "the MCP cell");
    assert!(
        !called.contains(&mcp_name),
        "the whole wire tool name reached the transcript: {called:?}",
    );
    assert!(
        !called.contains("mcp:"),
        "the prefix was translated instead of stripped, which splits at the \
         prefix's own join and still carries the separator: {called:?}",
    );
    assert!(
        called.contains("Call"),
        "an MCP cell opens with a verb like every other cell: {called:?}",
    );

    // **The `Started` row.** A slash-invoked skill submits the catalogue name and
    // echoes what was typed; without the echo the wire prompt is the row.
    let mut events = Events::new(DARK);
    events.set_echo(format!("/{} make me a portfolio", BRAINSTORM.drawn()));
    let started = flatten(events.event(
        &RunEvent::new(
            1,
            1,
            EventKind::Started {
                goal: format!("{} make me a portfolio", BRAINSTORM.wire()),
                provider: "openrouter".into(),
            },
        ),
        Duration::ZERO,
    ));
    translated(&started, &BRAINSTORM, "the Started row");
}

// --- status line ---------------------------------------------------------------

/// **The status page.** `status::committed` is the seam and a bundle's policy
/// layer is the leak site: io-harness rewrites a `[policy] layers` entry to
/// `<bundle>__<name>` exactly as it rewrites an agent.
///
/// The workspace row carries the legitimate half: the session root is a real
/// directory under `__snapshots__`, and the page prints the path it was handed.
#[test]
fn f7_the_status_page_translates_a_bundles_policy_layer() {
    let home = tempfile::tempdir().expect("a workspace");
    let root = home.path().join(JS_DIR).join("workspace");
    std::fs::create_dir_all(&root).expect("the workspace directory");

    // Two layers: one ordinary, one whose own name carries the separator.
    let policy = io_harness::Policy::permissive()
        .layer(BRAINSTORM.bundle.to_string() + NAMESPACE + "no-secrets")
        .deny_read(".env")
        .layer(DEEP.wire())
        .deny_write("secrets/**");
    let contract = io_harness::TaskContract::workspace("summarise the module", root.clone())
        .with_max_steps(20);
    let status = Status::new("anthropic/claude-sonnet-4.5");

    // 200 columns: a row too long for the terminal is folded, and a fold would
    // break a `contains` for a reason that has nothing to do with the separator.
    let page: String = status::committed(
        &status, &root, 1, None, &policy, &contract, None, &DARK, 200,
    )
    .iter()
    .map(|line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    })
    .collect::<Vec<_>>()
    .join("\n");

    let secrets = Name {
        bundle: BRAINSTORM.bundle,
        own: "no-secrets",
    };
    translated(&page, &secrets, "the status page");
    translated(&page, &DEEP, "the status page");
    survives(&page, JS_DIR, "the status page");
}

// --- pickers -------------------------------------------------------------------

/// The skills the panes and the palette are fed.
///
/// One ordinary bundle skill, one whose own name carries the separator, and one
/// whose file lives under a path with a legitimate `__` in it.
fn skills() -> Vec<skillview::Listed> {
    vec![
        skillview::Listed {
            name: BRAINSTORM.wire(),
            description: "shape an idea".into(),
            origin: skillview::Origin::Bundle(BRAINSTORM.bundle.into()),
            enabled: true,
            path: PathBuf::from("/bundles/ultraship/skills/brainstorm/SKILL.md"),
        },
        skillview::Listed {
            name: DEEP.wire(),
            description: "a nested one".into(),
            origin: skillview::Origin::Bundle(DEEP.bundle.into()),
            enabled: true,
            path: PathBuf::from("/bundles/bundle/skills/deep/SKILL.md"),
        },
        // No bundle contributed this one, and its description names a file with a
        // legitimate `__` in it. The description rather than the path, because the
        // palette draws no path column at all and the half that must survive has
        // to be somewhere every picker actually draws.
        skillview::Listed {
            name: "reviewer".into(),
            description: format!("walks {PY_PATH}"),
            origin: skillview::Origin::Yours,
            enabled: true,
            path: PathBuf::from("/home/you/.io-cli/skills/reviewer.md"),
        },
    ]
}

/// Every row of a picker as a reader sees it: mark, label and detail.
///
/// The fields are joined with a space rather than concatenated, so no assertion
/// here can be satisfied by a string that only exists across a field boundary —
/// `commands::COMMAND_MARK` is a bare `":"`, and glued to a label it manufactures
/// exactly the shape this file is looking for.
fn picker_text(picker: &Picker) -> String {
    picker
        .rows()
        .iter()
        .map(|row| {
            format!(
                "{} {} {}",
                row.mark.unwrap_or(""),
                row.label,
                row.detail.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **The pickers.** `commands::palette` builds the rows and `Picker::rows` is
/// what anything reading them back gets. The skill row's label is the leak site.
#[test]
fn f7_the_palette_picker_translates_a_bundle_skill() {
    let skills = skills();
    let picker = Picker::new("palette", commands::palette(&Templates::none(), &skills));
    let drawn = picker_text(&picker);

    translated(&drawn, &BRAINSTORM, "the palette");
    translated(&drawn, &DEEP, "the palette");
    survives(&drawn, PY_PATH, "the palette");
    // An unqualified skill is drawn as itself and gains no colon: `naming::display`
    // returns a name carrying no separator unchanged, and a picker that qualified
    // one would invent a bundle. By label equality rather than a `contains`, so a
    // row labelled `something:reviewer` cannot satisfy it.
    assert!(
        picker.rows().iter().any(|row| row.label == "reviewer"),
        "a skill no bundle contributed was drawn as though one had: {drawn:?}",
    );
}

// --- skill pane -----------------------------------------------------------------

/// **The skill pane.** `skillview::rows` is the seam. The label is the skill's
/// name; the detail carries the origin, the state, the description and — at a
/// width that has room for it — the file's own path.
#[test]
fn f7_the_skill_pane_translates_a_bundle_skill_and_keeps_a_path() {
    let skills = skills();
    for glyphs in [&UNICODE, &ASCII] {
        let rows = skillview::rows(&skills, 200, glyphs);
        let drawn = rows
            .iter()
            .map(|row| format!("{} {}", row.label, row.detail.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
            .join("\n");
        let surface = format!("the skill pane ({})", glyphs.name);

        translated(&drawn, &BRAINSTORM, &surface);
        translated(&drawn, &DEEP, &surface);
        survives(&drawn, PY_PATH, &surface);

        // The label, on its own and by equality. `contains` cannot tell
        // `bundle:deep__nested` from an over-translated `bundle:deep:nested`
        // wherever the two share a prefix; this can, and this is the assertion
        // that stops the untranslated half being repaired by translating
        // everything.
        assert_eq!(
            rows[1].label,
            DEEP.drawn(),
            "{surface}: only the first separator is the join, and what follows is \
             the contribution's own name",
        );
    }
}

// --- plugin pane -----------------------------------------------------------------

/// The bundle io-harness itself reads, written to disk and inspected.
///
/// The root sits under `__snapshots__` so that the skills row — a path io-harness
/// resolved, not a string this test wrote — carries a legitimate `__`. The agent
/// is named `deep__nested`, so what io-harness hands back is
/// `ultraship__deep__nested` and the drawn form has to keep the tail.
fn inspected(home: &Path) -> pluginview::Listed {
    let dir = home.join(JS_DIR).join("ultraship-bundle");
    std::fs::create_dir_all(dir.join("skills")).expect("the skills directory");
    std::fs::write(
        dir.join(pluginview::MANIFEST),
        format!(
            "name = \"{}\"\ndescription = \"The release workflow.\"\n\
             version = \"1.2.0\"\nskills = \"skills\"\n\n\
             [[agent]]\nname = \"{}\"\nmodel = \"cheap-model\"\ndeny_write = true\n\n\
             [policy]\nlayers = [\n  {{ name = \"no-secrets\", rules = [{{ act = \"write\", \
             effect = \"deny\", pattern = \"secrets/**\" }}] }},\n]\n",
            BRAINSTORM.bundle, DEEP.own,
        ),
    )
    .expect("the manifest");
    pluginview::copy_out(
        &io_harness::Plugins::inspect(Scope::Local, &dir)
            .unwrap_or_else(|error| panic!("io-harness reads {}: {error}", dir.display())),
        true,
    )
}

/// Every row of a pane, mark and label and detail, as a reader sees it. Joined
/// with a space for `picker_text`'s reason.
fn pane_text(rows: &[io_cli::picker::Row]) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "{} {} {}",
                row.mark.unwrap_or(""),
                row.label,
                row.detail.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// **The plugin pane.** `pluginview::detail` is the seam and it holds three
/// `naming::display` call sites — agents, MCP servers and policy layers.
///
/// The first half of this test drives a bundle io-harness actually read, so the
/// namespaced strings are the ones io-harness wrote rather than ones this file
/// spelled. The second half hands `detail` a `Listed` built here, because the
/// servers group needs an `[[mcp]]` this fixture has no reason to declare and the
/// hook and executable rows are where a bundle's *own* paths reach the pane.
///
/// `pluginview::rows` is deliberately not asserted for the first half: it draws
/// the bundle id, the contribution kinds, the version, the description and the
/// root, and a plugin id cannot contain `__` (io-harness's `check_id`), so it has
/// no namespaced name to leak. It is asserted for the legitimate half, which it
/// does draw.
#[test]
fn f7_the_plugin_pane_translates_every_contribution_it_names() {
    let home = tempfile::tempdir().expect("a bundle store");
    let listed = inspected(home.path());
    let agent = Name {
        bundle: BRAINSTORM.bundle,
        own: DEEP.own,
    };
    let layer = Name {
        bundle: BRAINSTORM.bundle,
        own: "no-secrets",
    };

    for glyphs in [&UNICODE, &ASCII] {
        // `u16::MAX` for the reason `marketplace::disclosure` uses it: nothing is
        // shortened, so no assertion here can fail on an ellipsis.
        let drawn = pane_text(&pluginview::detail(&listed, u16::MAX, glyphs));
        let surface = format!("the plugin pane ({})", glyphs.name);

        translated(&drawn, &agent, &surface);
        translated(&drawn, &layer, &surface);
        // The path io-harness resolved, drawn as it is.
        survives(&drawn, JS_DIR, &surface);

        // The pane's list of bundles names no contribution, and the root it draws
        // is a path like any other.
        let view = pluginview::View {
            plugins: vec![listed.clone()],
            refused: Vec::new(),
            adapters: None,
        };
        // `u16::MAX` again: at eighty columns the root is the field that gives
        // way, and a path dropped for width would fail the assertion below for a
        // reason that has nothing to do with the separator.
        let rows = pane_text(&pluginview::rows(&view, u16::MAX, glyphs));
        survives(&rows, JS_DIR, &format!("the plugin list ({})", glyphs.name));
    }

    // The two groups the fixture has no reason to declare, and the two that carry
    // a bundle's own paths rather than its names.
    let mut built = listed;
    built.servers = vec![format!("{}{NAMESPACE}docs", BRAINSTORM.bundle)];
    built.hooks = vec![("post_edit".into(), format!("python {PY_PATH}"))];
    built.bin = vec![("review".into(), format!("bin/{JS_DIR}/review"))];
    let server = Name {
        bundle: BRAINSTORM.bundle,
        own: "docs",
    };
    for glyphs in [&UNICODE, &ASCII] {
        let drawn = pane_text(&pluginview::detail(&built, u16::MAX, glyphs));
        let surface = format!("the plugin pane, every group ({})", glyphs.name);
        translated(&drawn, &server, &surface);
        // A hook's argv and an executable's path belong to the operator's tree.
        // io-harness namespaces neither, and translating either draws a program
        // that is not there.
        survives(&drawn, PY_PATH, &surface);
        survives(&drawn, JS_DIR, &surface);
    }
}

// --- marketplace pane -------------------------------------------------------------

/// **The marketplace pane.** The shape is `tests/marketplace.rs`'s F6: io-harness
/// inspects a directory nothing has declared, and `Disclosure::said` folds the
/// rows into the lines the operator consents to.
///
/// The property is the one that file already states — consent has to be given to
/// the name that appears on `/plugin`, `/skills`, the palette and the `Skill`
/// line, or the operator agrees to one thing and reads about another — with the
/// over-translation half added: the agent's own name carries the separator here,
/// so a disclosure that rewrote every occurrence would name an agent that does
/// not exist.
#[test]
fn f7_the_marketplace_disclosure_translates_the_harnesss_own_names() {
    let home = tempfile::tempdir().expect("a marketplace clone");
    // Written by the same helper the plugin pane uses, and the directory is taken
    // off what io-harness read rather than rebuilt here, so the two surfaces
    // cannot be reading different bundles.
    let dir = inspected(home.path()).root;

    let disclosure = marketplace::disclosure(Scope::Local, &dir)
        .unwrap_or_else(|error| panic!("io-harness read the directory: {error}"));
    assert_eq!(disclosure.id, BRAINSTORM.bundle);

    for glyphs in [&UNICODE, &ASCII] {
        let said = disclosure.said(glyphs).join("\n");
        let surface = format!("the consent surface ({})", glyphs.name);
        translated(
            &said,
            &Name {
                bundle: BRAINSTORM.bundle,
                own: DEEP.own,
            },
            &surface,
        );
        translated(
            &said,
            &Name {
                bundle: BRAINSTORM.bundle,
                own: "no-secrets",
            },
            &surface,
        );
        survives(&said, JS_DIR, &surface);
    }
}

// --- what makes this go red --------------------------------------------------------

/// One more time, over every surface at once: the exact drawn spelling of a
/// contribution whose own name carries the separator.
///
/// Written as a single equality per surface rather than a `contains`, because
/// `contains` is blind to the failure this release's translation is one edit away
/// from: rewriting *every* occurrence instead of the first.
#[test]
fn f7_only_the_first_separator_is_the_join_on_every_surface() {
    let glyphs = &UNICODE;

    // Skill pane.
    let rows = skillview::rows(&skills(), 200, glyphs);
    assert_eq!(rows[1].label, DEEP.drawn(), "the skill pane");

    // Palette.
    let picker = Picker::new("palette", commands::palette(&Templates::none(), &skills()));
    assert!(
        picker.rows().iter().any(|row| row.label == DEEP.drawn()),
        "the palette drew no row labelled {}: {:?}",
        DEEP.drawn(),
        picker
            .rows()
            .iter()
            .map(|row| &row.label)
            .collect::<Vec<_>>(),
    );

    // Plugin pane and marketplace pane, off one io-harness read.
    let home = tempfile::tempdir().expect("a bundle store");
    let listed = inspected(home.path());
    let qualified = format!("{}:{}", BRAINSTORM.bundle, DEEP.own);
    let labels: Vec<String> = pluginview::detail(&listed, u16::MAX, glyphs)
        .into_iter()
        .map(|row| row.label)
        .collect();
    assert!(
        labels.contains(&qualified),
        "the plugin pane drew no row labelled {qualified}: {labels:?}",
    );

    // Transcript.
    let loaded = cell(
        "read_skill",
        &DEEP.wire(),
        &format!("read skill {}", DEEP.wire()),
    );
    assert!(
        loaded.contains(&DEEP.drawn()) && !loaded.contains("bundle:deep:nested"),
        "the transcript rewrote more than the join: {loaded:?}",
    );
}

// --- the sabotage arm --------------------------------------------------------------
//
// One concrete edit per surface. Each is a single source change, and each must
// turn a named test in this file red. A gate whose arm has no site is a gate this
// product has shipped before, so where an arm does not exist that is said rather
// than invented.
//
// **Transcript, committed skill cell.** `src/events.rs`, `skill_and_file`: change
// the final line from `Some((crate::naming::display(skill), detail))` to
// `Some((skill.to_string(), detail))`. Fails
// `f7_the_transcript_translates_a_contributed_name_and_leaves_a_path_alone` on the
// `read skill ultraship__using-ultraship` cell.
//
// **Transcript, live row.** `src/events.rs`, the `EventKind::ToolCall` arm of
// `Events::event`: delete the
// `else if name == crate::events::READ_SKILL && names_a_skill(target)` branch, so
// the target falls through to `relative(target, &self.root)`. Fails the same test
// on the live-row block.
//
// **Transcript, MCP cell.** `src/events.rs`, the same arm: change
// `if let Some(tool) = name.strip_prefix(MCP_TOOL_PREFIX) { crate::naming::display(tool) }`
// to `crate::naming::display(name)` — the prefix translated instead of stripped.
// Fails the `mcp:` assertion in the same test.
//
// **Transcript, `Started` row.** `src/events.rs`, the `EventKind::Started` arm:
// change `let typed = self.echo.take().unwrap_or_else(|| goal.clone());` to
// `let typed = goal.clone();`. Fails the `Started` block in the same test.
//
// **Status page.** `src/status.rs`, `committed`, the `for layer in &policy.layers`
// loop: change `format!("policy {}", crate::naming::display(&layer.name))` to
// `format!("policy {}", layer.name)`. Fails
// `f7_the_status_page_translates_a_bundles_policy_layer`.
//
// **Palette.** `src/commands.rs`, `entries`, the skills loop: change
// `crate::naming::display(&skill.name)` to `skill.name.clone()`. Fails
// `f7_the_palette_picker_translates_a_bundle_skill`.
//
// **Skill pane.** `src/skillview.rs`, `rows`: change
// `Row::with_detail(crate::naming::display(&skill.name), detail)` to
// `Row::with_detail(skill.name.clone(), detail)`. Fails
// `f7_the_skill_pane_translates_a_bundle_skill_and_keeps_a_path` and
// `f7_only_the_first_separator_is_the_join_on_every_surface`.
//
// **Plugin pane.** `src/pluginview.rs`, `detail`: drop `crate::naming::display`
// from the agents map — `.map(|name| Row::new(name.clone()))`. Fails
// `f7_the_plugin_pane_translates_every_contribution_it_names`. The servers map and
// the layers map are two more sites in the same function and each has its own arm
// of the same shape; the agents one is named because it is the one both the plugin
// pane and the marketplace pane read.
//
// **Marketplace pane — and it has no arm of its own, which is the honest
// answer.** `src/marketplace.rs` contains no `naming` call: `disclosure` composes
// its rows out of `crate::pluginview::detail`, deliberately, so that the pane an
// operator opens after consenting says the same thing in the same order. Its arm
// is therefore the plugin pane's arm above, which fails
// `f7_the_marketplace_disclosure_translates_the_harnesss_own_names` at the same
// time. The one edit that breaks the marketplace surface *alone* is composing the
// disclosure out of the manifest instead — `tests/marketplace.rs`'s F6 already
// names that arm and already dies on it.
//
// **The over-translation arm, which is the half a bad fix would reach for.**
// `src/naming.rs`, `display`: change the body to `name.replace(NAMESPACE, ":")`.
// Every `__` in the output goes, so the untranslated half of every test above
// still passes — and `f7_only_the_first_separator_is_the_join_on_every_surface`
// goes red on all four surfaces, together with every `survives` assertion in this
// file. That arm is the reason this gate is not written as
// `!drawn.contains(NAMESPACE)`: under that spelling this edit is a *fix*.
