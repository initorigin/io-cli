//! Where a price comes from, what it claims, and when it was read.
//!
//! **io-cli compiles no prices in, and this module is the reason it does not have
//! to.** io-harness prices a call from a [`io_harness::pricing::PriceTable`] the operator's
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
//! module keeps the price and the source. No JSON is parsed here and no dependency
//! is added: io-harness did the parsing and the unit conversion, and
//! `ModelInfo::price` arrives already normalised into [`Price`]'s micro-units per
//! million tokens.
//!
//! **`price_tiers` is read and deliberately dropped, and the consequence is
//! stated rather than hidden.** io-harness's `[prices]` section carries `as_of`
//! and a map of `Price`, and is `deny_unknown_fields` — there is no TOML surface
//! for a tier, and `PriceTier` is constructible only in Rust. So a model whose
//! vendor charges more above a long-prompt threshold is priced here at its base
//! rate, and `/cost` cannot mark that as a floor because nothing in the store says
//! which calls crossed the line. It is an **understated** figure rather than an
//! invented one, and it is in the release record as a known limitation.
//!
//! # Whose price it is
//!
//! For two of the three vendors io-cli can connect to, the answer is not "the
//! vendor's". OpenAI and Anthropic publish no prices on any endpoint — their model
//! endpoints carry capabilities and limits and no cost field, and their cost APIs
//! report what was *spent* rather than what a token *costs*. The reference
//! catalogue does carry rows for their models, which is why
//! [`crate::verify::catalogue`] strips the `anthropic/` and `openai/` prefixes off
//! them to build its list.
//!
//! The fourth `ProviderSpec` is `Compatible`, and the reference catalogue cannot
//! speak for it at all: a list of one vendor's models says nothing about what a
//! server it has never heard of serves. That operator names their own catalogue
//! with `app.io-cli.prices.source_url`, which [`Catalogue::named`] reads without
//! narrowing.
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
        Self::read(spec, models, as_of, None)
    }

    /// The same, from a catalogue the operator named rather than the default.
    ///
    /// **A named catalogue is not narrowed, and that is the whole of what
    /// `app.io-cli.prices.source_url` means.** The default catalogue is one
    /// vendor's view of the entire field, so its rows have to be cut down to the
    /// provider in force — and for a `compatible` endpoint that cut can only
    /// remove everything, because a reference list cannot say what a server it has
    /// never heard of serves. An operator who set `source_url` has answered that:
    /// they pointed io-cli at the catalogue their own endpoint publishes, so every
    /// row of it is theirs and the ids are already spelled the way their provider
    /// names them.
    ///
    /// The source is recorded as that URL rather than as the vendor, because it is
    /// still not the vendor speaking — it is a list the operator chose, and a page
    /// that drew money under the provider's name would be attributing it wrongly.
    pub fn named(
        spec: &ProviderSpec,
        models: Vec<ModelInfo>,
        as_of: impl Into<String>,
        url: &str,
    ) -> Self {
        Self::read(spec, models, as_of, Some(url))
    }

    fn read(
        spec: &ProviderSpec,
        models: Vec<ModelInfo>,
        as_of: impl Into<String>,
        url: Option<&str>,
    ) -> Self {
        if let Some(url) = url.filter(|url| !url.is_empty()) {
            let served = models.len();
            let rows = crate::verify::priced(models);
            return Self {
                as_of: as_of.into(),
                source: PriceSource::Reference(url.to_string()),
                rows,
                served,
            };
        }
        let vendor = matches!(spec, ProviderSpec::OpenRouter { .. });
        // **Counted after the filter, not before it.** The reference catalogue
        // serves every model in the field; the number worth reporting is how many
        // of them *this* provider serves, because that is the denominator the
        // operator is being told their prices cover. Counting before the filter
        // told an Anthropic operator their fifteen rates covered fifteen of four
        // hundred and seventeen models, which is true of the catalogue and
        // meaningless about them.
        let mine = crate::verify::named(spec, models);
        let served = mine.len();
        let rows = crate::verify::priced(mine);
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
    /// A row per model rather than one enormous inline table, because a rate is
    /// something the operator is invited to correct by hand and a row per line is
    /// what makes correcting one a change they can find again. For OpenRouter that
    /// is four hundred rows; as a single inline value it would be one
    /// twenty-five-kilobyte line.
    ///
    /// **`has_section` decides the shape, and getting it wrong does not merely
    /// produce a worse file — it produces no file.** Every edit in a batch is
    /// resolved against the document as it was before the batch, so `set` on four
    /// hundred keys of a `[prices.models]` that does not exist yet appends four
    /// hundred `[prices.models]` headers and the read-back refuses the lot. A
    /// first fill therefore writes the section whole, once; a refresh sets key by
    /// key into the section that is already there.
    ///
    /// It is a parameter rather than something derived here because **it is a
    /// question about the file, and this type has never seen the file.** The
    /// obvious substitutes are both wrong: io-cli's own record of how many models
    /// it last wrote is zero for a `[prices]` an operator wrote by hand, and
    /// asking the `PriceTable` whether it prices any model this catalogue serves
    /// answers `false` for a real section whose models the provider has since
    /// replaced. [`has_models_section`] asks the file.
    ///
    /// That split has a second effect worth naming, because it is the behaviour
    /// rather than an artefact: on a refresh, **rows for models the catalogue no
    /// longer serves are left alone.** They are not stale. io-harness prices a
    /// call by the model name recorded on it, so an old row is exactly what prices
    /// an old run correctly, and `/cost` reports history as well as today.
    pub fn edits(&self, has_section: bool) -> Vec<Edit> {
        let mut edits = vec![Edit::set("prices.as_of", quoted(&self.as_of))];
        if !has_section {
            let body = self
                .rows
                .iter()
                .map(|(model, price)| format!("{} = {}", quoted(model), inline(price)))
                .collect::<Vec<_>>()
                .join("\n");
            edits.push(Edit::section("prices.models", body));
            return edits;
        }
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
pub fn changes(
    existing: Option<&io_harness::pricing::PriceTable>,
    fresh: &Catalogue,
) -> Vec<Change> {
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

/// How a file spells its price table, which decides how it can be written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// No `[prices.models]` anywhere. The section is created whole.
    Absent,
    /// `[prices.models]` with a row per model, which is what io-cli writes. Rows
    /// are set key by key, so models nobody named survive.
    Table,
    /// A `[prices.models."<id>"]` sub-table per model — legal TOML that
    /// io-harness reads perfectly well, and that io-cli cannot safely rewrite.
    SubTables,
}

/// How `text` spells its price table.
///
/// **The question [`Catalogue::edits`] needs answered, asked of the only thing
/// that can answer it**, and it has three answers rather than two.
///
/// A `[prices]` carrying an `as_of` and no models is a real state — a refresh
/// whose catalogue priced nothing leaves exactly that — so the test is for the
/// child table and not the parent.
///
/// [`Shape::SubTables`] is the one worth spelling out. `Price` deserializes from a
/// sub-table as happily as from an inline one, so an operator writing this section
/// by hand may well write `[prices.models."gpt-4.1"]` with the rates under it, and
/// io-harness will read it. io-cli cannot update it: setting a key named
/// `"gpt-4.1"` beside a table of the same name is a duplicate-key error, and
/// creating `[prices.models]` above the sub-tables collides with every id in both.
/// **So it is refused rather than attempted** — the operator owns this section,
/// and a writer that cannot express their shape should say so and leave it alone,
/// not produce a refusal from the TOML parser that names neither.
///
/// Reads through [`crate::edit::sections`], the same header walk `edit::apply`
/// itself does, so the two cannot disagree about what is in the file.
/// Whether `text` carries a `[prices.models]` table io-cli can set keys into.
///
/// The two-answer form of [`shape`], for [`Catalogue::edits`], which only needs to
/// know whether to create the section or write into it. [`Shape::SubTables`]
/// answers `false` here and must be refused by the caller *before* it gets this
/// far — see [`shape`] for why io-cli cannot write that spelling at all.
pub fn has_models_section(text: &str) -> bool {
    matches!(shape(text), Shape::Table)
}

pub fn shape(text: &str) -> Shape {
    let mut found = Shape::Absent;
    for path in crate::edit::sections(text) {
        if path.len() < 2 || path[0] != "prices" || path[1] != "models" {
            continue;
        }
        if path.len() > 2 {
            // Any sub-table settles it: this file is not one io-cli can edit, and
            // no later `[prices.models]` header makes it one.
            return Shape::SubTables;
        }
        found = Shape::Table;
    }
    found
}

/// Why io-cli will not write prices into `text`, if it will not.
///
/// One sentence for the operator, or `None` to go ahead. The only refusal today is
/// [`Shape::SubTables`], and it is a refusal rather than an attempt because the
/// operator owns this section: a writer that cannot express their spelling should
/// say so and leave their file alone.
pub fn refusal(text: &str) -> Option<String> {
    match shape(text) {
        Shape::SubTables => Some(
            "no prices were written: this file spells its models as \
             `[prices.models.\"<id>\"]` sub-tables, which io-harness reads and io-cli cannot \
             safely rewrite — the rates already there go on pricing your calls, and a refresh \
             means editing them by hand"
                .to_string(),
        ),
        Shape::Absent | Shape::Table => None,
    }
}

/// How many models `text`'s own `[prices.models]` table prices.
///
/// **What [`Catalogue::too_short`] should have been comparing against all along.**
/// It was handed `app.io-cli.prices.models`, io-cli's record of its own last write
/// — which is absent for a hand-written `[prices]`, absent for every install that
/// predates this release, and absent on the first fill. So `existing` was zero in
/// exactly the cases the guard exists for, and the guard was off. `PriceTable` has
/// no length, but the file has a row per model and can simply be counted.
pub fn priced_in(text: &str) -> usize {
    let Ok(document) = text.parse::<toml::Value>() else {
        return 0;
    };
    document
        .get("prices")
        .and_then(|prices| prices.get("models"))
        .and_then(|models| models.as_table())
        .map_or(0, |models| models.len())
}

/// The `[app.io-cli.prices]` edits that record where a table came from.
///
/// **The same defect as [`Catalogue::edits`]'s, one section over, and it is here
/// rather than at the call site because that is where it was.** The driver used to
/// push two bare `Edit::set`s for `source` and `models`. Neither section exists in
/// any file io-cli has ever written — `settings::render` writes `prices: None` and
/// the field is `skip_serializing_if` — so both fell to the append arm, each
/// emitted its own `[app.io-cli.prices]` header, and the read-back refused the
/// whole write. Every first fill would have ended in "the edit would have produced
/// a file that does not parse", including the one the wizard makes, and `/cost`
/// would have reported tokens forever.
///
/// It survived the fix that added [`crate::edit::Edit::section`] because the two
/// edits it applies to are added by the driver, and nothing under `tests/` can
/// link the driver. Putting them here is what makes them testable.
pub fn bookkeeping(source: &str, models: usize, has_section: bool) -> Vec<Edit> {
    if has_section {
        return vec![
            Edit::set("app.io-cli.prices.source", quoted(source)),
            Edit::set("app.io-cli.prices.models", models.to_string()),
        ];
    }
    vec![Edit::section(
        "app.io-cli.prices",
        format!("source = {}\nmodels = {models}", quoted(source)),
    )]
}

/// What a refresh found, committed before anything is written.
///
/// **Everything shown before a byte moves, and the reason is sharper than
/// courtesy.** io-cli cannot tell a rate the operator corrected by hand from one
/// an older catalogue served: the file records a number, not where it came from.
/// So it does not guess. It lists every rate that would move, with what it was
/// and what it would become, and an operator who finds their own correction in
/// that list can decline the whole refresh — which is the same answer `/import`
/// gives to the same problem, and the only honest one available.
///
/// A refusal is reported here too rather than by the caller, so the sentence that
/// explains a short catalogue lives beside the rule that refuses it.
pub fn report(
    catalogue: &Catalogue,
    moved: &[Change],
    existing: usize,
    theme: &crate::theme::Theme,
    width: u16,
) -> Vec<ratatui::text::Line<'static>> {
    let mut rows: Vec<crate::page::Row> = Vec::new();
    let source = source_word(&catalogue.source);
    rows.push(crate::page::Row::note(format!(
        "{source}, read {}: {} model{} served, {} priced",
        catalogue.as_of,
        catalogue.served,
        if catalogue.served == 1 { "" } else { "s" },
        catalogue.rows.len(),
    )));

    if catalogue.too_short(existing) {
        rows.push(crate::page::Row::caveat(format!(
            "that is short enough against the {existing} model{} you already have to be a \
             truncated read rather than a price change, so nothing was written and the prices \
             you have were kept",
            if existing == 1 { "" } else { "s" }
        )));
        return crate::page::commit("prices", &rows, theme, width);
    }

    // **A catalogue that answered with nothing is not a catalogue that agreed with
    // you**, and this arm exists because the one below it used to swallow the
    // case. `verify::served` returns an empty vector for a network failure, a
    // refused key and an unparseable body alike, so with no rows there is nothing
    // to compare and `moved` is empty — under which the next arm reported "no rate
    // has moved since the last read", a positive claim about a read that did not
    // happen. An operator on a flaky connection would have been told their prices
    // were confirmed current.
    if catalogue.rows.is_empty() {
        rows.push(crate::page::Row::caveat(format!(
            "the catalogue could not be read, or served no prices — {} model{} came back. \
             Nothing was written and the prices you have are unchanged, which means they are \
             as old as their date says and not as fresh as this attempt.",
            catalogue.served,
            if catalogue.served == 1 { "" } else { "s" }
        )));
        return crate::page::commit("prices", &rows, theme, width);
    }
    if moved.is_empty() {
        rows.push(crate::page::Row::note(
            "no rate has moved since the last read, so nothing was written",
        ));
        return crate::page::commit("prices", &rows, theme, width);
    }

    rows.push(crate::page::Row::Blank);
    rows.push(crate::page::Row::heading(format!(
        "{} rate{} would change",
        moved.len(),
        if moved.len() == 1 { "" } else { "s" }
    )));
    for change in moved {
        let value = match change.was {
            Some(was) => format!("{} -> {}", rate(&was), rate(&change.now)),
            // A model the table has never priced. Said as "new" rather than shown
            // as a change from nothing, because an operator scanning for what
            // moved is asking a different question from one asking what arrived.
            None => format!("new, {}", rate(&change.now)),
        };
        rows.push(crate::page::Row::fact(change.model.clone(), value));
    }
    rows.push(crate::page::Row::Blank);
    rows.push(crate::page::Row::caveat(
        "if one of these is a rate you corrected by hand, it is about to be replaced: \
         io-cli records what a rate is and not where it came from, so it cannot tell yours \
         from an older catalogue's. Decline and edit the file if so.",
    ));
    crate::page::commit("prices", &rows, theme, width)
}

/// One rate, as input and output per million tokens.
///
/// The two dimensions that move a bill, and not the other three: a refresh report
/// is read to answer "did this get more expensive", and five numbers per row is a
/// table nobody scans. The whole rate is in the file for anyone who wants it.
fn rate(price: &Price) -> String {
    format!(
        "{} in / {} out",
        crate::cost::money(price.input),
        crate::cost::money(price.output)
    )
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
///
/// Public because the driver writes two values of its own beside the table — the
/// source and the count — and `format!("\"{text}\"")` at a call site is a quoting
/// rule reimplemented, which is how the wrong one eventually gets written.
pub fn quoted(text: &str) -> String {
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
    let doe = z - era * 146_097;
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
