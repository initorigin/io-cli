//! The MCP servers a session configured, and what the session has seen of them.
//!
//! **Configured and reached are different sets, and the second is the one an
//! operator is asking about.** A server that is in the file and never answered
//! looks, on the status line, exactly like a server that is not in the file at
//! all: the aggregate `mcp` field counts what the run reached and says nothing
//! about what it was supposed to reach. This module is the difference, drawn.
//!
//! # Three states, and "not yet reached" is one of them
//!
//! The configured half comes from [`Config::mcp_servers`]. The operational half
//! comes only from `EventKind::Mcp`, which is emitted while a run is in flight —
//! so **a session that has not run a turn yet has reached nothing**, and every
//! server is in the third state. That is the state every server is in at session
//! start, and drawing it as a failure would tell an operator their configuration
//! is broken at the exact moment it is most likely to be fine.
//!
//! # Two counts, and they are not the same question
//!
//! **Offered** is how many tools a server announced. It arrives on `EventKind::Mcp`
//! as `tools: Option<u32>`, added by io-harness 0.68.0 and set **only** on the
//! event announcing a server reaching the run — over the server's full listed
//! catalogue. Every other form of the event, each `discovered` and every call,
//! carries `None`. Until 0.68.0 the fact was not on the wire at all and this
//! module said so; that sentence is now false, and the field it said did not
//! exist is what closes `US-IO-CLI-0.16.0-I01` / `US-IO-HARNESS-0.68.0-I01`.
//!
//! **`Some(0)` and `None` are different facts and this module must not collapse
//! them.** `Some(0)` is a server that stated it offers nothing; `None` is an
//! event with nothing to say about the count. Reading a missing count as zero
//! would make a server that has only ever answered CALLS — every one of whose
//! events carries `None` — report offering no tools while visibly using them.
//!
//! **Asked for** is the number of DISTINCT TOOLS this session has called, which
//! this module still counts itself because no event states it. It is a lower
//! bound on what is offered, and the two numbers are drawn as two numbers: one
//! replacing the other would answer a question nobody asked.
//!
//! # Disable, and why the write that was refused in 0.29.0 is correct in 0.30.0
//!
//! Through io-harness 0.69.0 this paragraph said `McpServer` was `id`, `transport`
//! and `timeout_secs`, that there was no key for it, and that an `enabled = false`
//! invented here would be accepted by the file and ignored by the harness so the
//! server would start anyway. **io-harness 0.70.0 falsified every clause of that**:
//! `enabled` is a real public field (`mcp.rs:261`) defaulting to true, and it is
//! honoured at the earliest possible point — a disabled server is never spawned,
//! never dialled and never even checked against the policy (`mcp.rs:356`), and
//! [`io_harness::probe_mcp`] answers `McpProbe::Disabled` (`mcp.rs:892`).
//!
//! So 0.29.0 read and drew the *state*, on [`Server::enabled`], for the reason
//! [`crate::pluginview`] was given the same treatment in the same release: a
//! capability missing from every listing reads exactly like one nobody ever
//! declared, and an operator whose turns have quietly lost their tools has nowhere
//! to look. It shipped **no writer**, and `tests/servers.rs` asserted the refusal —
//! not because the key was unwritable but because half a verb is worse than none:
//! a surface that can switch a server off has to be able to switch it back on, and
//! neither door could.
//!
//! **0.30.0 adds both halves, so the refusal has nothing left to protect.**
//! `enabled` is in [`KEYS`], [`switch`] is the edit both doors build, and what the
//! `KEYS` check was actually guarding is untouched: it still refuses every key
//! [`McpServer`] does not carry. The list gained **one name**, and that is the only
//! safe way to widen it — a check relaxed to accept anything would hand the
//! exemption below a misspelling to write, and there is no later failure to catch
//! one.
//!
//! **What writing it costs an older binary is not what the plugin key costs, and
//! the two sentences are deliberately different.** See [`OLDER_BINARY`] beside
//! [`crate::pluginview::OLDER_BINARY`]: a 0.69.0 binary *refuses the whole file*
//! over `enabled` in a `[[plugin]]`, and *silently ignores it* in an `[[mcp]]` —
//! running a server the operator switched off. One is loud and total, the other is
//! quiet and exactly the failure mode this module exists to close.
//!
//! # One verb this panel still does not offer
//!
//! **Reconnect**, because there is nothing to cycle: servers are attached per
//! turn through `TaskContract::with_mcp`. What an operator means by it is "pick
//! up the edit I just made", and that is what the next turn does.
//!
//! # A probe is not the observation beside it
//!
//! [`probe`] goes and looks: it spawns or dials the server, completes an MCP
//! handshake and shuts it down again. [`Observed`] does the opposite — it folds
//! events a *run* emitted and asserts nothing that did not happen on its own. So a
//! probe writes nothing into [`Observed`] and takes no `&mut` to one, and
//! [`Reached::NotYet`] and `McpProbe::Unreachable` are rendered in words that
//! cannot be mistaken for each other. "Nobody has called it yet" and "io-cli called
//! it and got nothing" are different facts, and only the second is a fault.
//!
//! # The write half, and the one number it must not be handed
//!
//! [`add`], [`edit`] and [`remove`] are the three verbs, and two of them address
//! an entry by **position in a file's `[[mcp]]` array**. That number is not a row
//! number, and this crate has already paid once for the difference: 0.20.0's
//! `pluginview::rows` drew two lists in an order no file shares, and handing one
//! of its row numbers to a remover would have deleted a bundle nobody named. So
//! neither verb takes a `usize` here — they take an [`At`], which only
//! [`At::of`] builds, by finding the id in the file's own bytes.
//!
//! **`[[mcp]]` is one of only two sections io-harness exempts from
//! `deny_unknown_fields`** (`config.rs:86`), which is what makes a wrong write
//! here quieter than anywhere else in the file: a misspelled key is accepted by
//! the parser and ignored by the harness, so the server starts with the setting
//! the operator thought they changed still at its default. That is why [`edit`]
//! refuses a key that is not one of [`KEYS`] instead of writing it, and why the
//! round-trip assertions in `tests/servers.rs` deserialise into [`McpServer`]
//! rather than looking for a string in the file.
//!
//! **A whole server is one edit and therefore one write.** [`crate::configure::write`]
//! splices, re-discovers and rolls back on refusal; a server that took two calls
//! would be two of those round trips with a half-written entry on disk between
//! them, and the second call is the one that can fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use io_harness::config::{Config, Scope};
use io_harness::{EventKind, McpProbe, McpServer, McpTransport, Policy};

use crate::configure::Decided;

/// What this session has seen of one configured server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reached {
    /// It answered.
    ///
    /// `tools` is how many **distinct tools it has been asked for**. `offered`
    /// is how many it **announced**, when an event said so — `Some(0)` is a
    /// server that offers nothing and `None` is a server whose count this
    /// session never saw, which is the state of one that has answered calls
    /// without the announcing event ever being folded in. See the module docs.
    Answered { tools: usize, offered: Option<u32> },
    /// A call to it failed, and this is the last one that did.
    Failed { tool: String },
    /// Nothing has been heard from it this session.
    ///
    /// **Not a failure.** It is the state every server is in before the first
    /// turn runs, and the state a server stays in for a whole session that never
    /// needed it.
    ///
    /// **Not `McpProbe::Unreachable` either**, and [`probed`] words the two so
    /// that neither can be read as the other: this is nobody having called the
    /// server, and that is io-cli having called it and got nothing back. A probe
    /// never writes this state and this state never renders as a probe's.
    NotYet,
}

impl Reached {
    /// The word this state draws as.
    pub fn word(&self) -> &'static str {
        match self {
            Reached::Answered { .. } => "answered",
            Reached::Failed { .. } => "failed",
            Reached::NotYet => "not reached",
        }
    }
}

/// What a server switched off in the file is called, on every surface.
///
/// One word, spelled once, so the panel and the two headless verbs cannot drift
/// into describing one state three ways. It is deliberately the same word
/// `io plugin list` prints for the same instruction in a `[[plugin]]` entry.
pub const DISABLED: &str = "disabled";

/// One row of the panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// The id it is configured under, which is also the name events carry.
    pub id: String,
    /// How it is reached — a command, or a URL.
    pub transport: String,
    /// Which file configured it.
    pub decided: Decided,
    /// What the session has seen of it.
    pub state: Reached,
    /// Whether the file lets it start at all.
    ///
    /// io-harness 0.70.0's `McpServer::enabled`, defaulting to true, so an entry
    /// written before the key existed means what it always did. `false` is not a
    /// failure and not a state the session reached — it is the file's own
    /// instruction, honoured before anything is spawned, dialled or even checked
    /// against the policy — which is why it is a field of its own beside
    /// [`Server::state`] rather than a variant of [`Reached`]. A server that is
    /// off has *no* state to have reached.
    pub enabled: bool,
}

/// Per-server facts accumulated from the run's own events.
///
/// Separate from [`crate::status::Status`]'s aggregate `mcp` pair on purpose: that
/// field answers "is anything connected" for a one-row status line, and this
/// answers "what happened to each of them" for a panel. One is not derivable from
/// the other — the pair has no server names in it at all.
/// What this session has seen one server do.
///
/// **A struct rather than the tuple this was, and clippy is right about when.**
/// Two facts read fine positionally; the third — the offered count 0.68.0 put on
/// the event — is where `entry.2` stops saying what it holds at the call site. The
/// names are the documentation now that there is more than one kind of number
/// here, and `asked` and `offered` are exactly the pair a reader must not confuse.
#[derive(Debug, Clone, Default)]
struct Seen {
    /// Distinct tool names this session has called on the server. A lower bound
    /// on what it offers, and counted here because no event states it.
    asked: BTreeSet<String>,
    /// The tool of the last call that failed, if one did.
    failed: Option<String>,
    /// How many tools the server announced, if an event ever said. `None` is not
    /// zero — see [`Observed::event`].
    offered: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct Observed {
    /// What each server has been seen to do, by the id it was configured under.
    seen: BTreeMap<String, Seen>,
}

impl Observed {
    /// Fold one event in.
    ///
    /// `EventKind::Mcp` means two things and the difference is whether `tool` is
    /// present: with none it is the server itself reaching the run, and with one
    /// it is a call. Both mark the server as reached; only the second can name a
    /// tool or a failure, and only the first states an offered count.
    pub fn event(&mut self, kind: &EventKind) {
        let EventKind::Mcp {
            server,
            tool,
            ok,
            tools,
            ..
        } = kind
        else {
            return;
        };

        let entry = self.seen.entry(server.clone()).or_default();
        // Only ever ASSIGNED from a `Some`. A `None` is an event with nothing to
        // say about the count, not a statement that there are none — so it must
        // neither write a zero nor erase a count an earlier event stated.
        if let Some(offered) = tools {
            entry.offered = Some(*offered);
        }
        if let Some(tool) = tool {
            entry.asked.insert(tool.clone());
            // `ok` is an `Option<bool>`: `Some(false)` is a failure, and `None`
            // is a call whose outcome the event did not carry, which is not the
            // same thing and must not be drawn as one.
            if *ok == Some(false) {
                entry.failed = Some(tool.clone());
            }
        }
    }

    /// Forget everything, for a `/clear`, a `/resume` or a rewind.
    ///
    /// The hole `Status::forget_run` was written to close, at the one other place
    /// that now accumulates per-run state — 0.8.0 shipped `Fleet::forget` with no
    /// caller for exactly this reason.
    pub fn forget(&mut self) {
        self.seen.clear();
    }

    /// What this session has seen of `id`.
    pub fn of(&self, id: &str) -> Reached {
        match self.seen.get(id) {
            None => Reached::NotYet,
            Some(seen) => match &seen.failed {
                Some(tool) => Reached::Failed { tool: tool.clone() },
                None => Reached::Answered {
                    tools: seen.asked.len(),
                    offered: seen.offered,
                },
            },
        }
    }
}

/// Every configured server, with what the session has seen of it.
///
/// The configured set is the whole list and the observed set only decorates it:
/// a server that answered but is no longer in the file is not a row, because the
/// panel is about the configuration an operator can act on.
pub fn servers(config: &Config, observed: &Observed) -> Vec<Server> {
    config
        .mcp_servers()
        .iter()
        .map(|server| {
            let origins = config.origin("mcp");
            Server {
                id: server.id.clone(),
                transport: transport(server),
                decided: match origins.last() {
                    Some(origin) => Decided::File {
                        scope: origin.scope,
                        path: origin.path.clone(),
                    },
                    None => Decided::Default,
                },
                state: observed.of(&server.id),
                enabled: server.enabled,
            }
        })
        .collect()
}

/// How a server is reached, in one short string.
fn transport(server: &io_harness::McpServer) -> String {
    // Exhaustive, with no wildcard: `McpTransport` is NOT `#[non_exhaustive]`,
    // so a variant added by a later io-harness breaks this build rather than
    // rendering as "unknown" in an operator's panel. That is the compile-time
    // gate 0.65.0 taught this crate to keep where the dependency still allows
    // one — see the `RunOutcome` note in `tests/exec.rs`.
    match &server.transport {
        io_harness::McpTransport::Stdio { command, .. } => command.clone(),
        io_harness::McpTransport::Http { url, .. } => url.clone(),
    }
}

/// The rows as the picker draws them.
///
/// Content before metadata: the id is the label, and the detail carries what the
/// session saw then how it is reached.
pub fn rows(servers: &[Server]) -> Vec<crate::picker::Row> {
    servers
        .iter()
        .map(|server| {
            let state = match &server.state {
                // Two numbers, drawn as two numbers. The offered count does not
                // replace the asked-for one: "10 offered · 2 used" is the answer
                // to a question either number alone gets wrong.
                Reached::Answered {
                    tools,
                    offered: Some(offered),
                } => format!("answered · {offered} offered · {tools} used"),
                // No count stated. The panel says what it knows and stays silent
                // about the rest rather than drawing a zero it did not hear.
                Reached::Answered {
                    tools,
                    offered: None,
                } => format!("answered · {tools} tools used"),
                Reached::Failed { tool } => format!("failed · {tool}"),
                Reached::NotYet => "not reached this session".to_string(),
            };
            // **A switched-off server has no state to have reached**, so the
            // instruction replaces the state rather than sitting beside it.
            // "not reached this session" over a server the file switched off is
            // true and reads as a server that simply has not been called yet,
            // which is the one wrong answer available here.
            let state = if server.enabled {
                state
            } else {
                DISABLED.to_string()
            };
            crate::picker::Row::with_detail(
                server.id.clone(),
                format!("{state}   {}", server.transport),
            )
        })
        .collect()
}

/// One entry's position in one file's `[[mcp]]` array.
///
/// **The type exists so that a row number cannot be spelled as a position.** The
/// two are the same integer often enough to look interchangeable — `[[mcp]]` is
/// *not* one of io-harness's appending keys (`config.rs:2052`), so the winning
/// scope replaces the array whole and [`servers`] does list it in file order —
/// and that is exactly the shape of a bug that survives every test written on a
/// one-scope fixture. `pluginview` learned it the expensive way in 0.20.0. Here
/// the id is looked up in the file's own bytes instead, so the two lists never
/// have to agree.
///
/// It carries the scope as well as the index, because a caller needs both and
/// they come from the same lookup: the index means nothing without the file it
/// counts in, and [`crate::configure::write`] takes the scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct At {
    /// The scope whose file carries the entry — the scope a write must go to.
    pub scope: Scope,
    /// Private so nothing outside this module can spell one. [`At::of`] is the
    /// only way to obtain one, and it reads a file to answer.
    index: usize,
}

impl At {
    /// Where `text` declares the `[[mcp]]` entry whose `id` is `id`.
    ///
    /// `text` must be the bytes of the file `scope` names — this counts entries
    /// in what it is given, and given the wrong file it would answer about the
    /// wrong array.
    ///
    /// The comparison is made twice, against the source spelling and against the
    /// unquoted text, because `id = "we\"ird"` is one id written two ways and
    /// this module may not parse TOML to settle it (`tests/dependencies.rs`).
    ///
    /// `None` when no entry carries that id, and a caller must say so rather
    /// than write to a position it guessed.
    pub fn of(scope: Scope, text: &str, id: &str) -> Option<At> {
        let wanted = quoted(id);
        // The array is walked until a gap, which is what `value_at` reports by
        // returning `None` — an array of tables is contiguous, and `id` is a
        // required field of `McpServer`, so the first miss is the end of it.
        for index in 0.. {
            let Some(raw) = crate::edit::value_at(text, &format!("mcp[{index}].id")) else {
                break;
            };
            let raw = raw.trim();
            if raw == wanted || unquoted(raw) == id {
                return Some(At { scope, index });
            }
        }
        None
    }

    /// The position, for a caller that has a sentence to write about it.
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Where the file that configured `server` declares it.
///
/// The bridge between a row on screen and an entry in a file, and it takes the
/// row rather than a number so there is nothing for a caller to get wrong. The
/// file is the one [`Server::decided`] already named — the same origin the panel
/// drew — so this cannot disagree with the column an operator is looking at.
///
/// `None` for a server io-harness's own default supplied (there is no file to
/// edit) and for one whose deciding file no longer names it, which is what an
/// operator editing the file under the session looks like.
pub fn declared_at(server: &Server) -> Option<At> {
    let Decided::File { scope, path } = &server.decided else {
        return None;
    };
    let text = std::fs::read_to_string(path).ok()?;
    At::of(*scope, &text, &server.id)
}

/// Where a scope's file declares the server called `id`, read from disk.
///
/// The form for a caller holding a root rather than a drawn row — an import, or
/// a command naming a server the panel is not showing. Searched in precedence
/// order and **stopped at the first scope that declares any `[[mcp]]` at all**,
/// which is the part that is not obvious: the array is replaced whole rather
/// than appended across scopes, so a lower file's entry is not merely
/// lower-priority, it is *not in force*. Editing it would change a file and
/// nothing else.
pub fn declared_in(root: &Path, id: &str) -> Option<At> {
    for scope in [Scope::Local, Scope::Project, Scope::User] {
        let Some(path) = crate::configure::scope_path(root, scope) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if crate::edit::value_at(&text, "mcp[0].id").is_none() {
            continue;
        }
        // This scope decides the whole array, so the answer is here or nowhere.
        return At::of(scope, &text, id);
    }
    None
}

/// The keys an `[[mcp]]` entry may carry, which are [`McpServer`]'s own.
///
/// A list rather than a free string because of the exemption in the module docs:
/// `[[mcp]]` is not held to `deny_unknown_fields`, so `comand = "…"` is written,
/// accepted, and ignored. The one write in this crate whose failure says nothing
/// at all is the one worth spending a `const` on.
///
/// **0.30.0 added one name to it and did not relax the check.** `enabled` is
/// io-harness 0.70.0's own field and 0.29.0 refused it only because the release
/// shipped no way to switch a server back on; both halves exist now, so the name
/// is listed. Widening [`edit`] to accept *any* key instead would be the same
/// change to look at and the opposite change to live with: the exemption above
/// means a misspelling would then be written, parsed, reported as landed, and
/// ignored by every turn after it.
pub const KEYS: &[&str] = &[
    "id",
    "transport",
    // Stdio.
    "command",
    "args",
    "env",
    // Http.
    "url",
    "headers",
    "timeout_secs",
    // Whether io-harness starts it at all. io-harness 0.70.0's, honoured before
    // the process or the socket exists.
    "enabled",
];

/// What writing an `enabled` key into an `[[mcp]]` entry costs an older binary.
///
/// **The mirror image of [`crate::pluginview::OLDER_BINARY`], and the dangerous
/// half of the pair.** `enabled` on a `[[plugin]]` makes an io-cli built against
/// io-harness 0.69.0 refuse the whole configuration file: loud, total, and
/// impossible to miss. `[[mcp]]` is one of the two sections that binary exempts
/// from `deny_unknown_fields`, so the same key there is **read, accepted and
/// ignored** — and the server the operator switched off starts and runs, on that
/// machine, with nothing on any surface saying so.
///
/// So the two sentences are two constants. One string used for both would be
/// telling an operator on the quiet path that they will notice.
pub const OLDER_BINARY: &str =
    "this writes an `enabled` key into an `[[mcp]]` entry, which is io-harness 0.70.0's: an io-cli \
     built against 0.69.0 does not refuse it — it ignores the key and starts the server anyway, \
     with nothing said";

/// The edit that switches the server at `at` on or off.
///
/// **One key and no other byte of the file**, for the reason
/// [`crate::pluginview::enable`] gives about the sibling section:
/// [`crate::edit::Edit::set`] replaces the value's own span, so the transport, the
/// arguments, the environment, every sibling entry and every comment come through
/// untouched. An operator who diffs their `io.toml` afterwards sees one word
/// change. **The entry is never removed** — a server switched off is one the file
/// still declares, which is the whole difference between this verb and
/// [`remove`].
///
/// The one write both doors build, so `io mcp disable <id>` and the `/mcp`
/// keystroke cannot produce different bytes. Says [`OLDER_BINARY`] to whoever is
/// about to be shown a sentence about it.
pub fn switch(at: &At, on: bool) -> crate::edit::Edit {
    // `true`/`false` bare rather than through `quoted`: `enabled` is a TOML
    // boolean, and `enabled = "false"` is a string io-harness refuses on
    // `configure::write`'s round trip.
    crate::edit::Edit::set(format!("mcp[{}].enabled", at.index), on.to_string())
}

/// The edit that adds a whole server.
///
/// A whole `[[mcp]]` entry, because an array of tables grows by gaining a block
/// and [`crate::edit::Edit::set`] can only reach inside one that exists.
///
/// **Every field in the one edit, and that is the point.** A server whose `args`
/// and `env` arrived in a second [`crate::configure::write`] would be a second
/// discover-and-roll-back round trip over a file that, in between, held a server
/// declared without the arguments that make it work — and if the second call is
/// the one io-harness refuses, that is the state it stays in.
///
/// It takes io-harness's own [`McpServer`] rather than a widening list of
/// parameters: that type is the authority on what an entry may carry, so a field
/// added to it is a compile error here rather than a key io-cli quietly stops
/// writing. The transport match below is exhaustive with no wildcard for the
/// reason the panel's own reader is: `McpTransport` is not `#[non_exhaustive]`,
/// so a variant a later io-harness adds breaks this build rather than being
/// written as a server that cannot be reached.
///
/// `args`, `env` and `headers` are omitted when empty and `timeout_secs` when it
/// is the harness's own default — all four are `#[serde(default)]`, so what is
/// read back is the same server, and a file gains no line stating a default.
pub fn add(server: &McpServer) -> crate::edit::Edit {
    // `transport` is required: `McpTransport` is `#[serde(tag = "transport")]`,
    // so the discriminant sits flat beside `id` rather than nesting, and an
    // entry without it fails to load with `missing field \`transport\``.
    let mut body = format!("id = {}\n", quoted(&server.id));
    match &server.transport {
        McpTransport::Stdio { command, args, env } => {
            body.push_str("transport = \"stdio\"\n");
            body.push_str(&format!("command = {}\n", quoted(command)));
            if !args.is_empty() {
                let items: Vec<&str> = args.iter().map(String::as_str).collect();
                body.push_str(&format!("args = {}\n", crate::edit::array(&items)));
            }
            if !env.is_empty() {
                body.push_str(&format!("env = {}\n", inline_table(env)));
            }
        }
        McpTransport::Http { url, headers } => {
            body.push_str("transport = \"http\"\n");
            body.push_str(&format!("url = {}\n", quoted(url)));
            if !headers.is_empty() {
                body.push_str(&format!("headers = {}\n", inline_table(headers)));
            }
        }
    }
    // Asked of the harness rather than written as `60` here: the default is
    // io-harness's, `default_timeout_secs` is private, and a literal copied into
    // io-cli is a number that goes stale in silence.
    if server.timeout_secs != McpServer::stdio("", "").timeout_secs {
        body.push_str(&format!("timeout_secs = {}\n", server.timeout_secs));
    }
    crate::edit::Edit::append("mcp", body)
}

/// The edit that changes one key of the entry at `at`.
///
/// `value` is TOML **source**, the way [`crate::edit::Edit::set`] takes it —
/// `"\"mcp-find\""`, `"[\"--verbose\"]"`, `"30"`. Build the first two with
/// [`quoted`] and [`crate::edit::array`] rather than a format string.
///
/// `None` for a key that is not one of [`KEYS`]. **That refusal is the whole
/// reason this returns an `Option`**: an unknown key inside `[[mcp]]` is not
/// rejected by io-harness, so `configure::write`'s round trip would accept it
/// and an operator would be told their change landed while the server ran on the
/// old value.
pub fn edit(at: &At, key: &str, value: &str) -> Option<crate::edit::Edit> {
    let path = format!("mcp[{}].{key}", at.index);
    KEYS.contains(&key)
        .then(|| crate::edit::Edit::set(path, value.to_string()))
}

/// The edit that removes the entry at `at`, whole.
pub fn remove(at: &At) -> crate::edit::Edit {
    crate::edit::Edit::remove(format!("mcp[{}]", at.index))
}

/// Try one configured server for real, and report what came back.
///
/// **[`crate::preflight`] asks the policy; this one goes and looks.** The preflight
/// is a pure question put to [`Policy`] and answers before anything exists, which
/// is right at the moment a server is *added*. It cannot tell an operator whether
/// the command is on this machine's path or whether the far end speaks MCP, and
/// those are the two things they are actually asking when a server they configured
/// contributes nothing. [`io_harness::probe_mcp`] answers all of it: it makes the
/// same policy check the run makes, in the same order, **before** spawning or
/// dialling; it really does spawn or dial and complete the handshake; it shuts the
/// server down on every path that started one; and it is bounded by the server's
/// own `timeout_secs`.
///
/// It returns no `Result` of its own — every failure is a variant of
/// [`McpProbe`] — so the only error here is the one io-harness cannot have an
/// opinion about: an id no configuration file in force declares.
///
/// **Nothing is folded into [`Observed`].** A probe is io-cli reaching out on its
/// own, and `Observed` is what a *run* was seen to do; a probe that wrote into it
/// would put a fact no turn produced under a heading that says a turn did. That is
/// enforced by this signature taking no `&mut Observed` rather than by a rule
/// somebody has to remember.
pub async fn probe(config: &Config, id: &str, policy: &Policy) -> Result<McpProbe, String> {
    let server = config
        .mcp_servers()
        .iter()
        .find(|server| server.id == id)
        .ok_or_else(|| {
            format!(
                "no configuration file in force declares an MCP server called `{id}`, so there is \
                 nothing to probe; `mcp list` shows the ones that are configured"
            )
        })?;
    Ok(io_harness::probe_mcp(server, policy).await)
}

/// What [`probed`] says about a state this build of io-cli has no arm for.
///
/// **[`McpProbe`] is `#[non_exhaustive]` and that is a promise, not a formality**:
/// io-harness's own rustdoc names the case it expects to add — a server that
/// answers but speaks a protocol version the crate will not talk. A `_` arm that
/// re-used one of the five known sentences would report that as something it is
/// not, and the operator would go and fix the wrong thing. So the unknown state is
/// spelled as unknown, and it names the repair that actually applies.
///
/// A `const` so a test can assert that no known variant renders it, and that it
/// renders no known variant's words.
pub const UNMODELLED: &str =
    "a newer io-harness reported an outcome this build of io-cli does not model, so there is \
     nothing here that can be said about it — upgrade io-cli rather than reading it as one of \
     the states this build knows";

/// The sentence a probe comes to, for the operator who asked for it.
///
/// Six outcomes and six sentences, because the whole value of [`McpProbe`] is that
/// they are different answers with different repairs: a refusal needs a policy
/// rule, a failed spawn needs the command fixed, a silent host needs somebody to
/// look at the host, and a switched-off server needs nothing at all. "It did not
/// work" is the report that costs a turn to act on.
///
/// **`Unreachable` is worded so it cannot be read as [`Reached::NotYet`].** That
/// state means no turn has called the server yet and is explicitly *not* a failure;
/// this one means io-cli went and looked and got nothing. See the module docs.
pub fn probed(id: &str, probe: &McpProbe) -> String {
    match probe {
        // Nothing was spawned, nothing was dialled, and the policy was not even
        // consulted — the same thing a run does with it, which is why this is not
        // reported as a failure to start.
        McpProbe::Disabled => format!(
            "`{id}` is {DISABLED} in the configuration, so nothing was started and nothing was \
             asked of the policy",
        ),
        // All four fields, because they are two different repairs: `act` and
        // `target` say what to allow, `rule` and `layer` say what denied it.
        McpProbe::Refused {
            act,
            target,
            rule,
            layer,
        } => format!(
            "`{id}` was refused before it was started: the policy refuses {act} on `{target}`, by \
             {}",
            decided_by(rule.as_deref(), layer.as_deref()),
        ),
        // The policy allowed it and the program is not there. The fix is the
        // command, and the server was never reached at all.
        McpProbe::NotStarted { reason } => format!(
            "`{id}` did not start: {reason}. The policy allowed it, so what is wrong is the \
             command",
        ),
        // It started, or the URL was allowed, and the far end gave nothing back.
        McpProbe::Unreachable { reason } => format!(
            "io-cli called `{id}` and it did not answer: {reason}. The command and the policy are \
             both fine; the far end is not",
        ),
        McpProbe::TimedOut { secs } => format!(
            "io-cli called `{id}` and it stayed silent for {secs}s, which is its own \
             `timeout_secs`",
        ),
        // The namespaced names, because those are what the model would see and
        // what the policy would decide on — the bare ones the server sent appear
        // nowhere a rule can name them.
        McpProbe::Answered { tools } if tools.is_empty() => {
            format!("`{id}` answered and offers no tools at all")
        }
        McpProbe::Answered { tools } => format!(
            "`{id}` answered and offers {} tools: {}",
            tools.len(),
            tools.join(", "),
        ),
        // **Required, and it must not name a state this build knows.** See
        // [`UNMODELLED`].
        _ => format!("`{id}`: {UNMODELLED}"),
    }
}

/// Who decided a refusal: the rule and its layer, or a default with no rule.
///
/// The same shape [`crate::preflight`] prints for the question it asks earlier, and
/// spelled again here rather than shared because that one is private to a module
/// whose input is a `Preflight` and this one's is io-harness's own two `Option`s.
fn decided_by(rule: Option<&str>, layer: Option<&str>) -> String {
    match (rule, layer) {
        (Some(rule), Some(layer)) => format!("the rule `{rule}` in the `{layer}` layer"),
        (Some(rule), None) => format!("the rule `{rule}`"),
        (None, _) => "the policy's own default for that act (no rule matched)".to_string(),
    }
}

/// A TOML basic string, escaped.
///
/// Here rather than through `toml::to_string`, which would need a document to
/// serialise: this crate writes VALUES, and a value is the one thing the
/// serialiser cannot be asked for on its own.
///
/// **Public, because [`edit`] takes TOML source and a caller has a Rust string.**
/// The alternative every call site reaches for is `format!("\"{value}\"")`, which
/// is either a parse error or a different value the moment the text carries a
/// quote or a backslash — a Windows command path is full of the second.
///
/// **Every escape TOML defines and not just the two that are obvious.** A quote
/// and a backslash are the pair a reader thinks of; a newline inside a basic
/// string is a parse error, and an MCP `env` value is whatever an imported
/// server definition put there. Getting it wrong is a refusal rather than a
/// corruption — [`crate::edit::apply`] reads its own result back — but a
/// refusal an operator cannot act on is still a verb that does not work.
pub fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            // The rest of C0, and DEL, which TOML forbids raw in a basic string.
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A TOML basic string with its quotes taken off, for comparing an id back.
fn unquoted(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| c == '"' || c == '\'')
        .to_string()
}

/// A map spelled as a TOML inline table.
///
/// **Inline rather than a `[mcp.env]` sub-header**, because [`crate::edit`] cuts
/// a document at every line-leading `[`: a header inside an appended entry would
/// end that entry's region early, so `mcp[i].timeout_secs` written after it
/// would be looked for in a region that no longer contains it. An inline table
/// is one value, and one value is what this module writes.
///
/// ponytail: only strings, because `env` and `headers` are the only two maps in
/// `[[mcp]]` and both are `BTreeMap<String, String>`. A map of anything else
/// would need the value spelled by kind, and there is none to spell.
fn inline_table(pairs: &BTreeMap<String, String>) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(key, value)| format!("{} = {}", quoted(key), quoted(value)))
        .collect();
    format!("{{ {} }}", body.join(", "))
}
