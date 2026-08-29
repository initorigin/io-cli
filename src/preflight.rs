//! Whether the policy in force will let a configured MCP server start — answered
//! when the operator adds it, not when a run finally needs it.
//!
//! **An MCP server is the one piece of configuration whose failure mode is
//! silence.** `claude mcp add` and `codex mcp add` write a line into a file and
//! hand it back; the entry looks exactly as valid as one that works, and the
//! operator learns otherwise on the next turn, from a run that ends before its
//! first step. The refusal is not a bug in either tool — it is the policy doing
//! its job — but the moment it is reported is a choice, and reporting it at add
//! time costs nothing and reporting it at run time costs a turn.
//!
//! # What actually gates a server
//!
//! Two acts, one per transport, both decided before the server process or socket
//! exists:
//!
//! * **stdio** — `McpSession::connect` calls `authorize_spawn` (io-harness 0.69.0,
//!   `src/mcp.rs:320`, defined at `:563-590`) *before* `TokioChildProcess::new` at
//!   `:326`. That is a plain [`Act::Exec`] check on the command string as written.
//! * **http** — `NetGuard::check` over the URL (`src/mcp.rs:336-339`), which
//!   normalises the URL to `host:port` and asks [`Act::Net`] about it.
//!
//! **A refusal from either is `Error::Refused { act, target, rule, layer }`, not
//! `Error::Mcp`.** The rustdoc on `McpServer` at `src/mcp.rs:170-173` says the run
//! "ends in `Error::Mcp` before the server process exists"; it does not — `Error::Mcp`
//! is what a *spawn failure* or a failed handshake produces, once the policy has
//! already allowed the spawn. This module reports the four fields of the refusal
//! io-harness would actually raise, because those are the four an operator can act
//! on: `rule` names the glob and `layer` names the file it came from.
//!
//! # Why this crate computes the answer instead of asking for it
//!
//! `authorize_spawn`, `NetGuard` and `net::target` are all `pub(crate)`. There is
//! no embedder-reachable call that answers "would this server start", so the only
//! ways to know are to run a turn and find out, or to ask the same question of the
//! same public [`Policy`] the harness will ask it of. This module does the second:
//! every verdict here comes from `Policy::check`, which *is* `Policy::explain`
//! (`src/policy.rs:594-596`) and is therefore the same function that enforces.
//! Nothing about permission is decided in this file. What is reproduced here is one
//! string transform — see [`target`] — and that copy is the module's whole risk
//! surface.
//!
//! # It is a disclosure, not a veto
//!
//! Nothing here refuses anything or returns an error. The operator asked for an
//! entry; the entry gets written and the command exits zero whatever this says.
//! A configuration tool that declined to record what its user typed because a
//! policy it does not own would currently refuse it would be wrong twice over: the
//! policy is editable, the server may be for a session that has not been configured
//! yet, and the refusal it is predicting is one the harness will raise perfectly
//! well on its own. The value added is the *sentence*, not a gate.
//!
//! # The boundary, stated honestly
//!
//! The answer is computed from the layers io-cli can see: the file's own `[policy]`
//! section, the tier defaults of the posture the operator chose, and the rules they
//! have allowed for this session — [`crate::approval::session_policy`] composes
//! exactly those three, and it is the same value the next turn runs under.
//!
//! On the ordinary path that is the *whole* policy `McpSession::connect` is handed:
//! `run.rs:3935` passes the caller's policy through untouched. Two layers exist that
//! an embedder cannot read, and neither one is silently in force here:
//!
//! * `net::provider_layer` (`net.rs:334`, `pub(crate)`) allows exactly the provider's
//!   own `host:port`, and is merged into the policy that reaches `connect` only on
//!   the path that resumes a run after a network approval (`run.rs:2780`, `:3122`).
//!   It widens, and it widens for one host that is not an MCP server's.
//! * `plan_lock` (`run/gate.rs:194`, `pub(super)`) denies `write` and `exec` outright
//!   while a plan is unreviewed — but it is merged inside the step loop
//!   (`run/step.rs:637`, `run/tree.rs:312`), *after* the servers have connected, so it
//!   gates the tools an MCP server offers and never the server's own spawn.
//!
//! So this module can be wrong in one direction — it can say a server starts where a
//! layer it cannot see would have widened something — and the direction that matters,
//! predicting a start that the harness refuses, is closed by construction, because
//! every deny it can see is a deny the harness sees too. `Policy::merge` can only
//! tighten defaults and a deny is absolute across layers, so a hidden layer cannot
//! turn a refusal here into a start there.

use io_harness::{Act, Effect, McpServer, McpTransport, Policy};

/// The policy target for `url`: its host and port as `host:port`.
///
/// **This is a deliberate copy of io-harness's `net::target`** (0.69.0,
/// `src/net.rs:187-219`), which is `pub(crate)` and therefore unreachable from
/// here. It is not a re-derivation of "how to parse a URL" and must never become
/// one: the only correct behaviour is whatever that function does, including the
/// parts a URL library would do differently.
///
/// **It can drift from its original silently.** Nothing links the two — no trait,
/// no test upstream can fail on this crate's behalf, and a `patch` release of
/// io-harness that tightened the authority split would leave this file compiling,
/// passing, and answering a question the runtime answers differently. A drift that
/// makes this stricter costs a false refusal in a report; a drift that makes it
/// looser makes the preflight lie. That is why `tests/preflight.rs` enumerates the
/// cases — the scheme table, the userinfo drop, both IPv6 spellings, every shape
/// that returns `None` — instead of asserting one happy path. The gate is the
/// enumeration; a single `https://example.com` assertion would pass against almost
/// any URL parser ever written and prove nothing about this one.
///
/// The rules, as upstream states and implements them:
///
/// * split once on `://`; no `://` at all is `None`.
/// * the authority ends at the first `/`, `?` or `#`, and an empty authority is
///   `None`.
/// * userinfo before an `@` is dropped — credentials are not part of the host.
/// * the port is filled from the scheme when the URL omits it: `https`/`wss` → 443,
///   `http`/`ws` → 80. **Any other scheme is `None`**, because it never opens a
///   connection the net act governs.
/// * an IPv6 literal keeps its brackets (`[::1]` → `[::1]:443`), which is what makes
///   the trailing `:port` split unambiguous.
///
/// A `None` is not permission to proceed. See [`check`], which turns it into a
/// refusal for the same reason `NetGuard::check` does.
pub fn target(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    // Authority ends at the first '/', '?', or '#'.
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|a| !a.is_empty())?;
    // Drop any userinfo; credentials are not part of the host.
    let hostport = authority.rsplit_once('@').map_or(authority, |(_, h)| h);

    let default_port = match scheme.to_ascii_lowercase().as_str() {
        "https" | "wss" => "443",
        "http" | "ws" => "80",
        _ => return None,
    };

    if let Some(close) = hostport.strip_prefix('[').and_then(|_| hostport.find(']')) {
        // IPv6 literal: [::1] or [::1]:8080
        let host = &hostport[..=close];
        return match hostport[close + 1..].strip_prefix(':') {
            Some(port) if !port.is_empty() => Some(format!("{host}:{port}")),
            _ => Some(format!("{host}:{default_port}")),
        };
    }

    match hostport.split_once(':') {
        Some((host, port)) if !host.is_empty() && !port.is_empty() => {
            Some(format!("{host}:{port}"))
        }
        Some(_) => None,
        None => Some(format!("{hostport}:{default_port}")),
    }
}

/// What the policy said about starting one server.
///
/// Four variants for three effects, and the fourth is the one this module exists
/// to get right. [`Outcome::Unresolvable`] is a **refusal** — it is what
/// `NetGuard::check` does with a URL that yields no host (`net.rs:275-284`: it
/// returns `Error::Refused` with `rule` and `layer` both `None`, before the policy
/// is consulted at all). It is spelled apart from [`Outcome::Refused`] only so the
/// sentence can say *why*, since there is no rule to name and telling an operator
/// their server was "denied by policy" would send them to edit a file that has
/// nothing to do with it.
///
/// It is a separate variant rather than a boolean beside `Refused` because a match
/// on this enum is exhaustive: a caller that forgets it does not compile. A caller
/// that reads it as "nothing to check, therefore fine" has written the one bug this
/// module was built to prevent, and `tests/preflight.rs` asserts against it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The policy allows the act outright.
    Permitted,
    /// The policy asks. Whether that starts the server depends on the transport —
    /// see [`Preflight::starts`], which is the only place that asymmetry is decided.
    Ask,
    /// A rule or a tier default denies it.
    Refused,
    /// The URL yields no host, so there is nothing to check — and an unchecked
    /// connection is exactly what the guard refuses.
    Unresolvable,
}

/// One server, one question, and everything needed to say who decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    /// The server's id, as the operator named it.
    pub server: String,
    /// The act the harness will check: [`Act::Exec`] for stdio, [`Act::Net`] for http.
    pub act: Act,
    /// What it will check — the command verbatim for stdio, the normalised
    /// `host:port` for http, and the URL as written when it could not be normalised.
    pub target: String,
    /// What that check says.
    pub outcome: Outcome,
    /// The glob that decided, `None` when the tier default did.
    pub rule: Option<String>,
    /// The layer that glob came from, `None` when the tier default decided.
    pub layer: Option<String>,
}

impl Preflight {
    /// Will the server actually start?
    ///
    /// **Not the same question as "was it permitted", because the two transports
    /// treat `Ask` in opposite ways, and both of those are upstream's behaviour
    /// rather than a reading this crate chose:**
    ///
    /// * A **stdio** spawn goes through `authorize_spawn`, which records a refusal
    ///   and returns `Error::Refused` on *anything that is not `Allow`*
    ///   (`mcp.rs:571-589`). Its own rustdoc says why: connecting happens before the
    ///   run's first step, and a server is configuration the operator wrote rather
    ///   than an action the agent chose, so there is nobody to ask and nothing to
    ///   ask about. Git's spawn used to have this same shape and no longer does:
    ///   io-harness#214 closed in 0.70.0, so an asking `exec` posture now raises a
    ///   real approval there. The MCP stdio spawn kept the old shape deliberately,
    ///   and that divergence is exactly why this report cannot be written by
    ///   analogy to git.
    /// * An **http** dial goes through `NetGuard::check`, which returns `Ask` to the
    ///   caller as a verdict — and `McpSession::connect` discards that verdict
    ///   (`mcp.rs:336-339` ends the expression in `?;`). Nobody is asked and the
    ///   connection is made.
    ///
    /// So an asking policy silently stops one transport and silently permits the
    /// other. Collapsing that into one answer would make the report wrong for
    /// whichever transport lost the coin toss, which is why the asymmetry is
    /// expressed here once, keyed on the act the harness itself keys on.
    pub fn starts(&self) -> bool {
        matches!(
            (self.act, self.outcome),
            (Act::Exec, Outcome::Permitted) | (Act::Net, Outcome::Permitted | Outcome::Ask)
        )
    }
}

/// Ask the policy what it will do with this server, without starting it.
///
/// Both branches ask the same public `Policy::check` the harness asks, with the
/// same act and the same target spelling, so a verdict here is the verdict there.
///
/// **The `None` from [`target`] is a refusal and not an absence.** `NetGuard::check`
/// refuses an unparseable target outright (`net.rs:275-284`) — "an unchecked
/// connection is exactly what this guard exists to prevent". Reporting it as
/// "nothing to check" would print *permitted* for a server the runtime is certain to
/// refuse, which is the preflight failing in the only direction that costs anything:
/// a false refusal is a sentence the operator can disprove in one turn, and a false
/// permission is a turn spent discovering that the tool lied.
///
/// This never errors and never refuses. See the module docs.
pub fn check(server: &McpServer, policy: &Policy) -> Preflight {
    let (act, target) = match &server.transport {
        // Verbatim: `authorize_spawn` is handed `command` exactly as the file
        // spells it, and an `Act::Exec` pattern is matched against the target and
        // its basename — never against a command line — so anything done to this
        // string here would be asking about a different program.
        McpTransport::Stdio { command, .. } => (Act::Exec, command.clone()),
        McpTransport::Http { url, .. } => match target(url) {
            Some(host_port) => (Act::Net, host_port),
            None => {
                return Preflight {
                    server: server.id.clone(),
                    act: Act::Net,
                    // The URL as written, which is what `Error::Refused` carries
                    // in this case — there is no `host:port` to carry instead.
                    target: url.clone(),
                    outcome: Outcome::Unresolvable,
                    rule: None,
                    layer: None,
                };
            }
        },
    };

    let verdict = policy.check(act, &target);
    Preflight {
        server: server.id.clone(),
        act,
        target,
        outcome: match verdict.effect {
            Effect::Allow => Outcome::Permitted,
            Effect::Ask => Outcome::Ask,
            Effect::Deny => Outcome::Refused,
        },
        rule: verdict.rule,
        layer: verdict.layer,
    }
}

/// The sentence to show the operator.
///
/// It names the rule and the layer whenever the verdict carried them, because a
/// refusal an operator cannot locate in their own files is one they cannot act on —
/// the same reason [`crate::commit::asked`] names them. When the tier default
/// decided there is no rule to name, and saying so is the answer: it points at the
/// posture rather than at a file, and those are two different repairs.
pub fn line(p: &Preflight) -> String {
    let id = &p.server;
    let target = &p.target;
    match (p.outcome, p.act) {
        (Outcome::Unresolvable, _) => format!(
            "`{id}` will not start: `{target}` has no host to check. io-harness refuses a \
             target it cannot resolve rather than dialling it, and only `http`, `https`, \
             `ws` and `wss` resolve to one."
        ),
        (Outcome::Refused, _) => format!(
            "`{id}` will not start: {act} `{target}` is denied by {by}.",
            act = word(p.act),
            by = decided_by(p),
        ),
        // The spawn is refused without anyone being asked, so the operator has to
        // be told the thing the word "ask" would hide from them.
        (Outcome::Ask, Act::Exec) => format!(
            "`{id}` will not start: running `{target}` is short of allow under {by}, and a server \
             is spawned before the first step — so it is refused rather than asked about.",
            by = decided_by(p),
        ),
        // And the mirror image: it does start, and nobody is asked either.
        (Outcome::Ask, _) => format!(
            "`{id}` will start: net `{target}` is set to ask by {by}, but the connection is opened \
             before any approver exists, so it proceeds unasked.",
            by = decided_by(p),
        ),
        (Outcome::Permitted, _) => format!(
            "`{id}` will start: {act} `{target}` is allowed by {by}.",
            act = word(p.act),
            by = decided_by(p),
        ),
    }
}

/// `exec` or `net`, spelled the way the policy spells it.
fn word(act: Act) -> &'static str {
    match act {
        Act::Exec => "exec",
        Act::Net => "net",
        Act::Read => "read",
        Act::Write => "write",
    }
}

/// Who decided: the rule and its layer, or the tier default.
///
/// The `rule`-without-`layer` arm cannot arise from `Policy::explain`, which sets
/// both or neither — it is written as a fallback rather than as an `unreachable!`
/// because the two are separate `Option`s on a public struct, and a panic in a
/// function whose entire job is to describe something is a poor trade.
fn decided_by(p: &Preflight) -> String {
    match (&p.rule, &p.layer) {
        (Some(rule), Some(layer)) => format!("the rule `{rule}` in the `{layer}` layer"),
        (Some(rule), None) => format!("the rule `{rule}`"),
        (None, _) => "the policy's own default for that act (no rule matched)".to_string(),
    }
}
