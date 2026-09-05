//! One parse for the managed surfaces, shared by the slash form and the argv one.
//!
//! `/mcp add …` typed into a composer and `io mcp add …` typed at a shell are the
//! same sentence arriving through two doors, and this module is the only room
//! behind both of them. Every verb these releases add — `mcp add|list|get|edit|
//! remove`, `plugin add|install|list|search|remove`, `plugin marketplace
//! add|list|remove`,
//! `config get|set|unset|list` — is turned into a [`Request`] here and into
//! [`crate::edit::Edit`]s by [`plan`], and neither entry point is allowed a second
//! reading of the same words.
//!
//! **One of them is turned into no edit at all, and that is not an exception to
//! the rule above.** `plugin marketplace add|remove` changes the operator's disk
//! rather than their configuration — there is no scope, no `[[…]]` entry and no
//! value to spell — so [`plan`] answers `None` for it and the act itself is
//! [`crate::marketplace`], one function each door calls. The parse is still the
//! only reading of the words, which is what criterion F1 is about; what the two
//! doors then differ in is where they *print*, which is all they have ever
//! differed in.
//!
//! # Why the parse is not clap's
//!
//! `src/cli.rs` is clap and stays clap: it describes `io`'s own arguments, it
//! renders `--help`, and it is the right tool for a process's argv. It cannot be
//! the tool for the slash form, because the slash form is not argv — it is a line
//! of text a person typed into a composer, and it reaches the driver as a
//! `String` with no process boundary anywhere near it. A clap-only parse would
//! therefore serve exactly one of the two doors, and the other would grow a
//! hand-written reading of the same grammar beside it. Two readings of one
//! grammar do not stay equal: the first flag added to one of them is the release
//! where `io mcp add` and `/mcp add` write different bytes, and an operator
//! discovers it by diffing a file they did not expect to differ.
//!
//! So this module parses a `&[String]` — the shape both doors can produce — and
//! clap's job on the io side shrinks to *handing its tokens through untouched*
//! (`trailing_var_arg` and `allow_hyphen_values` on the subcommand that collects
//! them). The slash side reaches the same shape through [`tokens`].
//!
//! # Why it is in the library and not in the driver
//!
//! Nothing under `tests/` links `src/main.rs`. A parse written there is a parse no
//! integration test can call and no sabotage can make fail — the same reasoning
//! that put `configure::refusal` and plain-mode resolution in the library rather
//! than beside their call sites. The acceptance criterion this module exists for
//! compares the **bytes two entry paths write**, and a comparison that cannot be
//! run is a criterion that is asserted by prose.
//!
//! # `--` ends io's arguments, and everything after it is opaque
//!
//! `io mcp add semlith -- semlith --store /path/to/.semlith mcp` declares a server
//! whose command is `semlith` and whose arguments are `--store`, `/path/to/.semlith`
//! and `mcp`, **verbatim**. The scan stops at the first `--` and copies the rest,
//! so a `--plain` written there is the server's flag and never io's: a parser that
//! kept looking for its own flags past that point would silently eat an argument
//! out of the middle of somebody's command line and start a server that behaves
//! differently from the one they wrote down.
//!
//! # The transport is decided by the form, and a disagreement is refused
//!
//! A URL means HTTP and a command after `--` means stdio; that is the whole rule,
//! and [`McpTransport`] is constructed in exactly one place below so that it stays
//! the whole rule. `--transport` is accepted because another harness's users type
//! it and their muscle memory is not a thing to punish, but it is read as an
//! **assertion about the form**, checked against it, and refused by name when the
//! two disagree. It is never a tie-break: resolving `--transport stdio --url …` by
//! precedence would silently discard half of what the operator wrote, and which
//! half depends on a rule nobody can see.
//!
//! That is also why the foreign ordering — `mcp add --transport http linear-server
//! https://…/mcp` — has no branch of its own. A second positional **is** a URL,
//! wherever `--transport` sits, so the two orderings differ only in where the URL
//! was found and converge on one [`McpServer`] before a single byte is written.
//! A branch keyed on the flag would be two constructions of one server, and the
//! ordering that got less attention would be the one that writes a different file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use io_harness::config::Scope;
use io_harness::{McpServer, McpTransport};

use crate::edit::Edit;

/// One sentence an operator typed, on whichever surface they typed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The MCP servers.
    Mcp(McpVerb),
    /// The capability bundles.
    Plugin(PluginVerb),
    /// The settings.
    Config(ConfigVerb),
    /// The skills in this home's own `skills/` directory.
    Skill(SkillVerb),
}

/// What `/skills` and `io skill` can be asked to do.
///
/// **Three verbs and not five.** Turning a skill off and back on stays a
/// keystroke: it is a rename inside a directory io-cli owns, it is reversible by
/// the row that did it, and there is nothing for an argument form to name that
/// the picker does not name better. Installing and removing are the two that take
/// a value only the operator can author — a path they have and a name they choose
/// — which is exactly the line `product.yaml` draws between a value that is
/// chosen and one that is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillVerb {
    /// Copy the file at `source` into this home's skills directory.
    Add { source: std::path::PathBuf },
    /// Every skill, whose it is, and whether it is on.
    List,
    /// Delete one by the name its frontmatter declares.
    ///
    /// The **resolved** name and not the file name, because that is what an
    /// operator reads off `/skills` and what `Skills::discover` keys on. A file
    /// called `mine.md` declaring `name: io-mcp` is `io-mcp` everywhere it is
    /// addressed, and asking for the filename here would be asking for the one
    /// spelling the surface never shows.
    Remove { name: String },
}

/// What `/mcp` and `io mcp` can be asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpVerb {
    /// Declare a whole server. `scope` is where it is written — the operator's
    /// own file unless `--scope` said otherwise.
    Add { server: McpServer, scope: Scope },
    /// Every configured server.
    List,
    /// One configured server, whole.
    Get { id: String },
    /// Change one key of one entry. `value` is **TOML source**, the way
    /// [`crate::servers::edit`] takes it, and it was rendered here rather than at
    /// the call site so that a command with a quote in it cannot become a file
    /// that no longer parses.
    Edit {
        id: String,
        key: String,
        value: String,
    },
    /// Take one entry away, whole.
    Remove { id: String },
    /// Let io-harness start it again — `enabled = true`.
    Enable { id: String },
    /// Stop io-harness starting it — `enabled = false`.
    ///
    /// **Not [`McpVerb::Remove`], and the pair is deliberate.** A removal takes the
    /// `[[mcp]]` entry away and the operator re-types the whole server to get it
    /// back; this changes one word and leaves every other key of the entry exactly
    /// as it was, so `mcp list` still shows the server and says it is off. Both go
    /// through [`crate::servers::switch`], which is also what the `/mcp` keystroke
    /// builds — one write, so the two doors cannot produce different bytes.
    Disable { id: String },
    /// Start it, once, on its own, and report what happened —
    /// [`crate::servers::probe`] over [`io_harness::probe_mcp`].
    ///
    /// A read as far as this module is concerned: it writes no configuration file,
    /// so [`plan`] answers `None` for it and each door runs the probe itself. It is
    /// not a *cheap* read — it spawns or dials a real server — but nothing about
    /// the operator's files changes, which is what a `Plan` is about.
    Probe { id: String },
}

/// What `/plugin` and `io plugin` can be asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginVerb {
    /// Declare a bundle. `path` is the word as typed — **a directory or the name
    /// of a bundle a marketplace holds** — and which of the two it was is decided
    /// against the disk by [`crate::marketplace::chosen`] in [`plan`], never here:
    /// a parse that judged it would be judging without having looked.
    ///
    /// Still a `PathBuf` for the one word, because the reading that wins in every
    /// existing case is the path and a `String` here would make the common verb
    /// convert at both call sites to serve the rarer one.
    ///
    /// **Which reading it was decides whether the entry is written on.** A
    /// directory the operator typed is declared on, as it has been since 0.28.0; a
    /// bundle out of a marketplace is a stranger's code and is declared
    /// `enabled = false` so io-harness reads, parses and trust-checks it before
    /// anything of it reaches a turn. See [`plan`] and
    /// [`crate::marketplace::Chosen`].
    Add { path: PathBuf, scope: Scope },
    /// Every declared bundle, loaded and refused.
    List,
    /// Every bundle in every added marketplace whose name or description carries
    /// `text`. A read: it opens no file and writes none.
    Search { text: String },
    /// Undeclare a bundle, named **by the directory it lives in or by the id its
    /// manifest carries**. No scope: the file that named it is the file the
    /// removal has to go to, and [`plan`] finds it.
    ///
    /// A `String` and not a `PathBuf`, because a path is only the first of the two
    /// readings and the second one is a name. Which it was is decided against the
    /// disk in [`plan`] and never here — the path first, always — for
    /// [`crate::marketplace::chosen`]'s reason: a parse that judged the *shape* of
    /// the word would make one word mean a directory on a machine that has one and
    /// a bundle's name on a machine that does not.
    Remove { word: String },
    /// The repositories bundles are fetched from.
    ///
    /// **A nested enum rather than three more variants here, and the argument is
    /// the same one `pluginview` makes about a third list.** Every `match` on
    /// this enum in the crate would otherwise grow three arms, and the next verb
    /// added to either half is the one somebody forgets in one of them — the
    /// `plan` arm below is a single line precisely because there is a single
    /// variant to name. It also keeps the surface honest: a marketplace is
    /// reached through `/plugin` because it holds plugins, not because it is a
    /// fourth managed surface, and `Request` still has exactly three.
    Marketplace(MarketVerb),
}

/// What `/plugin marketplace` and `io plugin marketplace` can be asked to do.
///
/// **Each verb carries a [`crate::fetch::Named`] rather than the text it was
/// typed as.** `fetch::resolve` is the only place in this crate a marketplace name
/// is judged — it is what refuses a leading `-`, a `..` and a whole URL — so
/// carrying the judged value means nothing downstream can re-read the operator's
/// string, and a name that reached [`plan`] or a driver unresolved would be a
/// second reading with a second opinion about what a name may contain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketVerb {
    /// Clone it into the operator's own home.
    Add(crate::fetch::Named),
    /// Every marketplace already there, and what each holds.
    List,
    /// Delete the clone. **Never a `[[plugin]]` entry** — see
    /// [`crate::marketplace::discard`], which is where criterion F3 lives.
    Remove(crate::fetch::Named),
}

/// What `/config` and `io config` can be asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigVerb {
    /// One key's value and what decided it.
    Get { key: String },
    /// Write one key. `value` is TOML source, already checked against the key's
    /// [`crate::configure::Kind`]: a choice arrives quoted, a number and a flag
    /// bare, so that what is written is a value io-harness reads back as the one
    /// that was typed.
    Set {
        key: String,
        value: String,
        /// `None` where `--scope` was not given, which means *inherit the file
        /// already deciding this key*. See `Args::scope_or_inherited`.
        scope: Option<Scope>,
    },
    /// Delete one key's line, so the layer below it decides again.
    Unset { key: String, scope: Option<Scope> },
    /// Every key this surface offers.
    List,
}

/// The edits a request comes to, and the one file they go in.
///
/// A `Vec` because [`crate::configure::write`] takes a slice and because a future
/// verb may need two edits in one round trip; every verb here produces exactly
/// one, and that is a property worth keeping rather than a shape worth widening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// An adapter io built while building this plan, which an **accepted** install
    /// puts in place and a **declined** one throws away.
    ///
    /// `Some` for exactly one request: `plugin add <name>` resolving to a Claude
    /// Code or Codex bundle, where the generated manifest has to exist before
    /// io-harness can read the bundle and there is therefore something to disclose
    /// at all. It is built beside its destination and not in it, so the answer
    /// decides whether the adapter the operator already has is replaced — see
    /// [`crate::adapt::Staged`]. The operator's configuration is never opened
    /// either way, and [`crate::marketplace::make`] and
    /// [`crate::marketplace::unmake`] are the one place each answer is performed,
    /// so two doors cannot disagree about what saying yes or no does.
    pub staged: Option<crate::adapt::Staged>,
    /// The scope whose file is written.
    pub scope: Scope,
    /// What to write, applied together or not at all.
    pub edits: Vec<Edit>,
    /// What the operator has to be shown **before** [`crate::configure::write`]
    /// is called, and consent to.
    ///
    /// `Some` for exactly one request: `plugin add <name>` resolved out of a
    /// marketplace, which is a stranger's directory nobody on this machine has
    /// read. It is already the whole answer — [`plan`] got it from
    /// [`crate::marketplace::disclosure`], which is `io_harness::Plugins::inspect`
    /// — so a driver holding it has nothing left to decide and no second reading
    /// to get wrong.
    ///
    /// **A door that ignores it writes an unconsented entry**, which is why it is
    /// a field on the `Plan` rather than a second call the driver has to remember:
    /// the edits and the disclosure that earns them travel together. A bundle
    /// io-harness would refuse never reaches here at all — `plan` answers `Err`
    /// with io-harness's own sentence and there is no `Plan` to write.
    pub disclosure: Option<crate::marketplace::Disclosure>,
}

/// The tokens of a slash line, as a shell would have handed them to `io`.
///
/// The leading `/` goes, so `/mcp add x` and `mcp add x` are one token slice, and
/// quoted runs survive as single tokens with their quotes removed — because that
/// is what the shell does to `io config set app.io-cli.gates.contains "all green"`
/// before `io` ever sees it. A slash surface that kept the quotes would write
/// `"\"all green\""` into the file: the same words, a different value, and a
/// difference that only shows up in the byte comparison this module exists to
/// pass.
///
/// ponytail: quotes and nothing else — no backslash escapes, no `$` expansion, no
/// globbing. A composer line is not a shell and the missing pieces are the ones
/// that would make it pretend to be one; an operator who needs them has the shell
/// itself, where `io mcp add …` reaches this exact parse.
#[must_use]
pub fn tokens(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('/').unwrap_or(line);
    let mut out: Vec<String> = Vec::new();
    let mut word = String::new();
    // Set by an opening quote, so that `""` is a deliberate empty value rather
    // than a token that vanishes on its way to a parse that would then report a
    // missing argument for something the operator did write.
    let mut quoted = false;
    let mut open: Option<char> = None;
    for c in line.chars() {
        match (open, c) {
            (Some(q), c) if c == q => open = None,
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                open = Some(c);
                quoted = true;
            }
            (None, c) if c.is_whitespace() => {
                if quoted || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                    quoted = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if quoted || !word.is_empty() {
        out.push(word);
    }
    out
}

/// Read one line of tokens into the request it names.
///
/// The token slice starts at the **surface** — `["mcp", "add", …]` — which is
/// exactly what `io mcp add …` leaves after the binary name and exactly what
/// [`tokens`] makes of `/mcp add …`. A leading `/` is tolerated here as well, so
/// a caller that forgot to strip one gets the same reading rather than a refusal
/// about a surface called `/mcp`. **`plugins` and `servers` are the same two
/// surfaces spelled plural** and are folded onto `plugin` and `mcp` here — see the
/// comment on the fold for why here and not in [`crate::commands`].
///
/// Every `Err` is a sentence naming what was wrong and what is accepted instead.
/// There is no bare "invalid argument" in this module: the operator is at a
/// terminal with no `--help` open, and a refusal that does not say what to type
/// next costs them a round trip to the documentation for something the parser
/// already knew.
pub fn parse(tokens: &[String]) -> Result<Request, String> {
    let Some(surface) = tokens.first() else {
        return Err(
            "nothing was asked for; the surfaces are `mcp`, `plugin`, `skill` and `config`, \
                    and each takes a verb after it — `mcp add`, `plugin list`, `skill add`, \
                    `config set`"
                .to_string(),
        );
    };
    let surface = surface.strip_prefix('/').unwrap_or(surface.as_str());
    // **The plural is the same surface, and it is folded here rather than in the
    // router.** [`crate::commands`] takes `/plugins` and `/servers` on purpose —
    // the thing being listed is plural, so the plural is what a hand reaches for
    // — and it routes those lines, whole, to this parse. Until 0.29.0 they arrived
    // and were refused by the arm at the bottom as surfaces io does not manage, so
    // `/plugins install x` was accepted by one module and refused by the next.
    //
    // Here rather than there because this is the only door **both** ways in go
    // through: `io plugins install x` from a shell reaches this function without
    // passing the router at all, and a fold written in the router would take the
    // slash form and leave the argv form refused — which is the same disagreement
    // moved rather than ended, and the one F6's byte comparison exists to forbid.
    //
    // The refusals below name the canonical spelling, which is the one the
    // operator's next line may as well carry. An unknown word is not folded and is
    // still echoed back as typed.
    let surface = match surface {
        "plugins" => "plugin",
        "servers" => "mcp",
        // `/skills` is the command and `io skill …` reads better in a shell, so
        // both spellings fold here for the plural's own reason: the fold has to be
        // on the door **both** ways in go through, or the argv form and the slash
        // form end up disagreeing about a word.
        "skills" => "skill",
        other => other,
    };
    let verb = tokens.get(1).map(String::as_str);
    let args = scan(tokens.get(2..).unwrap_or(&[]))?;

    match (surface, verb) {
        // `no_scope` for the reason every skill verb has it: a skill is a file in
        // io-cli's own home and no configuration file declares one, so a scope
        // typed here would name something this surface does not write.
        ("skill", Some("add")) => {
            args.no_scope("skill add")?;
            args.only("skill add", &[])?;
            Ok(Request::Skill(SkillVerb::Add {
                source: std::path::PathBuf::from(
                    args.one_word("skill add", "the path of a skill file")?,
                ),
            }))
        }
        ("skill", Some("list")) => {
            args.nothing("skill list")?;
            Ok(Request::Skill(SkillVerb::List))
        }
        ("skill", Some("remove")) => {
            args.no_scope("skill remove")?;
            args.only("skill remove", &[])?;
            Ok(Request::Skill(SkillVerb::Remove {
                name: args.one_word("skill remove", "the name of an installed skill")?,
            }))
        }
        ("mcp", Some("add")) => mcp_add(&args).map(Request::Mcp),
        ("mcp", Some("list")) => {
            args.nothing("mcp list")?;
            Ok(Request::Mcp(McpVerb::List))
        }
        ("mcp", Some("get")) => {
            args.only("mcp get", &[])?;
            Ok(Request::Mcp(McpVerb::Get {
                id: args.one_word("mcp get", "the id of a configured server")?,
            }))
        }
        ("mcp", Some("edit")) => mcp_edit(&args).map(Request::Mcp),
        // **One arm for the pair, because they are one verb with a value.** Two
        // arms would be two spellings of the same three refusals, and the one that
        // got less attention is the one that would stop refusing a `--scope`.
        // `no_scope` for `remove`'s reason: the entry lives in exactly one file and
        // io finds it by name, so a scope typed here would aim a position counted
        // in one file's array at another file's.
        ("mcp", Some(word @ ("enable" | "disable"))) => {
            let named = format!("mcp {word}");
            args.no_scope(&named)?;
            args.only(&named, &[])?;
            let id = args.one_word(&named, "the id of a configured server")?;
            Ok(Request::Mcp(if word == "enable" {
                McpVerb::Enable { id }
            } else {
                McpVerb::Disable { id }
            }))
        }
        ("mcp", Some("probe")) => {
            args.only("mcp probe", &[])?;
            Ok(Request::Mcp(McpVerb::Probe {
                id: args.one_word("mcp probe", "the id of a configured server")?,
            }))
        }
        ("mcp", Some("remove")) => {
            args.no_scope("mcp remove")?;
            args.only("mcp remove", &[])?;
            Ok(Request::Mcp(McpVerb::Remove {
                id: args.one_word("mcp remove", "the id of a configured server")?,
            }))
        }
        // **`install` is the same verb and not a second one.** It is the word an
        // operator arriving from any other tool types for this, and the whole cost
        // of admitting it is this pattern — one arm, one request, one refusal.
        // Spelling it as a second variant would be two verbs to keep writing the
        // same entry. The refusal below and `verbs` are updated together, always.
        ("plugin", Some("add" | "install")) => {
            args.only("plugin add", &["scope"])?;
            Ok(Request::Plugin(PluginVerb::Add {
                path: PathBuf::from(args.one_word(
                    "plugin add",
                    "the directory of a bundle, or the name of one a marketplace holds",
                )?),
                scope: args.scope()?,
            }))
        }
        ("plugin", Some("list")) => {
            args.nothing("plugin list")?;
            Ok(Request::Plugin(PluginVerb::List))
        }
        ("plugin", Some("search")) => {
            args.only("plugin search", &[])?;
            Ok(Request::Plugin(PluginVerb::Search {
                text: args.one_word(
                    "plugin search",
                    "some text to look for in every marketplace's bundles",
                )?,
            }))
        }
        // **Scanned from the sub-verb rather than from `args` above**, because
        // `args` was sorted out of `tokens[2..]` and here `tokens[2]` is the verb
        // itself: reading the name out of `args.positional[1]` would make
        // `plugin marketplace list add` a listing with a stray word nobody
        // refused. One extra `scan` is the price of the sub-verb having the same
        // flag rules, the same `--` rule and the same refusals as every other
        // verb in this module.
        ("plugin", Some("marketplace")) => marketplace(tokens).map(Request::Plugin),
        ("plugin", Some("remove")) => {
            args.no_scope("plugin remove")?;
            args.only("plugin remove", &[])?;
            Ok(Request::Plugin(PluginVerb::Remove {
                word: args.one_word(
                    "plugin remove",
                    "the directory of a bundle, or the name of a bundle that is declared",
                )?,
            }))
        }
        ("config", Some("get")) => {
            args.only("config get", &[])?;
            Ok(Request::Config(ConfigVerb::Get {
                key: args.one_word("config get", "a setting's dotted key")?,
            }))
        }
        ("config", Some("set")) => config_set(&args),
        ("config", Some("unset")) => {
            args.only("config unset", &["scope"])?;
            Ok(Request::Config(ConfigVerb::Unset {
                key: args.one_word("config unset", "a setting's dotted key")?,
                scope: args.scope_or_inherited()?,
            }))
        }
        ("config", Some("list")) => {
            args.nothing("config list")?;
            Ok(Request::Config(ConfigVerb::List))
        }
        // `skill` belongs here and was missing until 0.30.2, so a mistyped verb on
        // that one surface fell through to the arm below and answered "`skill` is
        // not a surface io manages; they are `mcp`, `plugin`, `skill` and
        // `config`" — a sentence that denies and asserts the same fact in one
        // breath, and never names the verbs the operator was reaching for. The
        // bare-surface arm underneath already listed all four, which is what makes
        // the omission an oversight rather than a decision.
        ("mcp" | "plugin" | "config" | "skill", Some(unknown)) => Err(format!(
            "`{unknown}` is not a verb `{surface}` takes; it takes {}",
            verbs(surface)
        )),
        ("mcp" | "plugin" | "config" | "skill", None) => Err(format!(
            "`{surface}` needs a verb after it; it takes {}",
            verbs(surface)
        )),
        (unknown, _) => Err(format!(
            "`{unknown}` is not a surface io manages; they are `mcp`, `plugin`, `skill` and \
             `config`"
        )),
    }
}

/// The verbs of one surface, for the sentence that refuses another word.
///
/// **Updated in the same edit as the reader above, always.** A refusal listing
/// verbs the parse does not take, or omitting one it does, tells an operator at a
/// terminal that a verb does not exist — which costs them the trip to the
/// documentation this whole module's refusals exist to save.
fn verbs(surface: &str) -> &'static str {
    match surface {
        "mcp" => "`add`, `list`, `get`, `edit`, `enable`, `disable`, `probe` and `remove`",
        // `remove` takes the same two readings `add` does — a directory, or the
        // name of a bundle — and says so here, because an operator who was refused
        // is being told what to type next and `remove <path>` alone would send the
        // one holding a name to the documentation.
        "plugin" => {
            "`add <path|name>` (also spelled `install`), `list`, `search <text>`, \
             `remove <path|name>` and `marketplace`"
        }
        "skill" => "`add <path>`, `list` and `remove <name>`",
        _ => "`get`, `set`, `unset` and `list`",
    }
}

/// The verbs `plugin marketplace` takes. Named once, for the same reason.
const MARKET_VERBS: &str = "`add <owner/repo>`, `list` and `remove <owner/repo>`";

/// `plugin marketplace (add <owner/repo> | list | remove <owner/repo>)`.
///
/// The sub-verb is `tokens[2]` and its own words are `tokens[3..]`, scanned here
/// so that a flag, a `--` or a second positional is refused by the same three
/// helpers every other verb uses. None of the three takes a flag at all —
/// including `--scope`, which is refused by [`Args::only`] with the rest, because
/// a marketplace is not written into any configuration file and so has no scope to
/// choose.
fn marketplace(tokens: &[String]) -> Result<PluginVerb, String> {
    let args = scan(tokens.get(3..).unwrap_or(&[]))?;
    let verb = match tokens.get(2).map(String::as_str) {
        Some("add") => {
            args.only("plugin marketplace add", &[])?;
            MarketVerb::Add(judged(&args.one_word(
                "plugin marketplace add",
                "the name of a marketplace, written `<owner>/<repo>`",
            )?)?)
        }
        Some("list") => {
            args.nothing("plugin marketplace list")?;
            MarketVerb::List
        }
        Some("remove") => {
            args.only("plugin marketplace remove", &[])?;
            MarketVerb::Remove(judged(&args.one_word(
                "plugin marketplace remove",
                "the name of a marketplace, written `<owner>/<repo>`",
            )?)?)
        }
        Some(unknown) => {
            return Err(format!(
                "`{unknown}` is not a verb `plugin marketplace` takes; it takes {MARKET_VERBS}"
            ))
        }
        None => {
            return Err(format!(
                "`plugin marketplace` needs a verb after it; it takes {MARKET_VERBS}"
            ))
        }
    };
    Ok(PluginVerb::Marketplace(verb))
}

/// A marketplace name, judged by the one function that judges them.
///
/// [`crate::fetch::resolve`] is the whole rule — two ordinary path segments, no
/// leading dash, no `..`, no third segment — and it is deliberately not repeated
/// here. The refusal says what is accepted and *where it is fetched from*, because
/// the single-forge ceiling is a stated bound rather than an oversight and an
/// operator who pasted a URL from somewhere else has to be told which fact refused
/// them.
///
/// Not called `named`: that name is already taken in this module by the flag-list
/// renderer, and two functions one letter apart in purpose is how the wrong one
/// gets called.
fn judged(text: &str) -> Result<crate::fetch::Named, String> {
    crate::fetch::resolve(text).ok_or_else(|| {
        format!(
            "`{text}` is not a marketplace name; a marketplace is a GitHub repository named \
             `<owner>/<repo>`, as in `plugin marketplace add zeroonething/ultraship` — a whole \
             URL, a local path and a name with a leading `-` or a `..` in it are all refused"
        )
    })
}

/// Which file a `config` write lands in: the one `--scope` named, or the one
/// already deciding the key.
///
/// **The same answer `/config`'s descent gives**, through the same function, so
/// the two entry paths cannot disagree about where a key lives. A configuration
/// that cannot be discovered at all is not a reason to refuse the write — the
/// caller's own `configure::write` reports that far better than a guess here
/// would — so it falls back to the user scope, which is where a key nothing
/// decides goes anyway.
fn decided_scope(root: &Path, key: &str, asked: Option<Scope>) -> Scope {
    if let Some(scope) = asked {
        return scope;
    }
    io_harness::config::Config::discover(root)
        .map(|config| crate::configure::destination(&config, key).0)
        .unwrap_or(Scope::User)
}

/// Turn a request into the file and the edits that carry it out.
///
/// `Ok(None)` is a **read** — `list`, `get` — which writes nothing and is
/// rendered by the caller from the `Config` it already holds. It is `None` rather
/// than an empty `Plan` because an empty edit list handed to
/// [`crate::configure::write`] would create a file, discover the whole tree and
/// report success for a question nobody asked to have answered in writing.
///
/// **Every write is one of the edits this crate already had**, and that is the
/// constraint rather than an observation: `servers::add`, `servers::edit`,
/// `servers::remove`, `pluginview::add`, `pluginview::remove`, `Edit::set` and
/// `Edit::unset` are what the interactive surfaces write through, and a second
/// path assembled here would be a second set of bytes to keep equal — which is
/// the defect the whole module is arranged against.
///
/// **`Err` is also how a bundle is refused before anything is written.** A
/// `plugin add <name>` resolved out of a marketplace runs
/// `io_harness::Plugins::inspect` here, so a manifest io-harness would drop — a
/// bad id, a `[[hook]]` in a committed file, a `${env:}` substitution — ends the
/// request with io-harness's own sentence and no `Plan` at all. Neither door can
/// therefore write first and find out afterwards, which is what both of them did
/// through 0.29.0. What a bundle it *accepts* would bring rides on
/// [`Plan::disclosure`], for the door to show and get consent for before it calls
/// [`crate::configure::write`].
///
/// The scope of a change to something that **already exists** is not the
/// operator's to choose: it is the file that declares it, found by
/// [`crate::servers::declared_in`] and [`crate::pluginview::declared_at`]. A
/// `--scope` there is refused at parse rather than honoured, because honouring it
/// would aim an index counted in one file's array at a different file's.
///
/// # `declared`, and why it is handed in rather than read
///
/// Every bundle the caller's already-resolved view names, as `(id, directory)`
/// pairs — [`crate::pluginview::ids`] builds it — and **the `plugin remove` arm is
/// the only thing in this function that reads it**. It is a parameter because
/// reading it here would mean calling `Config::plugins()`, which is not an
/// accessor: it re-reads, re-parses and re-trust-checks every declared manifest
/// off the disk on every call, and `tests/dependencies.rs` confines it by exact
/// path to `src/resolved.rs`. Both doors already hold the view, so nothing new is
/// computed by passing it; a door planning any other request may pass an empty
/// slice.
pub fn plan(
    root: &Path,
    request: &Request,
    declared: &[(String, PathBuf)],
) -> Result<Option<Plan>, String> {
    let plan = match request {
        // **No skill verb plans an `Edit`, and none ever will.** A skill is a
        // markdown file in io-cli's own home; no configuration file declares one,
        // there is no `enabled` key for one in the harness, and `Plan` is a list of
        // edits to somebody's `io.toml`. Installing copies a file and removing
        // unlinks one — acts that happen on the door, in `skillview`, the way a
        // probe does. `Ok(None)` is the honest answer and it is the same one every
        // read verb here gives.
        Request::Skill(_) => return Ok(None),
        Request::Mcp(McpVerb::Add { server, scope }) => Plan {
            scope: *scope,
            edits: vec![crate::servers::add(server)],
            disclosure: None,
            staged: None,
        },
        Request::Mcp(McpVerb::Edit { id, key, value }) => {
            let at = declared_server(root, id)?;
            let edit = crate::servers::edit(&at, key, value).ok_or_else(|| {
                format!(
                    "`{key}` is not a key an `[[mcp]]` entry carries, and io-harness would accept \
                     it into the file and ignore it; the keys are {}",
                    crate::servers::KEYS
                        .iter()
                        .map(|known| format!("`{known}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })?;
            Plan {
                scope: at.scope,
                edits: vec![edit],
                disclosure: None,
                staged: None,
            }
        }
        Request::Mcp(McpVerb::Remove { id }) => {
            let at = declared_server(root, id)?;
            Plan {
                scope: at.scope,
                edits: vec![crate::servers::remove(&at)],
                disclosure: None,
                staged: None,
            }
        }
        // **The entry is found and then edited, never removed and re-added.**
        // `servers::switch` is the one write, shared with the `/mcp` keystroke, so
        // whichever door was typed the file gains the same four bytes.
        Request::Mcp(McpVerb::Enable { id }) => switched(root, id, true)?,
        Request::Mcp(McpVerb::Disable { id }) => switched(root, id, false)?,
        Request::Plugin(PluginVerb::Add { path, scope }) => {
            // **Both readings of the word, decided in one place.** A directory
            // carrying a manifest is a path and everything else is a name looked
            // up across the marketplaces — the rule, and the refusal when neither
            // reading holds, are `marketplace::chosen`'s so that this arm cannot
            // grow a second opinion about which was meant. The marketplaces are
            // read behind a closure, so an ordinary `plugin add ./bundles/x` still
            // walks nothing but the directory it was given.
            //
            // `display()` gives back the word as typed: `path` is a `PathBuf` made
            // out of one token and is never rendered from components.
            // **The three homes an install may need, resolved here because this is
            // the door.** A remote entry is cloned, and a Claude Code or Codex
            // bundle gets a manifest generated for it; neither directory exists
            // when the word is typed. `marketplace::chosen` takes them rather than
            // reading `crate::home` itself, which is that module's standing rule.
            let homes = crate::marketplace::Homes {
                marketplaces: &crate::home::marketplaces().ok_or(crate::marketplace::NOWHERE)?,
                staging: &crate::home::staging().ok_or(crate::marketplace::NOWHERE)?,
                adapters: &crate::home::adapters().ok_or(crate::marketplace::NOWHERE)?,
            };
            let chosen = crate::marketplace::chosen(
                &resolve(root, path),
                || crate::marketplace::installed().unwrap_or_default(),
                &path.display().to_string(),
                homes,
            )?;
            let written = crate::pluginview::declared(root, chosen.dir());
            // **Which reading won decides whether anything is disclosed, and that
            // is the whole of the rule.** A directory the operator typed is
            // declared without ceremony, which is what `/plugin add ./bundles/x`
            // has done since 0.28.0 and is not a thing to second-guess. A bundle
            // resolved out of a marketplace is a stranger's code nobody here has
            // read, so io-harness reads, parses, validates and trust-checks it
            // **now** — `Plugins::inspect`, 0.71.0 — and a refusal is this
            // function's `Err`, in io-harness's own sentence, with the operator's
            // configuration file never opened.
            //
            // `Chosen::discloses` is the one place that question is answered, and
            // the scope handed to `inspect` is the scope this `Plan` is about to
            // be written into: a `[[hook]]` is the bundle's own business in a
            // user-scope file and is refused whole in a committed one, and an
            // install that inspected at the wrong scope would disclose a bundle
            // that then dropped, or refuse one that would have loaded.
            //
            // **`adapted_disclosure` and not `disclosure`, and the second argument
            // is the point.** For an adapted bundle the directory io-harness is
            // asked to load is the generated manifest's, which carries no hooks —
            // they do not cross. The author's own directory is where the hooks it
            // declares are, and naming them is the whole of what io owes an
            // operator who would otherwise reasonably assume they run.
            //
            // **`chosen.read()` and not `chosen.dir()` for the directory handed to
            // io-harness (0.38.0).** An adapted bundle's adapter is staged beside
            // its destination until the operator answers, so `dir()` is either a
            // directory that does not exist yet or one still holding the *previous*
            // install — and a disclosure read from it would describe last week's
            // bundle while the operator answers about this week's. `dir()` stays
            // the argument the operator is shown and the `[[plugin]]` entry names,
            // because that is where the adapter is a moment after they say yes.
            let disclosure = chosen
                .discloses()
                .then(|| {
                    crate::marketplace::adapted_disclosure(
                        *scope,
                        chosen.dir(),
                        chosen.read(),
                        (chosen.from() != chosen.dir()).then(|| chosen.from()),
                        chosen.copied(),
                    )
                })
                .transpose()
                .inspect_err(|_| crate::marketplace::unmake(chosen.staged()))?;
            // **And nothing is written at all when the entry is already there
            // (0.35.0).** Since the adapter became a copy of what it contributes,
            // **installing again is the update path** — and `pluginview::add`
            // appends unconditionally while `Edit::append` always splices at end
            // of file, so every refresh added a *second* `[[plugin]]` naming the
            // same directory. io-harness drops it with "a plugin with id `x` is
            // already declared and switched on", so the bundle kept working and
            // the operator's file grew an ignored entry on each update, quietly,
            // forever.
            //
            // There is no edit to make: the declaration that names the adapter is
            // already correct, and the refreshed adapter itself lands when the
            // operator accepts. The disclosure still travels, because what the
            // refresh moves is exactly what they need to see before they answer —
            // and on this path it is the *only* thing that travels, so a plan with
            // no edits still carries an act.
            let already = declared
                .iter()
                .any(|(_, path)| path.as_path() == chosen.dir());
            Plan {
                staged: chosen.staged().cloned(),
                scope: *scope,
                // One entry, switched on, written once. Through 0.29.0 a
                // marketplace bundle was written `enabled = false` first and
                // switched on afterwards, because declaring it was the only way to
                // have io-harness read it at all; `inspect` has replaced that
                // round trip, so there is no longer an entry in the file that the
                // operator has not agreed to. See `pluginview::add_off`.
                edits: if already {
                    Vec::new()
                } else {
                    vec![crate::pluginview::add(&written)]
                },
                disclosure,
            }
        }
        Request::Plugin(PluginVerb::Remove { word }) => {
            let (scope, index) = removal(root, word, declared)?;
            Plan {
                scope,
                edits: vec![crate::pluginview::remove(index)],
                disclosure: None,
                staged: None,
            }
        }
        Request::Config(ConfigVerb::Set { key, value, scope }) => Plan {
            scope: decided_scope(root, key, *scope),
            edits: vec![Edit::set(key.clone(), value.clone())],
            disclosure: None,
            staged: None,
        },
        Request::Config(ConfigVerb::Unset { key, scope }) => Plan {
            scope: decided_scope(root, key, *scope),
            // `unset` and not `remove`, and the two are not interchangeable:
            // `remove` takes a whole `[section]` or `[[array]]` entry away and
            // cannot name a key at all, so asked for `run.max_steps` it would
            // look for a `[run.max_steps]` header, find none, and refuse — or,
            // for a key whose name happens to be a section's, delete the
            // operator's entire block. See `Edit::unset`.
            edits: vec![Edit::unset(key.clone())],
            disclosure: None,
            staged: None,
        },
        // **A marketplace verb plans no write, and it is here beside the reads
        // rather than given a shape of its own.** `add` and `remove` are not
        // reads — they change the disk — but they change *the disk* and not a
        // configuration file: there is no scope to choose, no `[[…]]` entry to
        // splice and no [`Edit`] that could express either. A `Plan` carrying an
        // empty edit list is the thing `Ok(None)` exists to prevent (see this
        // function's own docs: `configure::write` would create the file, discover
        // the whole tree and report a write that never happened), and a second
        // member on `Plan` for "run this instead" would be the second write path
        // this module forbids. So the act lives in [`crate::marketplace`], which
        // both doors call and neither reimplements, and `plan` answers what is
        // true of it here: **nothing is written to any configuration file**. That
        // is also exactly criterion F3 — removing a marketplace cannot touch a
        // `[[plugin]]` entry, because this function never builds one.
        //
        // `search` is here as an ordinary read: it opens the marketplaces on the
        // disk rather than a configuration file, and it changes neither.
        //
        // **`mcp probe` is here, and it is the one read in this list that does
        // something.** It spawns or dials a real server and shuts it down again —
        // but it changes no configuration file, and a `Plan` is about a file. There
        // is no `Edit` that could express "go and look", and a `Plan` carrying an
        // empty edit list is what `Ok(None)` exists to prevent. So each door runs
        // `servers::probe` itself, through the one entry point, and prints
        // `servers::probed`'s one sentence.
        Request::Mcp(McpVerb::List | McpVerb::Get { .. } | McpVerb::Probe { .. })
        | Request::Plugin(
            PluginVerb::List | PluginVerb::Search { .. } | PluginVerb::Marketplace(_),
        )
        | Request::Config(ConfigVerb::Get { .. } | ConfigVerb::List) => return Ok(None),
    };
    Ok(Some(plan))
}

/// The plan `mcp enable` and `mcp disable` both come to.
///
/// One function because they are one write with a different value, and the file is
/// found the same way either way — by id, in whichever scope declares the entry.
fn switched(root: &Path, id: &str, on: bool) -> Result<Plan, String> {
    let at = declared_server(root, id)?;
    Ok(Plan {
        scope: at.scope,
        edits: vec![crate::servers::switch(&at, on)],
        disclosure: None,
        staged: None,
    })
}

/// Where the deciding file declares the server called `id`.
///
/// The refusal names the id and the verb that lists them, because "not found" over
/// a set of three configuration files tells an operator nothing about which of
/// them to open.
fn declared_server(root: &Path, id: &str) -> Result<crate::servers::At, String> {
    crate::servers::declared_in(root, id).ok_or_else(|| {
        format!(
            "no configuration file in force declares an MCP server called `{id}`, so there is \
             nothing to change; `mcp list` shows the ones that are configured"
        )
    })
}

/// Which `[[plugin]]` entry `plugin remove <word>` means: the directory, or the
/// bundle of that name.
///
/// **The path is read first and the disk is what answers**, which is
/// [`crate::marketplace::chosen`]'s standing rule for the same word on `plugin
/// add` and is deliberately the same rule here. Nothing about the *shape* of the
/// word is read — no rule about a `/`, a leading `.` or an extension — so
/// `io plugin remove ./bundles/rust-review` keeps meaning exactly what
/// `docs/guide/headless.md` says it means, and a directory that is declared is
/// always removed as one.
///
/// Only when no configuration file declares that directory is the word read as a
/// bundle's id, over the set the caller already resolved. **Every hit is collected
/// and the first is never taken.** [`crate::pluginview::Listed::id`] is unique
/// among the bundles io-harness *loaded* — two declared `enabled = false` may
/// share one, which is the swap the flag exists for — so taking one of them
/// deletes a `[[plugin]]` entry the operator never pointed at, silently, and they
/// find out when a bundle's skills stop being offered. Two candidates are
/// therefore refused with **both directories named**: the directory is the
/// spelling that tells them apart, every listing already prints it, and it is the
/// spelling that resolves through the path reading above.
///
/// The refused and dropped entries are in `declared` too, and they are exactly the
/// entries an operator most wants gone — a bundle whose manifest will not parse is
/// one they cannot fix from the manifest.
fn removal(
    root: &Path,
    word: &str,
    declared: &[(String, PathBuf)],
) -> Result<(Scope, usize), String> {
    let typed = resolve(root, Path::new(word));
    if let Some(at) = crate::pluginview::declared_at(root, &typed) {
        return Ok(at);
    }
    let hits = crate::pluginview::by_id(declared, word);
    let dir = match hits.as_slice() {
        [only] => *only,
        [] => {
            return Err(format!(
                "no configuration file declares {}, and no bundle one declares is called \
                 `{word}`, so there is no `[[plugin]]` entry to remove; `plugin list` shows what \
                 is declared",
                typed.display()
            ))
        }
        several => {
            let spellings = several
                .iter()
                .map(|dir| format!("`{}`", dir.display()))
                .collect::<Vec<_>>()
                .join(" and ");
            return Err(format!(
                "{} declared bundles are called `{word}`, and removing whichever was found first \
                 would take away an entry you did not choose; say which one by its directory: \
                 {spellings}",
                several.len()
            ));
        }
    };
    // A bundle that is in the resolved view was declared by *some* file, so this
    // is not the ordinary miss above: it is a declaration whose written path does
    // not match the directory io-harness read it from — a symlinked root, most
    // likely — and saying which directory was looked for is the whole of what an
    // operator can act on.
    crate::pluginview::declared_at(root, dir).ok_or_else(|| {
        format!(
            "the bundle called `{word}` was read from {}, and no configuration file declares that \
             directory, so there is no `[[plugin]]` entry to remove; `plugin list` shows what is \
             declared",
            dir.display()
        )
    })
}

/// A typed path, against the discovery root when it is relative.
fn resolve(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

// --- the scan -----------------------------------------------------------------

/// One line's words, sorted into the three things a word can be.
///
/// Deliberately not a per-verb struct: every verb reads the same three piles, and
/// a scanner per verb is how the two doors would come to disagree about what a
/// `--` means.
#[derive(Debug, Default)]
struct Args {
    /// Words that are not flags and are not past the `--`.
    positional: Vec<String>,
    /// `(name, value)` in the order they were written, repeats kept — `--env` and
    /// `--header` are repeatable and a map here would silently keep the last.
    flags: Vec<(String, String)>,
    /// Everything after the first `--`, verbatim. `Some(vec![])` is a `--` with
    /// nothing after it, which is a different mistake from no `--` at all.
    opaque: Option<Vec<String>>,
}

/// Sort a verb's words into [`Args`].
///
/// **The scan stops dead at the first `--`.** Nothing after it is examined, so a
/// server's own `--plain`, `--json` or `--scope` is copied through as text and
/// cannot be read as io's.
fn scan(tokens: &[String]) -> Result<Args, String> {
    let mut args = Args::default();
    let mut i = 0;
    while i < tokens.len() {
        let token = &tokens[i];
        if token == "--" {
            args.opaque = Some(tokens[i + 1..].to_vec());
            break;
        }
        let Some(name) = token.strip_prefix("--") else {
            // A single dash followed by a LETTER is somebody reaching for a short
            // flag io does not have, and saying so is better than writing their
            // `-v` into a file as a server's name. A single dash followed by
            // anything else is a value: `app.io-cli.gates.expect_exit` is the one
            // signed setting in the catalogue and `-1` is a legitimate value for
            // it, so a rule that refused every leading dash would refuse the only
            // key that needs one.
            if token.starts_with('-') && token.chars().nth(1).is_some_and(char::is_alphabetic) {
                return Err(format!(
                    "`{token}` is not one of io's own flags — they are all spelled with two \
                     dashes — and a flag meant for the server itself belongs after `--`"
                ));
            }
            args.positional.push(token.clone());
            i += 1;
            continue;
        };
        // `--name=value` as well as `--name value`, because the first is what a
        // shell user types and the second is what a composer user types, and a
        // parse that took only one of them would be a parse that works on one
        // surface.
        let (name, inline) = match name.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (name, None),
        };
        if name.is_empty() {
            return Err(format!(
                "`{token}` names no flag; io's flags are spelled `--name value` or `--name=value`"
            ));
        }
        let value = match inline {
            Some(value) => {
                i += 1;
                value
            }
            None => {
                let Some(value) = tokens.get(i + 1) else {
                    return Err(format!(
                        "`--{name}` needs a value after it and the line ends there"
                    ));
                };
                i += 2;
                value.clone()
            }
        };
        args.flags.push((name.to_string(), value));
    }
    Ok(args)
}

impl Args {
    /// The one value of `name`, or `None`.
    ///
    /// Repetition is refused rather than resolved: `--url a --url b` is two
    /// different servers written down, and picking either is picking for someone
    /// who has already told you they are not sure.
    fn one(&self, name: &str) -> Result<Option<&str>, String> {
        let mut found = self.flags.iter().filter(|(flag, _)| flag == name);
        let first = found.next().map(|(_, value)| value.as_str());
        if found.next().is_some() {
            return Err(format!(
                "`--{name}` was given more than once and it takes a single value; nothing was \
                 written"
            ));
        }
        Ok(first)
    }

    /// Every value of `name`, in the order written. For the repeatable pair.
    fn all(&self, name: &str) -> Vec<&str> {
        self.flags
            .iter()
            .filter(|(flag, _)| flag == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// Refuse any flag `verb` does not take, by name.
    fn only(&self, verb: &str, known: &[&str]) -> Result<(), String> {
        for (name, _) in &self.flags {
            if !known.contains(&name.as_str()) {
                return Err(format!(
                    "`--{name}` is not a flag `{verb}` takes; it takes {}",
                    named(known)
                ));
            }
        }
        Ok(())
    }

    /// Refuse a `--` section on a verb that has no command to carry.
    fn no_command(&self, verb: &str) -> Result<(), String> {
        match &self.opaque {
            None => Ok(()),
            Some(rest) => Err(format!(
                "`{verb}` takes no command, so the `--` and everything after it ({}) has nowhere \
                 to go; a server's command is written when it is added",
                rest.join(" ")
            )),
        }
    }

    /// Refuse `--scope` where the scope is not the operator's to choose.
    fn no_scope(&self, verb: &str) -> Result<(), String> {
        if self.all("scope").is_empty() {
            return Ok(());
        }
        Err(format!(
            "`{verb}` takes no `--scope`: the change goes to the file that declares the entry, \
             which io finds by name — a scope chosen here would aim a position counted in one \
             file's array at another file's"
        ))
    }

    /// `--scope`, defaulting to the operator's own file.
    ///
    /// [`Scope::User`] rather than the workspace, because it is the file that is
    /// this person's and is not committed: a default of `Scope::Project` would put
    /// one operator's server into a repository everyone else clones, which is a
    /// disclosure and not a convenience.
    /// The scope `--scope` named, or `None` when it was not given.
    ///
    /// **`None` is not "the user scope"**, and collapsing the two is F13's own
    /// named sabotage. A `config set` with no `--scope` inherits the file already
    /// deciding the key; answering `Scope::User` here instead would silently
    /// shadow a committed project setting with a personal one, which is the change
    /// an operator is least able to see afterwards. The verbs that create a new
    /// entry — `mcp add`, `plugin add` — have nothing to inherit and resolve the
    /// `None` to the user scope themselves.
    fn scope_or_inherited(&self) -> Result<Option<Scope>, String> {
        Ok(match self.one("scope")? {
            None => None,
            Some("user") => Some(Scope::User),
            Some("project") => Some(Scope::Project),
            Some("local") => Some(Scope::Local),
            Some(other) => {
                return Err(format!(
                    "`{other}` is not a scope; they are `user` (your own file), `project` (the \
                     committed one) and `local` (this checkout only)"
                ))
            }
        })
    }

    fn scope(&self) -> Result<Scope, String> {
        Ok(match self.one("scope")? {
            None => Scope::User,
            Some("user") => Scope::User,
            Some("project") => Scope::Project,
            Some("local") => Scope::Local,
            Some(other) => {
                return Err(format!(
                    "`{other}` is not a scope; they are `user` (your own file), `project` (the \
                     committed one) and `local` (this checkout only)"
                ))
            }
        })
    }

    /// Exactly one positional, described by what it should have been.
    fn one_word(&self, verb: &str, wanted: &str) -> Result<String, String> {
        self.no_command(verb)?;
        match self.positional.len() {
            1 => Ok(self.positional[0].clone()),
            0 => Err(format!("`{verb}` needs {wanted} after it")),
            _ => Err(format!(
                "`{verb}` takes {wanted} and nothing else, and {} words were given ({}); quote a \
                 value that contains a space",
                self.positional.len(),
                self.positional.join(" ")
            )),
        }
    }

    /// No positionals, no flags, no command. For the two listing verbs.
    fn nothing(&self, verb: &str) -> Result<(), String> {
        self.no_command(verb)?;
        self.only(verb, &[])?;
        if self.positional.is_empty() {
            return Ok(());
        }
        Err(format!(
            "`{verb}` lists everything and takes no arguments, so `{}` was not understood",
            self.positional.join(" ")
        ))
    }
}

/// A flag list for a refusal, or the sentence for a verb that takes none.
fn named(flags: &[&str]) -> String {
    if flags.is_empty() {
        return "no flags at all".to_string();
    }
    flags
        .iter()
        .map(|flag| format!("`--{flag}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

// --- mcp ----------------------------------------------------------------------

/// The flags `mcp add` takes. Named once, so the refusal and the reader agree.
const ADD_FLAGS: &[&str] = &["url", "transport", "env", "header", "timeout-secs", "scope"];

/// `mcp add <id> (--url <URL> | -- <command> [args…]) [flags]`.
///
/// The two shapes an operator may have learned elsewhere both arrive here and
/// neither gets a branch of its own — see the module docs. What varies between
/// them is one thing, `url`, and where it was read from; everything below that
/// line is the same code for both.
fn mcp_add(args: &Args) -> Result<McpVerb, String> {
    args.only("mcp add", ADD_FLAGS)?;
    let scope = args.scope()?;

    let Some(id) = args.positional.first().cloned() else {
        return Err(
            "`mcp add` needs a name for the server, then either `--url <URL>` for an HTTP \
                    server or `-- <command> [args…]` for one io starts itself"
                .to_string(),
        );
    };
    // A second positional IS a URL, whatever `--transport` says and wherever it
    // sits. That is what makes `mcp add --transport http linear https://…/mcp`
    // and `mcp add linear --url https://…/mcp` one reading rather than two: the
    // flag never selects a parse, it only asserts a claim that is checked below.
    let positional_url = match args.positional.get(1) {
        None => None,
        Some(url) if url.starts_with("http://") || url.starts_with("https://") => Some(url.clone()),
        Some(other) => {
            return Err(format!(
                "a second word after the server's name is read as its URL, and `{other}` is not \
                 one; the command of a server io starts itself goes after `--`, as in `mcp add \
                 {id} -- {other} …`"
            ))
        }
    };
    if args.positional.len() > 2 {
        return Err(format!(
            "`mcp add` takes a name and at most a URL, and {} words were given ({}); a command \
             and its arguments go after `--`",
            args.positional.len(),
            args.positional.join(" ")
        ));
    }
    let url = match (args.one("url")?, positional_url) {
        (Some(flag), Some(positional)) if flag != positional => {
            return Err(format!(
                "two different URLs were given — `--url {flag}` and `{positional}` — and io will \
                 not choose between them"
            ))
        }
        (Some(flag), _) => Some(flag.to_string()),
        (None, positional) => positional,
    };

    // The claim, checked against the form rather than used to pick one. Both arms
    // name the flag AND the form it contradicts, because the operator has to know
    // which of the two things they wrote is the one to delete.
    match args.one("transport")? {
        Some("stdio") if url.is_some() => {
            return Err(
                "`--transport stdio` was given together with a URL, and a stdio server \
                        has no URL: io starts it as a command. Drop the `--transport stdio` for \
                        an HTTP server, or write the command after `--` instead of the URL"
                    .to_string(),
            )
        }
        Some("http") if args.opaque.is_some() => {
            return Err(
                "`--transport http` was given together with a command after `--`, and an \
                        HTTP server is not started by io: it is dialled. Drop the `--transport \
                        http` for a server io starts, or give `--url <URL>` instead of the \
                        command"
                    .to_string(),
            )
        }
        Some("http" | "stdio") | None => {}
        Some(other) => {
            return Err(format!(
                "`--transport {other}` is not a transport; they are `http`, which is written \
                 `--url <URL>`, and `stdio`, which is written `-- <command> [args…]`"
            ))
        }
    }

    let env = pairs(&args.all("env"), "env", "KEY=VALUE")?;
    let headers = pairs(&args.all("header"), "header", "NAME=VALUE")?;

    // The one construction site for `McpTransport` in this module. Two orderings,
    // one server, and a change to how a server is built cannot reach one door
    // without reaching the other.
    let transport = match (url, &args.opaque) {
        (Some(_), Some(_)) => {
            return Err(
                "both a URL and a command after `--` were given, and a server is reached \
                        one way or the other; keep the URL for a server io dials, or the command \
                        for one io starts"
                    .to_string(),
            )
        }
        (None, None) => {
            return Err(format!(
                "`mcp add {id}` says nothing about how to reach the server; give `--url <URL>` \
                 for one io dials over HTTP, or `-- <command> [args…]` for one io starts itself"
            ))
        }
        (Some(url), None) => {
            if !env.is_empty() {
                return Err(format!(
                    "`--env` sets the environment of a process io starts, and `{id}` is an HTTP \
                     server io only dials; a value an HTTP server needs is sent as `--header \
                     NAME=VALUE`"
                ));
            }
            McpTransport::Http { url, headers }
        }
        (None, Some(rest)) => {
            let Some((command, arguments)) = rest.split_first() else {
                return Err(format!(
                    "nothing follows the `--`, so there is no command to start `{id}` with; the \
                     shape is `mcp add {id} -- <command> [args…]`"
                ));
            };
            if !headers.is_empty() {
                return Err(format!(
                    "`--header` is sent with an HTTP request, and `{id}` is a server io starts as \
                     a process; something a started server needs is passed as `--env KEY=VALUE` \
                     or as one of its own arguments after `--`"
                ));
            }
            McpTransport::Stdio {
                command: command.clone(),
                // Verbatim, in the order written, including anything that looks
                // like one of io's own flags. See the module docs.
                args: arguments.to_vec(),
                env,
            }
        }
    };

    // Built by hand rather than through `McpServer::stdio(…).with_args(…)`,
    // because `env` and `headers` have no builder at all and `with_args` is a
    // silent no-op on an HTTP server (`io-harness-0.79.0/src/mcp.rs:439-447`: the
    // body writes only into the `Stdio` arm) — a constructor chain here would drop
    // the arguments of half the servers it was handed and say nothing.
    // Asked of the harness rather than written as literals, the way `servers::add`
    // asks it: the defaults are io-harness's, and a value copied into io-cli is one
    // that goes stale in silence. `enabled` is io-harness 0.70.0's; an added server
    // is on, and **since 0.30.0 it has both a keystroke and an argument form** —
    // `/mcp`'s toggle row and `io mcp enable|disable <id>`, which share
    // `servers::switch`.
    let defaults = McpServer::stdio("", "");
    let mut server = McpServer {
        id,
        transport,
        timeout_secs: defaults.timeout_secs,
        enabled: defaults.enabled,
    };
    if let Some(seconds) = args.one("timeout-secs")? {
        server.timeout_secs = timeout(seconds)?;
    }
    Ok(McpVerb::Add { server, scope })
}

/// `--timeout-secs`, refusing the two values that would break a server quietly.
fn timeout(value: &str) -> Result<u64, String> {
    let seconds: u64 = value.parse().map_err(|_| {
        format!(
            "`--timeout-secs {value}` is not a whole number of seconds; it is a per-call \
                 timeout, as in `--timeout-secs 30`"
        )
    })?;
    if seconds == 0 {
        return Err(
            "`--timeout-secs 0` would time every call out before it was made; leave the \
                    flag off for io-harness's own default"
                .to_string(),
        );
    }
    Ok(seconds)
}

/// `KEY=VALUE` pairs from a repeatable flag.
///
/// Split at the **first** `=`, because a value may contain one — an `Authorization`
/// header and a base64 token both do — and a split at the last would move half the
/// value into the name.
fn pairs(given: &[&str], flag: &str, shape: &str) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for pair in given {
        let Some((name, value)) = pair.split_once('=') else {
            return Err(format!(
                "`--{flag} {pair}` has no `=` in it; the shape is `--{flag} {shape}`"
            ));
        };
        if name.is_empty() {
            return Err(format!(
                "`--{flag} {pair}` names nothing before the `=`; the shape is `--{flag} {shape}`"
            ));
        }
        // Last wins for a repeated name, which is the only reading with a
        // meaning: the operator typed the second one after the first.
        out.insert(name.to_string(), value.to_string());
    }
    Ok(out)
}

/// `mcp edit <id> --<key> <value>`, one key at a time.
///
/// The vocabulary is `add`'s, so nothing has to be learned twice, and the three
/// keys that describe the *shape* of a server — `--transport`, `--env` and
/// `--header` — are refused by name rather than half-applied. Changing a server
/// from stdio to HTTP is not one key: it is a different set of keys, and an entry
/// that gained a `url` while keeping its `command` is one io-harness accepts and
/// reaches the wrong way. Re-adding the server writes the whole entry in one go,
/// which is what `servers::add` exists to do.
fn mcp_edit(args: &Args) -> Result<McpVerb, String> {
    const EDIT_FLAGS: &[&str] = &["command", "url", "timeout-secs"];
    for shaping in ["transport", "env", "header"] {
        if !args.all(shaping).is_empty() {
            return Err(format!(
                "`--{shaping}` is part of how a server is reached rather than one key of it, and \
                 an entry edited halfway into another transport is one io-harness loads and \
                 cannot use; remove the server and add it again, which writes the whole entry at \
                 once"
            ));
        }
    }
    args.no_scope("mcp edit")?;
    args.only("mcp edit", EDIT_FLAGS)?;

    let Some(id) = args.positional.first().cloned() else {
        return Err(
            "`mcp edit` needs the id of a configured server, then one of `--command`, \
                    `--url`, `--timeout-secs`, or `-- <args…>` for its arguments"
                .to_string(),
        );
    };
    if args.positional.len() > 1 {
        return Err(format!(
            "`mcp edit` takes one server id and a change to make, and `{}` was given after \
             `{id}`; a new value goes behind its flag, as in `mcp edit {id} --command mcp-find`",
            args.positional[1..].join(" ")
        ));
    }

    // Each candidate is `(key, TOML source)`, rendered here so that a command
    // path with a backslash or a quote in it cannot become a file that no longer
    // parses — `servers::edit` takes source, and `format!("\"{value}\"")` is the
    // call site every caller reaches for and the one that breaks on Windows.
    let mut changes: Vec<(String, String)> = Vec::new();
    if let Some(command) = args.one("command")? {
        changes.push(("command".to_string(), crate::servers::quoted(command)));
    }
    if let Some(url) = args.one("url")? {
        changes.push(("url".to_string(), crate::servers::quoted(url)));
    }
    if let Some(seconds) = args.one("timeout-secs")? {
        changes.push(("timeout_secs".to_string(), timeout(seconds)?.to_string()));
    }
    if let Some(rest) = &args.opaque {
        let items: Vec<&str> = rest.iter().map(String::as_str).collect();
        changes.push(("args".to_string(), crate::edit::array(&items)));
    }

    match changes.len() {
        1 => {
            let (key, value) = changes.remove(0);
            Ok(McpVerb::Edit { id, key, value })
        }
        0 => Err(format!(
            "`mcp edit {id}` says nothing to change; give one of `--command <path>`, `--url \
             <URL>`, `--timeout-secs <n>`, or `-- <args…>` to replace the server's arguments"
        )),
        // One key per write, because `configure::write` splices, re-discovers and
        // rolls back per call: two keys would be two round trips with a
        // half-changed server on disk between them, and the second is the one
        // that can be refused.
        _ => Err(format!(
            "`mcp edit` changes one key at a time and {} were given ({}); run it once per key",
            changes.len(),
            changes
                .iter()
                .map(|(key, _)| format!("`{key}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

// --- config -------------------------------------------------------------------

/// `config set <key> <value…> [--scope]`.
fn config_set(args: &Args) -> Result<Request, String> {
    args.no_command("config set")?;
    args.only("config set", &["scope"])?;
    let scope = args.scope_or_inherited()?;

    let Some(key) = args.positional.first().cloned() else {
        return Err(
            "`config set` needs a setting's dotted key and a value, as in `config set \
                    run.max_steps 30`; `config list` shows the keys"
                .to_string(),
        );
    };
    let value = config_value(&key, &args.positional[1..])?;

    // Reported here rather than discovered by the round trip, because the round
    // trip's refusal takes the WHOLE FILE: `refuse_widening` runs before
    // deserialization, so an operator who picks one of these in a project file
    // does not get a rejected setting, they get a configuration that no longer
    // parses. `configure::write` would roll it back and say so in io-harness's
    // words; saying it here means the file is never written at all.
    // **Only for a scope the operator named.** An inherited one is not known until
    // `plan` reads the configuration, so a key the *project file already decides*
    // is caught there by `configure::write`'s round trip in io-harness's own
    // words rather than here. Guessing at parse time would mean discovering the
    // workspace from a function whose whole job is reading a line of text.
    // **Both untrusted scopes, and the advice names neither of them (0.35.0).**
    // This guard read `Scope::Project` alone, so `--scope local` walked straight
    // past io-cli's own check and was caught only by `configure::write`'s
    // round-trip rollback — and the sentence it printed sent the operator to
    // `--scope local`, which io-harness 0.74.0 now refuses for exactly the same
    // reason. `io.local.toml` is not committed, but it sits in the workspace root
    // a run's own agent can write to, so one `write_file` of an unremarkable name
    // was an escalation. The user scope is the only destination left.
    if matches!(scope, Some(Scope::Project | Scope::Local))
        && crate::configure::widens_workspace(&key, &value)
    {
        return Err(format!(
            "io-harness refuses `{key} = {value}` in a file inside the workspace, because \
             `io.toml` is what a `git clone` hands to everyone and `io.local.toml` sits in a root \
             this run's own agent can write to — and this widens what they may do without asking \
             you; write it with `--scope user` for yourself"
        ));
    }

    Ok(Request::Config(ConfigVerb::Set { key, value, scope }))
}

/// The TOML source for `words`, checked against what the key admits.
///
/// **The body moved to [`crate::configure::source_for`] in 0.38.1, and the move
/// is the fix rather than a tidy-up.** It lived here while `io config set` was
/// the only door that spelled a value. The session's `/config` never reached it:
/// `Action::Config` carried the operator's raw text straight to `write_where` as
/// TOML source, so `4` landed as the integer `4` there and as the string `"4"`
/// here, and `allow` landed as a bare word TOML cannot parse at all. Two doors,
/// two spellings, neither of them right — and the module that owns the catalogue
/// is the one that should own the spelling. This name is kept because it is what
/// this module's own parse reads well as.
fn config_value(key: &str, words: &[String]) -> Result<String, String> {
    crate::configure::source_for(key, words)
}
