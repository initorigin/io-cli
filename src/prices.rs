//! Where a price comes from, what it claims, and when it was read.
//!
//! **io-cli compiles no prices in, and this module is the reason it does not have
//! to.** io-harness prices a call from a [`PriceTable`] the operator's
//! configuration carries, and that table has to be filled from somewhere. Filling
//! it from a list baked into the binary would be a promise the binary cannot keep:
//! providers move prices without announcing it, a release cadence is not a pricing
//! cadence, and an operator reading a confident wrong number is worse off than one
//! reading no number at all. So the table is filled from the catalogue the
//! operator's own provider serves — which io-cli was already fetching and already
//! throwing away.
//!
//! # The call was already there
//!
//! [`crate::verify::catalogue`] has read the model catalogue since 0.1.0, to offer
//! the wizard a list of models. It mapped every [`ModelInfo`] down to its `id` and
//! discarded the `price`, `price_tiers` and `price_source` on the same row. This
//! module keeps them. No JSON is parsed here and no dependency is added: io-harness
//! did the parsing and the unit conversion, and `ModelInfo::price` arrives already
//! normalised into [`Price`]'s micro-units per million tokens.
//!
//! # Whose price it is
//!
//! For three of the four providers io-cli knows, the answer is not "the vendor's".
//! OpenAI, Anthropic and Google publish no prices on any endpoint — their model
//! endpoints carry capabilities and limits and no cost field, and their cost APIs
//! report what was *spent* rather than what a token *costs*. The reference
//! catalogue does carry rows for their models, which is why
//! [`crate::verify::catalogue`] strips the `anthropic/` and `openai/` prefixes off
//! them to build its list.
//!
//! So an operator on Anthropic gets prices *about* Anthropic *from* the reference
//! catalogue, and every surface that draws money says which. io-harness models the
//! distinction already — [`PriceSource::Vendor`] against
//! [`PriceSource::Reference`] — and this module carries it through rather than
//! flattening it into the connected provider's name. On OpenRouter the two
//! coincide, which [`crate::verify::catalogue`] already says in as many words: the
//! reference catalogue is OpenRouter's own, so for OpenRouter it is not a
//! reference at all.
//!
//! # Two files, because one of them is not ours
//!
//! The table itself lives under `[prices]`, which io-harness owns and reads
//! through `Config::prices`. That section is `deny_unknown_fields` and carries
//! exactly `as_of` and `models` — so a key of io-cli's own put beside them would
//! not be ignored, it would make the operator's whole configuration file
//! unreadable. Anything this module needs to remember that io-harness does not
//! model therefore lives under `[app.io-cli.prices]`, which is the section
//! io-harness deliberately does not validate.
//!
//! # Nothing here reads a clock
//!
//! `tests/timing.rs` permits `SystemTime::now` in the driver and nowhere else, so
//! the date a table is stamped with is handed in rather than taken. [`date`] is
//! the conversion, and it is a pure function of a number for that reason as much
//! as for testability.

use io_harness::pricing::Price;
use io_harness::{ModelInfo, PriceSource, ProviderSpec};

use crate::edit::Edit;

/// The catalogue io-harness reads when nothing names another.
///
/// Re-exported rather than re-spelled: a second copy of this URL in this
/// repository would be a second thing to move when io-harness moves it.
pub use io_harness::provider::catalog::DEFAULT_REFERENCE_URL;

/// A catalogue read: what it priced, where it came from, and when.
///
/// `rows` is already filtered to the models the connected provider actually
/// serves and spelled the way that provider names them — the same filter and the
/// same stripping [`crate::verify::catalogue`] applies, because the key a price is
/// stored under has to match the `model` io-harness records on a provider call,
/// and that is the name the operator configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalogue {
    /// The date this was read, `YYYY-MM-DD`. Not a date any provider supplied:
    /// every timestamp on offer anywhere is a model *release* date, and no
    /// provider publishes when it last changed a price.
    pub as_of: String,
    /// Where the rows came from, in io-harness's own terms.
    pub source: PriceSource,
    /// One row per priced model. A model the catalogue served with no price is
    /// **absent**, never entered at zero, so `Spend::unpriced_calls` counts it
    /// and the page that draws it can say the total is a floor.
    pub rows: Vec<(String, Price)>,
    /// How many models the catalogue served in total, priced or not. Kept
    /// separately from `rows.len()` because a catalogue that answered with plenty
    /// of models and no prices is a different failure from one that answered with
    /// nothing, and the operator is owed the difference.
    pub served: usize,
}

impl Catalogue {
    /// The rows a catalogue read yields, filtered and named for `spec`.
    ///
    /// `models` is what the catalogue served; `as_of` is the date the caller
    /// read it. Separated from the fetch so the whole of this decision is
    /// testable without a socket — the fetch itself is one io-harness call and
    /// has nothing in it worth a test double.
    pub fn of(spec: &ProviderSpec, models: Vec<ModelInfo>, as_of: impl Into<String>) -> Self {
        let served = models.len();
        let vendor = matches!(spec, ProviderSpec::OpenRouter { .. });
        let rows = crate::verify::priced(spec, models);
        Self {
            as_of: as_of.into(),
            // **The one place the two coincide.** The reference catalogue is
            // OpenRouter's own, so for an OpenRouter operator it is the vendor
            // speaking for itself and calling it a reference would understate it.
            // For everyone else it is a third party and saying otherwise would
            // attribute a number to a vendor that never published one.
            source: if vendor {
                PriceSource::Vendor
            } else {
                PriceSource::Reference(DEFAULT_REFERENCE_URL.to_string())
            },
            rows,
            served,
        }
    }

    /// Whether this read is too short to replace `existing` rows.
    ///
    /// **The one failure in this release that loses money quietly.** A truncated
    /// or partial catalogue response that replaced a full table with a handful of
    /// rows would turn most of an operator's spending into "unpriced" and shrink
    /// their reported bill without anything failing. So a replacement that comes
    /// back empty, or far shorter than what it would replace, is refused and the
    /// old table kept.
    ///
    /// A first fill has nothing to compare against and is never refused: an
    /// operator with no table has nothing to lose.
    pub fn too_short(&self, existing: usize) -> bool {
        if existing == 0 {
            return false;
        }
        self.rows.is_empty() || self.rows.len() * 2 < existing
    }

    /// The edits that write this catalogue into a configuration file.
    ///
    /// One `set` for the date and one per model, rather than one edit rewriting
    /// the section whole. That is not a smaller diff for its own sake: a rate is
    /// something the operator is invited to correct by hand, and a row per line is
    /// what makes correcting one a one-line change they can find again.
    ///
    /// **Rows for models the catalogue no longer serves are left alone.** They are
    /// not stale — io-harness prices a call by the model name on it, so an old row
    /// is what prices an old run correctly, and `/cost` reports history as well as
    /// today.
    pub fn edits(&self) -> Vec<Edit> {
        let mut edits = vec![Edit::set("prices.as_of", quoted(&self.as_of))];
        for (model, price) in &self.rows {
            edits.push(Edit::set(
                format!("prices.models.{}", quoted(model)),
                inline(price),
            ));
        }
        edits
    }
}

/// What a refresh would change about one model's rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The model, spelled as the price table keys it.
    pub model: String,
    /// What the table in force says now, or `None` for a model it does not price.
    pub was: Option<Price>,
    /// What the catalogue served.
    pub now: Price,
}

/// Every rate a refresh would move, against the table in force.
///
/// **Shown before anything is written, which is why this is separate from
/// [`Catalogue::edits`].** io-cli cannot tell a rate the operator corrected by
/// hand from one an older catalogue served — the file records a number and not
/// where it came from — so it does not guess. It shows what would move and lets
/// the operator decline the lot, which is the shape `/import` established in
/// 0.21.0: everything shown before anything is written.
///
/// A model the table does not price yet is a change with `was: None`. A rate that
/// has not moved is not a change and is not listed, so a refresh that found
/// nothing new says so in one line rather than in four hundred.
pub fn changes(existing: Option<&io_harness::pricing::PriceTable>, fresh: &Catalogue) -> Vec<Change> {
    fresh
        .rows
        .iter()
        .filter_map(|(model, now)| {
            let was = existing.and_then(|table| table.price(model));
            (was != Some(*now)).then(|| Change {
                model: model.clone(),
                was,
                now: *now,
            })
        })
        .collect()
}

/// A `Price` as a one-line TOML inline table, naming only what is charged for.
///
/// A dimension the vendor does not charge for is left out rather than written as
/// a zero, because `Price` defaults every field and a file that spells three
/// zeros to say "nothing" is three lines an operator has to read to learn nothing.
/// `..Price::ZERO` is io-harness's own advice for constructing one, and this is
/// the same decision spelled in TOML.
fn inline(price: &Price) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (name, value) in [
        ("input", price.input),
        ("output", price.output),
        ("cache_read", price.cache_read),
        ("cache_write", price.cache_write),
        ("per_server_tool_request", price.per_server_tool_request),
    ] {
        if value != 0 {
            parts.push(format!("{name} = {value}"));
        }
    }
    if parts.is_empty() {
        // A model every dimension of which is free still has to round-trip as a
        // `Price`, and `{}` is the inline table that means exactly that.
        return "{}".to_string();
    }
    format!("{{ {} }}", parts.join(", "))
}

/// A string as TOML source, escaped by the TOML crate rather than by hand.
///
/// Model ids carry dots, slashes and colons, and a dot in a key can only be
/// spelled quoted — bare keys are letters, digits, `_` and `-` and nothing else.
/// So this is not decoration: an unquoted `gpt-4.1` is two path segments and
/// reaches the wrong place, or no place. `edit::array` sets the precedent of
/// spelling TOML through `toml` rather than with `format!`.
fn quoted(text: &str) -> String {
    toml::Value::String(text.to_string()).to_string()
}

/// `YYYY-MM-DD`, UTC, from a count of seconds since the Unix epoch.
///
/// **A pure function of a number, and that is a gate rather than a preference.**
/// `tests/timing.rs` permits `SystemTime::now` in the driver and refuses it in
/// every other file under `src/`, so the driver reads the clock and hands the
/// number here. It also happens to be the only shape in which a date conversion
/// can be tested without owning the machine's clock.
///
/// Howard Hinnant's `civil_from_days`, whose era begins on 0000-03-01 so that the
/// leap day falls at the end of a year and the month arithmetic needs no table.
/// Correct for every date the epoch can express; io-cli has no use for one before
/// 1970 and does not pretend to handle one.
///
/// ```
/// assert_eq!(io_cli::prices::date(0), "1970-01-01");
/// assert_eq!(io_cli::prices::date(1_772_150_400), "2026-02-27");
/// ```
pub fn date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// How a price source reads on a surface that draws money.
///
/// `PriceSource` is `#[non_exhaustive]`, so the `_` arm is required rather than
/// defensive — and it says "a catalogue" rather than guessing, because a variant
/// io-harness gains and this release has not seen is one whose name io-cli would
/// be inventing.
pub fn source_word(source: &PriceSource) -> String {
    match source {
        PriceSource::Vendor => "the provider's own catalogue".to_string(),
        PriceSource::Reference(url) => format!("the reference catalogue at {url}"),
        _ => "a catalogue this release does not recognise".to_string(),
    }
}
