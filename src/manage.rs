//! One parse for the managed surfaces, shared by the slash form and the argv one.
//!
//! `/mcp add …` typed into a composer and `io mcp add …` typed at a shell are the
//! same sentence arriving through two doors, and this module is the only room
//! behind both of them. Every verb this release adds — `mcp add|list|get|edit|
//! remove`, `plugin add|list|remove`, `config get|set|unset|list` — is turned into
//! a [`Request`] here and into [`crate::edit::Edit`]s by [`plan`], and neither
//! entry point is allowed a second reading of the same words.
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

use crate::configure::Kind;
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
}

/// What `/plugin` and `io plugin` can be asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginVerb {
    /// Declare a bundle directory.
    Add { path: PathBuf, scope: Scope },
    /// Every declared bundle, loaded and refused.
    List,
    /// Undeclare a bundle. No scope: the file that named it is the file the
    /// removal has to go to, and [`plan`] finds it.
    Remove { path: PathBuf },
}

/// What `/config` and `io config` can be asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigVerb {
    /// One key's value and what decided it.
    Get { key: String },
    /// Write one key. `value` is TOML source, already checked against the key's
    /// [`Kind`]: a choice arrives quoted, a number and a flag bare, so that what
    /// is written is a value io-harness reads back as the one that was typed.
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
    /// The scope whose file is written.
    pub scope: Scope,
    /// What to write, applied together or not at all.
    pub edits: Vec<Edit>,
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
/// about a surface called `/mcp`.
///
/// Every `Err` is a sentence naming what was wrong and what is accepted instead.
/// There is no bare "invalid argument" in this module: the operator is at a
/// terminal with no `--help` open, and a refusal that does not say what to type
/// next costs them a round trip to the documentation for something the parser
/// already knew.
pub fn parse(tokens: &[String]) -> Result<Request, String> {
    let Some(surface) = tokens.first() else {
        return Err(
            "nothing was asked for; the surfaces are `mcp`, `plugin` and `config`, and \
                    each takes a verb after it — `mcp add`, `plugin list`, `config set`"
                .to_string(),
        );
    };
    let surface = surface.strip_prefix('/').unwrap_or(surface.as_str());
    let verb = tokens.get(1).map(String::as_str);
    let args = scan(tokens.get(2..).unwrap_or(&[]))?;

    match (surface, verb) {
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
        ("mcp", Some("remove")) => {
            args.no_scope("mcp remove")?;
            args.only("mcp remove", &[])?;
            Ok(Request::Mcp(McpVerb::Remove {
                id: args.one_word("mcp remove", "the id of a configured server")?,
            }))
        }
        ("plugin", Some("add")) => {
            args.only("plugin add", &["scope"])?;
            Ok(Request::Plugin(PluginVerb::Add {
                path: PathBuf::from(args.one_word("plugin add", "the directory of a bundle")?),
                scope: args.scope()?,
            }))
        }
        ("plugin", Some("list")) => {
            args.nothing("plugin list")?;
            Ok(Request::Plugin(PluginVerb::List))
        }
        ("plugin", Some("remove")) => {
            args.no_scope("plugin remove")?;
            args.only("plugin remove", &[])?;
            Ok(Request::Plugin(PluginVerb::Remove {
                path: PathBuf::from(args.one_word("plugin remove", "the directory of a bundle")?),
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
        ("mcp" | "plugin" | "config", Some(unknown)) => Err(format!(
            "`{unknown}` is not a verb `{surface}` takes; it takes {}",
            verbs(surface)
        )),
        ("mcp" | "plugin" | "config", None) => Err(format!(
            "`{surface}` needs a verb after it; it takes {}",
            verbs(surface)
        )),
        (unknown, _) => Err(format!(
            "`{unknown}` is not a surface io manages; they are `mcp`, `plugin` and `config`"
        )),
    }
}

/// The verbs of one surface, for the sentence that refuses another word.
fn verbs(surface: &str) -> &'static str {
    match surface {
        "mcp" => "`add`, `list`, `get`, `edit` and `remove`",
        "plugin" => "`add`, `list` and `remove`",
        _ => "`get`, `set`, `unset` and `list`",
    }
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
/// The scope of a change to something that **already exists** is not the
/// operator's to choose: it is the file that declares it, found by
/// [`crate::servers::declared_in`] and [`crate::pluginview::declared_at`]. A
/// `--scope` there is refused at parse rather than honoured, because honouring it
/// would aim an index counted in one file's array at a different file's.
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

pub fn plan(root: &Path, request: &Request) -> Result<Option<Plan>, String> {
    let plan = match request {
        Request::Mcp(McpVerb::Add { server, scope }) => Plan {
            scope: *scope,
            edits: vec![crate::servers::add(server)],
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
            }
        }
        Request::Mcp(McpVerb::Remove { id }) => {
            let at = declared_server(root, id)?;
            Plan {
                scope: at.scope,
                edits: vec![crate::servers::remove(&at)],
            }
        }
        Request::Plugin(PluginVerb::Add { path, scope }) => {
            let dir = resolve(root, path);
            if let Some(refusal) = crate::pluginview::refusal(&dir) {
                return Err(refusal);
            }
            Plan {
                scope: *scope,
                edits: vec![crate::pluginview::add(&crate::pluginview::declared(
                    root, &dir,
                ))],
            }
        }
        Request::Plugin(PluginVerb::Remove { path }) => {
            let dir = resolve(root, path);
            let (scope, index) = crate::pluginview::declared_at(root, &dir).ok_or_else(|| {
                format!(
                    "no configuration file declares {}, so there is no `[[plugin]]` entry to \
                     remove; `plugin list` shows what is declared",
                    dir.display()
                )
            })?;
            Plan {
                scope,
                edits: vec![crate::pluginview::remove(index)],
            }
        }
        Request::Config(ConfigVerb::Set { key, value, scope }) => Plan {
            scope: decided_scope(root, key, *scope),
            edits: vec![Edit::set(key.clone(), value.clone())],
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
        },
        Request::Mcp(McpVerb::List | McpVerb::Get { .. })
        | Request::Plugin(PluginVerb::List)
        | Request::Config(ConfigVerb::Get { .. } | ConfigVerb::List) => return Ok(None),
    };
    Ok(Some(plan))
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
    // silent no-op on an HTTP server (`io-harness-0.69.0/src/mcp.rs:267`) — a
    // constructor chain here would drop the arguments of half the servers it was
    // handed and say nothing.
    // Asked of the harness rather than written as literals, the way `servers::add`
    // asks it: the defaults are io-harness's, and a value copied into io-cli is one
    // that goes stale in silence. `enabled` is io-harness 0.70.0's; an added server
    // is on, and the flag has no keystroke here yet — see the known limitations.
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
    if scope == Some(Scope::Project) && crate::configure::widens_project(&key, &value) {
        return Err(format!(
            "io-harness refuses `{key} = {value}` in a project file, because `io.toml` is what a \
             `git clone` hands to everyone and this widens what they may do without asking them; \
             write it with `--scope local` for this checkout or `--scope user` for yourself"
        ));
    }

    Ok(Request::Config(ConfigVerb::Set { key, value, scope }))
}

/// The TOML source for `words`, checked against what the key admits.
///
/// **The kind decides the shape and never the meaning.** io-harness owns what a
/// setting does; [`crate::configure::kind_of`] answers only how a value is
/// obtained, and this turns a typed word into the source that expresses it. A key
/// the catalogue does not name is written as a string and left to
/// `configure::write`'s round trip, which is io-harness reading its own file back
/// — inventing a kind for it here would be io-cli deciding a schema it does not
/// own.
fn config_value(key: &str, words: &[String]) -> Result<String, String> {
    let kind = crate::configure::kind_of(key);
    if matches!(kind, Some(Kind::Machine)) {
        return Err(format!(
            "`{key}` is written by io itself rather than typed — it dates the price table, and a \
             date set by hand is a claim about a fetch that never happened; `/config` offers the \
             refresh that writes it"
        ));
    }
    // The one key whose value is a list (`app.io-cli.gates.command`), so the
    // remaining words are the value rather than a mistake. A scalar written there
    // is a value io-harness cannot read back.
    if matches!(kind, Some(Kind::List)) {
        if words.is_empty() {
            return Err(format!(
                "`{key}` is a command and its arguments, so it needs at least one word, as in \
                 `config set {key} cargo test`"
            ));
        }
        let items: Vec<&str> = words.iter().map(String::as_str).collect();
        return Ok(crate::edit::array(&items));
    }

    let word = match words {
        [word] => word.as_str(),
        [] => {
            return Err(format!(
                "`config set {key}` has no value after it; {}",
                offered(key, kind.as_ref())
            ))
        }
        _ => {
            return Err(format!(
                "`{key}` takes one value and {} words were given ({}); quote a value that \
                 contains a space",
                words.len(),
                words.join(" ")
            ))
        }
    };

    match kind {
        Some(Kind::Flag) => match word {
            "true" | "false" => Ok(word.to_string()),
            other => Err(format!(
                "`{other}` is not a value `{key}` takes; it is on or off, written `true` or \
                 `false`"
            )),
        },
        Some(Kind::Choice(options)) => {
            if options.iter().any(|option| option == word) {
                Ok(crate::servers::quoted(word))
            } else {
                Err(format!(
                    "`{word}` is not a value `{key}` takes; the options are {}",
                    options
                        .iter()
                        .map(|option| format!("`{option}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            }
        }
        Some(Kind::Number { signed }) => {
            let number: i64 = word.parse().map_err(|_| {
                format!("`{word}` is not a whole number, and `{key}` counts in whole numbers")
            })?;
            if !signed && number < 0 {
                return Err(format!(
                    "`{key}` counts something and cannot be negative; `{word}` was not written"
                ));
            }
            // Re-rendered from the parsed number rather than passed through, so
            // that a form TOML would reject (`+5`, a leading zero run) becomes
            // the value it meant instead of a refused write.
            Ok(number.to_string())
        }
        // A model name, a path, free text, and a key no catalogue entry names.
        // All four are strings in the file, and all four are escaped rather than
        // wrapped in quotes by hand.
        Some(Kind::Model | Kind::File | Kind::Text) | None => Ok(crate::servers::quoted(word)),
        // Both are answered above, before a single word was demanded, so this arm
        // is unreachable — and it is written as a refusal rather than left to a
        // wildcard or a `panic!`. A wildcard would swallow a `Kind` variant a
        // later release adds and write it as a string; a panic would end the
        // process on something an operator typed. `Kind` is io-cli's own enum, so
        // the exhaustive match is a compile-time gate this crate can keep.
        Some(Kind::List | Kind::Machine) => Err(format!(
            "`{key}` was read as two different kinds in one parse, which is a defect in io \
             rather than in what you typed; nothing was written"
        )),
    }
}

/// What to say a key admits, for the refusal that has no value to name.
fn offered(key: &str, kind: Option<&Kind>) -> String {
    match kind {
        Some(Kind::Flag) => "it is on or off, written `true` or `false`".to_string(),
        Some(Kind::Choice(options)) => format!(
            "the options are {}",
            options
                .iter()
                .map(|option| format!("`{option}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Some(Kind::Number { .. }) => "it takes a whole number".to_string(),
        _ => format!("`config get {key}` shows what it is set to now"),
    }
}
