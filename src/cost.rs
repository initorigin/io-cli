//! What has been spent: `/cost`, and `/usage` under the name the field uses.
//!
//! **Every number here has been in `runs.db` since io-harness 0.18.0 and nothing
//! has ever read one.** The harness records a row per provider call — the model,
//! the prompt, completion, cache-read, cache-write and reasoning token split, the
//! latency and the time to first token — and io-cli reported token counts and
//! called the money question unanswerable. It was answerable four releases ago.
//!
//! # Nothing is estimated
//!
//! Every figure is a row already in the store, or that row multiplied by a rate
//! the operator can point at. There is no sampling, no extrapolation from a
//! partial window and no model of what a token "usually" costs. A call the
//! provider reported no usage for is **unknown**, never zero; a model with no rate
//! in the table is **unpriced**, and a total containing one is a floor rather than
//! a total, which this page says out loud. io-harness's own pricing documentation
//! calls a renderer that hides that count "lying by omission", and it is right.
//!
//! # The three honesty rules, and why each is a rule
//!
//! **Cache reads and writes are a breakdown of the prompt, not an addition to
//! it.** `Usage::cache_read_tokens` and `cache_write_tokens` are already inside
//! `prompt_tokens`; adding them would over-report every cached turn, which is most
//! of them. So this page shows the prompt total and the three parts under it, and
//! the parts sum to the total rather than past it.
//!
//! **Reasoning tokens are a breakdown of completion, likewise** — a vendor that
//! reports reasoning separately still bills it as output, and on Anthropic the
//! count is zero because thinking is billed inside `output_tokens` and never
//! split out.
//!
//! **A `None` usage is not a zero usage.** io-harness stores `total_tokens` as
//! SQL `NULL` for a call the provider said nothing about, and reads it back as
//! `None` for exactly that reason. Summing it as zero would report a turn that
//! cost something as a turn that cost nothing.
//!
//! # The unit
//!
//! io-harness prices in micro-units and does not name a currency, because it is
//! not the layer that knows one. io-cli does: every catalogue it can read quotes
//! USD, and every provider it can connect to bills in it. So `$` is written, and
//! the page names the catalogue the rates came from and the date they were read,
//! which is the claim behind the symbol.

use io_harness::pricing::{PriceTable, Spend};
use io_harness::{ProviderCall, Store, Usage};

use crate::page::{self, Row};
use crate::theme::Theme;

// # What this page costs to draw, stated rather than discovered
//
// The `this run` and `this session` sections are bounded by construction: one
// run's calls, or one conversation's turns. The `by model` and `by day` sections
// are not. `Store::spend_by_model` and `Store::spend_by_day` each read **every**
// `provider_calls` row in the store and group in Rust — pricing is a Rust value
// SQLite cannot see, so it cannot be a `GROUP BY` — which makes both linear in the
// whole store and growing for the life of the install.
//
// **io-cli does not bound them, and the reason is that it cannot bound them
// honestly.** A `LIMIT` would have to be applied to rows rather than to groups, so
// a bounded read would report the most recent N calls under headings that say
// "by model" and "by day" without qualification — a partial total presented as a
// whole one, which is the exact failure the rest of this module is built to
// avoid. There is no public API to ask io-harness for the last N days grouped, and
// `ProviderCall` carries no timestamp of its own for io-cli to group by instead.
//
// So the release measures the cost against a realistically sized store, records
// the number, and leaves the read whole. The bound belongs upstream, beside the
// grouping, and is reported there.

/// What one set of calls came to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Total {
    /// Calls made, failed attempts included — a retry costs what it costs.
    pub calls: u64,
    /// Calls the provider reported no usage for. **Unknown, not zero.**
    pub unknown: u64,
    /// Calls whose model the table does not price.
    pub unpriced: u64,
    /// The token split, summed. Cache and reasoning are breakdowns of the prompt
    /// and completion figures beside them, never additions to them.
    pub usage: Usage,
    /// Micro-units. A floor rather than a total whenever `unpriced` is non-zero.
    pub micros: u64,
}

impl Total {
    /// Sum a run's calls, priced by `table`.
    ///
    /// `table` is never `Option`: an operator with no prices gets an empty table,
    /// under which every call is unpriced and every token figure is still exact.
    /// One code path rather than two, and the no-price case is the same page with
    /// the money left off rather than a different page.
    pub fn of(calls: &[ProviderCall], table: &PriceTable) -> Self {
        let mut total = Self {
            calls: calls.len() as u64,
            ..Self::default()
        };
        for call in calls {
            let Some(usage) = call.usage else {
                total.unknown += 1;
                continue;
            };
            add(&mut total.usage, &usage);
            let priced = call
                .model
                .as_deref()
                .and_then(|model| table.cost_micros(model, &usage));
            match priced {
                Some(micros) => total.micros = total.micros.saturating_add(micros),
                None => total.unpriced += 1,
            }
        }
        total
    }

    /// Whether the money figure is a floor rather than a total.
    pub fn is_floor(&self) -> bool {
        self.unpriced > 0 || self.unknown > 0
    }
}

/// Add `from` into `into`, field by field.
///
/// Written out rather than derived: `Usage` is `#[non_exhaustive]`-adjacent in
/// spirit — io-harness adds a dimension when a vendor starts billing one — and a
/// field added upstream that this function silently dropped would under-report a
/// bill. Spelling every field means the compiler names the new one.
fn add(into: &mut Usage, from: &Usage) {
    into.prompt_tokens += from.prompt_tokens;
    into.completion_tokens += from.completion_tokens;
    into.total_tokens += from.total_tokens;
    into.cache_read_tokens += from.cache_read_tokens;
    into.cache_write_tokens += from.cache_write_tokens;
    into.reasoning_tokens += from.reasoning_tokens;
    into.server_tool_requests += from.server_tool_requests;
}

/// Where the prices came from, for the line at the foot of the page.
///
/// **The claim behind every `$` on the page**, and it is carried rather than
/// re-derived so that one sentence cannot drift from the figures above it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Provenance {
    /// The table's own `as_of`, or `None` when there is no table at all.
    pub as_of: Option<String>,
    /// The catalogue, in words. `None` when nothing has been read yet.
    pub source: Option<String>,
    /// How many models the table prices, as the last read recorded it.
    pub models: usize,
}

impl Provenance {
    /// What the configuration in force says about its prices.
    ///
    /// The date comes from io-harness's own `[prices]` section, because that is
    /// the table actually pricing the calls; the source and the count come from
    /// `[app.io-cli.prices]`, because io-harness models neither. A file carrying
    /// prices but no io-cli section — hand-written, or written by an older
    /// release — still gets its date, and the page says the source was not
    /// recorded rather than inventing one.
    pub fn of(config: &io_harness::Config, stored: Option<&crate::settings::CliSettings>) -> Self {
        let prices = stored.and_then(|settings| settings.prices.as_ref());
        Self {
            as_of: config.prices().map(|table| table.as_of().to_string()),
            source: prices.and_then(|p| p.source.clone()),
            models: prices.and_then(|p| p.models).unwrap_or(0),
        }
    }
}

/// The price table in force, or an empty one dated to nothing.
///
/// **One code path rather than two.** An operator with no prices gets a table
/// that prices nothing, under which every call is unpriced and every token figure
/// is still exact — so the page is the same page with the money left off, rather
/// than a second page for the unconfigured case that would go stale unread.
pub fn table(config: &io_harness::Config) -> PriceTable {
    config.prices().unwrap_or_else(|| PriceTable::new(""))
}

/// The `/cost` page.
///
/// `run` is the turn in flight or the last one; `session` is every turn of this
/// conversation. Both are `None` before anything has run, and the page says so
/// rather than drawing a row of zeroes — the same distinction the status line
/// holds every one of its counters to.
pub fn committed(
    store: &Store,
    table: &PriceTable,
    provenance: &Provenance,
    run: Option<i64>,
    session: Option<i64>,
    theme: &Theme,
    width: u16,
) -> Result<Vec<ratatui::text::Line<'static>>, String> {
    let mut rows: Vec<Row> = Vec::new();

    if provenance.as_of.is_none() {
        rows.push(Row::caveat(
            "no prices are configured, so this page reports tokens and no money. \
             Prices arrive from the catalogue your provider serves when you connect \
             one, and `/config` refreshes them."
                .to_string(),
        ));
        rows.push(Row::Blank);
    }

    rows.push(Row::heading("this run".to_string()));
    match run {
        Some(id) => {
            let calls = store.provider_calls(id).map_err(|e| e.to_string())?;
            rows.extend(section(&Total::of(&calls, table)));
        }
        None => rows.push(Row::note("nothing has run in this session yet")),
    }

    rows.push(Row::Blank);
    rows.push(Row::heading("this session".to_string()));
    match session {
        Some(id) => {
            let turns = store.session_turns(id).map_err(|e| e.to_string())?;
            let mut calls: Vec<ProviderCall> = Vec::new();
            for turn in &turns {
                calls.extend(store.provider_calls(turn.run_id).map_err(|e| e.to_string())?);
            }
            // **Walked rather than grouped, because io-harness offers no session
            // grouping.** `spend_by_run` keys on a run and `spend_by_day` on a
            // date; a session is neither, and reporting one run's cost under the
            // heading of a whole conversation would be the wrong number under the
            // right word.
            rows.extend(section(&Total::of(&calls, table)));
            rows.push(Row::note(format!(
                "across {} turn{}",
                turns.len(),
                if turns.len() == 1 { "" } else { "s" }
            )));
        }
        None => rows.push(Row::note("this session has no turns yet")),
    }

    rows.push(Row::Blank);
    rows.push(Row::heading("by model".to_string()));
    rows.extend(grouped(
        store.spend_by_model(table).map_err(|e| e.to_string())?,
    ));

    rows.push(Row::Blank);
    rows.push(Row::heading("by day".to_string()));
    rows.extend(grouped(store.spend_by_day(table).map_err(|e| e.to_string())?));

    rows.push(Row::Blank);
    rows.extend(footer(provenance));

    Ok(page::commit("cost", &rows, theme, width))
}

/// The rows for one [`Total`].
fn section(total: &Total) -> Vec<Row> {
    if total.calls == 0 {
        return vec![Row::note("no provider calls")];
    }
    let usage = &total.usage;
    let mut rows = vec![
        Row::fact("calls", total.calls.to_string()),
        Row::fact("cost", money(total.micros)),
        Row::fact("prompt", tokens(usage.prompt_tokens)),
    ];
    // The two cache figures are indented under the prompt they are part of, by
    // being named as parts of it rather than by being drawn further right — this
    // page has no columns to indent into, and a reader in `--plain` has only the
    // words.
    if usage.cache_read_tokens > 0 || usage.cache_write_tokens > 0 {
        rows.push(Row::fact(
            "  of which cache read",
            tokens(usage.cache_read_tokens),
        ));
        rows.push(Row::fact(
            "  of which cache written",
            tokens(usage.cache_write_tokens),
        ));
        rows.push(Row::fact(
            "  of which fresh",
            tokens(
                usage
                    .prompt_tokens
                    .saturating_sub(usage.cache_read_tokens)
                    .saturating_sub(usage.cache_write_tokens),
            ),
        ));
    }
    rows.push(Row::fact("completion", tokens(usage.completion_tokens)));
    if usage.reasoning_tokens > 0 {
        rows.push(Row::fact(
            "  of which reasoning",
            tokens(usage.reasoning_tokens),
        ));
    }
    if usage.server_tool_requests > 0 {
        rows.push(Row::fact(
            "provider tool requests",
            usage.server_tool_requests.to_string(),
        ));
    }
    rows.extend(caveats(total.unknown, total.unpriced));
    rows
}

/// The rows for a set of io-harness's own [`Spend`] groups.
fn grouped(spend: Vec<Spend>) -> Vec<Row> {
    if spend.is_empty() {
        return vec![Row::note("nothing recorded")];
    }
    let mut rows: Vec<Row> = Vec::new();
    for group in spend {
        let mut value = format!(
            "{} · {} call{}",
            money(group.cost_micros),
            group.calls,
            if group.calls == 1 { "" } else { "s" }
        );
        if group.usage.total_tokens > 0 {
            value.push_str(&format!(" · {} tok", tokens(group.usage.total_tokens)));
        }
        if group.unpriced_calls > 0 {
            // **Never hidden, and on the row rather than in a note below it.** A
            // reader scanning for the largest figure has to be able to see which
            // of them is incomplete without reading past the list.
            value.push_str(&format!(" · {} unpriced", group.unpriced_calls));
        }
        rows.push(Row::fact(group.key, value));
    }
    rows
}

/// The sentences that qualify a figure, if any qualify it.
fn caveats(unknown: u64, unpriced: u64) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    if unpriced > 0 {
        rows.push(Row::caveat(format!(
            "{unpriced} call{} used a model with no rate in the price table, so \
             the cost above is a floor and not a total",
            if unpriced == 1 { "" } else { "s" }
        )));
    }
    if unknown > 0 {
        rows.push(Row::caveat(format!(
            "{unknown} call{} reported no usage at all, which is unknown rather \
             than free — neither its tokens nor its cost is in the figures above",
            if unknown == 1 { " " } else { "s " }
        )));
    }
    rows
}

/// Where the prices came from and when, at the foot of the page.
fn footer(provenance: &Provenance) -> Vec<Row> {
    match (&provenance.as_of, &provenance.source) {
        (Some(as_of), Some(source)) => vec![Row::note(format!(
            "prices: {} model{} read from {source} on {as_of}",
            provenance.models,
            if provenance.models == 1 { "" } else { "s" }
        ))],
        (Some(as_of), None) => vec![Row::note(format!(
            "prices: {} model{}, dated {as_of}, from a source this install did not record",
            provenance.models,
            if provenance.models == 1 { "" } else { "s" }
        ))],
        _ => vec![Row::note(
            "prices: none configured".to_string(),
        )],
    }
}

/// Micro-units as money.
///
/// Four decimals below a unit and two above it: a turn costs a fraction of a cent
/// and a month costs dollars, and one precision cannot show both without either
/// rounding the turn to nothing or padding the month with noise. Integer
/// arithmetic throughout — a bill is not a floating-point quantity, and
/// `cost_micros` is exact.
pub fn money(micros: u64) -> String {
    if micros == 0 {
        return "$0".to_string();
    }
    let units = micros / 1_000_000;
    let rest = micros % 1_000_000;
    if units == 0 {
        format!("$0.{:04}", rest / 100)
    } else {
        format!("${units}.{:02}", rest / 10_000)
    }
}

/// A token count, in the spelling the status line already uses.
fn tokens(count: u64) -> String {
    crate::status::format_tokens(count)
}
