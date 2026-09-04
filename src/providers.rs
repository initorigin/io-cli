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
//! (`config.rs:3130`): the winning scope replaces the chain whole, "because a
//! half-appended fallback chain is not a chain". So exactly one file decides the
//! whole array, and [`declared_at`] finds that file through [`decided`] — the
//! same origin [`chain`] already reads the credentials out of, so the position
//! and the text cannot come from two different files.
//!
//! # Exactly one of `preset` and `base_url`
//!
//! io-harness refuses a `compatible` entry that names both or neither, by index,
//! at load (`config.rs:571`). [`crate::configure::write`] would catch it on the
//! round trip and roll back, which is a good failure — but [`add`] takes an
//! [`Endpoint`] whose two `compatible` shapes are separate variants, so the entry
//! that fails cannot be constructed at all.
//!
//! # Changing a link rather than replacing it
//!
//! A key rotates and a model is renamed, and until [`edit`] existed neither was
//! reachable from this surface: an operator whose `OPENROUTER_API_KEY` changed
//! could add a link, promote it, demote it and remove it, and to change one word
//! of one had to open `io.toml`. [`edit`] is `servers::edit`'s twin — one key of
//! one addressed entry, `value` as TOML source — and it is deliberately narrower
//! than `ProviderSpec` is wide: [`KEYS`] is `model` and `api_key` and nothing
//! else. `kind`, `preset` and `base_url` are the link's **identity**, and an
//! entry that reaches a different vendor is a different link; [`remove`] and
//! [`add`] say that out loud, and they cannot leave behind the both-bases entry
//! `preset = "groq"` written over a `base_url` entry would be.
//!
//! # The three shapes a credential has, and which one is the default
//!
//! [`Key`] is the write side of [`Credential`]: the same three shapes the file
//! format already has, spelled as a choice rather than as a string a caller
//! assembles. **[`Key::Environment`] is the default that matters** — it is the
//! only one of the three under which `io.toml` never holds the secret at all.
//! [`variable`] names the environment variable a given endpoint would read and
//! [`variable_is_set`] says whether it currently has one, so a caller can offer
//! "use `$OPENROUTER_API_KEY`, which is already set" as the row that is already
//! selected. Neither of them, and nothing else in this module, ever returns the
//! variable's contents: what is being decided is *where a key lives*, and that
//! question is answerable without reading one.

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

/// How a new or changed credential is supplied — the write side of
/// [`Credential`].
///
/// The same three shapes the file format already has, offered as a choice rather
/// than as a string every call site assembles for itself. A caller that built the
/// text by hand would be one `format!` away from writing `${env:GROQ_API_KEY}`
/// without the braces, or a literal where an indirection was meant, and the
/// second of those is a secret in a file the operator did not ask to hold one.
///
/// **[`Key::Environment`] is the shape to default to.** It is the only one under
/// which the key is never in `io.toml` — nothing to leak in a screenshot, a
/// backup, a `git add -A`, or a support paste. The other two are what an operator
/// asks for, and a literal is what they should have to ask for twice.
///
/// It is `Copy` and borrows its text, because a credential should live in this
/// process for as long as it takes to write it and no longer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Key<'a> {
    /// No key in the file, so the endpoint's own environment variable answers.
    ///
    /// **What that means depends on the endpoint, and the difference is not
    /// cosmetic.** For `openrouter`, `anthropic` and `openai` io-harness reads
    /// the vendor's variable itself when `api_key` is absent, so the shape is a
    /// key that is *not written*. For a `compatible` entry there is no variable
    /// to fall back to — io-harness says so in the variant's own documentation —
    /// so the same intention has to be written as `${env:…}`, and an absent key
    /// there means an endpoint that needs none. [`Key::written`] resolves that,
    /// once, rather than leaving every caller to know it.
    Environment,
    /// A `${env:VAR}` or `${file:PATH}` reference, written literally.
    ///
    /// io-harness substitutes it as it parses, and **errors outright when the
    /// variable is unset** — so a reference to a name that does not exist is
    /// refused by [`crate::configure::write`]'s round trip and rolled back.
    Indirect(&'a str),
    /// The key itself, written into the file.
    ///
    /// The shape that puts a secret on disk. Nothing here refuses it — an
    /// operator with no environment to lean on is a real operator — but it is the
    /// shape a caller must have been told to write, naming the file it lands in.
    Literal(&'a str),
}

// **Hand-written, because `derive(Debug)` on this type prints the key.** A
// `Key` is one `{:?}` away from a log line, a panic message, or an
// `assert_eq!` failure in somebody's CI output, and a derived `Debug` would put
// the literal in all three. The variant is the part worth seeing; the bytes are
// never the part worth seeing.
impl std::fmt::Debug for Key<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Key::Environment => f.write_str("Environment"),
            // The variable's NAME is information and its contents are not, which
            // is the same line `Credential::Indirect` and `configure::redact`
            // already draw.
            Key::Indirect(text) => write!(f, "Indirect({text:?})"),
            Key::Literal(_) => f.write_str("Literal(<redacted>)"),
        }
    }
}

impl Key<'_> {
    /// What [`add`] writes as `api_key`, or `None` for no `api_key` line at all.
    ///
    /// The one place the `Environment` split above is resolved. For the three
    /// vendor kinds it is `None`, which is the absence io-harness reads as "use
    /// my own variable". For a preset it is `${env:VAR}` for the variable
    /// [`variable`] names — because a `compatible` entry has no fallback, and an
    /// absent key there would silently be an unauthenticated request rather than
    /// an authenticated one. For a bare `base_url`, and for the local runtimes,
    /// there is no variable to name and `None` is the honest answer: those are
    /// the endpoints that genuinely need no key.
    pub fn written(&self, endpoint: Endpoint<'_>) -> Option<String> {
        match self {
            Key::Environment => match endpoint {
                Endpoint::OpenRouter | Endpoint::Anthropic | Endpoint::OpenAi => None,
                Endpoint::Preset(_) | Endpoint::BaseUrl(_) => {
                    variable(endpoint).map(|var| format!("${{env:{var}}}"))
                }
            },
            Key::Indirect(text) => Some((*text).to_string()),
            Key::Literal(key) => Some((*key).to_string()),
        }
    }

    /// What [`edit`] takes as `value` — TOML source, and `""` to unset.
    ///
    /// The pair to [`Key::written`] for an entry that already exists, and it is
    /// two lines rather than a note at the call site for one reason: the caller
    /// holding a `Key` has a Rust string and [`edit`] takes TOML, and the
    /// obvious bridge — `format!("\"{key}\"")` — is either a parse error or a
    /// different value the moment the key carries a quote or a backslash.
    ///
    /// An empty result is not an empty key. See [`edit`]: it is the request to
    /// take the line away.
    pub fn source(&self, endpoint: Endpoint<'_>) -> String {
        self.written(endpoint)
            .map(|text| quoted(&text))
            .unwrap_or_default()
    }
}

/// The environment variable an endpoint's credential would come from.
///
/// For the three vendor kinds these are io-harness's own — the names its
/// `from_env` constructors read, which is why a shell that already works with the
/// harness works here — and [`crate::provider::key_for`] falls back to exactly
/// these.
///
/// For a preset the name is **derived**, `<PRESET>_API_KEY`, and that is a
/// deliberate choice over a hand-kept table of thirteen. io-harness has no
/// variable for `compatible` at all, so there is nothing here for a table to be
/// checked against, and a list nothing can disagree with is decoration that goes
/// stale in silence — the same argument [`PRESETS`] answers by being provable.
/// The derivation matches what io-harness's own documentation writes
/// (`config.rs:546` spells `${env:GROQ_API_KEY}`), and it is an *offer*: a
/// vendor whose variable is spelled differently is one [`Key::Indirect`] away,
/// and [`variable_is_set`] means a name nobody's shell carries is never the
/// default that gets taken.
///
/// `None` where there is no variable to name — the eight local runtimes and a
/// bare `base_url`, which need no credential at all.
pub fn variable(endpoint: Endpoint<'_>) -> Option<String> {
    match endpoint {
        Endpoint::OpenRouter => Some("OPENROUTER_API_KEY".to_string()),
        Endpoint::Anthropic => Some("ANTHROPIC_API_KEY".to_string()),
        Endpoint::OpenAi => Some("OPENAI_API_KEY".to_string()),
        Endpoint::Preset(name) if !is_local(name) => Some(format!(
            "{}_API_KEY",
            name.to_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        )),
        Endpoint::Preset(_) | Endpoint::BaseUrl(_) => None,
    }
}

/// Whether `variable` currently holds a non-empty value.
///
/// **Whether, and never what.** This is the whole of what the surface above it
/// needs: an offer reads "use `$OPENROUTER_API_KEY`, which is already set", and
/// the sentence is complete without the key ever entering this process — let
/// alone a row, a log line or a `{:?}`. Returning the value would put a secret
/// into every caller that only wanted to know whether to preselect a row.
///
/// Empty counts as unset, for the reason [`edit`] refuses to write one: an empty
/// key is a key [`crate::provider::key_for`] hands back as valid, and every
/// request then fails authentication with nothing to read.
pub fn variable_is_set(variable: &str) -> bool {
    std::env::var(variable).is_ok_and(|value| !value.trim().is_empty())
}

/// The keys of a `[[provider]]` entry [`edit`] will change.
///
/// **Narrower than `ProviderSpec` is wide, on purpose.** The variant carries
/// seven fields; two of them are what an operator wants to change about a link
/// they already have, and the other five are either its identity or a setting no
/// verb on this surface asks for:
///
/// - `model` — renamed, deprecated, or simply the wrong one. The common edit.
/// - `api_key` — rotated. The edit this surface existed without, and the one an
///   operator was opening `io.toml` by hand to make.
/// - `kind`, `preset`, `base_url` — the link's **identity**. An entry pointed at
///   a different vendor is a different link, and [`remove`] plus [`add`] says so
///   in words. Allowing them here would also make the one entry io-harness
///   refuses expressible again: a `preset` written onto a `base_url` entry names
///   both bases, which fails at load by index (`config.rs:571`) — a loud failure,
///   but a loud failure produced by a control that looked like renaming a field.
/// - `auth`, `name`, `reference_prices` — nothing asks for them, and the last
///   turns on an outbound request to a host the file did not name. A control
///   that does that belongs on the surface that shows what it costs.
///
/// Unlike `servers::KEYS` this list is not the only thing standing between a
/// typo and a silently ignored key — `[[provider]]` *is* held to
/// `deny_unknown_fields`, so `modle = "…"` is refused at load and
/// [`crate::configure::write`] rolls it back. The `const` is here so a caller
/// cannot spell a key at all, and so the five omissions above are a decision with
/// a reason written beside it rather than an accident of what got implemented.
///
/// ponytail: no `base_url` even for an entry that already has one. Add it when
/// a moved gateway is a thing an operator actually reports, and pair it with the
/// exactly-one check [`Endpoint`] enforces at add time.
pub const KEYS: &[&str] = &["model", "api_key"];

/// The edit that changes one key of the entry at `at`.
///
/// `value` is TOML **source**, the way [`crate::edit::Edit::set`] takes it —
/// `"\"gpt-4o\""`, `"\"${env:GROQ_API_KEY}\""`. Build it with [`Key::source`] or
/// [`crate::servers::quoted`] rather than a format string: `api_key` is pasted
/// text, and a raw newline or a backslash inside a hand-built basic string is a
/// parse error rather than a value.
///
/// `None` for a key that is not one of [`KEYS`], so a caller cannot invent one.
///
/// # An empty value unsets the key, and only `api_key` may be unset
///
/// **`api_key` with an empty `value` deletes the line rather than writing an
/// empty string**, through [`crate::edit::Edit::unset`]. This is the one place
/// the distinction is load-bearing: moving a link back from a written literal to
/// its environment variable means the key must be *absent*, because that absence
/// is exactly what io-harness reads as "use the vendor's own variable".
/// `api_key = ""` is not that. It is a key that is set, to nothing:
/// [`crate::provider::key_for`] returns it as a valid credential, the request
/// carries an empty bearer token, and the vendor answers 401 for a reason no
/// message in this program will ever name. Both spellings of empty — no text at
/// all, and the TOML source `""` — are read as the unset, because the second is
/// precisely what a caller building a value from a cleared input field produces.
///
/// `model` cannot be unset, and gets `None` for an empty value rather than an
/// edit: it is required on every variant, so removing it makes an entry that no
/// longer loads. That is caught on the round trip and rolled back, but a verb
/// that can only ever fail is better refused where it is spelled.
pub fn edit(at: &At, key: &str, value: &str) -> Option<Edit> {
    if !KEYS.contains(&key) {
        return None;
    }
    let path = format!("provider[{}].{key}", at.index);
    let trimmed = value.trim();
    let empty = trimmed.is_empty() || trimmed == "\"\"" || trimmed == "''";
    match (key, empty) {
        ("api_key", true) => Some(Edit::unset(path)),
        (_, true) => None,
        _ => Some(Edit::set(path, value.to_string())),
    }
}

/// The edit that appends a provider to the end of the chain.
///
/// Last, because the end of an array of tables is the end of the chain and the
/// front of it is the provider a run uses: an entry inserted anywhere else would
/// change which vendor the next turn bills to, which is not what "add" says.
///
/// `api_key` is written as given, so `Some("${env:GROQ_API_KEY}")` is how a
/// credential is named without being copied into the file — and [`Key::written`]
/// is what a caller holding a chosen shape should build that argument with,
/// because "from the environment" is an absent key for a vendor kind and an
/// `${env:…}` for a preset. **io-harness resolves
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
