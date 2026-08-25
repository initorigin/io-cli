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

use io_harness::config::Config;
use io_harness::{Compatible, ProviderSpec};

use crate::configure::Decided;
use crate::edit::Edit;

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

/// The edit that appends a provider to the end of the chain.
pub fn add(kind: &str, model: &str, preset: Option<&str>) -> Edit {
    let mut body = format!("kind = {}\nmodel = {}", quoted(kind), quoted(model));
    if let Some(preset) = preset {
        body.push_str(&format!("\npreset = {}", quoted(preset)));
    }
    Edit::append("provider", body)
}

/// The edit that moves an entry one place towards the front.
pub fn promote(index: usize) -> Option<Edit> {
    (index > 0).then(|| Edit::move_entry("provider", index, index - 1))
}

/// The edit that moves an entry one place towards the back.
pub fn demote(index: usize, len: usize) -> Option<Edit> {
    (index + 1 < len).then(|| Edit::move_entry("provider", index, index + 1))
}

/// The edit that removes an entry.
pub fn remove(index: usize) -> Edit {
    Edit::remove(format!("provider[{index}]"))
}

/// A TOML basic string, escaped.
fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
