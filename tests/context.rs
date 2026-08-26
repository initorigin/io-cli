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

use io_cli::context::{self, Request, Seen};
use io_cli::glyphs::ASCII;
use io_cli::provider::{watching, Watched};
use io_cli::theme::{Theme, DARK};
use io_harness::context::estimate_tokens;
use io_harness::{
    CompletionRequest, CompletionResponse, McpServer, Message, Provider, TaskContract, ToolSpec,
};
use ratatui::text::Line;

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
    let empty = drawn(&context::committed(None, &contract(), None, &ascii(), 80));
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
