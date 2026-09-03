//! What has been spent, and the three ways of not knowing it.
//!
//! **Every number on `/cost` is a row already in `runs.db` or that row multiplied
//! by a rate the operator can point at.** There is no sampling, no extrapolation
//! and no model of what a token "usually" costs — which means the interesting
//! failures in this module are not arithmetic failures at all. They are the three
//! ways a page can report a number it does not have: a call the provider said
//! nothing about summed as zero, a model outside the price table priced at
//! nothing, and a total containing either of those drawn as though it were
//! complete. io-harness's own pricing documentation calls the last one "lying by
//! omission", and it is right.
//!
//! So the arithmetic is asserted once, against a figure this file computes from
//! the rates and the token split rather than against a literal — a wrong constant
//! baked into both sides of an `assert_eq!` proves nothing at all — and the rest
//! of the file is about what the page says when it cannot answer.
//!
//! The store is real throughout. `ProviderCall` rows go in through
//! `Store::record_provider_call` and come back out through the same reads the
//! page makes, because `usage: None` survives the round trip as SQL `NULL` and
//! that column is the whole of the unknown/free distinction.

use io_harness::pricing::{Price, PriceTable};
use io_harness::{ProviderCall, Store, Usage};

use io_cli::cost::{self, Provenance, Total};
use io_cli::glyphs::ASCII;
use io_cli::theme::{Theme, DARK};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Wide enough that nothing folds, so a `contains` here fails for the reason it
/// names rather than because a row broke across two.
const ROOMY: u16 = 200;

/// The rates. Five dimensions, all different, and none of them a round number of
/// dollars — a table where every rate was the same could not tell an
/// implementation that priced completion at the input rate from one that did not.
const INPUT: u64 = 3_000_000;
const OUTPUT: u64 = 15_000_000;
const CACHE_READ: u64 = 300_000;
const CACHE_WRITE: u64 = 3_750_000;
const PER_REQUEST: u64 = 10_000;

/// The model the table prices, and the one it does not.
const PRICED: &str = "claude-sonnet-4.5";
const UNPRICED: &str = "some-lab/experimental-preview";

fn table() -> PriceTable {
    PriceTable::new("2026-08-27").with(
        PRICED,
        Price {
            input: INPUT,
            output: OUTPUT,
            cache_read: CACHE_READ,
            cache_write: CACHE_WRITE,
            per_server_tool_request: PER_REQUEST,
        },
    )
}

/// The token split every arithmetic assertion below runs over.
///
/// **The cache figures are inside the prompt, not beside it.** Ten thousand
/// prompt tokens of which four thousand were served from cache and one thousand
/// written into it leaves five thousand read fresh — and a page that added the
/// three would report fifteen thousand prompt tokens for a turn that had ten.
fn split() -> Usage {
    Usage {
        prompt_tokens: 10_000,
        completion_tokens: 2_000,
        total_tokens: 12_000,
        cache_read_tokens: 4_000,
        cache_write_tokens: Some(1_000),
        reasoning_tokens: 500,
        server_tool_requests: 3,
    }
}

/// The split fixture's cache-write count, unwrapped.
///
/// io-harness 0.76.0 made `cache_write_tokens` an `Option<u64>`, and `split()` is
/// deliberately a provider that *did* report one — so every arithmetic assertion
/// below is about a known number and reads the way it did before the pin.
///
/// **The unknown case does not share this helper and must not.** A test that
/// unwrapped its way past `None` would assert nothing about the state this
/// release added; the three-state arms below construct their own usage.
fn written(usage: &Usage) -> u64 {
    usage
        .cache_write_tokens
        .expect("the split fixture reports a cache-write count")
}

fn call(model: Option<&str>, usage: Option<Usage>) -> ProviderCall {
    ProviderCall {
        step: 1,
        attempt: 0,
        provider: "anthropic".into(),
        model: model.map(str::to_string),
        usage,
        latency_ms: 1_200,
        ttft_ms: Some(300),
        // `..Default::default()` rather than every field, because io-harness adds
        // one to this struct when a vendor starts reporting something new and a
        // test that spelled all nine would stop compiling on a harness bump for a
        // reason that has nothing to do with what it asserts.
        ..Default::default()
    }
}

/// A store with one run inside one session, and the calls recorded against it.
struct Seeded {
    store: Store,
    run: i64,
    session: i64,
}

fn seeded(calls: &[ProviderCall]) -> Seeded {
    let store = Store::memory().expect("an in-memory store");
    let run = store
        .start_run("summarise the module", "/repo")
        .expect("a run");
    let session = store.create_session("/repo").expect("a session");
    store
        .record_turn(session, None, run, "summarise the module")
        .expect("a turn");
    for call in calls {
        store
            .record_provider_call(run, call)
            .expect("the call is recorded");
    }
    Seeded {
        store,
        run,
        session,
    }
}

fn ascii() -> Theme {
    DARK.with_glyphs(ASCII)
}

/// The `/cost` page as a reader sees it: every row, spans concatenated.
fn page(seeded: &Seeded, table: &PriceTable, provenance: &Provenance) -> Vec<String> {
    cost::committed(
        &seeded.store,
        table,
        provenance,
        Some(seeded.run),
        Some(seeded.session),
        &ascii(),
        ROOMY,
    )
    .expect("the page draws")
    .iter()
    .map(|line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    })
    .collect()
}

/// The whole page as one string, for asking whether a word is anywhere on it.
fn text(rows: &[String]) -> String {
    rows.join("\n")
}

/// The value of the first row reading `label: …`, or a failure naming the page.
fn field<'a>(rows: &'a [String], label: &str) -> &'a str {
    let prefix = format!("{label}: ");
    rows.iter()
        .find_map(|row| row.trim_start().strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no `{label}` row on the page:\n{}", text(rows)))
}

/// The provenance a configured operator has.
fn provenance() -> Provenance {
    Provenance {
        as_of: Some("2026-08-27".into()),
        source: Some("the reference catalogue at https://openrouter.ai/api/v1/models".into()),
        models: Some(417),
    }
}

// ---------------------------------------------------------------------------
// The arithmetic
// ---------------------------------------------------------------------------

/// **The total is the rates multiplied by the split, and the expected figure is
/// computed here rather than copied out of a run.**
///
/// A test that asserted `total.micros == 79_950` would go green on the day it was
/// written and would keep going green if somebody changed a rate constant in the
/// fixture — the two sides would move together and the assertion would be that
/// the function is the function. So the expected value is built from the same
/// five rates and the same seven token counts an operator would multiply by hand,
/// and the fixture is chosen so the division is exact: io-harness's rounding rule
/// is io-harness's to test, and re-implementing it here would smuggle it into an
/// assertion about io-cli's summing.
///
/// The cross-check against `PriceTable::cost_micros` is what stops this file
/// inventing its own pricing model. If the arithmetic here and the harness's
/// disagree, this test says so before anything else does.
///
/// Sabotage: sum `usage.prompt_tokens` at the input rate rather than
/// `prompt_tokens - cache_read - cache_write`, which is the natural mistake and
/// over-reports every cached turn — which is most of them. Under it this fails by
/// exactly the cache tokens times the input rate and no rendered page changes
/// shape.
#[test]
fn the_total_is_the_rates_times_the_split_and_not_a_figure_typed_here() {
    let usage = split();
    let fresh = usage.prompt_tokens - usage.cache_read_tokens - written(&usage);
    assert_eq!(fresh, 5_000, "the fixture's fresh prompt tokens");

    // Per-million everywhere except the server tool requests, which vendors bill
    // per request and which a token-only sum silently loses.
    let mtok = fresh as u128 * INPUT as u128
        + usage.completion_tokens as u128 * OUTPUT as u128
        + usage.cache_read_tokens as u128 * CACHE_READ as u128
        + written(&usage) as u128 * CACHE_WRITE as u128;
    assert_eq!(
        mtok % 1_000_000,
        0,
        "the fixture is chosen so that no rounding rule is under test here",
    );
    let expected =
        (mtok / 1_000_000 + usage.server_tool_requests as u128 * PER_REQUEST as u128) as u64;
    assert!(expected > 0, "a fixture that costs nothing proves nothing");

    // io-harness agrees about what one call costs, so what follows is a claim
    // about io-cli's summing rather than about pricing.
    assert_eq!(
        table().cost_micros(PRICED, &usage),
        Some(expected),
        "this file and io-harness disagree about what one call costs",
    );

    let total = Total::of(&[call(Some(PRICED), Some(usage))], &table());
    assert_eq!(total.calls, 1);
    assert_eq!(total.unknown, 0);
    assert_eq!(total.unpriced, 0);
    assert_eq!(total.micros, expected);
    assert!(
        !total.is_floor(),
        "a total with nothing missing from it was reported as a floor",
    );

    // **Reasoning is inside completion, never beside it.** A vendor that reports
    // reasoning separately still bills it as output, so it is carried for the
    // page to break down and contributes nothing of its own to the money.
    let mut without_reasoning = usage;
    without_reasoning.reasoning_tokens = 0;
    assert_eq!(
        Total::of(&[call(Some(PRICED), Some(without_reasoning))], &table()).micros,
        expected,
        "reasoning tokens were billed a second time on top of completion",
    );

    // And the sum over calls is a sum. Three of the same call cost three times as
    // much, and the token counters add rather than replace.
    let three = Total::of(
        &[
            call(Some(PRICED), Some(usage)),
            call(Some(PRICED), Some(usage)),
            call(Some(PRICED), Some(usage)),
        ],
        &table(),
    );
    assert_eq!(three.micros, expected * 3);
    assert_eq!(three.usage.prompt_tokens, usage.prompt_tokens * 3);
    assert_eq!(three.usage.cache_read_tokens, usage.cache_read_tokens * 3);
    assert_eq!(
        three.usage.server_tool_requests,
        usage.server_tool_requests * 3
    );
}

/// **A call the provider said nothing about is unknown, and unknown is not
/// free.**
///
/// io-harness stores `total_tokens` as SQL `NULL` for a call whose provider
/// reported no usage and reads it back as `None` for exactly that reason. Summing
/// it as a zeroed `Usage` would report a turn that cost something as a turn that
/// cost nothing — and it would do it silently, because a zero adds correctly to
/// every other figure on the page.
///
/// Three things therefore have to hold at once: the call is counted (it happened,
/// and it was billed by somebody), it contributes no tokens (there are none to
/// contribute), and the total is marked a floor (there is a cost nobody can
/// state). A page that got the first two right and the last one wrong would draw
/// a complete-looking total that is missing a turn.
///
/// Sabotage: `let usage = call.usage.unwrap_or_default();` — one method call,
/// reads as a tidy-up of an `Option`, and turns every unreported call into a free
/// one. Under it `unknown` stays zero, `is_floor` goes false, and the caveat
/// leaves the page.
#[test]
fn a_call_the_provider_said_nothing_about_is_unknown_and_never_free() {
    let calls = [
        call(Some(PRICED), Some(split())),
        call(Some(PRICED), None),
        call(None, None),
    ];
    let total = Total::of(&calls, &table());

    assert_eq!(
        total.calls, 3,
        "a call that reported nothing still happened"
    );
    assert_eq!(total.unknown, 2);
    assert_eq!(
        total.unpriced, 0,
        "an unreported call is unknown, which is a different gap from unpriced — \
         counting it as both would report one missing call twice",
    );
    assert_eq!(
        total.usage,
        split(),
        "the unreported calls contributed tokens they never reported",
    );
    assert!(
        total.is_floor(),
        "two calls are missing from this total and it does not say so",
    );

    // And the page says it in words, in the vocabulary that distinguishes the two
    // gaps. `unknown rather than free` is the sentence; a page that said "0
    // tokens" would be stating the thing that is not true.
    let fixture = seeded(&calls);
    let rows = page(&fixture, &table(), &provenance());
    let drawn = text(&rows);
    assert!(
        drawn.contains("reported no usage at all"),
        "the page hides the calls it could not read:\n{drawn}",
    );
    assert!(
        drawn.contains("unknown rather than free"),
        "the page does not distinguish an unreported call from a free one:\n{drawn}",
    );
}

/// **A model outside the table is unpriced, and a total containing one is a floor
/// rather than a total.**
///
/// The gap the whole module is shaped around. io-cli compiles no prices in, so an
/// operator's table covers whatever their catalogue happened to price on the day
/// they read it — a model added upstream last week is a real cost with no rate,
/// and it is the ordinary case rather than the exotic one.
///
/// The tokens are still exact. That is the point of counting the two gaps
/// separately: an unpriced call contributes every token it reported and no money,
/// so the token figures on the page are complete and the money figure is a floor,
/// and the page has to be able to say both at once.
///
/// Sabotage: price a missing model at `Price::ZERO` rather than counting it —
/// under which `unpriced` stays zero, `is_floor` goes false, the caveat leaves the
/// page, and the operator reads a total that is confidently short.
#[test]
fn a_model_the_table_does_not_price_is_a_floor_and_says_so() {
    let calls = [
        call(Some(PRICED), Some(split())),
        call(Some(UNPRICED), Some(split())),
        // No model at all, which the provider is allowed not to name. It cannot
        // be priced either, and for the same reason it cannot be attributed.
        call(None, Some(split())),
    ];
    let total = Total::of(&calls, &table());

    assert_eq!(total.calls, 3);
    assert_eq!(total.unknown, 0, "all three reported their usage");
    assert_eq!(total.unpriced, 2, "two of the three cannot be priced");
    assert!(
        total.is_floor(),
        "a total missing two calls' money is a floor"
    );
    assert_eq!(
        total.usage.prompt_tokens,
        split().prompt_tokens * 3,
        "an unpriced call still reported its tokens, and they are still exact",
    );
    assert_eq!(
        total.micros,
        table()
            .cost_micros(PRICED, &split())
            .expect("the priced one"),
        "the money is exactly the one call the table can price",
    );

    let fixture = seeded(&calls);
    let rows = page(&fixture, &table(), &provenance());
    let drawn = text(&rows);
    assert!(
        drawn.contains("no rate in the price table"),
        "the page does not say why the total is short:\n{drawn}",
    );
    assert!(
        drawn.contains("floor and not a total"),
        "the page draws an incomplete total as though it were complete:\n{drawn}",
    );
    // **On the grouped rows too, and on the row rather than under it.** A reader
    // scanning `by model` for the largest figure has to be able to see which of
    // them is incomplete without reading past the list.
    //
    // "not priced" and not "unpriced", deliberately: io-harness's
    // `Spend::unpriced_calls` counts a call with no usage as unpriced, while this
    // module's `Total` splits those out as *unknown*. The same call falls in
    // different buckets on the two halves of this page, and one word for both
    // would make the two sections look like they disagreed about a number.
    assert!(
        drawn.contains("not priced"),
        "the `by model` rows hide the calls they could not price:\n{drawn}",
    );
    assert!(
        rows.iter()
            .any(|row| row.contains(UNPRICED) && row.contains("not priced")),
        "the unpriced model's own row does not say it could not be priced:\n{drawn}",
    );
}

/// **The cache figures are a breakdown of the prompt, and the three parts sum to
/// it rather than past it.**
///
/// `Usage::cache_read_tokens` and `cache_write_tokens` are already inside
/// `prompt_tokens`, so a page that listed them as separate figures would
/// over-report every cached turn — and on a long agentic run almost every turn is
/// cached. The page draws the prompt total and the three parts beneath it, named
/// as parts (`of which cache read`) rather than drawn further right, because this
/// page has no columns to indent into and a reader in `--plain` has only the
/// words.
///
/// Asserted on the underlying counts as well as on the rendered strings: the
/// rendered figures are rounded to one decimal by `format_tokens`, so agreement
/// between them is necessary and not sufficient.
///
/// Sabotage: add the two cache counters to the prompt figure — under which
/// `prompt` reads 15.0k for a turn that had 10.0k of prompt, the three parts sum
/// past it, and every other test in this file still passes.
#[test]
fn the_cache_figures_are_inside_the_prompt_and_never_added_to_it() {
    let usage = split();
    let fresh = usage.prompt_tokens - usage.cache_read_tokens - written(&usage);
    assert_eq!(
        usage.cache_read_tokens + written(&usage) + fresh,
        usage.prompt_tokens,
        "the fixture's own parts do not sum to its prompt",
    );

    let fixture = seeded(&[call(Some(PRICED), Some(usage))]);
    let rows = page(&fixture, &table(), &provenance());

    let spelled = io_cli::status::format_tokens;
    assert_eq!(
        field(&rows, "prompt"),
        spelled(usage.prompt_tokens),
        "the prompt figure is not the prompt the provider reported:\n{}",
        text(&rows),
    );
    assert_eq!(
        field(&rows, "of which cache read"),
        spelled(usage.cache_read_tokens),
    );
    assert_eq!(
        field(&rows, "of which cache written"),
        spelled(written(&usage)),
    );
    assert_eq!(
        field(&rows, "of which fresh"),
        spelled(fresh),
        "the fresh figure is not the prompt less what came from cache",
    );
    assert_eq!(field(&rows, "completion"), spelled(usage.completion_tokens));
    assert_eq!(
        field(&rows, "of which reasoning"),
        spelled(usage.reasoning_tokens),
        "reasoning is a breakdown of completion and is drawn as one",
    );

    // A turn with no cache at all draws no cache rows, rather than three zeroes
    // an operator has to read to learn nothing.
    let plain = Usage {
        prompt_tokens: 900,
        completion_tokens: 100,
        total_tokens: 1_000,
        ..Default::default()
    };
    let fixture = seeded(&[call(Some(PRICED), Some(plain))]);
    let drawn = text(&page(&fixture, &table(), &provenance()));
    assert!(
        !drawn.contains("of which cache"),
        "a turn that cached nothing drew a cache breakdown of zeroes:\n{drawn}",
    );
    assert!(
        !drawn.contains("of which reasoning"),
        "a turn that reasoned about nothing drew a reasoning row:\n{drawn}",
    );
}

/// **F10 — a cache-write count nobody reported is a third state, and it is drawn
/// as one.**
///
/// io-harness 0.76.0 made `Usage::cache_write_tokens` an `Option<u64>`, which is
/// the shape this module's first honesty rule has always wanted: a count the
/// provider reported no usage for is unknown, never zero. It is not a
/// hypothetical distinction — **every OpenRouter call reports `None`**, and
/// OpenRouter is the provider this product's own evidence is taken on, so the
/// obvious `unwrap_or(0)` would put a fabricated `0` on the page an operator
/// reads to find out what a turn cost.
///
/// Three states, three assertions, and the pairwise distinctness asserted
/// explicitly — the shape `tests/status.rs`'s
/// `f14_a_probe_and_a_state_nobody_reached_are_different_words` uses for exactly
/// the same reason.
///
/// Sabotage: `unwrap_or(0)` in `cost::page`. Under it the unknown arm draws the
/// same string as the reported-zero arm, the distinctness assertion fails by
/// name, and no other test in this file moves.
#[test]
fn f10_an_unreported_cache_write_is_unknown_and_a_reported_zero_is_zero() {
    let spelled = io_cli::status::format_tokens;

    let usage_with = |cache_write| Usage {
        prompt_tokens: 10_000,
        completion_tokens: 2_000,
        total_tokens: 12_000,
        cache_read_tokens: 4_000,
        cache_write_tokens: cache_write,
        ..Default::default()
    };
    let written_row = |cache_write| {
        let fixture = seeded(&[call(Some(PRICED), Some(usage_with(cache_write)))]);
        field(
            &page(&fixture, &table(), &provenance()),
            "of which cache written",
        )
        .to_string()
    };

    let unknown = written_row(None);
    let zero = written_row(Some(0));
    let some = written_row(Some(1_000));

    assert_eq!(
        some,
        spelled(1_000),
        "a provider that reported a cache-write count must have it drawn",
    );
    assert_eq!(
        zero,
        spelled(0),
        "a provider that reported zero cache writes reported a number, and it is drawn as one",
    );
    assert_eq!(
        unknown, "unknown",
        "a provider that reported no cache-write count at all must not be drawn a number",
    );

    // The distinctness is the property, and it is asserted rather than left to
    // follow from the three equalities above: a later change that made both
    // render `0` would break two of those and this one, and this is the one whose
    // failure names the actual defect.
    assert_ne!(
        unknown, zero,
        "unknown and reported-zero drew the same string, so the page cannot tell an operator \
         which one happened",
    );

    // And the remainder inherits the silence. Fresh is the prompt less both
    // cached parts, so substituting zero for the unknown write would draw a fresh
    // figure too large by exactly the amount nobody measured — and it would look
    // like a fact.
    let fixture = seeded(&[call(Some(PRICED), Some(usage_with(None)))]);
    let rows = page(&fixture, &table(), &provenance());
    assert_eq!(
        field(&rows, "of which fresh"),
        "unknown",
        "the fresh figure was computed from a cache-write count that does not exist",
    );
    assert_eq!(
        field(&rows, "of which cache read"),
        spelled(4_000),
        "the read count was reported and stays a number",
    );
}

/// **F10's other half — summing an unknown with a known does not erase the
/// known.**
///
/// `cost::add` folds many calls into one `Usage`. Two silences stay a silence;
/// one silence beside a number keeps the number, because a turn that reported
/// 1,000 cache writes and a turn that reported nothing together wrote *at least*
/// 1,000, and answering `unknown` there would hide a figure the provider did give
/// us.
///
/// Sabotage: make the mixed case `None`. The page then reads `unknown` for a run
/// where one call reported a real count, and this fails while the single-call
/// arms above all still pass.
#[test]
fn f10_folding_an_unreported_count_with_a_reported_one_keeps_the_number() {
    // A non-zero read count is load-bearing on the fixture rather than
    // decoration: with no reads and no reported writes the page draws no cache
    // block at all — correctly, since there is nothing to say — and the row this
    // test reads would not exist. The reads are what put the block on the page so
    // the *write* row's three states can be compared.
    let with = |cache_write| Usage {
        prompt_tokens: 1_000,
        completion_tokens: 100,
        total_tokens: 1_100,
        cache_read_tokens: 100,
        cache_write_tokens: cache_write,
        ..Default::default()
    };
    let spelled = io_cli::status::format_tokens;
    let drawn = |calls: &[_]| {
        field(
            &page(&seeded(calls), &table(), &provenance()),
            "of which cache written",
        )
        .to_string()
    };

    assert_eq!(
        drawn(&[
            call(Some(PRICED), Some(with(None))),
            call(Some(PRICED), Some(with(None))),
        ]),
        "unknown",
        "no call reported a cache-write count, so the fold has nothing to report",
    );
    assert_eq!(
        drawn(&[
            call(Some(PRICED), Some(with(None))),
            call(Some(PRICED), Some(with(Some(1_000)))),
        ]),
        spelled(1_000),
        "one call reported 1,000 cache writes and the fold dropped it",
    );
    assert_eq!(
        drawn(&[
            call(Some(PRICED), Some(with(Some(250)))),
            call(Some(PRICED), Some(with(Some(750)))),
        ]),
        spelled(1_000),
        "two reported counts must sum",
    );
}

// ---------------------------------------------------------------------------
// `money`
// ---------------------------------------------------------------------------

/// **Money is integer arithmetic at every magnitude, and the precision changes
/// with the magnitude because one precision cannot show both ends.**
///
/// A turn costs a fraction of a cent and a month costs dollars. Two decimals
/// round the turn to `$0.00`, which reads as free; four decimals pad the month
/// with `$412.9100`, which reads as noise. So it is four below a unit and two
/// above it, and the switch is at the unit rather than at some threshold nobody
/// could name.
///
/// Every figure here is a division of `u64` by `u64`. A bill is not a
/// floating-point quantity: `cost_micros` is exact, and rendering it through an
/// `f64` would reintroduce a drift the harness went to the trouble of summing in
/// `u128` to avoid.
///
/// Sabotage: render through `micros as f64 / 1_000_000.0` — under which most of
/// these still pass and the multi-dollar cases drift in the last place.
#[test]
fn money_is_integer_arithmetic_at_every_magnitude() {
    // Zero is bare. A total of nothing is not `$0.0000`, which reads as a small
    // number somebody rounded.
    assert_eq!(cost::money(0), "$0");

    // **Below the smallest figure four decimals can show, the answer says so
    // rather than rounding to a zero.** `$0.0000` reads as free, and a cheap model
    // answering a short prompt lands under a hundredth of a cent every time — so
    // that form would have printed "free" for most single turns on a small model.
    // `$0` above is reserved for a cost that really is zero, which a free model
    // has and this does not.
    assert_eq!(cost::money(1), "<$0.0001");
    assert_eq!(cost::money(99), "<$0.0001");
    assert_eq!(cost::money(100), "$0.0001");
    assert_eq!(cost::money(12_345), "$0.0123");

    // Sub-dollar, up to the last micro-unit before the switch.
    assert_eq!(cost::money(999_999), "$0.9999");

    // At and above a unit, two decimals.
    assert_eq!(cost::money(1_000_000), "$1.00");
    assert_eq!(cost::money(1_234_567), "$1.23");
    assert_eq!(cost::money(12_345_678_901), "$12345.67");

    // **Truncated and never rounded up**, which is the honest direction for a
    // floor: a figure that rounded to `$2.00` from `$1.999999` would report more
    // than the store can account for.
    assert_eq!(cost::money(1_999_999), "$1.99");
    assert_eq!(cost::money(99_999), "$0.0999");

    // The rendering never loses a digit or gains one: every sub-dollar figure it
    // can express has four decimals, at every magnitude in the sweep. The
    // below-a-hundred-micro-units form is deliberately a different width — it is a
    // different *claim*, "smaller than this" rather than "this", and a bound that
    // looked identical to a figure would be read as one.
    for micros in [100u64, 500, 99_999, 999_999] {
        let drawn = cost::money(micros);
        assert_eq!(
            drawn.len(),
            "$0.0000".len(),
            "a sub-dollar figure changed width: {drawn}",
        );
    }
    for micros in [1u64, 7, 99] {
        assert_eq!(cost::money(micros), "<$0.0001");
    }
    for micros in [1_000_000u64, 9_999_999, 123_456_789] {
        let drawn = cost::money(micros);
        assert_eq!(
            drawn.split('.').next_back().map(str::len),
            Some(2),
            "a figure at or above a unit did not carry two decimals: {drawn}",
        );
    }
}

// ---------------------------------------------------------------------------
// The page as a whole
// ---------------------------------------------------------------------------

/// **An empty price table draws every token figure and no money at all.**
///
/// This is the state every operator is in before their first catalogue read, and
/// the state an operator on a self-hosted endpoint stays in permanently. The
/// module's own argument for having one code path rather than two is written on
/// `cost::table`: "the page is the same page with the money left off, rather than
/// a second page for the unconfigured case that would go stale unread."
///
/// *Left off* is the claim. A page that draws `cost: $0` under a caveat saying the
/// figure is a floor has not left the money off — it has stated a number nobody
/// measured, in the one situation where no number is available, which is the
/// invented figure the whole module exists to refuse. `Status::note_cost_from`
/// already holds the status line to exactly this rule and says so in as many
/// words: "Nothing priced is not zero priced. A run whose models are all outside
/// the table has a real cost that this program cannot state, and stating it as
/// `$0` would be the invented number the whole of `/cost` is built to avoid." The
/// two surfaces have to agree, and the one-row form is the one that currently
/// does.
///
/// Sabotage: draw the `cost` row unconditionally, and draw `money(0)` on every
/// grouped row whose calls could none of them be priced.
#[test]
fn an_empty_price_table_draws_tokens_and_no_money() {
    let empty = PriceTable::new("");
    let fixture = seeded(&[
        call(Some(PRICED), Some(split())),
        call(Some(UNPRICED), Some(split())),
    ]);
    // No `[prices]` section at all, which is what `Provenance::of` reports for a
    // file that has never had one.
    let rows = page(&fixture, &empty, &Provenance::default());
    let drawn = text(&rows);

    // The page says so, in the sentence that also says what to do about it.
    assert!(
        drawn.contains("no prices are configured"),
        "the page does not say the table is empty:\n{drawn}",
    );
    assert!(
        drawn.contains("tokens and no money"),
        "the page does not say what it can still answer:\n{drawn}",
    );

    // Every token figure is still exact, because nothing about counting tokens
    // needs a rate.
    let spelled = io_cli::status::format_tokens;
    assert_eq!(
        field(&rows, "prompt"),
        spelled(split().prompt_tokens * 2),
        "the token figures went missing with the prices:\n{drawn}",
    );
    assert_eq!(
        field(&rows, "completion"),
        spelled(split().completion_tokens * 2)
    );

    // And no currency anywhere. Not a zero, not a floor of zero, not a dollar
    // sign on a grouped row: there is no rate on this install and every figure
    // that would need one is absent.
    assert!(
        !drawn.contains('$'),
        "a page with no prices drew a money figure, which is the invented number \
         `Status::note_cost_from` refuses on the status line:\n{drawn}",
    );
}

/// **Before anything has run, the page says so rather than drawing a row of
/// zeroes.**
///
/// The same distinction the status line holds every one of its counters to: a
/// session that has run nothing has no numbers, and a row of zeroes is a claim
/// that it ran and cost nothing. `run` and `session` are both `None` before the
/// first turn, and they are different absences — a session can have turns behind
/// it with no run in flight.
///
/// Sabotage: `unwrap_or_default()` either id and read run zero, which exists in
/// no store and reads back as an empty call list — under which the page draws
/// `no provider calls` for a session that has never had one, losing the
/// difference between "nothing yet" and "nothing this time".
#[test]
fn a_session_that_has_run_nothing_says_so_rather_than_drawing_zeroes() {
    let store = Store::memory().expect("an in-memory store");
    let lines = cost::committed(&store, &table(), &provenance(), None, None, &ascii(), ROOMY)
        .expect("the page draws with nothing in the store");
    let drawn: String = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        drawn.contains("nothing has run in this session yet"),
        "the run section drew something for a session with no run:\n{drawn}",
    );
    assert!(
        drawn.contains("this session has no turns yet"),
        "the session section drew something for a session with no turns:\n{drawn}",
    );
    assert!(
        drawn.contains("nothing recorded"),
        "the grouped sections drew something for an empty store:\n{drawn}",
    );
    assert!(
        !drawn.contains("calls: 0"),
        "an empty store was reported as a run that made no calls:\n{drawn}",
    );
}

/// **The page's sections are the questions an operator asks, and each figure is
/// under the heading that names its scope.**
///
/// Seven lists on one page — this run, this session, by model, by day — and a
/// reader has to be able to tell which figure belongs to which question. A page
/// that reported one run's cost under the heading of a whole conversation would be
/// the wrong number under the right word, which is why the session total is walked
/// turn by turn rather than taken from `spend_by_run`.
///
/// Sabotage: report `Total::of` over the run's calls under `this session` as well
/// — which on a one-turn fixture like this one produces the same number, so the
/// second turn below is what makes the test able to tell.
#[test]
fn the_session_total_is_every_turn_and_not_the_run_in_flight() {
    let store = Store::memory().expect("an in-memory store");
    let session = store.create_session("/repo").expect("a session");
    let mut runs = Vec::new();
    for _ in 0..2 {
        let run = store.start_run("summarise", "/repo").expect("a run");
        store
            .record_turn(session, None, run, "summarise")
            .expect("a turn");
        store
            .record_provider_call(run, &call(Some(PRICED), Some(split())))
            .expect("recorded");
        runs.push(run);
    }

    let table = table();
    let one = table
        .cost_micros(PRICED, &split())
        .expect("one call's cost");
    let lines = cost::committed(
        &store,
        &table,
        &provenance(),
        Some(runs[1]),
        Some(session),
        &ascii(),
        ROOMY,
    )
    .expect("the page draws");
    let rows: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect();

    // Both sections exist, and they carry different numbers: one run against two
    // turns. The `cost` rows come in section order, so the first belongs to the
    // run in flight and the second to the conversation.
    let costs: Vec<&str> = rows
        .iter()
        .filter_map(|row| row.trim_start().strip_prefix("cost: "))
        .collect();
    assert_eq!(
        costs.first().copied(),
        Some(cost::money(one).as_str()),
        "the run section is not this run:\n{}",
        text(&rows),
    );
    assert_eq!(
        costs.get(1).copied(),
        Some(cost::money(one * 2).as_str()),
        "the session section is not every turn of the conversation:\n{}",
        text(&rows),
    );
    assert!(
        text(&rows).contains("across 2 turns"),
        "the session section does not say how many turns it walked:\n{}",
        text(&rows),
    );
}

/// **Every `$` on the page is backed by a sentence naming where the rate came
/// from and when it was read.**
///
/// io-cli reads prices from a catalogue the operator's own provider serves, and
/// for three of the four providers it knows that catalogue is a third party's —
/// OpenAI, Anthropic and Google publish no prices on any endpoint. A figure drawn
/// without that sentence attributes a number to a vendor who never published one.
///
/// The three cases are three different states of an install, and the middle one
/// is the one a hand-written file produces: prices with no record of where they
/// came from. It gets its date and an honest admission rather than an invented
/// source.
///
/// Sabotage: fall through to "prices: none configured" whenever `source` is
/// `None`, and a hand-written table reads as no table at all.
#[test]
fn the_footer_names_the_catalogue_and_the_date_or_admits_it_cannot() {
    let fixture = seeded(&[call(Some(PRICED), Some(split()))]);

    let full = text(&page(&fixture, &table(), &provenance()));
    assert!(
        full.contains("417 models") && full.contains("2026-08-27"),
        "the footer does not say how many models the rates cover, or when:\n{full}",
    );
    assert!(
        full.contains("openrouter.ai"),
        "the footer does not name the catalogue the rates came from:\n{full}",
    );

    let dated_only = Provenance {
        as_of: Some("2026-08-27".into()),
        source: None,
        models: Some(3),
    };
    let hand_written = text(&page(&fixture, &table(), &dated_only));
    assert!(
        hand_written.contains("2026-08-27"),
        "a hand-written table lost its date:\n{hand_written}",
    );
    assert!(
        hand_written.contains("did not record"),
        "a table with no recorded source got one invented for it:\n{hand_written}",
    );

    let none = text(&page(
        &fixture,
        &PriceTable::new(""),
        &Provenance::default(),
    ));
    assert!(
        none.contains("prices: none configured"),
        "an install with no prices does not say so at the foot of the page:\n{none}",
    );
}

/// **`cost::table` gives one code path rather than two, and `Provenance::of`
/// reads the date from the section that is actually pricing the calls.**
///
/// The date lives under `[prices]`, which io-harness owns and validates; the
/// source and the model count live under `[app.io-cli.prices]`, which io-harness
/// deliberately does not. That split is not filing: `[prices]` is
/// `deny_unknown_fields` and carries exactly `as_of` and `models`, so a key of
/// io-cli's own put beside them would not be ignored — it would make the
/// operator's whole configuration file fail to parse, taking the policy, the
/// providers and the run budgets down with it.
///
/// Sabotage: read `as_of` off io-cli's own section, and a table an operator
/// edited by hand reports the date of the last catalogue read instead of the date
/// on the rates in force.
#[test]
fn the_table_in_force_and_its_date_come_from_the_section_that_prices_the_calls() {
    let configured = io_harness::Config::from_toml(
        "[prices]\nas_of = \"2026-08-27\"\n\n[prices.models]\n\
         \"claude-sonnet-4.5\" = { input = 3000000, output = 15000000 }\n",
    )
    .expect("io-harness accepts the fixture");

    let table = cost::table(&configured);
    assert_eq!(table.as_of(), "2026-08-27");
    assert!(table.price(PRICED).is_some());

    let provenance = Provenance::of(&configured, None);
    assert_eq!(provenance.as_of.as_deref(), Some("2026-08-27"));
    assert_eq!(
        provenance.source, None,
        "a file with no io-cli section got a source invented for it",
    );
    // **`None`, and asserting `0` here was asserting the defect.** A `[prices]`
    // an operator wrote by hand has no `[app.io-cli.prices]` beside it, so nothing
    // recorded how many models it prices — which is not the same as its pricing
    // none. The field was a `usize` defaulting to zero, and the footer read
    // "0 models" over a table pricing four hundred.
    assert_eq!(
        provenance.models, None,
        "an unrecorded count was reported as a measured zero",
    );

    // No `[prices]` at all: an empty table dated to nothing, under which every
    // call is unpriced and every token figure is still exact.
    let bare = io_harness::Config::from_toml("").expect("an empty configuration");
    let empty = cost::table(&bare);
    assert_eq!(empty.as_of(), "");
    assert_eq!(empty.price(PRICED), None);
    assert_eq!(
        Provenance::of(&bare, None).as_of,
        None,
        "an install with no prices reported a date it does not have",
    );
}
