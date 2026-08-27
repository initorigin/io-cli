//! The `[[provider]]` array as the ordered fallback chain it has always been.
//!
//! io-harness has read this array as a chain since its 0.27.0 — the first entry
//! is the provider a run uses and each later one is the next link
//! ([`Config::provider_spec`] and [`Config::fallback_specs`]) — and no io-cli
//! release has offered a way to arrange it. This interface has even drawn an
//! event for a fallback happening without ever being able to cause one.
//!
//! # The order is the meaning
//!
//! Which is why reordering goes through [`crate::edit::Edit::move_entry`] rather
//! than through anything that rewrites the array: an entry has to arrive at its
//! new position with its own comments and its own keys, and a chain rebuilt from
//! io-cli's model would quietly drop whatever io-cli does not model.
//!
//! # The presets, and the door that is actually public
//!
//! io-harness reaches twenty-one vendors through one `Compatible` provider —
//! thirteen hosted and eight local runtimes — and **none of the ways it lists
//! them is public**: `PRESETS`, `preset_names()` and `preset_list()` are all
//! `pub(crate)`. What is public is twenty-one named constructors, and
//! [`Compatible::preset`], whose error names every preset that exists.
//!
//! So io-cli carries [`PRESETS`], its own list — and [`harness_presets`] reads
//! the real one back out of that error, so a gate can prove the two agree. **A
//! list nothing has to agree with is decoration**, and this one would drift into
//! an operator selecting a vendor and being told "unknown provider preset".
//!
//! # Arranging the chain, and the two numbers that must not be confused
//!
//! [`add`], [`promote`], [`demote`] and [`remove`] are the verbs, and three of
//! them address an entry by **position in a file's `[[provider]]` array**. That
//! is a different number from a row on screen the moment anything filters or
//! reorders the view, and getting it wrong here does not fail loudly: it moves a
//! provider an operator did not name to the front of the chain, and the next turn
//! bills to a vendor they did not choose. So those three take an [`At`], which
//! only [`At::of`] builds — by counting entries in the file's own bytes.
//!
//! `[[provider]]` is deliberately **not** one of io-harness's appending keys
//! (`config.rs:2052`): the winning scope replaces the chain whole, "because a
//! half-appended fallback chain is not a chain". So exactly one file decides the
//! whole array, and [`declared_at`] finds that file through [`decided`] — the
//! same origin [`chain`] already reads the credentials out of, so the position
//! and the text cannot come from two different files.
//!
//! # Exactly one of `preset` and `base_url`
//!
//! io-harness refuses a `compatible` entry that names both or neither, by index,
//! at load (`config.rs:456`). [`crate::configure::write`] would catch it on the
//! round trip and roll back, which is a good failure — but [`add`] takes an
//! [`Endpoint`] whose two `compatible` shapes are separate variants, so the entry
//! that fails cannot be constructed at all.

use io_harness::config::{Config, Scope};
use io_harness::{Compatible, ProviderSpec};

use crate::configure::Decided;
use crate::edit::Edit;
// **The twin this module used to keep its own copy of.** Both spelled a value
// the same way and neither escaped a control character; a model id is tame, but
// an `api_key` and a `base_url` are pasted text, and a raw newline inside a
// basic string is a parse error rather than a value. One copy, fixed once.
use crate::servers::quoted;

/// Every vendor preset io-harness reaches through `Compatible`.
///
/// Thirteen hosted, then eight runtimes that run on the operator's own machine.
/// The split is worth keeping in the order because it is the one an operator
/// chooses along: a key is needed for everything above the line and nothing
/// below it.
///
/// Proved against io-harness's own list by
/// `tests/providers.rs::f8_the_preset_list_is_the_harness_s_own`.
pub const PRESETS: &[&str] = &[
    // Hosted, and each wants a credential.
    "cerebras",
    "deepseek",
    "fireworks",
    "gemini",
    "groq",
    "minimax",
    "mistral",
    "moonshot",
    "perplexity",
    "qwen",
    "together",
    "xai",
    "zhipu",
    // On this machine, and none of them wants one.
    "jan",
    "koboldcpp",
    "llamacpp",
    "lmstudio",
    "localai",
    "ollama",
    "sglang",
    "vllm",
];

/// The presets io-harness itself knows, read out of its own error message.
///
/// `preset_names()` is `pub(crate)`, so this is the only public door to the real
/// list. [`Compatible::preset`] fails "naming the presets that do exist", which
/// is a documented property of that function rather than an accident of its
/// wording — its doc comment says so in those words.
///
/// Returns an empty vector if the message ever stops carrying them, which the
/// gate treats as a failure rather than as agreement: two empty lists comparing
/// equal is exactly the shape of a control that cannot fail.
pub fn harness_presets() -> Vec<String> {
    // A name no vendor will ever have. The call must fail for this to work, and
    // a name that accidentally existed would return `Ok` and no list at all.
    let Err(refusal) = Compatible::preset("\u{0}-not-a-preset", "", "") else {
        return Vec::new();
    };
    let message = refusal.to_string();
    let Some((_, listed)) = message.split_once("the presets are: ") else {
        return Vec::new();
    };
    listed
        .trim()
        .trim_end_matches('.')
        .split(", ")
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Whether a preset runs on the operator's own machine.
///
/// Which decides whether a credential is asked for at all. Derived from the base
/// URL io-harness resolves rather than from a second list here, so it cannot
/// disagree with the harness about where a preset points.
pub fn is_local(preset: &str) -> bool {
    Compatible::preset(preset, "", "")
        .map(|built| built.base().contains("localhost"))
        .unwrap_or(false)
}

/// Where a preset points, as io-harness resolves it.
pub fn endpoint_of(preset: &str) -> Option<String> {
    Compatible::preset(preset, "", "")
        .ok()
        .map(|built| built.base().to_string())
}

/// How an entry's credential is supplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// No key written, so the provider's own environment variable answers.
    FromEnvironment(&'static str),
    /// A `${env:…}` or `${file:…}` reference, shown as written — the name is the
    /// information and the contents are not.
    Indirect(String),
    /// A key written into the file. Never shown in full.
    Written,
    /// This endpoint needs none.
    NotNeeded,
}

impl Credential {
    /// The words this draws as.
    pub fn word(&self) -> String {
        match self {
            Credential::FromEnvironment(var) => format!("${var}"),
            Credential::Indirect(text) => text.clone(),
            Credential::Written => "key in file".to_string(),
            Credential::NotNeeded => "no key needed".to_string(),
        }
    }
}

/// One link of the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its position, which is its place in the fallback order.
    pub index: usize,
    /// `openrouter`, `anthropic`, `openai`, or the preset or endpoint a
    /// `compatible` entry names.
    pub kind: String,
    /// The model id, as this endpoint spells it.
    pub model: String,
    /// Where it goes, where that is knowable.
    pub endpoint: Option<String>,
    /// How its credential is supplied.
    pub credential: Credential,
}

/// The whole chain, in the order it is tried.
///
/// `provider_spec()` is the head and `fallback_specs()` is the tail, which is
/// io-harness's own split; joining them here is what makes the panel's order the
/// file's order rather than a second opinion about it.
///
/// **The credential is read from the file's own bytes and not from the spec**,
/// because io-harness substitutes `${env:…}` and `${file:…}` while it parses —
/// and errors outright when the variable is unset, so by the time a `Config`
/// exists an indirection has already become the value it pointed at. Showing an
/// operator which variable they named therefore means quoting the text they
/// wrote, through the same [`crate::edit::value_at`] path `configure` uses for
/// keys the harness exposes no accessor for.
pub fn chain(config: &Config) -> Vec<Entry> {
    let written = decided(config)
        .path()
        .and_then(|path| std::fs::read_to_string(path).ok());
    let mut specs: Vec<&ProviderSpec> = Vec::new();
    if let Some(head) = config.provider_spec() {
        specs.push(head);
    }
    specs.extend(config.fallback_specs());

    specs
        .into_iter()
        .enumerate()
        .map(|(index, spec)| {
            let raw = written.as_deref().and_then(|text| {
                crate::edit::value_at(text, &format!("provider[{index}].api_key"))
            });
            entry(index, spec, raw.as_deref())
        })
        .collect()
}

/// Which file configured the chain.
///
/// **Not `origin("provider")`.** io-harness keys an array of tables per element
/// and per key — `provider.[0].api_key` is the shape its own error messages use
/// — so the bare array name has no origin at all and asking for it returns
/// nothing. The first key under it is what names the file.
pub fn decided(config: &Config) -> Decided {
    let found = config
        .origins()
        .filter(|(key, _)| key.starts_with("provider"))
        .flat_map(|(_, origins)| origins.last())
        .next()
        .cloned();
    match found.as_ref() {
        Some(origin) => Decided::File {
            scope: origin.scope,
            path: origin.path.clone(),
        },
        None => Decided::Default,
    }
}

fn entry(index: usize, spec: &ProviderSpec, raw_key: Option<&str>) -> Entry {
    // `ProviderSpec` is `#[non_exhaustive]` from the release it was introduced
    // in, so the wildcard is required — and it is the honest arm rather than a
    // shrug: a variant this build does not know is still a configured provider,
    // and drawing it as nothing at all would hide a link of the chain.
    match spec {
        ProviderSpec::OpenRouter { model, api_key } => Entry {
            index,
            kind: "openrouter".into(),
            model: model.clone(),
            endpoint: None,
            credential: credential(api_key.as_deref(), raw_key, "OPENROUTER_API_KEY"),
        },
        ProviderSpec::Anthropic { model, api_key } => Entry {
            index,
            kind: "anthropic".into(),
            model: model.clone(),
            endpoint: None,
            credential: credential(api_key.as_deref(), raw_key, "ANTHROPIC_API_KEY"),
        },
        ProviderSpec::OpenAi { model, api_key } => Entry {
            index,
            kind: "openai".into(),
            model: model.clone(),
            endpoint: None,
            credential: credential(api_key.as_deref(), raw_key, "OPENAI_API_KEY"),
        },
        ProviderSpec::Compatible {
            model,
            preset,
            base_url,
            api_key,
            ..
        } => {
            let kind = preset
                .clone()
                .or_else(|| base_url.clone())
                .unwrap_or_else(|| "compatible".into());
            let endpoint = match (preset.as_deref(), base_url.as_deref()) {
                (Some(name), _) => endpoint_of(name),
                (None, Some(url)) => Some(url.to_string()),
                _ => None,
            };
            // **There is no environment variable to fall back to here**, because
            // there is no single vendor to name one for — io-harness says so in
            // the variant's own documentation. So an absent key is either an
            // endpoint that needs none or a gap, and the preset's own base URL
            // is what tells the two apart.
            let credential = match (raw_key.map(unquoted), api_key.as_deref()) {
                (Some(bare), _) if is_indirect(&bare) => Credential::Indirect(bare),
                (_, Some(_)) => Credential::Written,
                _ => Credential::NotNeeded,
            };
            Entry {
                index,
                kind,
                model: model.clone(),
                endpoint,
                credential,
            }
        }
        _ => Entry {
            index,
            kind: "unknown".into(),
            model: String::new(),
            endpoint: None,
            credential: Credential::NotNeeded,
        },
    }
}

fn credential(api_key: Option<&str>, raw_key: Option<&str>, variable: &'static str) -> Credential {
    // The raw text first: it is what the operator wrote, and it is the only
    // place an indirection still exists by the time a `Config` has parsed.
    if let Some(raw) = raw_key {
        // The raw text arrives with its quotes — `value_at` returns the value's
        // own bytes, and a TOML string's bytes include them.
        let bare = unquoted(raw);
        if is_indirect(&bare) {
            return Credential::Indirect(bare);
        }
    }
    match api_key {
        None => Credential::FromEnvironment(variable),
        Some(_) => Credential::Written,
    }
}

/// A TOML basic string with its quotes taken off, for showing back.
fn unquoted(text: impl AsRef<str>) -> String {
    text.as_ref().trim().trim_matches('"').to_string()
}

fn is_indirect(text: &str) -> bool {
    text.starts_with("${env:") || text.starts_with("${file:")
}

/// The rows as the picker draws them.
///
/// The position leads, because the whole point of this surface is that the order
/// is the chain — a list whose first row did not say "1" would be a list an
/// operator has to be told how to read.
pub fn rows(chain: &[Entry]) -> Vec<crate::picker::Row> {
    chain
        .iter()
        .map(|entry| {
            let place = if entry.index == 0 {
                "1 · used".to_string()
            } else {
                format!("{} · fallback", entry.index + 1)
            };
            crate::picker::Row::with_detail(
                format!("{place}   {}", entry.kind),
                format!("{}   {}", entry.model, entry.credential.word()),
            )
        })
        .collect()
}

/// One entry's position in one file's `[[provider]]` array, and how long that
/// array is.
///
/// **The length is carried rather than passed.** [`demote`] used to take it as a
/// second argument, which meant the bound on "is there anywhere to move to" came
/// from whatever the caller happened to be counting — a filtered view, a chain
/// rendered from a different `Config`, or the same number typed twice. Here both
/// numbers are read from the same file in the same pass, so they cannot disagree
/// about the array they describe.
///
/// It carries the scope too, because a caller needs it and it comes from the
/// same lookup: [`crate::configure::write`] takes a scope, and an index means
/// nothing without the file it counts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    /// The scope whose file carries the chain — the scope a write must go to.
    pub scope: Scope,
    /// Private so nothing outside this module can spell one from a row number.
    index: usize,
    /// How many `[[provider]]` entries that file declares.
    len: usize,
}

impl At {
    /// The `index`-th `[[provider]]` entry of `text`.
    ///
    /// `text` must be the bytes of the file `scope` names. Entries are counted by
    /// walking `provider[n].kind` — the one key `#[serde(tag = "kind")]` makes
    /// required on every variant — until the first gap, which is the end of a
    /// contiguous array of tables.
    ///
    /// `None` when the file declares no entry at that position, so a move or a
    /// removal aimed past the end is refused here rather than turning into
    /// io-harness's "there is no `provider[n]` in this file" further down.
    pub fn of(scope: Scope, text: &str, index: usize) -> Option<At> {
        let mut len = 0usize;
        while crate::edit::value_at(text, &format!("provider[{len}].kind")).is_some() {
            len += 1;
        }
        (index < len).then_some(At { scope, index, len })
    }

    /// The position, for a caller that has a sentence to write about it.
    ///
    /// ponytail: no accessor for the length beside it. Nothing needs to draw
    /// "3 of 4" yet, and the one thing the length is *for* — the bound on
    /// [`demote`] — is answered inside this module. Add one when a row wants it.
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Where the file that decided the chain declares `entry`.
///
/// It takes the [`Entry`] rather than a number so there is nothing for a caller
/// to get wrong, and it reads the file [`decided`] names — which is the file
/// [`chain`] itself read, so the position it returns is a position in the array
/// the operator is looking at.
///
/// `None` where no file declares a chain at all, and where the deciding file no
/// longer carries that position — an operator editing `io.toml` under the
/// session is exactly what the second looks like.
pub fn declared_at(config: &Config, entry: &Entry) -> Option<At> {
    let Decided::File { scope, path } = decided(config) else {
        return None;
    };
    let text = std::fs::read_to_string(path).ok()?;
    let at = At::of(scope, &text, entry.index)?;
    // **The position is confirmed against the entry's own content, and without
    // this the newtype is a wrapper rather than a guard.**
    //
    // `At::of` counts `provider[n].kind` in the file's TOP-LEVEL array and
    // bounds-checks the number it was handed. [`chain`] builds its rows from the
    // RESOLVED configuration. Those are the same list right up until a profile is
    // in force: `provider` is not an appending key, so `[[profile.fast.provider]]`
    // *replaces* the top-level array, and io-harness then rewrites the origins to
    // `provider.*` — so `decided` still names this file and every positional check
    // still passes while the rows on screen describe entries that are not at those
    // positions. Removing "the only link" would have deleted the operator's real
    // primary provider, which was never on screen.
    //
    // So the row and the file have to agree about *what is there*, not merely that
    // something is. Disagreement means the array being counted is not the array
    // being looked at, and the honest answer is that no file in force declares
    // this link — which the caller already renders as a refusal.
    let model = crate::edit::value_at(&text, &format!("provider[{}].model", entry.index))?;
    let kind = crate::edit::value_at(&text, &format!("provider[{}].kind", entry.index))?;
    (unquoted(&model) == entry.model && unquoted(&kind) == entry.kind).then_some(at)
}

/// What a new `[[provider]]` entry reaches.
///
/// **The two `compatible` shapes are two variants and not two `Option`s.**
/// io-harness takes exactly one of `preset` and `base_url` and refuses both and
/// neither by index at load; a pair of options is a signature in which three of
/// four combinations are wrong. The `kind` string is derived here rather than
/// taken, for the same reason: `deny_unknown_fields` is on the `[[provider]]`
/// variants, so `kind = "openai"` beside a `preset` is refused too, and a free
/// string is a way to ask for that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint<'a> {
    /// `kind = "openrouter"`. An absent key means `OPENROUTER_API_KEY`.
    OpenRouter,
    /// `kind = "anthropic"`. An absent key means `ANTHROPIC_API_KEY`.
    Anthropic,
    /// `kind = "openai"`. An absent key means `OPENAI_API_KEY`.
    OpenAi,
    /// A `compatible` entry reached through one of [`PRESETS`].
    ///
    /// A name io-harness does not know is refused on the round trip, naming every
    /// preset that exists — which is a better sentence than io-cli would write,
    /// so this does not check the name itself.
    Preset(&'a str),
    /// A `compatible` entry reached at a base URL the operator gives, with
    /// `/chat/completions` appended to it by the harness.
    BaseUrl(&'a str),
}

impl Endpoint<'_> {
    /// The `kind` this writes, which is the tag io-harness dispatches on.
    fn kind(&self) -> &'static str {
        match self {
            Endpoint::OpenRouter => "openrouter",
            Endpoint::Anthropic => "anthropic",
            Endpoint::OpenAi => "openai",
            Endpoint::Preset(_) | Endpoint::BaseUrl(_) => "compatible",
        }
    }
}

/// The edit that appends a provider to the end of the chain.
///
/// Last, because the end of an array of tables is the end of the chain and the
/// front of it is the provider a run uses: an entry inserted anywhere else would
/// change which vendor the next turn bills to, which is not what "add" says.
///
/// `api_key` is written as given, so `Some("${env:GROQ_API_KEY}")` is how a
/// credential is named without being copied into the file. **io-harness resolves
/// that at parse time and fails outright when the variable is unset**, so a
/// reference to a name that does not exist is refused by
/// [`crate::configure::write`]'s round trip and rolled back — the file is never
/// left naming a variable that stops the session from starting.
pub fn add(endpoint: Endpoint<'_>, model: &str, api_key: Option<&str>) -> Edit {
    let mut body = format!("kind = {}", quoted(endpoint.kind()));
    // Exhaustive, with no wildcard: a variant added later must be given its own
    // second line here rather than falling through to a `compatible` entry that
    // names neither base and is refused at load.
    match endpoint {
        Endpoint::OpenRouter | Endpoint::Anthropic | Endpoint::OpenAi => {}
        Endpoint::Preset(name) => body.push_str(&format!("\npreset = {}", quoted(name))),
        Endpoint::BaseUrl(url) => body.push_str(&format!("\nbase_url = {}", quoted(url))),
    }
    body.push_str(&format!("\nmodel = {}", quoted(model)));
    if let Some(key) = api_key {
        body.push_str(&format!("\napi_key = {}", quoted(key)));
    }
    Edit::append("provider", body)
}

/// The edit that moves an entry one place towards the front of the chain.
///
/// **Which is a change to what the next turn runs on, not a cosmetic one**: the
/// first entry is the provider in force. Promoting the second entry makes it the
/// provider and demotes the one that was.
///
/// `None` for the entry that is already first, so a caller cannot draw a control
/// that does nothing.
pub fn promote(at: &At) -> Option<Edit> {
    (at.index > 0).then(|| Edit::move_entry("provider", at.index, at.index - 1))
}

/// The edit that moves an entry one place towards the back of the chain.
///
/// `None` for the last entry, bounded by the length [`At`] read from the file
/// rather than by a count the caller supplied.
pub fn demote(at: &At) -> Option<Edit> {
    (at.index + 1 < at.len).then(|| Edit::move_entry("provider", at.index, at.index + 1))
}

/// The edit that removes an entry.
///
/// Removing the first one promotes the second, because the chain is the array's
/// order and nothing else — worth saying out loud, since it is the one removal
/// on this surface that changes which provider a run uses.
pub fn remove(at: &At) -> Edit {
    Edit::remove(format!("provider[{}]", at.index))
}
