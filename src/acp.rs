//! The Agent Client Protocol wire: newline-delimited JSON-RPC 2.0 over stdio.
//!
//! An ACP client — Zed, a JetBrains IDE — spawns `io acp` as a child process and
//! speaks JSON-RPC at its stdin, reading answers off its stdout. This module is
//! the framing and the dispatch. It decides nothing about a run: the translation
//! of io-harness's events into protocol notifications is [`crate::acp_map`], and
//! the run itself is io-harness's as it is everywhere else in this crate.
//!
//! # Why the transport is written here rather than taken
//!
//! There is an official `agent-client-protocol` crate, it is Apache-2.0, it ships
//! a real `Agent` trait, and it was read before this was written. It arrives with
//! twenty-six further lockfile crates, among them `async-io`, `async-process`,
//! `async-task`, `blocking` and `polling` — a second async executor family beside
//! this crate's tokio, with its stdio transport putting `blocking::Unblock` over
//! the real stdin and stdout. A second owner of stdin is not a theoretical
//! hazard in this product: [`crate::stdin`] exists, with its own history, because
//! of what one cost. The protocol is JSON-RPC 2.0 in newline-delimited frames,
//! `serde_json` and `tokio` are already here, and the direct dependency set stays
//! at ten names.
//!
//! The cost of that choice is that protocol drift is this crate's to track, and
//! it is stated rather than discovered: [`PROTOCOL_VERSION`] is answered
//! explicitly and every method below is written against the published v1
//! specification at <https://agentclientprotocol.com/protocol/v1/overview>.
//!
//! # The framing, which is not LSP's
//!
//! ACP does **not** use `Content-Length` headers. Its transport page is explicit:
//! *"Messages are delimited by newlines (`\n`), and MUST NOT contain embedded
//! newlines."* So a frame is one line, and the obligation runs both ways — this
//! module must never emit a message containing an interior newline, which is why
//! [`encode`] goes through `serde_json::to_string` and never a pretty-printer.
//! `to_string` escapes a newline inside a string value as `\n`, two characters,
//! so a frame stays one line however hostile its content.
//!
//! Diagnostics go to stderr. stdout is the protocol, and anything written there
//! that is not a JSON-RPC message corrupts the session — that separation is a
//! correctness property rather than a convention.
//!
//! # Why this file may deserialize JSON
//!
//! `tests/dependencies.rs` permits `serde_json::from_str` in `src/import.rs` and
//! `src/adapt.rs`, and now here, by exact path. The rule's own argument is that a
//! module deciding what somebody else's *file* means is a second opinion about a
//! question that already has one — the operator's Claude configuration and a
//! stranger's plugin manifest each have one reader for that reason. **A protocol
//! this process is the server for is not somebody else's file.** There is no
//! other reader of an ACP frame to disagree with, no on-disk format whose meaning
//! is being re-decided, and refusing to parse here would mean not speaking the
//! protocol at all. The exemption is granted for that reason and is held to the
//! properties `tests/dependencies.rs` asserts beside it.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The ACP major version this adapter speaks.
///
/// An integer rather than a string, and a major rather than a triple — the
/// specification versions the protocol by a single integer that a client and an
/// agent must agree on at `initialize`. v2 exists as a draft behind the reference
/// implementation's `unstable_protocol_v2` feature and is deliberately not spoken
/// here: answering a version this module has not implemented is worse than
/// declining one it has.
pub const PROTOCOL_VERSION: u32 = 1;

/// Every method this adapter dispatches.
///
/// The list is public and asserted against the declared capabilities in both
/// directions, because the failure this product has actually shipped is the other
/// one: 0.30.0 documented `io skill` while the argv door had no variant for it and
/// 1,609 tests passed over the gap, all of them entering one layer below the door.
/// A capability promising a method nobody serves is the same defect in the
/// protocol's own vocabulary.
pub const SERVED: &[&str] = &[
    "initialize",
    "session/new",
    "session/prompt",
    "session/cancel",
];

/// Agent methods a capability must declare before a client may call them, and the
/// capability that declares each.
///
/// Only the gated ones belong here. `session/new`, `session/prompt` and
/// `session/cancel` are the protocol's baseline and no capability governs them,
/// so a table that listed them would make the agreement below vacuous by
/// asserting something that cannot fail.
pub const GATED: &[(&str, &str)] = &[("session/load", "loadSession")];

/// Does this adapter serve `method`?
pub fn serves(method: &str) -> bool {
    SERVED.contains(&method)
}

/// The JSON-RPC error codes this module can answer with.
///
/// The four standard ones from the JSON-RPC 2.0 specification. `ParseError` and
/// `InvalidRequest` are answered with a null id, because a frame that did not
/// parse has no id to answer to and the specification says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Parse,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Internal,
}

impl ErrorCode {
    /// The wire number.
    pub fn code(self) -> i32 {
        match self {
            Self::Parse => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let word = match self {
            Self::Parse => "parse error",
            Self::InvalidRequest => "invalid request",
            Self::MethodNotFound => "method not found",
            Self::InvalidParams => "invalid params",
            Self::Internal => "internal error",
        };
        f.write_str(word)
    }
}

/// A frame that arrived.
///
/// A request carries an id and is answered; a notification carries none and is
/// not. The distinction is the protocol's and it matters at exactly one place —
/// answering a notification is a protocol violation, and `session/cancel` is a
/// notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

impl Incoming {
    /// The method name, whichever shape this is.
    pub fn method(&self) -> &str {
        match self {
            Self::Request { method, .. } | Self::Notification { method, .. } => method,
        }
    }

    /// The parameters, whichever shape this is.
    pub fn params(&self) -> &Value {
        match self {
            Self::Request { params, .. } | Self::Notification { params, .. } => params,
        }
    }
}

/// A frame being sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Result { id: Value, result: Value },
    Error { id: Value, code: ErrorCode, message: String },
    Notification { method: String, params: Value },
}

impl Outgoing {
    /// An error answering a request whose id is not known.
    ///
    /// JSON-RPC 2.0 requires a null id when the id could not be determined, which
    /// is every frame that failed to parse.
    pub fn unattributed(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error {
            id: Value::Null,
            code,
            message: message.into(),
        }
    }
}

/// What a frame looked like on the wire, before it was understood.
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    params: Option<Value>,
}

/// Read one frame.
///
/// **Every failure here is answerable rather than fatal.** A client that sends a
/// malformed frame gets a JSON-RPC error and the session continues, because the
/// alternative — exiting — turns one bad frame into a lost conversation. The
/// caller distinguishes the two by the `Result`: `Err` is a frame to answer,
/// never a reason to stop reading.
pub fn decode(line: &str) -> Result<Incoming, Outgoing> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err(Outgoing::unattributed(
            ErrorCode::Parse,
            "an empty line is not a JSON-RPC message",
        ));
    }

    let envelope: Envelope = serde_json::from_str(trimmed)
        .map_err(|err| Outgoing::unattributed(ErrorCode::Parse, err.to_string()))?;

    // The version is checked rather than assumed. A client speaking a different
    // JSON-RPC revision at this socket is a misconfiguration worth naming once,
    // and it is cheap: the field is required by the specification.
    if envelope.jsonrpc != "2.0" {
        return Err(Outgoing::Error {
            id: envelope.id.unwrap_or(Value::Null),
            code: ErrorCode::InvalidRequest,
            message: format!(
                "jsonrpc must be \"2.0\"; this frame said {:?}",
                envelope.jsonrpc
            ),
        });
    }

    let Some(method) = envelope.method else {
        // A frame with an id and no method is a *response* to something this
        // adapter asked. Those are correlated elsewhere; reaching here means one
        // arrived that nobody was waiting for, which is a client error rather
        // than a crash.
        return Err(Outgoing::Error {
            id: envelope.id.unwrap_or(Value::Null),
            code: ErrorCode::InvalidRequest,
            message: "a request or notification names a method".into(),
        });
    };

    // Absent params is `{}` rather than an error. The specification makes the
    // member optional, and several methods take none.
    let params = envelope.params.unwrap_or(Value::Object(Default::default()));

    match envelope.id {
        Some(id) => Ok(Incoming::Request { id, method, params }),
        None => Ok(Incoming::Notification { method, params }),
    }
}

/// Render a frame as the single line it must be.
///
/// `serde_json::to_string` and never `to_string_pretty`: the transport delimits
/// messages by newline and forbids an embedded one, so a pretty-printer here
/// would split one message into as many frames as it has lines and desynchronise
/// the session permanently. The returned string carries no trailing newline —
/// the writer adds exactly one.
pub fn encode(frame: &Outgoing) -> String {
    let value = match frame {
        Outgoing::Result { id, result } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Outgoing::Error { id, code, message } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code.code(), "message": message },
        }),
        Outgoing::Notification { method, params } => serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }),
    };

    // A serialization failure on a value this module built is not reachable —
    // every branch above is objects, strings and numbers — but answering it with
    // a panic would take the session down for a fault that is ours. The fallback
    // is a well-formed internal error, still one line.
    serde_json::to_string(&value).unwrap_or_else(|_| {
        String::from(
            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"internal error"}}"#,
        )
    })
}

/// What this adapter tells a client it can do.
///
/// **Everything omitted is unsupported, and that is the specification's rule
/// rather than this module's**: *"Clients and Agents MUST treat all capabilities
/// omitted in the `initialize` request as UNSUPPORTED."* So the honest way to
/// decline a capability is to say `false` or say nothing, which is what the two
/// excluded families do.
///
/// `loadSession` is `false` because `session/load` is not served. io has `io
/// resume` and the mapping is plausible, but the correspondence between an ACP
/// session id and a stored run has to be designed rather than guessed, and
/// declaring a capability whose method returns "method not found" is the defect
/// this module's own [`SERVED`]/[`GATED`] agreement exists to prevent.
///
/// The filesystem and terminal families are the client's to implement, not the
/// agent's, and this adapter never calls them — io-harness owns the disk inside
/// its own sandbox and publishes no seam to route a read through the client.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub load_session: bool,
    pub prompt_capabilities: PromptCapabilities,
}

/// What a `session/prompt` may carry into this agent.
///
/// All three are `false` in 0.36.0. io-harness accepts images and documents and
/// this adapter will carry them, but a capability is a promise about a wire shape
/// that has to be translated and tested, and promising one before it is is how a
/// client discovers a gap at runtime.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PromptCapabilities {
    pub image: bool,
    pub audio: bool,
    pub embedded_context: bool,
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            load_session: false,
            prompt_capabilities: PromptCapabilities {
                image: false,
                audio: false,
                embedded_context: false,
            },
        }
    }
}

impl AgentCapabilities {
    /// Is the capability named `name` declared supported?
    ///
    /// By name rather than by field, so the agreement in `tests/acp.rs` can walk
    /// [`GATED`] and ask this the same way a reader of the specification would.
    /// An unknown name is not supported, which is the specification's own default
    /// for anything omitted.
    pub fn declares(&self, name: &str) -> bool {
        match name {
            "loadSession" => self.load_session,
            "image" => self.prompt_capabilities.image,
            "audio" => self.prompt_capabilities.audio,
            "embeddedContext" => self.prompt_capabilities.embedded_context,
            _ => false,
        }
    }
}

/// What answers a decoded frame.
///
/// A method may produce several frames — `session/prompt` streams
/// `session/update` notifications and then returns a result — so a handler
/// answers with a list rather than an `Option`. An empty list is the correct
/// answer to a notification, and [`serve`] does not check that for the handler:
/// a dispatcher that answered `session/cancel` would be sending an unsolicited
/// response mid-turn, and that is the handler's rule to keep because only the
/// handler knows which method it is serving.
///
/// Native `async fn` in a trait, used generically and never behind `dyn`. The
/// crate's MSRV is well past what that needs, and it keeps the alternative — an
/// `async-trait` dependency — out of a tree whose whole point in this release is
/// that it did not grow.
pub trait Handler {
    /// Answer one frame.
    fn handle(&mut self, incoming: Incoming) -> impl std::future::Future<Output = Vec<Outgoing>>;
}

/// Read frames until the client closes stdin, answering each.
///
/// **Every error is answered rather than fatal, and that is the loop's whole
/// shape.** A malformed frame produces a JSON-RPC error and the session
/// continues; only end-of-input and an unreadable stream stop it. The alternative
/// — exiting on a bad frame — turns one client bug into a lost conversation, and
/// a conversation here is an operator's actual work.
///
/// Generic over the streams so a test can drive the real loop with in-memory
/// buffers. That is deliberate rather than incidental: the stdout-backed
/// [`crate::term::Screen`] taught this crate that a transport reachable only from
/// the binary is a transport no test has ever run, and 0.32.0 spent a task
/// undoing exactly that.
///
/// The writer is flushed after every frame. A client is blocked reading the line
/// it is waiting for, so a buffered answer is a hang.
pub async fn serve<R, W, H>(reader: R, mut writer: W, handler: &mut H) -> std::io::Result<()>
where
    R: tokio::io::AsyncBufRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
    H: Handler,
{
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    let mut reader = reader;
    let mut line = String::new();

    loop {
        line.clear();
        // Zero bytes is end of input: the client closed the pipe, which is how an
        // ACP session ends normally. It is not an error and must not be answered.
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }

        let answers = match decode(&line) {
            Ok(incoming) => handler.handle(incoming).await,
            Err(refusal) => vec![refusal],
        };

        for answer in &answers {
            writer.write_all(encode(answer).as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
        // Flushed once per frame batch rather than once per frame: the batch is
        // what a client is waiting on, and a `session/prompt` answer arrives
        // behind its own notifications either way.
        writer.flush().await?;
    }
}

/// The `initialize` result.
///
/// `authMethods` is empty and that is legal. `io setup` is a wizard that writes
/// io-harness's configuration file, not an ACP authentication method, and
/// credentials stay exactly where they are — an empty list says "this agent needs
/// no authentication step from you", which is true.
pub fn initialize_result(agent_version: &str) -> Value {
    serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "agentInfo": { "name": "io", "version": agent_version },
        "agentCapabilities": AgentCapabilities::default(),
        "authMethods": [],
    })
}
