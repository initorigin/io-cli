//! F7 — `/context` says what is in the window, from the request that carried it.
//!
//! **Every assertion here is made against a `CompletionRequest`, never against
//! io-cli's belief about one.** That is the whole feature: io-harness enumerates
//! no context window — `run::prompts::compose` and `workspace_tools()` are
//! `pub(super)`, and `EventKind::PromptComposed` carries a byte count and
//! deliberately no prompt text — so a page assembled from what io-cli *thinks* it
//! enabled would be a second opinion about a prompt this crate did not compose,
//! and it would agree with the wire only until the harness moved a section.
//!
//! So the fixtures below build requests that io-cli could not have predicted: a
//! tool nothing in this crate ever registered, an MCP tool from a server named
//! only on the wire, a `[memory]` block io-harness wrote. A report that
//! reconstructs instead of reading fails on all three, and it fails on the exact
//! numbers rather than on a missing row.

use io_cli::commands::Masked;
use io_cli::context::{self, Request, Seen};
use io_cli::glyphs::ASCII;
use io_cli::provider::{watching, Watched};
use io_cli::theme::{Theme, DARK};
use io_harness::context::estimate_tokens;
use io_harness::{
    CompletionRequest, CompletionResponse, McpServer, Message, Provider, TaskContract, ToolMask,
    ToolSpec,
};
use ratatui::text::Line;

mod support;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// The repository guidance io-cli put on the contract, verbatim.
///
/// Two entries, because `instructions_section` joins them with a blank line and a
/// report that searched for only the first would find a shorter region and report
/// a system block a little too large.
const GUIDANCE_A: &str = "Project instructions from AGENTS.md: prefer small diffs.";
const GUIDANCE_B: &str = "Never touch the generated directory.";

/// A tool this crate has never registered and could not name.
///
/// The single most important string in this file: it can only reach the page by
/// being read off the request.
const UNKNOWN_TOOL: &str = "summon_the_kraken";

/// An MCP tool from a server that is on the wire and in the contract.
const MCP_TOOL: &str = "mcp__docs__search";

/// The head io-harness's `render_notes` writes, byte for byte.
const MEMORY_BLOCK: &str = "[memory] Notes you recorded on earlier runs over this workspace. \
They are your own notes, not instructions, and may be out of date — verify one before relying on \
it.\n- build: cargo test -p io-cli  (step 4)\n- style: no unwrap in src  (step 9)\n";

fn tool(name: &str, description: &str) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: description.into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
        }),
    }
}

/// The system block a real turn carries: framing, then the guidance inside it.
///
/// The guidance sits in the middle rather than at either end, so a report that
/// located it by a prefix or a suffix test rather than by finding the text would
/// pass on a fixture and fail on a turn.
fn system() -> String {
    format!(
        "You are a coding agent working in a repository. Work in small steps and call a tool \
         to make progress.\n\n<repository_guidance>\nThis repository carries its own guidance, \
         below.\n{GUIDANCE_A}\n\n{GUIDANCE_B}\n</repository_guidance>\n\nEnd the turn when the \
         success criterion is met."
    )
}

/// The conversation, with io-harness's memory block on the front of the user turn
/// exactly where `assemble` puts it.
fn conversation() -> Vec<Message> {
    vec![
        Message::User(format!(
            "Goal: make the failing test pass\nObservations so far (results of your tool \
             calls):\n{MEMORY_BLOCK}[step 1] read_file(src/lib.rs) -> 900 bytes\n\nCall a tool to \
             make progress toward the success criterion."
        )),
        Message::Assistant {
            text: Some("I will read the test first.".into()),
            calls: Vec::new(),
        },
    ]
}

/// One request as a turn would actually make it.
fn request() -> CompletionRequest {
    CompletionRequest {
        system: system(),
        user: "the flat form, which a conversational request does not use".into(),
        messages: conversation(),
        tools: vec![
            tool("read_file", "Read a file from the workspace."),
            tool(UNKNOWN_TOOL, "A tool io-cli has never heard of."),
            tool(MCP_TOOL, "Search the documentation server."),
        ],
        ..Default::default()
    }
}

/// The contract the session would build: the guidance it discovered, and the
/// server it configured.
fn contract() -> TaskContract {
    TaskContract::workspace("make the failing test pass", "/repo")
        .with_instruction(GUIDANCE_A)
        .with_instruction(GUIDANCE_B)
        .with_mcp([McpServer::stdio("docs", "mcp-docs")])
}

/// The theme the sweeps draw in: the ordinary palette, ASCII glyph set.
fn ascii() -> Theme {
    DARK.with_glyphs(ASCII)
}

/// One rendered line as a reader sees it.
fn row(line: &Line<'_>) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

fn drawn(lines: &[Line<'_>]) -> Vec<String> {
    lines.iter().map(row).collect()
}

/// The section carrying `label`, or a failure naming what there was.
fn section<'a>(sections: &'a [context::Section], label: &str) -> &'a context::Section {
    sections
        .iter()
        .find(|s| s.label == label)
        .unwrap_or_else(|| {
            panic!(
                "no section labelled {label:?}; there were {:?}",
                sections.iter().map(|s| &s.label).collect::<Vec<_>>()
            )
        })
}

// ---------------------------------------------------------------------------
// The mechanism: the request is read off the wire
// ---------------------------------------------------------------------------

/// A provider that answers nothing and records what it was asked.
struct Silent;

impl Provider for Silent {
    async fn complete(
        &self,
        _request: CompletionRequest,
    ) -> io_harness::Result<CompletionResponse> {
        Ok(CompletionResponse {
            text: Some("done".into()),
            ..Default::default()
        })
    }
}

#[tokio::test]
async fn f7_the_window_is_read_from_the_request_that_carried_the_turn() {
    let seen = Seen::default();
    assert!(
        seen.latest().is_none(),
        "before a turn has run there is nothing to report, and a zeroed page \
         would read as an empty window rather than as an unasked one",
    );

    let watched = Watched::new(Silent, seen.clone());
    let answer = watched.complete(request()).await.expect("the mock answers");
    assert_eq!(
        answer.text.as_deref(),
        Some("done"),
        "the decorator delegates"
    );

    let carried = seen.latest().expect("the request went past");
    assert_eq!(carried.system, system(), "the system block, byte for byte");
    assert_eq!(carried.messages, 2);
    assert_eq!(
        carried
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["read_file", UNKNOWN_TOOL, MCP_TOOL],
        "the catalogue as offered, in the order it was offered",
    );
}

#[tokio::test]
async fn f7_a_model_switch_keeps_reporting_because_the_maker_is_what_is_wrapped() {
    // `/model` calls the maker again, so a decorator applied to a provider rather
    // than to a maker would leave the session reporting a window that stopped
    // updating at the first switch — a page that is wrong and still draws.
    let seen = Seen::default();
    let make = watching(
        |_name: &str| -> Result<Silent, String> { Ok(Silent) },
        seen.clone(),
    );

    let first = make("model-a").expect("the maker makes");
    first.complete(request()).await.expect("the mock answers");
    assert!(seen.latest().is_some());

    seen.forget();
    let switched = make("model-b").expect("the maker makes again");
    switched
        .complete(request())
        .await
        .expect("the mock answers");
    assert!(
        seen.latest().is_some(),
        "the provider built by the second call to the maker is watched too",
    );
}

// ---------------------------------------------------------------------------
// The six sections, their counts, and the sum
// ---------------------------------------------------------------------------

#[test]
fn f7_every_section_is_named_and_carries_a_count() {
    let seen = Request::of(&request());
    let sections = context::sections(&seen, &contract());

    for label in [
        "system block",
        "repository instructions",
        "tool catalogue",
        "mcp docs",
        "recalled memory",
        "conversation",
    ] {
        let section = section(&sections, label);
        assert!(
            section.tokens > 0,
            "{label} is in this request and should not read as empty: {section:?}",
        );
    }
}

#[test]
fn f7_the_mcp_row_is_the_server_the_request_offered_tools_from() {
    // Grouped by the id the contract configured rather than by splitting the
    // namespaced name on `__`, because a server id may contain the separator.
    let seen = Request::of(&request());
    let sections = context::sections(&seen, &contract());
    let mcp = section(&sections, "mcp docs");
    assert_eq!(mcp.detail, "1 tool(s) offered");
    assert_eq!(
        mcp.tokens,
        estimate_tokens(&format!(
            "{}\n{}\n{}",
            MCP_TOOL,
            "Search the documentation server.",
            tool(MCP_TOOL, "").parameters
        )),
        "counted over the text the tool actually occupies: name, description and schema",
    );

    // And a request with no MCP tool still draws the section, because a row that
    // vanishes when it is empty cannot be told from one that was never drawn.
    let mut bare = request();
    bare.tools.retain(|t| t.name != MCP_TOOL);
    let sections = context::sections(&Request::of(&bare), &contract());
    let none = section(&sections, "mcp tools");
    assert_eq!(none.tokens, 0);
    assert_eq!(none.detail, "no server offered a tool to this request");
}

#[test]
fn f7_the_counts_are_io_harness_estimate_tokens_over_the_real_strings() {
    let seen = Request::of(&request());
    let sections = context::sections(&seen, &contract());

    // The guidance, exactly as `instructions_section` joins it.
    let guidance = format!("{GUIDANCE_A}\n\n{GUIDANCE_B}");
    assert!(
        seen.system.contains(&guidance),
        "the fixture must carry the guidance the way the harness writes it",
    );
    assert_eq!(
        section(&sections, "repository instructions").tokens,
        estimate_tokens(&guidance),
    );

    // The system block is the rest of it, which is what keeps the two from
    // counting the same bytes twice.
    assert_eq!(
        section(&sections, "system block").tokens,
        estimate_tokens(&seen.system) - estimate_tokens(&guidance),
    );

    // The memory block, located by io-harness's own head sentence.
    assert_eq!(
        section(&sections, "recalled memory").tokens,
        estimate_tokens(MEMORY_BLOCK),
    );
    assert_eq!(
        section(&sections, "recalled memory").detail,
        "2 note(s) carried",
    );

    // And the conversation is what is left of it.
    assert_eq!(
        section(&sections, "conversation").tokens,
        estimate_tokens(&seen.conversation) - estimate_tokens(MEMORY_BLOCK),
    );

    // Nothing here is io-cli's own heuristic. Asserted directly, because a
    // hand-rolled estimator agreeing to within a few percent would pass every
    // assertion above and still put a number beside a ceiling measured in a
    // different unit.
    assert_eq!(
        estimate_tokens("abcd"),
        1,
        "the estimator under test is io-harness's own",
    );
}

#[test]
fn f7_the_sections_sum_to_the_total_and_the_total_is_stated_against_the_window() {
    let seen = Request::of(&request());
    let contract = contract();
    let sections = context::sections(&seen, &contract);
    let total = context::total(&sections);

    assert_eq!(total, sections.iter().map(|s| s.tokens).sum::<u64>());

    // The denominator is the contract's own budget, not `ContextBudget::default`:
    // an operator who tightened `[context]` must see the ceiling they set.
    assert_eq!(context::window(&contract, None), 24_000);
    let tightened =
        TaskContract::workspace("g", "/repo").with_context_budget(io_harness::ContextBudget {
            max_tokens: 8_000,
            share: 0.25,
        });
    assert_eq!(context::window(&tightened, None), 8_000);
    assert_eq!(
        context::window(&tightened, Some(20_000)),
        5_000,
        "with a run budget the window is a share of what is left",
    );
}

// ---------------------------------------------------------------------------
// The catalogue is the wire's, not io-cli's
// ---------------------------------------------------------------------------

#[test]
fn f7_the_catalogue_names_a_tool_only_the_request_knew_about() {
    // io-cli registers no tool called this and has no list it could come from.
    // If it is on the page, the page was read from the request.
    let sources: String = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .expect("src is readable")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "rs"))
        .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
        .collect();
    assert!(
        !sources.contains(UNKNOWN_TOOL),
        "the fixture only means something while no source file names this tool",
    );

    let page = drawn(&context::committed(
        Some(&Request::of(&request())),
        &contract(),
        None,
        &ToolMask::none(),
        &ascii(),
        80,
    ));
    assert!(
        page.iter().any(|row| row.contains(UNKNOWN_TOOL)),
        "the tool catalogue must list what was offered, including what io-cli \
         never registered: {page:#?}",
    );
    assert!(
        page.iter().any(|row| row.contains(MCP_TOOL)),
        "and the MCP tools with them: {page:#?}",
    );
}

// ---------------------------------------------------------------------------
// The sabotage
// ---------------------------------------------------------------------------

/// **Kills the sabotage.** Report the system block from `PromptComposed`'s byte
/// count instead of the request's own text.
///
/// It is the shape most likely to be written, because the byte count is the one
/// number io-harness *does* hand out for the prompt — and it survives every
/// weaker assertion: the row is present, the number is large and plausible, and
/// nothing about the page looks broken. What it cannot survive is arithmetic. A
/// count of bytes presented as tokens is roughly four times the section it names,
/// so the rows no longer add up to the total printed beside them, and the total
/// no longer means anything against the window it is stated over.
#[test]
fn f7_the_system_block_is_tokens_of_its_own_text_and_not_a_byte_count() {
    let seen = Request::of(&request());
    let contract = contract();
    let sections = context::sections(&seen, &contract);
    let block = section(&sections, "system block");

    let bytes = seen.system.len() as u64;
    assert!(
        bytes > block.tokens * 2,
        "the fixture is only a trap if the two numbers are far apart: \
         {bytes} bytes against {} tokens",
        block.tokens,
    );
    assert_ne!(
        block.tokens, bytes,
        "the system block is measured with `io_harness::context::estimate_tokens` \
         over the request's own text; a byte count is a different unit wearing the \
         same label",
    );

    // The arithmetic, on the rendered page rather than on the values behind it —
    // which is where an operator does this subtraction and where a sabotaged row
    // stops reconciling.
    let page = drawn(&context::committed(
        Some(&seen),
        &contract,
        None,
        &ToolMask::none(),
        &ascii(),
        80,
    ));
    let counted: u64 = page
        .iter()
        .filter_map(|row| row.split_once(": "))
        .filter(|(label, _)| !label.trim().starts_with("total"))
        .filter_map(|(_, rest)| rest.split_whitespace().next())
        .filter_map(|n| n.parse::<u64>().ok())
        .sum();
    let total = context::total(&sections);
    assert_eq!(
        counted, total,
        "the numbers on the page must add up to the total printed under them: \
         {page:#?}",
    );
    assert!(
        page.iter()
            .any(|row| row.contains(&format!("total: {total} tokens of 24000"))),
        "and the total is stated against the window the contract declares: {page:#?}",
    );
}

// ---------------------------------------------------------------------------
// The surface
// ---------------------------------------------------------------------------

#[test]
fn f7_the_page_draws_in_ascii_and_says_so_before_a_turn_has_run() {
    let empty = drawn(&context::committed(
        None,
        &contract(),
        None,
        &ToolMask::none(),
        &ascii(),
        80,
    ));
    assert!(
        empty
            .iter()
            .any(|row| row.contains("nothing has been sent yet")),
        "a window nothing has been sent through has no contents, and zeroes would \
         read as an empty one: {empty:#?}",
    );

    let page = drawn(&context::committed(
        Some(&Request::of(&request())),
        &contract(),
        None,
        &ToolMask::none(),
        &ascii(),
        80,
    ));
    let all = empty.join("\n") + "\n" + &page.join("\n");
    if let Some(bad) = all.chars().find(|c| !c.is_ascii()) {
        panic!(
            "the /context page drew {bad:?} (U+{:04X}) under the ASCII set; every \
             glyph must have an ASCII form.\n{all}",
            bad as u32,
        );
    }

    // Committed upward as a page, and every row inside the terminal it was given.
    assert!(page.first().is_some_and(|row| row.contains("context")));
    assert!(page.last().is_some_and(|row| row.contains("context ends")));
    for row in &page {
        assert!(
            row.chars().count() <= 80,
            "a committed row is folded to the width it was given, never allowed to \
             be wrapped somewhere nothing chose: {row:?}",
        );
    }
}

/// F7 + F10 — the page and the status line report ONE number.
///
/// **The defect a live run found, pinned so it cannot come back.** `/context`
/// totalled 4,363 tokens of 24,000 while the status line one keystroke away said
/// `ctx 0%`, because the page measured the whole request and the field measured
/// the observation section inside it. Each number was defensible on its own; the
/// pair was not, and the percentage is what makes an operator open the page.
///
/// So this asserts the field IS the page's total over the page's window, computed
/// from the same snapshot by the same calls — not merely that the two are close,
/// which is a tolerance somebody widens later.
#[test]
fn f10_the_status_share_is_the_page_total_over_the_page_window() {
    let contract = contract();
    let seen = Request::of(&request());

    let sections = context::sections(&seen, &contract);
    let total = context::total(&sections);
    let window = context::window(&contract, contract.max_tokens);
    assert!(
        total > 0 && window > 0,
        "the fixture has to put something in a window for this to mean anything",
    );

    let mut status = io_cli::status::Status::new("a-model");
    status.budgets = io_cli::status::Budgets::in_force(&contract);
    status.note_context_request(&seen, &contract, contract.max_tokens);

    let expected = (total as f64 / window as f64 * 100.0).round() as u8;
    assert_eq!(
        status.context,
        Some(expected),
        "the line says {:?} where the page says {total} of {window}",
        status.context,
    );

    // And it is not the degenerate agreement of two zeroes, which is exactly the
    // shape the defect wore on screen.
    assert!(
        expected > 0,
        "a fixture where both read zero would pass this test and prove nothing",
    );
}

/// The window is forgotten with the conversation it described.
///
/// **The review found `/clear` was the one site that did not call this**, and
/// `Seen::forget`'s own doc names `/clear` first of the three. The consequence is
/// the release's own headline failure wearing different clothes: `/context` draws
/// a whole conversation the operator has just discarded, on a session with no
/// turns in it, while the `ctx` field beside it is blank because `forget_run` did
/// clear that — two surfaces disagreeing about the same fact.
///
/// A source gate, because the sites are in the driver and nothing under tests/
/// links the binary.
#[test]
fn every_site_that_forgets_a_run_forgets_the_window_with_it() {
    let driver = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("the driver");

    // FOUR sites, and the count is stated rather than derived because the fourth
    // is why this test exists. Three drop the run's facts here in the driver —
    // `/resume`, `/fork`, the rewind — and `/clear` drops them one layer down in
    // `App::clear_conversation`, which is exactly how its missing `seen.forget()`
    // hid: a reader comparing the two counts in this file would have found them
    // equal at three and concluded all was well.
    let forgets_run = driver.matches("forget_run()").count();
    let forgets_window = driver.matches("seen.forget()").count();
    assert_eq!(
        forgets_run, 3,
        "three sites drop a run's facts in the driver; the fourth is `/clear`, \
         which does it through `App::clear_conversation`",
    );
    assert_eq!(
        forgets_window, 4,
        "the window belongs to the conversation, so every one of the four drops \
         it — a site that keeps it draws a whole page for a conversation the \
         operator has just discarded, beside a `ctx` field that is blank because \
         `forget_run` did clear that",
    );

    // And the fourth by name, since it is the one that was missing and a count
    // alone would be satisfied by any four.
    let clear = driver
        .split("Action::Clear => {")
        .nth(1)
        .expect("the clear arm");
    let clear = &clear[..clear.find("Action::").unwrap_or(clear.len())];
    assert!(
        clear.contains("seen.forget()"),
        "`/clear` is the site `Seen::forget`'s own doc names first, and was the \
         one that did not call it",
    );
}

// ---------------------------------------------------------------------------
// 0.37.0 — the catalogue is attributed to what put it on the wire
// ---------------------------------------------------------------------------

/// The skill catalogue io-harness appends, exactly as `with_skill_catalog` writes
/// it: one paragraph of framing, a newline, then one `- name: description` line
/// per skill. The second entry is namespaced, because a bundle's skills are.
const SKILL_BLOCK: &str = "Skills available to you — instructions written for this repository. \
     Only each skill's name and description is shown; call `read_skill` with a name to read that \
     skill's full text when its description matches what you are doing.\n\
     - alpha: the operator's own skill, unqualified\n\
     - docs__writing: a skill the docs bundle ships";

/// The system block of a turn whose session had skills configured.
fn system_with_skills() -> String {
    format!("{}\n\n{SKILL_BLOCK}", system())
}

/// **F2.** The head this module scans for is a sentence io-harness actually
/// writes, read out of the pinned source rather than trusted.
///
/// The two memory heads above it are matched against a dependency's prose with no
/// such gate, and their failure mode is the one this exists to close: io-harness
/// copy-edits the sentence, the section silently reports zero, and a zero is
/// indistinguishable from a session that configured no skills. A wrong number is
/// worse here than a missing row, because the page's whole claim is that its parts
/// sum to the request.
///
/// Sabotage: change one word of `SKILLS_HEAD` in `src/context.rs`. Nothing else in
/// the suite notices — every other arm builds its own fixture from the same
/// constant, so they all agree with each other and all disagree with the wire.
#[test]
fn f2_the_skill_catalogue_head_is_a_sentence_io_harness_writes() {
    let source = support::harness_source_at(&["run", "prompts.rs"]);
    assert!(
        source.contains(context::SKILLS_HEAD),
        "`SKILLS_HEAD` is `{}`, which is not in io-harness's own run/prompts.rs. \
         The pin moved the sentence and the skill catalogue section now reports zero \
         for every session — which reads exactly like a session with no skills.",
        context::SKILLS_HEAD,
    );
    assert!(
        source.contains("fn with_skill_catalog"),
        "the function that writes the catalogue is gone from run/prompts.rs, so the \
         needle above is being matched against something else"
    );
}

/// **F2.** A catalogue is located and counted; a perturbed one is reported absent
/// rather than as zero.
///
/// Both directions, because one direction is what let 0.30.0 document a door with
/// no caller. The failing direction is the one that matters: a needle that has
/// drifted produces `None`, the page says "no skill was named to this request",
/// and an operator with twelve skills reads that as the truth.
///
/// Sabotage: make `skills_in` return `Some` unconditionally, or drop the
/// `starts_with("- ")` line test so the block runs to the end of the system
/// prompt. The first fails the absent arm; the second fails the token equality,
/// because the block would then swallow the framing after it.
#[test]
fn f2_a_skill_catalogue_is_located_and_a_perturbed_one_is_absent() {
    let mut seen = Request::of(&CompletionRequest {
        system: system_with_skills(),
        ..request()
    });
    let sections = context::sections(&seen, &contract());
    let row = section(&sections, context::SKILL_CATALOGUE);
    assert_eq!(
        row.tokens,
        estimate_tokens(SKILL_BLOCK),
        "the section must be the block io-harness wrote, whole and nothing else"
    );
    assert!(
        row.detail.contains("2 skill(s)"),
        "two catalogue lines, counted off the wire: {}",
        row.detail
    );

    // The same request with the head sentence perturbed — which is what a pin
    // that copy-edits it produces.
    seen.system = seen.system.replace("Skills available", "Skills offered");
    let sections = context::sections(&seen, &contract());
    let row = section(&sections, context::SKILL_CATALOGUE);
    assert_eq!(row.tokens, 0);
    assert!(
        row.detail.contains("no skill"),
        "an unlocated catalogue must say so, never draw a bare zero: {}",
        row.detail
    );
}

/// **F1.** The sections still partition the request once the catalogue is one of
/// them.
///
/// The skill block is inside `request.system`, so a section that reported it
/// beside the system block rather than out of it would count those bytes twice and
/// the total would exceed the request. This is the arithmetic the page's whole
/// design rests on and it is asserted against the request rather than against the
/// sum of the rows — a sum of the rows compared to itself is a tautology.
///
/// Sabotage: drop the second `saturating_sub` in `sections`. Every row still draws
/// and only this equality fails.
#[test]
fn f1_the_sections_still_sum_to_the_request_with_a_catalogue_present() {
    let seen = Request::of(&CompletionRequest {
        system: system_with_skills(),
        ..request()
    });
    let sections = context::sections(&seen, &contract());

    let request_tokens = estimate_tokens(&seen.system)
        + seen
            .tools
            .iter()
            .map(|t| estimate_tokens(&format!("{}\n{}\n{}", t.name, t.description, t.parameters)))
            .sum::<u64>()
        + estimate_tokens(&seen.conversation);

    assert_eq!(
        context::total(&sections),
        request_tokens,
        "the sections are a partition of the request, so they sum to it exactly"
    );
}

/// **F3.** `server_cost` is the same measurement the page draws, keyed for a
/// surface to look up.
///
/// `/mcp` and `/plugin` draw their numbers from these maps and `/context` draws
/// its rows from `sections`; if the two could disagree, two surfaces would state
/// different costs for one server and the operator would have no way to tell which
/// was real. Asserted from **one** snapshot, which is the same discipline that
/// keeps `ctx N%` and the page total in agreement.
///
/// Sabotage: count `tool.name` alone in either function. The other keeps counting
/// the schema, and this equality is the only thing that fails.
#[test]
fn f3_server_cost_is_the_number_the_page_draws_for_the_same_server() {
    let seen = Request::of(&request());
    let contract = contract();
    let costs = context::server_cost(&seen, &contract);
    let sections = context::sections(&seen, &contract);

    assert_eq!(costs.len(), 1, "one server offered a tool: {costs:?}");
    let (id, tokens) = costs.iter().next().expect("the one server");
    assert_eq!(
        *tokens,
        section(&sections, &format!("mcp {id}")).tokens,
        "the map and the page must be one measurement, not two"
    );
}

/// **F3.** A server that has not been on a wire is absent from the map rather than
/// present at zero.
///
/// The two mean different things and only one of them is true. A configured server
/// that has offered nothing yet is unmeasured; zero would tell an operator it is
/// free, which is the opposite of unknown and is the claim they would act on.
///
/// Sabotage: seed the map from `contract.mcp` before the loop. Every other arm
/// passes.
#[test]
fn f3_a_server_that_has_not_been_on_a_wire_is_absent_not_zero() {
    let seen = Request::of(&CompletionRequest {
        tools: vec![tool("read_file", "Read a file from the workspace.")],
        ..request()
    });
    let costs = context::server_cost(&seen, &contract());
    assert!(
        costs.is_empty(),
        "the contract configures `docs`, but no tool of its reached this request, \
         so it is unmeasured rather than free: {costs:?}"
    );
}

/// **F9.** The sentence a withhold prints says the cost went **up**, not down.
///
/// This is the one line in the release that can be got backwards, and getting it
/// backwards is invisible: a reader told a tool was withheld assumes the request
/// shrank, because that is what the word means everywhere else and it is what this
/// release's own roadmap entry assumed. io-harness sends a byte-identical
/// catalogue and appends a sentence naming the withheld tools, so a mask costs a
/// little more and saves nothing.
///
/// Sabotage: delete the "still costs its definition" clause from
/// `context::withhold`, or write "saving" anywhere in it. The mask still works,
/// every other arm passes, and the product lies to its operator.
#[test]
fn f9_a_withhold_says_the_tool_is_still_offered_and_still_costs() {
    let catalogue = vec!["docx_write".to_string()];
    let (mask, line) = context::withhold(
        &ToolMask::none(),
        &Masked::Withhold("docx_write".into()),
        &catalogue,
    );
    assert!(mask.withholds("docx_write"));
    assert!(
        line.contains("still costs its definition"),
        "the cost direction must be stated: {line}"
    );
    assert!(
        line.contains("refused"),
        "and what actually happens must be stated: {line}"
    );
    for wrong in ["saves", "saving", "cheaper", "smaller", "reduces"] {
        assert!(
            !line.to_lowercase().contains(wrong),
            "`{wrong}` in a withhold's own sentence is the release's top risk: {line}"
        );
    }
}

/// **F9.** The page's withheld row says the same thing, and is absent when
/// nothing is withheld.
///
/// Absent rather than "nothing is withheld": a row that appears on every draw and
/// says nothing trains a reader to skip the place the answer appears, which is the
/// one place it must not be skipped.
///
/// Sabotage: return `Some` unconditionally from `withheld_line`, or drop its
/// closing clause. The first fails the empty arm, the second the wording arm.
#[test]
fn f9_the_withheld_row_is_absent_until_there_is_one_and_then_states_its_cost() {
    assert_eq!(context::withheld_line(&ToolMask::none(), "-"), None);

    let mask = ToolMask::withholding(["docx_write", "xlsx_write"]);
    let line = context::withheld_line(&mask, "-").expect("a mask draws a row");
    assert!(line.contains("docx_write") && line.contains("xlsx_write"));
    assert!(
        line.contains("saves nothing"),
        "the row must not let a reader infer a saving: {line}"
    );

    // And it reaches the drawn page, which is the surface an operator reads —
    // the function above being right is not evidence that anything calls it.
    let page = drawn(&context::committed(
        Some(&Request::of(&request())),
        &contract(),
        None,
        &mask,
        &ascii(),
        80,
    ));
    assert!(
        page.iter().any(|row| row.contains("withheld:")),
        "the mask must reach the page: {page:#?}"
    );
}

/// **F9.** `allow` never silently does nothing, and bare `allow` names what came
/// back.
///
/// An operator who mistypes a name and gets no answer believes the tool was
/// allowed again. And a bare `allow` changes several things at once, so it owes
/// the list — which is the whole reason it is safe to leave unguarded.
///
/// Sabotage: return the same line whether or not `retain` removed anything, or
/// drop the name list from the `Clear` arm.
#[test]
fn f9_allow_reports_a_no_op_and_a_clear_names_what_returned() {
    let mask = ToolMask::withholding(["docx_write", "pdf_write"]);

    let (_, line) = context::withhold(&mask, &Masked::Allow("never_withheld".into()), &[]);
    assert!(
        line.contains("was not withheld"),
        "a mistyped name must not read as success: {line}"
    );

    let (cleared, line) = context::withhold(&mask, &Masked::Clear, &[]);
    assert!(cleared.is_empty());
    assert!(
        line.contains("docx_write") && line.contains("pdf_write"),
        "clearing must name every tool it re-offered: {line}"
    );
}

// ---------------------------------------------------------------------------
// Defects the adversarial review found behind a fully green suite
// ---------------------------------------------------------------------------

/// **F9 — withholding a name the catalogue does not carry says so.**
///
/// **The defect this replaces was the safety lever being silently inert.**
/// io-harness keeps an unknown mask name rather than rejecting it, deliberately,
/// so a mask stays portable across builds with different cargo features
/// (`io-harness-0.76.0/src/tools/mod.rs:55-58`). That means `mask_gate` matches on
/// the exact string and a misspelling withholds nothing — while the operator was
/// told "calling it will be refused before anything starts" and `/context` drew it
/// on the withheld row. `/context withhold Docx_Write` and the file gets written.
///
/// The asymmetry was the tell: `Masked::Allow` already refused to be a silent
/// no-op and was tested for it. The guard existed only on the harmless direction.
///
/// The name is still added — refusing it would break the portability io-harness
/// designed for — and only the sentence changes.
///
/// Sabotage: drop the `unknown` branch, or pass `&[]` at either call site in
/// `src/main.rs`. The second is the 0.35.0 shape exactly, where a correct guard
/// was handed an empty slice by the only door that reached it.
#[test]
fn f9_withholding_a_name_the_catalogue_does_not_carry_says_so() {
    let catalogue: Vec<String> = vec!["docx_write".into(), "write_file".into()];

    // The case the operator actually hits: right tool, wrong case.
    let (mask, line) = context::withhold(
        &ToolMask::none(),
        &Masked::Withhold("Docx_Write".into()),
        &catalogue,
    );
    assert!(
        line.contains("no tool of that name"),
        "a name matching nothing on the wire must not be confirmed as refused: {line}"
    );
    assert!(
        mask.withholds("Docx_Write"),
        "it is still added — io-harness keeps unknown names so a mask stays \
         portable, and refusing here would break that"
    );

    // A real name is confirmed, without the warning.
    let (_, line) = context::withhold(
        &ToolMask::none(),
        &Masked::Withhold("docx_write".into()),
        &catalogue,
    );
    assert!(!line.contains("no tool of that name"), "{line}");

    // And before any turn there is no catalogue to check against, so nothing is
    // warned about — a warning on every name at a fresh prompt is noise that
    // trains the reader to ignore the one that matters.
    let (_, line) = context::withhold(&ToolMask::none(), &Masked::Withhold("anything".into()), &[]);
    assert!(!line.contains("no tool of that name"), "{line}");
}

/// **F2 — the planning directive is not counted as part of the skill catalogue.**
///
/// **io-harness glues the directive onto the last catalogue line with no newline
/// between them**, so a line scan cannot see the boundary: `compose` calls
/// `with_skill_catalog` and then `out.push_str(&directive)`
/// (`run/prompts.rs:73-76`), `Skills::catalog()` ends with no trailing newline
/// (`skills.rs:470-476`), and `planning_directive` begins with a space
/// (`run/gate.rs:282-286`). The final line therefore still starts with `- ` and a
/// naive scan swallows roughly 117 tokens of directive.
///
/// It is reachable on **every contained turn**, because registering a plan gate
/// turns io-harness's planning phase on. And the damage is not only the row: this
/// block is what `bundle_cost` splits per line, so the directive's tokens are
/// charged to whichever bundle owns the alphabetically last skill, and that
/// bundle's `/plugin` figure moves when the operator toggles `/plan`.
///
/// Sabotage: delete the `PLAN_DIRECTIVE_HEADS` cut from `skills_in`. The partition
/// still sums correctly — the bytes are inside `request.system` either way — which
/// is exactly why the sum test cannot see this and this arm has to exist.
#[test]
fn f2_the_planning_directive_is_not_part_of_the_skill_catalogue() {
    // The wire shape: catalogue, then the directive glued to the last line.
    let directive = " Before you do anything else you must call `propose_plan` with the ordered \
                      steps you intend to take, and wait. Until that plan is approved you may \
                      read, search and think.";
    let system = format!("{}\n\n{SKILL_BLOCK}{directive}", system());

    let seen = Request::of(&CompletionRequest {
        system,
        ..request()
    });
    let sections = context::sections(&seen, &contract());
    let row = section(&sections, context::SKILL_CATALOGUE);

    assert_eq!(
        row.tokens,
        estimate_tokens(SKILL_BLOCK),
        "the catalogue row must be the catalogue, not the catalogue plus whatever \
         io-harness appended to it without a newline"
    );
    assert!(
        row.detail.contains("2 skill(s)"),
        "and the line count is unchanged by the directive: {}",
        row.detail
    );
}

/// **F2 — both directive openers are sentences io-harness actually writes.**
///
/// The same gate `SKILLS_HEAD` gets, for the same reason: a constant matched
/// against a dependency's prose fails silently when the dependency rewords it, and
/// the failure here is a row that over-reports with nothing going red.
///
/// Sabotage: reword either constant. Nothing else in the suite notices, because
/// every other fixture builds its directive from these same constants.
#[test]
fn f2_the_planning_directive_openers_are_io_harness_s_own() {
    let source = support::harness_source_at(&["run", "gate.rs"]);
    for head in context::PLAN_DIRECTIVE_HEADS {
        assert!(
            source.contains(head),
            "`{head}` is not in io-harness's run/gate.rs, so the skill catalogue \
             row silently swallows the planning directive on every contained turn",
        );
    }
}

/// **F1 — one server does not absorb another's cost when an id is a prefix.**
///
/// `contract.mcp` lists the operator's own `[[mcp]]` entries before the plugin
/// servers `Plugins::apply_to` appends, and a bare id is validated by nobody — so
/// `github` and `github__enterprise` can both be configured. Taking the first
/// match charged every enterprise tool to `github` and left `github__enterprise`
/// reading "not yet on a request" for the life of the session.
///
/// Sabotage: change `max_by_key` back to a first-match return in `server_of`.
#[test]
fn f1_the_longest_configured_server_id_wins_not_the_first() {
    let contract = TaskContract::workspace("goal", "/repo").with_mcp([
        McpServer::stdio("github", "gh"),
        McpServer::stdio("github__enterprise", "ghe"),
    ]);
    let seen = Request::of(&CompletionRequest {
        tools: vec![tool(
            "mcp__github__enterprise__list_repos",
            "List the repositories.",
        )],
        ..request()
    });

    let costs = context::server_cost(&seen, &contract);
    assert!(
        costs.contains_key("github__enterprise"),
        "the enterprise server's own tool was charged elsewhere: {costs:?}"
    );
    assert!(
        !costs.contains_key("github"),
        "`github` was charged for a tool that is not its own: {costs:?}"
    );
}
