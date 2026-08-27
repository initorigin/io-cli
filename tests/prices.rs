//! Where a price comes from, what it is spelled as, and what it refuses.
//!
//! **Four separate claims live in `src/prices.rs` and only one of them is
//! arithmetic.** The date conversion is a pure function of a number and can be
//! checked against a calendar; the rest are claims about *naming* — which id a
//! rate is filed under, whose catalogue it is attributed to, and whether the row
//! survives a trip through a real configuration file and back out of io-harness.
//! A test that only exercised `Catalogue::of` in memory would pass on every one
//! of those and still ship a release that writes nothing an operator can read.
//!
//! So the end of this file is deliberately not a unit test. It takes the edits
//! the module produces, runs them through `io_cli::edit::apply` over a
//! hand-written `io.toml`, hands the result to `io_harness::Config::from_toml`,
//! and asks the resulting `PriceTable` what a call on that model costs. That
//! round trip is the property — every step before it is a step towards it, and a
//! module that got the spelling wrong is a module whose prices exist in a file
//! nothing can read.

use io_cli::edit;
use io_cli::prices::{self, Catalogue, DEFAULT_REFERENCE_URL};
use io_harness::pricing::{Price, PriceTable};
use io_harness::{Config, ModelInfo, PriceSource, ProviderSpec};

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A configuration naming one provider, held so a `&ProviderSpec` can be taken
/// off it.
///
/// **Built through io-harness's own parser rather than as a struct literal**, and
/// that is not ceremony. `ProviderSpec` is `#[non_exhaustive]` and its
/// `Compatible` variant carries seven fields; a literal here would be a second
/// spelling of a shape the harness owns, and it would stop compiling the release
/// the harness widens it — which is the wrong test failing for the wrong reason.
/// A `[[provider]]` table is also exactly what an operator writes, so the fixture
/// and the field agree by construction.
fn configured(body: &str) -> Config {
    Config::from_toml(body).expect("the fixture is a configuration io-harness accepts")
}

fn openrouter() -> Config {
    configured("[[provider]]\nkind = \"openrouter\"\nmodel = \"anthropic/claude-sonnet-4.5\"\n")
}

fn anthropic() -> Config {
    configured("[[provider]]\nkind = \"anthropic\"\nmodel = \"claude-sonnet-4.5\"\n")
}

fn openai() -> Config {
    configured("[[provider]]\nkind = \"openai\"\nmodel = \"gpt-4.1\"\n")
}

/// A provider the reference catalogue has never heard of and cannot speak for.
fn local() -> Config {
    configured(
        "[[provider]]\nkind = \"compatible\"\nbase_url = \"http://localhost:11434/v1\"\n\
         model = \"llama3.2\"\nauth = \"none\"\n",
    )
}

fn spec(config: &Config) -> &ProviderSpec {
    config.provider_spec().expect("the fixture names a provider")
}

/// One catalogue row the vendor put a rate on.
fn priced(id: &str, input: u64, output: u64) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        price: Some(Price {
            input,
            output,
            ..Price::ZERO
        }),
        price_source: Some(PriceSource::Vendor),
        ..Default::default()
    }
}

/// One catalogue row the vendor served and put no rate on.
///
/// **`price: None`, never `Some(Price::ZERO)`**, which is io-harness's own rule
/// stated on `ModelInfo::price` in as many words: zero is a rate and unknown is
/// not one, and a catalogue that conflated them would report a real bill as free.
fn unpriced(id: &str) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        price: None,
        price_source: None,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// `date`
// ---------------------------------------------------------------------------

/// Whether `year` carries a twenty-ninth of February, by the rule rather than by
/// the shortcut.
///
/// Divisible by four, except centuries, except centuries divisible by four
/// hundred. The third clause is the one a naive `% 4` gets wrong, and 2100 is the
/// first year the epoch can express where it matters — which is why the sweep
/// below runs past it.
fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if leap(year) {
                29
            } else {
                28
            }
        }
    }
}

/// **The date is the civil calendar, checked against a second calendar rather
/// than against itself.**
///
/// `prices::date` is Howard Hinnant's `civil_from_days`: an era of four hundred
/// years, a year-of-era, a day-of-year and two integer divisions that reconstruct
/// the month with no lookup table. It is correct and it is completely opaque, and
/// a test that asserted `date(1_772_150_400) == "2026-02-27"` would be asserting
/// that somebody once ran the function and wrote down what it said.
///
/// So there are two halves here. The first is a handful of dates an ordinary
/// person can verify by counting — the epoch itself, a leap day with the day
/// either side of it, a year boundary with the day either side of *it*, and
/// 2100-03-01, which follows a February the century rule shortens back to
/// twenty-eight. The second is a walk: an independent day-by-day calendar,
/// written out above with the leap rule spelled in full, stepped forward from the
/// epoch for a hundred and ten years and compared against `date` at every single
/// step. The two implementations share nothing but the definition of a day.
///
/// Sabotage: drop the `|| year % 400 == 0` clause from Hinnant's era arithmetic —
/// which is to say make 2000 an ordinary century — and the walk fails on
/// 2000-02-29 while every hand-written date before it still passes. Or drop the
/// `if month <= 2 { year + 1 }` correction, and every January and February in the
/// range moves back a year while the whole of March to December stays right.
#[test]
fn the_date_is_the_civil_calendar_and_not_a_transcription_of_the_function() {
    // Counted by hand. 1970-01-01 is day zero; 2000-01-01 is thirty years of
    // three hundred and sixty-five days plus the seven leap days of 1972 to
    // 1996, which is 10,957 days, which is 946,684,800 seconds.
    for (secs, expected) in [
        (0u64, "1970-01-01"),
        // The last second of the first day. A conversion that divided wrongly
        // would tip over here rather than at midnight.
        (86_399, "1970-01-01"),
        (86_400, "1970-01-02"),
        // A year boundary, with the day either side of it.
        (946_598_400, "1999-12-31"),
        (946_684_800, "2000-01-01"),
        // A leap day, with the day either side of it. 2000 is the century that
        // *is* a leap year, which is the rule's hardest case in this range.
        (951_696_000, "2000-02-28"),
        (951_782_400, "2000-02-29"),
        (951_868_800, "2000-03-01"),
        // And the century that is not. 2100 is divisible by four and has no
        // twenty-ninth of February, so the first of March follows the
        // twenty-eighth directly.
        (4_107_456_000, "2100-02-28"),
        (4_107_542_400, "2100-03-01"),
    ] {
        assert_eq!(
            prices::date(secs),
            expected,
            "{secs} seconds after the epoch is {expected}",
        );
    }

    // The seconds inside a day do not reach the answer. Asserted rather than
    // assumed, because the whole point of taking a `u64` of seconds is that the
    // caller hands over a clock reading and not a date.
    for offset in [0u64, 1, 3_600, 43_200, 86_399] {
        assert_eq!(
            prices::date(951_782_400 + offset),
            "2000-02-29",
            "a time of day moved the date",
        );
    }

    // The walk. Forty thousand days is 1970-01-01 through the middle of 2079,
    // which covers twenty-seven leap years, one leap century and — with the
    // hand-written pair above — the century that is not one.
    let (mut year, mut month, mut day) = (1970i64, 1i64, 1i64);
    for index in 0..40_000u64 {
        assert_eq!(
            prices::date(index * 86_400),
            format!("{year:04}-{month:02}-{day:02}"),
            "the two calendars disagree {index} days after the epoch",
        );
        day += 1;
        if day > days_in(year, month) {
            day = 1;
            month += 1;
        }
        if month > 12 {
            month = 1;
            year += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// `Catalogue::of`
// ---------------------------------------------------------------------------

/// **On OpenRouter the reference catalogue is the vendor, and saying otherwise
/// would understate it.**
///
/// The catalogue io-cli reads *is* OpenRouter's own `/api/v1/models`, so an
/// OpenRouter operator is being quoted the provider's published rate by the
/// provider. Calling that a reference would attach a hedge to the one case where
/// no hedge is owed. The ids are also kept whole — `anthropic/claude-sonnet-4.5`
/// is how OpenRouter names the model, and it is therefore what io-harness records
/// on the provider call and what the price has to be filed under.
///
/// Sabotage: strip the vendor prefix for OpenRouter too, which reads as a tidy-up
/// and is the one change that makes every OpenRouter row unfindable — the key in
/// the table becomes `claude-sonnet-4.5` and the `model` on every call stays
/// `anthropic/claude-sonnet-4.5`, so `Spend::unpriced_calls` counts the lot and
/// `/cost` reports a floor of zero without a single thing failing.
#[test]
fn openrouter_keeps_the_catalogues_own_spelling_and_is_quoted_as_the_vendor() {
    let config = openrouter();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("anthropic/claude-sonnet-4.5", 3_000_000, 15_000_000),
            priced("openai/gpt-4.1", 2_000_000, 8_000_000),
        ],
        "2026-08-27",
    );

    assert_eq!(catalogue.as_of, "2026-08-27");
    assert_eq!(catalogue.served, 2);
    assert_eq!(
        catalogue.source,
        PriceSource::Vendor,
        "the catalogue is OpenRouter's own, so for OpenRouter it is not a reference",
    );
    let ids: Vec<&str> = catalogue.rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["anthropic/claude-sonnet-4.5", "openai/gpt-4.1"],
        "an id was re-spelled, so the key will not match the model on a call",
    );
}

/// **Anthropic and OpenAI lose the prefix and gain the hedge, and both halves
/// matter.**
///
/// Neither vendor publishes a price on any endpoint, so the numbers come from a
/// third party's catalogue and every surface that draws money has to say so. The
/// spelling moves in the opposite direction from the attribution: the catalogue
/// namespaces the model (`anthropic/claude-sonnet-4.5`) and the operator's own
/// configuration does not (`claude-sonnet-4.5`), and it is the operator's
/// spelling that io-harness records on the call.
///
/// The rows a prefix does not match are dropped entirely rather than kept under
/// their catalogue name. An OpenAI operator has no use for a row keyed
/// `anthropic/claude-sonnet-4.5`; it can never match a call they make, and it
/// would inflate the model count `/cost` reports and the count `too_short`
/// measures a later read against.
///
/// Sabotage: keep the whole catalogue for these two providers rather than
/// filtering it, and this test fails on the row count while every other test in
/// this file — including the round trip — still passes.
#[test]
fn anthropic_and_openai_lose_their_prefix_and_are_quoted_as_a_reference() {
    let served = || {
        vec![
            priced("anthropic/claude-sonnet-4.5", 3_000_000, 15_000_000),
            priced("anthropic/claude-opus-4.1", 15_000_000, 75_000_000),
            // Served by this provider, and priced by nobody. It is what makes
            // `served` and `rows.len()` different numbers below.
            unpriced("anthropic/claude-experimental-preview"),
            priced("openai/gpt-4.1", 2_000_000, 8_000_000),
            priced("google/gemini-2.5-pro", 1_250_000, 10_000_000),
        ]
    };

    let config = anthropic();
    let catalogue = Catalogue::of(spec(&config), served(), "2026-08-27");
    let ids: Vec<&str> = catalogue.rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["claude-opus-4.1", "claude-sonnet-4.5"],
        "the prefix survived, or a model this provider does not serve did",
    );
    assert_eq!(
        catalogue.source,
        PriceSource::Reference(DEFAULT_REFERENCE_URL.to_string()),
        "Anthropic publishes no prices, so a rate about Anthropic is not from Anthropic",
    );
    // **`served` counts what THIS provider serves, and `rows` counts what of it
    // carries a rate.** The two are deliberately different numbers, and the gap is
    // the sentence `fill_prices` writes: "priced two of the three models the
    // reference catalogue serves". Counting the reference's whole four-hundred-row
    // answer here would tell an Anthropic operator their rates cover two of four
    // hundred, which is true of the catalogue and meaningless about them.
    assert_eq!(catalogue.served, 3, "three of the fixture are Anthropic's");
    assert_eq!(catalogue.rows.len(), 2, "two of those three carry a rate");

    let config = openai();
    let catalogue = Catalogue::of(spec(&config), served(), "2026-08-27");
    let ids: Vec<&str> = catalogue.rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["gpt-4.1"]);
    assert_eq!(
        catalogue.source,
        PriceSource::Reference(DEFAULT_REFERENCE_URL.to_string()),
    );
}

/// **A provider the reference cannot speak for gets no rows at all.**
///
/// A `compatible` endpoint is a URL and a model slug: a proxy, a gateway, a
/// runtime on a port. The reference catalogue has never heard of it, and the only
/// thing it could offer is a row whose slug happens to collide — `llama3.2` on
/// somebody's laptop is not `meta-llama/llama-3.2-3b-instruct` on OpenRouter, and
/// pricing a local run at a hosted rate would be an invented bill.
///
/// `served` is zero for the same reason `rows` is: the count is of what *this*
/// provider serves, and the reference cannot say that a local endpoint serves
/// anything at all. Reporting the reference's own four hundred here would be the
/// same confident wrong answer in a different column.
///
/// Sabotage: make the `_` arm of `verify::named` return `models` unchanged, and a
/// local endpoint gains a full price table for models it does not run.
#[test]
fn a_provider_the_reference_cannot_speak_for_yields_no_rows() {
    let config = local();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("meta-llama/llama-3.2-3b-instruct", 20_000, 20_000),
            priced("llama3.2", 20_000, 20_000),
        ],
        "2026-08-27",
    );

    assert!(
        catalogue.rows.is_empty(),
        "a local endpoint was priced from a catalogue that has never seen it: {:?}",
        catalogue.rows,
    );
    assert_eq!(
        catalogue.served, 0,
        "the reference claimed to know what this endpoint serves",
    );
}

/// **A model the catalogue served with no price is ABSENT, never entered at
/// zero.**
///
/// This is the single most consequential line in the module and it is one that
/// leaves no trace when it goes wrong. `PriceTable::price` returning `None` is
/// what makes `Spend::unpriced_calls` count the call, which is what makes
/// `Total::is_floor` true, which is what makes `/cost` say the number it just
/// drew is a floor rather than a total. A row entered as `Price::ZERO` satisfies
/// every one of those checks — the model *is* priced, at nothing — so the call is
/// counted as free, the total is reported as complete, and every surface agrees
/// with every other surface about a number that is wrong.
///
/// io-harness says the same thing on `ModelInfo::price` itself: "never
/// `Price::ZERO` to mean unknown". This asserts io-cli honours it.
///
/// Sabotage: `.map(|m| (m.id, m.price.unwrap_or(Price::ZERO)))` in
/// `verify::priced` — five characters that read as a tidy-up of an `Option`, and
/// under which this is the only test in the repository that fails.
#[test]
fn a_model_the_catalogue_priced_at_nothing_is_absent_rather_than_zero() {
    let config = openrouter();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("anthropic/claude-sonnet-4.5", 3_000_000, 15_000_000),
            unpriced("some-lab/experimental-preview"),
            unpriced("another-lab/unreleased"),
        ],
        "2026-08-27",
    );

    assert_eq!(catalogue.served, 3, "all three were served");
    let ids: Vec<&str> = catalogue.rows.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["anthropic/claude-sonnet-4.5"],
        "an unpriced model reached the table, where it will be counted as free",
    );
    // And the proof that absence is what the table sees: the row that is missing
    // is missing from the `PriceTable` a write of these rows would produce.
    let table = catalogue
        .rows
        .iter()
        .fold(
            PriceTable::new(catalogue.as_of.clone()),
            |table, (id, price)| table.with(id.clone(), *price),
        );
    assert_eq!(table.price("some-lab/experimental-preview"), None);
    assert!(table.price("anthropic/claude-sonnet-4.5").is_some());
}

// ---------------------------------------------------------------------------
// `Catalogue::too_short`
// ---------------------------------------------------------------------------

/// **The refusal that stops a truncated read from shrinking a bill in silence.**
///
/// Every other failure in this area is loud: a catalogue that cannot be reached
/// leaves the operator with no prices and a sentence saying so. A catalogue that
/// answers with four rows where four hundred were expected is the quiet one — the
/// write succeeds, the file parses, `/cost` draws a smaller number, and nothing
/// anywhere says the table lost three hundred and ninety-six models.
///
/// The rule has three arms and each is a different judgement. A first fill has
/// nothing to lose and is never refused, whatever it holds. An empty answer
/// against a table that has rows is refused outright — a replacement that prices
/// nothing is not a price change. And an answer under half the size of what it
/// would replace is refused as a truncation, while an answer merely *smaller* is
/// allowed through, because vendors do retire models and a rule that refused
/// every shrinkage would freeze the table the first time one did.
///
/// Sabotage: return `false` for the empty case — under which the emptiest possible
/// answer, the one a half-read socket produces, is the one answer that is always
/// accepted.
#[test]
fn a_first_fill_is_never_refused_and_a_truncated_answer_always_is() {
    let config = openrouter();
    let of = |count: usize| {
        Catalogue::of(
            spec(&config),
            (0..count)
                .map(|index| priced(&format!("vendor/model-{index}"), 1_000_000, 2_000_000))
                .collect(),
            "2026-08-27",
        )
    };

    // A first fill. Nothing to compare against, so nothing is refused — including
    // the empty answer, which the caller handles separately by saying no prices
    // were written rather than by keeping a table that does not exist.
    assert!(!of(0).too_short(0), "a first fill has nothing to lose");
    assert!(!of(1).too_short(0));
    assert!(!of(400).too_short(0));

    // An empty answer against a table with rows in it. Refused at every size,
    // including one, which is the case a `> existing / 2` rule would let through.
    assert!(of(0).too_short(1), "an empty answer replaced a real table");
    assert!(of(0).too_short(400));

    // Drastically shorter: refused. Similar: not. The line sits at half, and
    // both sides of it are asserted so that moving it fails this rather than
    // passing quietly in whichever direction it moved.
    assert!(of(4).too_short(400), "a truncated read replaced a full table");
    assert!(of(199).too_short(400), "under half is a truncation");
    assert!(
        !of(200).too_short(400),
        "exactly half is allowed: vendors do retire models",
    );
    assert!(!of(380).too_short(400), "a slightly smaller answer is a normal one");
    assert!(!of(420).too_short(400), "a larger answer is never a truncation");
}

// ---------------------------------------------------------------------------
// `Catalogue::edits` — the round trip
// ---------------------------------------------------------------------------

/// The `[prices]` section a file already carrying prices has.
const EXISTING: &str = "\
[[provider]]
kind = \"openai\"
model = \"gpt-4.1\"

[prices]
as_of = \"2026-01-01\"

[prices.models]
\"gpt-4o\" = { input = 2500000, output = 10000000 }
";

/// A file written by the wizard: a provider, io-cli's own section, and no prices
/// at all. This is what a first fill is applied to.
const FRESH: &str = "\
[[provider]]
kind = \"anthropic\"
model = \"claude-sonnet-4.5\"

[app.io-cli]
theme = \"dark\"
";

/// **A model id with a dot in it survives the write, and the proof is that
/// io-harness prices a call with it afterwards.**
///
/// A bare TOML key is `A-Za-z0-9_-` and nothing else. `gpt-4.1` written bare is
/// not a key with a dot in it — it is two path segments, `gpt-4` and `1`, and the
/// rate lands in a table nothing reads. Nothing fails: the file parses, the
/// section exists, `Config::prices()` returns a table, and `price("gpt-4.1")`
/// answers `None` forever.
///
/// So the assertion deliberately does not stop at the text. It applies the edits
/// to a real file, parses the result with io-harness's own loader, and asks the
/// resulting `PriceTable` what the model costs — because the text containing
/// `"gpt-4.1"` and the table answering to `gpt-4.1` are two different claims and
/// only the second one is the feature.
///
/// Sabotage: drop the `quoted` call in `Catalogue::edits` and spell the path with
/// `format!("prices.models.{model}")`. The file still parses, this test fails on
/// `price`, and nothing else in the repository notices.
#[test]
fn a_dotted_model_id_survives_the_write_and_prices_a_call() {
    let config = openai();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("openai/gpt-4.1", 2_000_000, 8_000_000),
            priced("openai/gpt-4.1-mini", 400_000, 1_600_000),
            priced("openai/o3", 2_000_000, 8_000_000),
        ],
        "2026-08-27",
    );

    // The flag comes from the file rather than from a literal, because that pair
    // is the feature: `has_models_section` asks the document and `edits` chooses
    // its shape from the answer, and a test that hard-coded `true` here would pass
    // against a reader that always said so.
    assert!(
        prices::has_models_section(EXISTING),
        "the fixture is meant to be the refresh case",
    );
    let written = edit::apply(EXISTING, &catalogue.edits(prices::has_models_section(EXISTING)))
        .expect("the edits produce a configuration file that parses");

    // The spelling, so a failure here names the cause rather than the symptom.
    assert!(
        written.contains("\"gpt-4.1\""),
        "the dotted id was written bare, which is two keys and not one:\n{written}",
    );

    // The property. io-harness reads the file it would really read, and the table
    // it builds answers to the id io-harness would really record on a call.
    let reloaded = Config::from_toml(&written).expect("io-harness accepts the written file");
    let table = reloaded.prices().expect("the file carries a price table");
    assert_eq!(table.as_of(), "2026-08-27", "the date did not move");
    assert_eq!(
        table.price("gpt-4.1"),
        Some(Price {
            input: 2_000_000,
            output: 8_000_000,
            ..Price::ZERO
        }),
        "the dotted id reached a key io-harness cannot find:\n{written}",
    );
    assert!(table.price("gpt-4.1-mini").is_some());
    assert!(table.price("o3").is_some());

    // **A row the catalogue no longer serves is left alone**, which is the module's
    // own rule and not an accident of the splice: io-harness prices a call by the
    // model name recorded on it, so the old row is what prices an old run
    // correctly and `/cost` reports history as well as today.
    assert_eq!(
        table.price("gpt-4o"),
        Some(Price {
            input: 2_500_000,
            output: 10_000_000,
            ..Price::ZERO
        }),
        "a model the catalogue stopped serving lost the rate that prices its history",
    );
}

/// **The first fill: the case every new install takes, against a file that has no
/// `[prices]` section at all.**
///
/// The test above starts from a file that already carries `[prices.models]`, so
/// every model edit finds a section to insert a key into. That is the *second*
/// fill. The first one — the wizard writes `io.toml`, the credential check
/// passes, `fill_prices` reads the catalogue and writes it — starts from a file
/// with neither `[prices]` nor `[prices.models]` in it, and it is the only path
/// an operator takes on day one.
///
/// **Every edit in a batch is resolved against the document as it was BEFORE the
/// batch**, which is why the two cases cannot share a shape. `edit::apply` walks
/// the file's headers once and every `set` is answered from that walk — so four
/// hundred `set`s into a `[prices.models]` that does not exist yet each fall
/// through to the append arm and each emit their own `[prices.models]` header. The
/// file gains four hundred definitions of one table, TOML refuses the lot, and the
/// only thing an operator sees is "no prices were written". The first fill could
/// not have worked at all.
///
/// So a first fill writes the section once, whole, and this test is the one that
/// says so: exactly one header, every model in it, and the whole thing readable
/// back through io-harness. The header count is asserted directly rather than left
/// to the parse, because a duplicate is only *usually* a parse error — a second
/// header carrying keys the first did not would parse on some shapes and silently
/// split the table on others.
///
/// Sabotage: pass `true` here, or make `has_models_section` answer on the parent
/// `[prices]` rather than on the child — under which this fails with a duplicate
/// table and every other test in this file still passes.
#[test]
fn a_first_fill_writes_every_model_into_a_file_that_has_no_prices_section() {
    let config = anthropic();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("anthropic/claude-sonnet-4.5", 3_000_000, 15_000_000),
            priced("anthropic/claude-opus-4.1", 15_000_000, 75_000_000),
            priced("anthropic/claude-haiku-4.5", 1_000_000, 5_000_000),
        ],
        "2026-08-27",
    );
    assert_eq!(catalogue.rows.len(), 3, "the fixture prices three models");
    assert!(
        !prices::has_models_section(FRESH),
        "the fixture is meant to be the first-fill case",
    );

    let edits = catalogue.edits(prices::has_models_section(FRESH));
    assert_eq!(
        edits.len(),
        2,
        "a first fill is the date and the section, however many models it holds",
    );

    let written = edit::apply(FRESH, &edits).unwrap_or_else(|error| {
        panic!(
            "a first fill of three models produced a file that does not parse, so an operator's \
             very first `/cost` reports tokens and no money: {error}"
        )
    });

    assert_eq!(
        written.matches("[prices.models]").count(),
        1,
        "the fill wrote the table more than once:\n{written}",
    );

    let reloaded = Config::from_toml(&written).expect("io-harness accepts the written file");
    let table = reloaded.prices().expect("the file carries a price table");
    assert_eq!(table.as_of(), "2026-08-27");
    for (model, price) in &catalogue.rows {
        assert_eq!(
            table.price(model).as_ref(),
            Some(price),
            "`{model}` is not in the table the first fill wrote:\n{written}",
        );
    }
    // Nothing else in the file moved. The provider the operator just configured
    // is the one thing a broken splice is most likely to take with it.
    assert_eq!(
        reloaded.provider_spec(),
        config.provider_spec(),
        "the write disturbed the provider:\n{written}",
    );

    // **And it is not a special case that only works at three.** Four hundred is
    // what an OpenRouter operator's first fill really is, and it is the size at
    // which the old shape produced four hundred headers rather than two.
    let config = openrouter();
    let many = Catalogue::of(
        spec(&config),
        (0..400)
            .map(|index| priced(&format!("vendor-{index}/model-4.1"), 1_000_000, 2_000_000))
            .collect(),
        "2026-08-27",
    );
    let written = edit::apply(FRESH, &many.edits(false))
        .expect("four hundred models are one section, not four hundred");
    assert_eq!(written.matches("[prices.models]").count(), 1);
    let table = Config::from_toml(&written)
        .expect("io-harness accepts it")
        .prices()
        .expect("the file carries a table");
    assert!(
        (0..400).all(|index| table.price(&format!("vendor-{index}/model-4.1")).is_some()),
        "a model was lost from a four-hundred-row first fill",
    );
}

/// **`has_models_section` asks the file, and asks about the child table rather
/// than the parent.**
///
/// The question `edits` needs answered is whether a `set` will find somewhere to
/// insert a key, and that is a question about `[prices.models]` and nothing else.
/// A `[prices]` carrying an `as_of` and no models is a real state — it is exactly
/// what a refresh whose catalogue priced nothing leaves behind — and answering
/// `true` for it sends the next fill down the `set` path into a section that is
/// not there.
///
/// The two substitutes that look equivalent are both wrong, and the file is the
/// only thing that is not: io-cli's own record of how many models it last wrote
/// is zero for a `[prices]` an operator typed by hand, and asking the loaded
/// `PriceTable` whether it prices anything this catalogue serves answers `false`
/// for a real section whose models the provider has since replaced.
///
/// Sabotage: drop the `path.len() == 2` test and match on any path beginning
/// `prices.models` — under which a file whose models are written as
/// `[prices.models."gpt-4.1"]` sub-tables answers `true`, takes the `set` path,
/// and lands its rows in a section that does not exist.
#[test]
fn the_models_section_is_looked_for_in_the_file_and_not_inferred() {
    assert!(prices::has_models_section(EXISTING));
    assert!(!prices::has_models_section(FRESH));

    // A `[prices]` with a date and no models: the state a refresh that priced
    // nothing leaves, and the one an inference off the parent gets wrong.
    assert!(
        !prices::has_models_section("[prices]\nas_of = \"2026-01-01\"\n"),
        "a `[prices]` with no models was read as a models section",
    );

    // An unparseable file answers `false`, which sends the caller down the create
    // path — where `apply` refuses it with a complaint about the file rather than
    // about the prices, which is the truthful one.
    assert!(!prices::has_models_section("[prices\nas_of ="));
}

/// **On a refresh it is one edit per model, because a rate is something an
/// operator is invited to correct by hand.**
///
/// The alternative — one edit rewriting `[prices.models]` whole, which is what the
/// first fill does because it must — produces the same file and a very different
/// consequence. `Edit::section` replaces nothing and refuses a section that
/// exists, precisely so that a refresh cannot take this path: a whole-section
/// write discards every row nobody named, which on a refresh is every model the
/// catalogue stopped serving *and* every rate the operator corrected by hand.
/// io-harness prices a call by the model name recorded on it, so those old rows
/// are exactly what price an old run correctly, and `/cost` reports history as
/// well as today.
///
/// The unpriced model is the other half: it contributes no edit at all, so a
/// model the catalogue served without a rate never reaches the file as a zero.
///
/// Sabotage: collect the rows into one `Edit::section` on the refresh path too —
/// under which the first `/config` refresh silently deletes every hand-corrected
/// rate and every model the provider retired, and the file still parses.
#[test]
fn a_refresh_writes_one_row_per_model_and_leaves_the_rest_alone() {
    let config = openrouter();
    let catalogue = Catalogue::of(
        spec(&config),
        vec![
            priced("a/one", 1_000_000, 2_000_000),
            priced("a/two", 1_000_000, 2_000_000),
            unpriced("a/three"),
        ],
        "2026-08-27",
    );

    let edits = catalogue.edits(true);
    assert_eq!(
        edits.len(),
        1 + catalogue.rows.len(),
        "the date plus one row per PRICED model, and nothing for the unpriced one",
    );
    assert_eq!(edits[0].path(), "prices.as_of");
    assert!(
        edits[1..]
            .iter()
            .all(|edit| edit.path().starts_with("prices.models.")),
        "a refresh edit addresses something other than one model's row",
    );

    // And `Edit::section` refuses the file that already has one, which is what
    // stops the two paths being interchangeable by accident.
    let refused = edit::apply(EXISTING, &catalogue.edits(false))
        .expect_err("a whole-section write over an existing section must be refused");
    assert!(
        refused.contains("already in this file"),
        "the refusal does not say why it refused: {refused}",
    );

    // A model with every dimension free still round-trips as a `Price` rather
    // than being dropped: `{}` is the inline table that says exactly that, and a
    // free model is a fact the table can state where an unpriced one is not.
    let free = Catalogue::of(
        spec(&config),
        vec![ModelInfo {
            id: "a/free".into(),
            price: Some(Price::ZERO),
            price_source: Some(PriceSource::Vendor),
            ..Default::default()
        }],
        "2026-08-27",
    );
    let written = edit::apply(EXISTING, &free.edits(true)).expect("a free model is writable");
    let table = Config::from_toml(&written)
        .expect("io-harness accepts it")
        .prices()
        .expect("the file carries a table");
    assert_eq!(
        table.price("a/free"),
        Some(Price::ZERO),
        "a model that costs nothing is priced at nothing, which is not the same as unpriced",
    );
}

// ---------------------------------------------------------------------------
// `changes`
// ---------------------------------------------------------------------------

/// **A refresh lists what moved and nothing else, and a model with no row yet is
/// `was: None` rather than a change from zero.**
///
/// The distinction is the difference between two sentences an operator reads
/// differently: "this got cheaper, from nothing" is nonsense, and "this is new"
/// is the answer. A `was` defaulted to `Price::ZERO` would render every arrival
/// as a price rise from free, which on a four-hundred-model catalogue turns the
/// first refresh after an upstream addition into four hundred rows of noise with
/// the real change buried in it.
///
/// The other half is the silence. A rate that has not moved is not a change, so a
/// refresh that found nothing says so in one line rather than reprinting the
/// whole table — and an operator who has been shown four hundred unchanged rows
/// once will not read the list the time it matters.
///
/// Sabotage: compare on the model id rather than on the price, and every refresh
/// lists every model forever.
#[test]
fn changes_lists_only_what_moved_and_calls_an_unpriced_model_new() {
    let config = openrouter();
    let fresh = Catalogue::of(
        spec(&config),
        vec![
            // Moved.
            priced("a/dearer", 4_000_000, 16_000_000),
            // Unchanged, to the micro-unit.
            priced("a/steady", 1_000_000, 2_000_000),
            // Not in the table at all.
            priced("a/arrived", 500_000, 1_500_000),
        ],
        "2026-08-27",
    );

    let existing = PriceTable::new("2026-01-01")
        .with(
            "a/dearer",
            Price {
                input: 3_000_000,
                output: 12_000_000,
                ..Price::ZERO
            },
        )
        .with(
            "a/steady",
            Price {
                input: 1_000_000,
                output: 2_000_000,
                ..Price::ZERO
            },
        );

    let moved = prices::changes(Some(&existing), &fresh);
    let names: Vec<&str> = moved.iter().map(|change| change.model.as_str()).collect();
    // **Alphabetical, because `Catalogue::rows` is and `changes` preserves it** —
    // which is a property worth pinning rather than working around. A refresh
    // report an operator reads twice should list the same rates in the same order
    // both times, and catalogue order is whatever a remote server felt like.
    assert_eq!(
        names,
        ["a/arrived", "a/dearer"],
        "an unchanged rate was listed, a changed one was not, or the order is not stable",
    );

    // Found by name rather than by index, so a future ordering change fails the
    // assertion above — which is about order — instead of silently moving these,
    // which are about values.
    let by = |model: &str| {
        moved
            .iter()
            .find(|change| change.model == model)
            .unwrap_or_else(|| panic!("`{model}` is not in {names:?}"))
    };

    let dearer = by("a/dearer");
    assert_eq!(dearer.was.map(|price| price.input), Some(3_000_000));
    assert_eq!(dearer.now.input, 4_000_000);

    let arrived = by("a/arrived");
    assert_eq!(
        arrived.was, None,
        "a model the table has never priced is new, not a change from zero",
    );
    assert_eq!(arrived.now.output, 1_500_000);

    // Nothing moved: nothing listed. Including the case an operator hits most
    // often, which is running `/config` twice in a day.
    assert!(
        prices::changes(Some(&existing), &Catalogue::of(spec(&config), vec![
            priced("a/steady", 1_000_000, 2_000_000)
        ], "2026-08-27"))
        .is_empty(),
        "an unchanged table reported a change",
    );

    // No table at all: every row is new. The first fill, from the other side.
    let first = prices::changes(None, &fresh);
    assert_eq!(first.len(), 3);
    assert!(
        first.iter().all(|change| change.was.is_none()),
        "a model was reported as having had a rate in a table that does not exist",
    );
}

// ---------------------------------------------------------------------------
// `source_word`
// ---------------------------------------------------------------------------

/// **The sentence under every `$` on every page, and it names the catalogue
/// rather than the connected provider.**
///
/// An operator on Anthropic is being shown numbers about Anthropic that Anthropic
/// never published. Flattening that into "Anthropic's prices" would attribute a
/// figure to a vendor who did not state it, which is the one claim this whole
/// module exists to avoid making. So the reference form carries the URL — the
/// operator can go and read the source — and the vendor form does not, because
/// there is nothing to point at that is not the provider they are already
/// connected to.
///
/// The `_` arm is required rather than defensive: `PriceSource` is
/// `#[non_exhaustive]`, so io-harness can add a variant this release has never
/// seen, and the honest answer for one is that io-cli does not recognise it — not
/// a name io-cli invented for it.
///
/// Sabotage: make the `_` arm say "the provider's own catalogue", and a future
/// harness variant is silently attributed to the vendor.
#[test]
fn source_word_names_the_catalogue_and_never_the_connected_provider() {
    let vendor = prices::source_word(&PriceSource::Vendor);
    assert_eq!(vendor, "the provider's own catalogue");
    assert!(
        !vendor.contains("http"),
        "the vendor form points at a URL the operator did not need",
    );

    let reference = prices::source_word(&PriceSource::Reference(
        DEFAULT_REFERENCE_URL.to_string(),
    ));
    assert!(
        reference.contains(DEFAULT_REFERENCE_URL),
        "the reference form does not say which reference: {reference}",
    );
    assert!(
        reference.contains("reference"),
        "the hedge is the word, not the URL: {reference}",
    );

    // The URL is io-harness's own constant re-exported, not a second copy of it
    // in this repository — which is what makes it move when the harness moves it.
    assert_eq!(
        DEFAULT_REFERENCE_URL,
        io_harness::provider::catalog::DEFAULT_REFERENCE_URL,
    );
}

// ---------------------------------------------------------------------------
// The filter, and why it is applied exactly once
// ---------------------------------------------------------------------------

/// **`verify::named` is not idempotent, and that is why `priced` no longer
/// filters.**
///
/// The two used to be one call each, and both filtered: `Catalogue::of` handed the
/// raw catalogue to `named` to get its count and the raw catalogue to `priced` to
/// get its rows, and `priced` filtered again internally. Applied twice to an
/// Anthropic catalogue the second pass finds no `anthropic/` prefix left to strip
/// — every id has already lost it — and `strip`'s `strip_prefix(prefix)?` discards
/// the row rather than keeping it. The result is a table with nothing in it, from
/// a catalogue that priced fifteen models, with nothing failing anywhere.
///
/// This test states the non-idempotence directly rather than trusting the call
/// sites, because the property is what makes the call sites' shape mandatory: a
/// future edit that "tidies up" by filtering inside `priced` again is not a
/// duplicate of work already done, it is an erasure.
///
/// The OpenRouter arm is the control. Its filter is the identity, so applying it
/// twice changes nothing — which is exactly why a test written only against
/// OpenRouter would have passed against the broken shape.
///
/// Sabotage: put the `named` call back inside `verify::priced` — under which this
/// fails on the Anthropic arm and `Catalogue::of` starts returning an empty table
/// for both providers whose prefix it strips.
#[test]
fn the_catalogue_filter_is_applied_exactly_once_because_it_is_not_idempotent() {
    let config = anthropic();
    let raw = vec![
        priced("anthropic/claude-sonnet-4.5", 3_000_000, 15_000_000),
        priced("openai/gpt-4.1", 2_000_000, 8_000_000),
    ];

    let once = io_cli::verify::named(spec(&config), raw.clone());
    assert_eq!(once.len(), 1, "one of the two is Anthropic's");
    assert_eq!(once[0].id, "claude-sonnet-4.5", "the prefix is stripped");

    let twice = io_cli::verify::named(spec(&config), once.clone());
    assert!(
        twice.is_empty(),
        "the filter is idempotent after all, and this test is asserting nothing: {:?}",
        twice.iter().map(|model| &model.id).collect::<Vec<_>>(),
    );

    // So `priced` takes rows that have already been named and does not filter
    // again — which is the only shape under which `Catalogue::of` can both count
    // and price the same set.
    let rows = io_cli::verify::priced(once);
    assert_eq!(rows.len(), 1, "pricing filtered a second time");
    assert_eq!(rows[0].0, "claude-sonnet-4.5");

    // The control. OpenRouter's filter is the identity, so it survives being
    // applied twice — and a test written only against it would have passed
    // against the shape that lost every Anthropic row.
    let config = openrouter();
    let once = io_cli::verify::named(spec(&config), raw.clone());
    let twice = io_cli::verify::named(spec(&config), once.clone());
    assert_eq!(
        once.len(),
        twice.len(),
        "OpenRouter's filter is the identity and has to survive being applied twice",
    );
    assert_eq!(twice.len(), 2);
}

/// **`quoted` is the module's only speller, and it escapes rather than decorates.**
///
/// Model ids carry dots, slashes and colons, and a dot in a TOML key can only be
/// spelled quoted — a bare key is `A-Za-z0-9_-` and nothing else. So this is not
/// presentation: an unquoted `gpt-4.1` is two path segments and reaches the wrong
/// place, or no place. It is spelled through the `toml` crate rather than with a
/// `format!` for the case a hand-written quote gets wrong, which is a value
/// containing a quote or a backslash — and a model id from a catalogue is
/// somebody else's string.
///
/// Sabotage: `format!("\"{text}\"")` — which passes for every id anyone has ever
/// seen and produces a file that does not parse the first time a catalogue serves
/// one with a quote in it.
#[test]
fn a_model_id_is_spelled_by_the_toml_crate_and_not_by_a_format_string() {
    assert_eq!(prices::quoted("gpt-4.1"), "\"gpt-4.1\"");
    assert_eq!(prices::quoted("anthropic/claude-sonnet-4.5"), "\"anthropic/claude-sonnet-4.5\"");

    // The two a hand-written pair of quotes gets wrong. Both round-trip through
    // the parser rather than being compared to a spelling typed here, because the
    // escape rules are the `toml` crate's and this file should not have a second
    // opinion about them.
    for id in ["a\"b", "back\\slash", "new\nline"] {
        let spelled = prices::quoted(id);
        let parsed = toml::from_str::<toml::value::Table>(&format!("probe = {spelled}"))
            .unwrap_or_else(|error| {
                panic!("`{id}` was spelled as {spelled}, which is not TOML: {error}")
            });
        assert_eq!(
            parsed.get("probe").and_then(|value| value.as_str()),
            Some(id),
            "`{id}` was spelled as {spelled}, which parses back as something else",
        );
    }
}
