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
//! # Two verbs this panel does not offer
//!
//! **Disable**, because `McpServer` is `id`, `transport` and `timeout_secs` and
//! there is no key for it — and because the type is `#[serde(flatten)]`-based, an
//! `enabled = false` invented here would be *accepted* by the file and *ignored*
//! by the harness, so the server would start anyway. A panel that said "disabled"
//! over a running server would be worse than one that does not offer the verb.
//!
//! **Reconnect**, because there is nothing to cycle: servers are attached per
//! turn through `TaskContract::with_mcp`. What an operator means by it is "pick
//! up the edit I just made", and that is what the next turn does.

use std::collections::{BTreeMap, BTreeSet};

use io_harness::config::Config;
use io_harness::EventKind;

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
}

/// Per-server facts accumulated from the run's own events.
///
/// Separate from [`crate::status::Status`]'s aggregate `mcp` pair on purpose: that
/// field answers "is anything connected" for a one-row status line, and this
/// answers "what happened to each of them" for a panel. One is not derivable from
/// the other — the pair has no server names in it at all.
#[derive(Debug, Clone, Default)]
pub struct Observed {
    /// Distinct tool names seen per server, the last failure if there was one,
    /// and the offered count if an event ever stated it.
    seen: BTreeMap<String, (BTreeSet<String>, Option<String>, Option<u32>)>,
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
            entry.2 = Some(*offered);
        }
        if let Some(tool) = tool {
            entry.0.insert(tool.clone());
            // `ok` is an `Option<bool>`: `Some(false)` is a failure, and `None`
            // is a call whose outcome the event did not carry, which is not the
            // same thing and must not be drawn as one.
            if *ok == Some(false) {
                entry.1 = Some(tool.clone());
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
            Some((tools, failure, offered)) => match failure {
                Some(tool) => Reached::Failed { tool: tool.clone() },
                None => Reached::Answered {
                    tools: tools.len(),
                    offered: *offered,
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
            crate::picker::Row::with_detail(
                server.id.clone(),
                format!("{state}   {}", server.transport),
            )
        })
        .collect()
}

/// The edit that adds a server.
///
/// A whole `[[mcp]]` entry, because an array of tables grows by gaining a block
/// and [`crate::edit::Edit::set`] can only reach inside one that exists.
pub fn add(id: &str, command: &str) -> crate::edit::Edit {
    crate::edit::Edit::append(
        "mcp",
        // `transport` is required: `McpTransport` is `#[serde(tag = "transport")]`,
        // so the discriminant sits flat beside `id` rather than nesting, and an
        // entry without it fails to load with `missing field \`transport\``.
        format!(
            "id = {}\ntransport = \"stdio\"\ncommand = {}",
            quoted(id),
            quoted(command)
        ),
    )
}

/// The edit that changes one key of the `index`-th server.
pub fn edit(index: usize, key: &str, value: &str) -> crate::edit::Edit {
    crate::edit::Edit::set(format!("mcp[{index}].{key}"), value.to_string())
}

/// The edit that removes the `index`-th server whole.
pub fn remove(index: usize) -> crate::edit::Edit {
    crate::edit::Edit::remove(format!("mcp[{index}]"))
}

/// A TOML basic string, escaped.
///
/// Here rather than through `toml::to_string`, which would need a document to
/// serialise: this crate writes VALUES, and a value is the one thing the
/// serialiser cannot be asked for on its own.
fn quoted(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}
