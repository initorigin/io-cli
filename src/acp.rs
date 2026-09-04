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
// One name per line: `tests/dependencies.rs` forbids `use serde_json::{`
// everywhere, permitted modules included, because a name spelled around is a
// parse that appears in no sweep. This file is permitted to parse and therefore
// writes `serde_json::from_str` out in full where it does.
use serde_json::json;
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
    /// An answer to something **this** adapter asked the client.
    ///
    /// A frame with an id and no method. **Since 0.38.0 this is the ordinary way
    /// an approval is answered**, and it is handled by the read task inside
    /// [`serve_with`] rather than by a [`Handler`]: settling a waiter needs no
    /// dispatch and no `&mut` anything, and doing it on the reader is what lets an
    /// answer be read while the turn that asked for it is still waiting.
    ///
    /// Through 0.37.0 nothing arrived here, because nothing was ever asked. The
    /// shape was decoded rather than refused even then — a transport that answered
    /// a well-formed response with an error would have had to change before a
    /// request could ever be sent.
    ///
    /// `outcome` carries the client's `error` object as an `Err`, so a client that
    /// refuses the request is distinguishable from one that answered it — the run
    /// must not treat a protocol error as a permission decision.
    Response {
        id: Value,
        outcome: Result<Value, String>,
    },
}

impl Incoming {
    /// The method name, or `""` for a response, which names none.
    pub fn method(&self) -> &str {
        match self {
            Self::Request { method, .. } | Self::Notification { method, .. } => method,
            Self::Response { .. } => "",
        }
    }

    /// The parameters, or the result for a response that carried one.
    pub fn params(&self) -> &Value {
        // A `const` rather than a temporary, so the `Err` arm has something with
        // the right lifetime to hand back.
        const NOTHING: &Value = &Value::Null;
        match self {
            Self::Request { params, .. } | Self::Notification { params, .. } => params,
            Self::Response { outcome, .. } => outcome.as_ref().unwrap_or(NOTHING),
        }
    }
}

/// The requests this adapter has asked the client and is still waiting on.
///
/// **This is the piece 0.36.0 was missing.** The frame builders were written,
/// tested and shipped with no production caller, and the module's own doc said
/// the missing piece was the correlation rather than the design. It is: a
/// request needs an id nobody else will use, somewhere to park the waiting side
/// of it, and a reader that can settle it *while the turn that raised it is
/// still running*.
///
/// The last of those is why [`serve_with`] reads on its own task. Awaiting the
/// handler inline in the read loop — which is what 0.36.0 did — means the answer
/// to a question that turn asked sits unread in the pipe until the turn that is
/// waiting for it returns. That is not a slow path, it is a deadlock, and it is
/// the same shape as the writer starvation 0.36.0 shipped one layer up in this
/// same function.
///
/// A `std::sync::Mutex` and not tokio's: the lock is taken to insert or remove
/// one map entry and is never held across an `await`, which is the only thing
/// the async mutex would buy.
#[derive(Debug, Default)]
pub struct Correlator {
    next: std::sync::atomic::AtomicI64,
    waiting: std::sync::Mutex<
        std::collections::HashMap<i64, tokio::sync::oneshot::Sender<Result<Value, String>>>,
    >,
}

impl Correlator {
    /// A correlator with nothing outstanding.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Claim an id and the receiving half of its answer.
    ///
    /// Ids ascend from 1 and are never reused within a process. A client that
    /// answers the same id twice settles the first and is dropped by the second,
    /// because the sender is taken out of the map when it is used.
    #[must_use]
    pub fn issue(&self) -> (i64, tokio::sync::oneshot::Receiver<Result<Value, String>>) {
        let id = self
            .next
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1);
        let (tx, rx) = tokio::sync::oneshot::channel();
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.insert(id, tx);
        }
        (id, rx)
    }

    /// Hand a client's answer to whoever is waiting for it.
    ///
    /// A response naming an id nobody is waiting for is dropped. That is the
    /// correct answer and not a gap: the protocol has no response to a response,
    /// so there is nothing to send back, and a client that answers twice or
    /// invents an id must not be able to make this adapter say anything.
    pub fn settle(&self, id: &Value, outcome: Result<Value, String>) {
        let Some(id) = id.as_i64() else {
            return;
        };
        let waiting = self.waiting.lock().ok().and_then(|mut map| map.remove(&id));
        if let Some(sender) = waiting {
            let _ = sender.send(outcome);
        }
    }

    /// Give up on every outstanding request.
    ///
    /// Called when the reader stops, which is the only way a request that was
    /// sent can come to have no answer coming. Dropping each sender makes every
    /// waiter's `recv` fail, and the caller turns that into a denial — **so a
    /// client that disconnects mid-approval denies rather than hangs, and this
    /// adapter needs no timeout to guarantee it.**
    ///
    /// A timeout was the obvious alternative and it would have been a number
    /// invented here. An approval is answered by a person: a minute is too short
    /// for someone reading a diff and an hour is indistinguishable from a hang.
    /// The connection ending is the real event, and it is observable.
    pub fn abandon(&self) {
        if let Ok(mut waiting) = self.waiting.lock() {
            waiting.clear();
        }
    }
}

/// A frame being sent.
#[derive(Debug, Clone, PartialEq)]
pub enum Outgoing {
    Result {
        id: Value,
        result: Value,
    },
    Error {
        id: Value,
        code: ErrorCode,
        message: String,
    },
    Notification {
        method: String,
        params: Value,
    },
    /// Something **this** adapter is asking the client, which it will wait for an
    /// answer to.
    ///
    /// The id is allocated by [`Correlator`] and is a number, deliberately: the
    /// client's ids are its own and may be anything JSON-RPC permits, so an
    /// adapter that reused their shape could collide with one. Two id spaces
    /// travel in opposite directions on one pipe and only one of them is ours.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
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
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<Value>,
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
        // adapter asked. 0.36.0 asks nothing, so this arm is reached only by a
        // client answering a request an older or newer `io` made; it is decoded
        // rather than refused, and the handler ignores it.
        let Some(id) = envelope.id else {
            return Err(Outgoing::unattributed(
                ErrorCode::InvalidRequest,
                "a frame with neither a method nor an id is neither a request, a \
                 notification nor a response",
            ));
        };
        // An `error` object and a `result` are different answers and the run must
        // not read the first as a permission decision: a client that refused the
        // request has not decided anything, and treating that as consent is the
        // one mistake this seam cannot be allowed to make.
        let outcome = match envelope.error {
            Some(error) => Err(error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("the client refused the request")
                .to_string()),
            None => Ok(envelope.result.unwrap_or(Value::Null)),
        };
        return Ok(Incoming::Response { id, outcome });
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
        Outgoing::Request { id, method, params } => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
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
pub async fn serve<R, W, H>(reader: R, writer: W, handler: &mut H) -> std::io::Result<()>
where
    R: tokio::io::AsyncBufRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
    H: Handler,
{
    let (_unused, rx) = tokio::sync::mpsc::unbounded_channel();
    // A correlator of its own, because this door asks the client nothing: a
    // handler that raises no request never issues an id, so the map stays empty
    // and abandoning it at end-of-input is a no-op.
    serve_with(
        reader,
        writer,
        handler,
        rx,
        std::sync::Arc::new(Correlator::new()),
    )
    .await
}

/// [`serve`], plus a channel the agent writes frames on without being asked.
///
/// Two things travel that way and neither is an answer to an incoming frame: the
/// `session/update` notifications a run streams while it is working, and the
/// refusal an approval raises through [`Consulting`]. Both originate
/// inside a turn, on another task, while this loop is blocked reading — so they
/// cannot be returns from [`Handler::handle`] and there has to be a second way in.
///
/// The two sources are joined with `select!` rather than by a writer task of their
/// own, because a `Mutex` over the writer is the alternative and a lock held
/// across an `await` on a pipe a slow client is not draining is a deadlock with no
/// error message.
pub async fn serve_with<R, W, H>(
    reader: R,
    mut writer: W,
    handler: &mut H,
    mut outbound: tokio::sync::mpsc::UnboundedReceiver<Outgoing>,
    correlator: std::sync::Arc<Correlator>,
) -> std::io::Result<()>
where
    // `Send + 'static` for the same reason the writer needs it: the reader moves
    // onto its own task too, so that a response can be read while the turn that
    // is waiting for it runs. See the note on the read task below.
    R: tokio::io::AsyncBufRead + Unpin + Send + 'static,
    // `Send + 'static` because the writer moves onto its own task. That bound is
    // the visible cost of the fix below and it is worth paying: a borrowed writer
    // is what forced the single-loop shape that could not stream.
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
    H: Handler,
{
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    // **The writer runs on its own task, and that is the whole of why anything
    // streams.** The obvious shape — one loop that `select!`s over the reader and
    // the outbound channel — does not work here, and the first version of this
    // function shipped that bug: `handler.handle(...).await` for a
    // `session/prompt` *is the entire turn*, minutes long, and it runs inside the
    // select arm's body. `select!` resolves once and then the body runs to
    // completion, so the outbound channel is not polled again until the turn
    // returns. Every `session/update` the run produced queued behind the result,
    // and the client received the whole conversation after being told the turn had
    // ended. `biased` cannot help: the loop is not at the select point to be
    // biased about.
    //
    // A task that owns the writer is polled by the runtime independently of what
    // this loop is doing, so a notification sent from inside a running turn is
    // written while the turn is still running. It owns the writer outright rather
    // than sharing it behind a `Mutex`, because a lock held across an `await` on a
    // pipe a slow client is not draining is a deadlock with no error message.
    let (frames, mut queue) = tokio::sync::mpsc::unbounded_channel::<Outgoing>();
    let pump = tokio::spawn(async move {
        while let Some(frame) = queue.recv().await {
            if writer.write_all(encode(&frame).as_bytes()).await.is_err()
                || writer.write_all(b"\n").await.is_err()
                || writer.flush().await.is_err()
            {
                // The client closed the pipe. Not an error worth propagating: the
                // read side sees end-of-input and ends the session.
                return;
            }
        }
    });

    // Everything the run sends goes to the same writer, so the two sources cannot
    // interleave inside a frame.
    let relay = frames.clone();
    let forward = tokio::spawn(async move {
        while let Some(frame) = outbound.recv().await {
            if relay.send(frame).is_err() {
                return;
            }
        }
    });

    // **The reader runs on its own task, and that is why an approval can be
    // answered at all.** The 0.36.0 shape awaited `handler.handle(...)` inline
    // here, and for a `session/prompt` that call *is* the whole turn — so a
    // `session/request_permission` raised by that turn could never be answered:
    // the client's reply sat unread in the pipe until the turn returned, and the
    // turn was waiting for the reply. Not a slow path, a deadlock, and exactly
    // the shape of the writer starvation this same function shipped one layer up.
    //
    // Splitting it puts the settling of a response — which needs no handler and
    // no `&mut` anything — on a task that keeps reading, while requests and
    // notifications queue for the loop below to take one at a time. Queueing them
    // is right rather than a limitation: ACP runs one turn per session, and a
    // second prompt arriving mid-turn must not start a second one.
    let (dispatch, mut arrivals) =
        tokio::sync::mpsc::unbounded_channel::<Result<Incoming, Outgoing>>();
    let settling = std::sync::Arc::clone(&correlator);
    let read = tokio::spawn(async move {
        let mut reader = reader;
        let mut line = String::new();
        let outcome = loop {
            line.clear();
            // Zero bytes is end of input: the client closed the pipe, which is
            // how an ACP session ends normally. Not an error, and not answered.
            match reader.read_line(&mut line).await {
                Ok(0) => break Ok(()),
                Ok(_) => {}
                Err(error) => break Err(error),
            }

            match decode(&line) {
                // Settled here rather than forwarded, because this is the whole
                // point of the split: a waiter parked inside a running turn is
                // released without the loop below having to reach this line.
                Ok(Incoming::Response { id, outcome }) => settling.settle(&id, outcome),
                Ok(incoming) => {
                    if dispatch.send(Ok(incoming)).is_err() {
                        break Ok(());
                    }
                }
                Err(refusal) => {
                    if dispatch.send(Err(refusal)).is_err() {
                        break Ok(());
                    }
                }
            }
        };
        // Every outstanding request now has no answer coming. Abandoning them
        // turns each waiter into a denial instead of a hang — the reason this
        // adapter needs no approval timeout.
        settling.abandon();
        outcome
    });

    let outcome = loop {
        let Some(arrival) = arrivals.recv().await else {
            break Ok(());
        };

        let answers = match arrival {
            Ok(incoming) => handler.handle(incoming).await,
            Err(refusal) => vec![refusal],
        };

        for answer in answers {
            // A closed writer means the client is gone; stop reading rather than
            // serving a conversation nobody receives.
            if frames.send(answer).is_err() {
                break;
            }
        }
    };

    // Dropping both senders ends the pump, which flushes what it already holds
    // before it returns — so a frame queued by the last turn is not lost when the
    // client closes the pipe a moment later.
    drop(frames);
    forward.abort();
    let _ = pump.await;

    // **The read task owns the real outcome.** The loop above ends when its
    // channel closes, which happens because the reader stopped — so its own
    // `Ok(())` says only "there is nothing more to serve" and would swallow the
    // io error that caused it. A join failure is reported as `Ok(())` for the
    // same reason end-of-input is: the session is over either way and there is no
    // second thing to say about it.
    match read.await {
        Ok(from_reader) => from_reader,
        Err(_) => outcome,
    }
}

/// The options an ACP client is offered for one approval, as
/// `(optionId, name, kind)`.
///
/// **Three, and the missing fourth is the point.** ACP names four
/// `PermissionOption` kinds — `allow_once`, `allow_always`, `reject_once` and
/// `reject_always` — and this adapter offers three, because
/// `io_harness::Decision` has no way to express the fourth. `Decision::Approve`
/// carries a `remember: Vec<Rule>`, which is what makes `allow_always` real;
/// `Decision::Deny` carries a reason and nothing else, so a remembered refusal
/// cannot be recorded and a later matching action would ask again.
///
/// Offering `reject_always` anyway and quietly behaving as `reject_once` is the
/// defect shape this product keeps finding in itself — a surface that accepts an
/// instruction it cannot carry out and reports success. So it is not offered, the
/// omission is documented in the guide, and the ask goes upstream as an issue.
///
/// The ids are io-cli's own words rather than the ACP kind names, because the
/// kind is a separate field and a client renders `name`. They map one-to-one onto
/// [`crate::approval::Answer`], which is the vocabulary the interactive overlay
/// has used since 0.2.0 — one set of permissions for the product, not two.
pub const PERMISSION_OPTIONS: &[(&str, &str, &str)] = &[
    ("allow-once", "Allow once", "allow_once"),
    ("allow-session", "Allow for this session", "allow_always"),
    ("deny", "Deny", "reject_once"),
];

/// The `session/request_permission` params for one pending approval.
///
/// `title` is the act and its target in the words io-harness used, never a
/// rewrite: the operator is being asked to authorise a specific action and the
/// sentence they read has to be the one the run will perform.
pub fn permission_params(session_id: &str, act: &str, target: &str, tool_call_id: &str) -> Value {
    json!({
        "sessionId": session_id,
        "toolCall": {
            "toolCallId": tool_call_id,
            "title": format!("{act} {target}"),
            "status": "pending",
        },
        "options": PERMISSION_OPTIONS
            .iter()
            .map(|(id, name, kind)| json!({ "optionId": id, "name": name, "kind": kind }))
            .collect::<Vec<_>>(),
    })
}

/// What the client's answer means, or `None` when it is not an option that was
/// offered.
///
/// **An unrecognised option is not a silent approval.** A client that answers
/// with an id this adapter never offered has said something it cannot have meant,
/// and the caller turns `None` into a denial — the direction a permission surface
/// must fail in.
pub fn answer_for(option_id: &str) -> Option<crate::approval::Answer> {
    use crate::approval::Answer;
    match option_id {
        "allow-once" => Some(Answer::Once),
        "allow-session" => Some(Answer::Session),
        "deny" => Some(Answer::Deny),
        _ => None,
    }
}

/// Read the client's `session/request_permission` result.
///
/// The specification's outcome is `{outcome: "cancelled"}` or
/// `{outcome: "selected", optionId}`. A cancellation is a refusal rather than a
/// pause: the run is mid-turn and holding it open on a client that has walked
/// away is how a session hangs with no error.
pub fn permission_answer(result: &Value) -> crate::approval::Answer {
    use crate::approval::Answer;
    match result.get("outcome").and_then(Value::as_str) {
        Some("selected") => result
            .get("optionId")
            .and_then(Value::as_str)
            .and_then(answer_for)
            .unwrap_or(Answer::Deny),
        // Every other shape — "cancelled", a missing outcome, a member this
        // adapter has never seen — is a denial. There is exactly one safe
        // direction to be wrong in here.
        _ => Answer::Deny,
    }
}

/// The observer a run reports through, and the flag `session/cancel` sets.
///
/// **It lives in the library and not in `src/main.rs`, and that is not a
/// preference.** Nothing under `tests/` links the driver, so a cancellation
/// written there could be neither tested nor sabotaged — the rule `AGENTS.md`
/// states and that 0.33.0 paid for with three acceptance criteria gated by
/// nothing. The driver holds wiring; the decision is here.
///
/// The cancel flag is an `AtomicBool` behind an `Arc` rather than a channel,
/// because `Observer::event` is `&self` and synchronous, running on the run's own
/// task: it has to answer immediately, and it may be asked from several agents in
/// a contained tree at once. That is the same shape `io_harness::Approver`
/// documents for itself.
pub struct Reporter {
    session_id: String,
    updates: tokio::sync::mpsc::UnboundedSender<Outgoing>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The step the last observed event belonged to.
    ///
    /// **The approver needs this and has no other way to learn it.** A cell's id
    /// on this wire is `{run_id}-{step}` ([`crate::acp_map`]), and an approval
    /// request has to name the cell the client already has — but an
    /// [`io_harness::Approver`] is handed a [`io_harness::Request`], which
    /// carries the act and the target and no step at all. The observer is the
    /// only thing in this module that sees one.
    step: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

/// What a turn's observer and its approver share.
///
/// Two flags rather than a struct with methods, because each has exactly one
/// writer and one reader and the whole of the coupling is that they are the same
/// allocation.
pub struct Shared {
    /// Set by `session/cancel`, read before each event is translated.
    pub cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Written by the observer on every event, read when an approval is raised.
    pub step: std::sync::Arc<std::sync::atomic::AtomicU32>,
}

impl Reporter {
    /// A reporter for one session, and the handles that steer and read it.
    pub fn new(
        session_id: impl Into<String>,
        updates: tokio::sync::mpsc::UnboundedSender<Outgoing>,
    ) -> (Self, Shared) {
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let step = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        (
            Self {
                session_id: session_id.into(),
                updates,
                cancelled: std::sync::Arc::clone(&cancelled),
                step: std::sync::Arc::clone(&step),
            },
            Shared { cancelled, step },
        )
    }
}

impl io_harness::Observer for Reporter {
    fn event(&self, event: &io_harness::RunEvent) -> io_harness::Flow {
        // The flag is read **before** the event is translated, so a cancellation
        // takes effect on the first event after it is set rather than after one
        // more notification has been written. A client that cancelled and then
        // read another chunk would reasonably think the cancel was ignored.
        if self.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
            return io_harness::Flow::Cancel;
        }

        // Recorded before the translation and unconditionally, because an event
        // this module draws nothing for still moved the run on — and the next
        // approval must name the step the agent is actually on, not the last one
        // that happened to render.
        self.step
            .store(event.step, std::sync::atomic::Ordering::Relaxed);

        if let Some(update) = crate::acp_map::translate(event) {
            // A closed receiver means the session is gone. That is not a reason
            // to cancel the run here — the same argument `src/exec.rs` makes
            // about a broken pipe: the work is the operator's and the stream is a
            // report on it.
            let _ = self.updates.send(Outgoing::Notification {
                method: "session/update".into(),
                params: json!({ "sessionId": self.session_id, "update": update }),
            });
        }
        io_harness::Flow::Continue
    }
}

/// Serve ACP on the process's real stdin and stdout.
///
/// The door `io acp` opens. It builds the same store, session, policy and
/// provider chain every other door builds — this adapter adds a protocol, not a
/// second way of being configured — then reads frames until the client closes
/// the pipe.
///
/// **Nothing else in this process may touch stdin or stdout.** stdout is the
/// protocol and stdin is the frame stream; a `Screen`, raw mode or a cursor query
/// would take frames away from the reader and put bytes in the middle of a
/// message. `src/main.rs` returns here before any of that is built.
pub async fn main(
    config: io_harness::Config,
    root: std::path::PathBuf,
    model_override: Option<String>,
) -> Result<u8, String> {
    // As `io exec` does, and for the same reason: this door returns from `main`
    // before the interactive path resolves anything, so a bundle's declared
    // program reaches no `PATH` unless it is placed here. 0.34.0 shipped with
    // exactly this missing on two headless doors and the live gate found it.
    for notice in crate::bundle_path::install_for(&config) {
        eprintln!("{notice}");
    }

    let spec = crate::exec::spec_for(None, &config, model_override.as_deref())?;
    let store = crate::settings::store_path().ok_or("no place to keep the run store")?;
    let store = io_harness::Store::open(&store).map_err(|error| error.to_string())?;
    let session = io_harness::Session::open(&store, &root).map_err(|error| error.to_string())?;
    let policy = crate::exec::policy_for(&config, None);

    crate::provider::build(
        spec,
        model_override,
        Editor {
            store,
            session,
            config,
            policy,
        },
    )
    .await?
}

/// The ACP session, as something [`crate::provider::build`] can run.
struct Editor {
    store: io_harness::Store,
    session: io_harness::Session,
    config: io_harness::Config,
    policy: io_harness::Policy,
}

impl crate::provider::WithProvider for Editor {
    type Out = Result<u8, String>;

    async fn call<P: io_harness::Provider>(
        mut self,
        make: impl Fn(&str) -> Result<P, String>,
        model: String,
    ) -> Self::Out {
        let provider = make(&model)?;
        let (outbound, rx) = tokio::sync::mpsc::unbounded_channel();
        // One correlator for the session, shared three ways: the read task
        // settles into it, `Consulting` waits on it, and it is created here
        // because both of those are built from this frame.
        let correlator = std::sync::Arc::new(Correlator::new());
        let mut handler = Editing {
            store: &self.store,
            session: &mut self.session,
            config: &self.config,
            policy: &self.policy,
            provider: &provider,
            outbound,
            session_id: None,
            cancel: None,
            correlator: std::sync::Arc::clone(&correlator),
        };

        let stdin = tokio::io::BufReader::new(tokio::io::stdin());
        serve_with(stdin, tokio::io::stdout(), &mut handler, rx, correlator)
            .await
            .map_err(|error| error.to_string())?;
        Ok(0)
    }
}

/// The live session's state, between frames.
struct Editing<'a, P: io_harness::Provider> {
    store: &'a io_harness::Store,
    session: &'a mut io_harness::Session,
    config: &'a io_harness::Config,
    policy: &'a io_harness::Policy,
    provider: &'a P,
    outbound: tokio::sync::mpsc::UnboundedSender<Outgoing>,
    /// The one session this adapter holds. ACP permits an agent to hold several;
    /// 0.36.0 holds one and refuses a second rather than multiplexing badly.
    session_id: Option<String>,
    cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Where an approval this session raises will be answered. Handed to each
    /// turn's [`Consulting`], and shared with the read task inside
    /// [`serve_with`], which is the only thing that settles it.
    correlator: std::sync::Arc<Correlator>,
}

impl<P: io_harness::Provider> Handler for Editing<'_, P> {
    async fn handle(&mut self, incoming: Incoming) -> Vec<Outgoing> {
        match incoming {
            Incoming::Request { id, method, params } => {
                match self.request(&method, &params).await {
                    Ok(result) => vec![Outgoing::Result { id, result }],
                    Err((code, message)) => vec![Outgoing::Error { id, code, message }],
                }
            }
            Incoming::Notification { method, params } => {
                self.notify(&method, &params);
                // A notification is never answered. Answering one is a protocol
                // violation and would put an unsolicited response mid-turn.
                Vec::new()
            }
            // 0.36.0 asks the client nothing it then waits on, so a response
            // arriving here is one nobody is waiting for. Ignored rather than
            // answered: the protocol has no response to a response.
            Incoming::Response { .. } => Vec::new(),
        }
    }
}

impl<P: io_harness::Provider> Editing<'_, P> {
    async fn request(
        &mut self,
        method: &str,
        params: &Value,
    ) -> Result<Value, (ErrorCode, String)> {
        match method {
            "initialize" => Ok(initialize_result(env!("CARGO_PKG_VERSION"))),
            "session/new" => {
                if self.session_id.is_some() {
                    return Err((
                        ErrorCode::InvalidRequest,
                        "this agent holds one session per process; start another `io acp` \
                         for a second"
                            .into(),
                    ));
                }
                let id = format!("io-{}", self.session.id());
                self.session_id = Some(id.clone());
                Ok(json!({ "sessionId": id }))
            }
            "session/prompt" => {
                let Some(session_id) = self.session_id.clone() else {
                    return Err((
                        ErrorCode::InvalidRequest,
                        "no session; send `session/new` first".into(),
                    ));
                };
                let goal = prompt_text(params).ok_or((
                    ErrorCode::InvalidParams,
                    "a prompt carries at least one text content block".to_string(),
                ))?;
                let reason = self.run(&session_id, goal).await;
                Ok(json!({ "stopReason": reason }))
            }
            _ => Err((
                ErrorCode::MethodNotFound,
                format!("`{method}` is not served by this agent"),
            )),
        }
    }

    fn notify(&mut self, method: &str, _params: &Value) {
        if method == "session/cancel" {
            // Set the flag the reporter reads. Cancelling a turn that is not
            // running is not an error — a client racing its own prompt would
            // otherwise be refused for doing the right thing.
            if let Some(cancel) = &self.cancel {
                cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }

    /// Drive one turn, streaming updates, and answer with its stop reason.
    async fn run(&mut self, session_id: &str, goal: String) -> &'static str {
        let (reporter, shared) = Reporter::new(session_id, self.outbound.clone());
        self.cancel = Some(std::sync::Arc::clone(&shared.cancelled));

        let contract = crate::exec::contract(self.config, self.session, goal, None);
        let outcome = self
            .session
            .turn_bounded_observed(
                &contract,
                self.provider,
                self.store,
                self.policy,
                &Consulting {
                    session_id: session_id.to_string(),
                    outbound: self.outbound.clone(),
                    run_id: self.session.id(),
                    step: std::sync::Arc::clone(&shared.step),
                    correlator: std::sync::Arc::clone(&self.correlator),
                },
                &reporter,
            )
            .await;

        self.cancel = None;
        match outcome {
            Ok(result) => crate::acp_map::stop_reason(&result.outcome),
            // A turn that failed did not refuse anything and did not run out of
            // budget, so `end_turn` with the reason on stderr is the honest
            // answer until ACP grows a shape for it.
            Err(error) => {
                eprintln!("io: {error}");
                "end_turn"
            }
        }
    }
}

/// The text of a `session/prompt`, joined from its content blocks.
///
/// ACP carries a prompt as a list of typed blocks. 0.36.0 declares no image,
/// audio or embedded-context capability, so a conforming client sends text —
/// and anything else is skipped rather than rendered as a placeholder the model
/// would try to read.
pub fn prompt_text(params: &Value) -> Option<String> {
    let blocks = params.get("prompt")?.as_array()?;
    let text: Vec<&str> = blocks
        .iter()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect();
    if text.is_empty() {
        return None;
    }
    Some(text.join("\n"))
}

/// The approver for an ACP turn.
///
/// **0.36.0 stops short here, and this comment says exactly where — an earlier
/// draft of it did not, and the adversarial review caught five documents
/// describing behaviour the code did not have.** What is sent is a
/// `session/update` **notification** telling the client the action was refused.
/// It is *not* a `session/request_permission` request: raising one would mean
/// waiting for the answer, and routing that answer back into this future is the
/// piece that is not wired. Sending a request nobody reads the reply to would be
/// worse than not sending one — the client would render a prompt whose outcome
/// is ignored, which is a lie told in an interface rather than in prose.
///
/// [`permission_params`], [`PERMISSION_OPTIONS`] and [`permission_answer`] are
/// built and tested and have no production caller yet. They are kept rather than
/// deleted because the shape of the request is settled — three options, because
/// `io_harness::Decision::Deny` carries no rules — and the piece missing is the
/// correlation, not the design. **`tests/acp.rs` asserts they agree with each
/// other and cannot assert a caller exists**, which is the same gap that let
/// 0.30.0 ship `io skill`; the release record carries it as a known limitation.
///
/// Denying is the safe direction and it is what `io exec` already does: no
/// permission is granted that the operator did not grant, and a refusal an
/// operator can see beats one they cannot.
struct Consulting {
    session_id: String,
    outbound: tokio::sync::mpsc::UnboundedSender<Outgoing>,
    /// The run this is approving for, so the request lands on the right cell.
    run_id: i64,
    /// The step the run is on, written by [`Reporter`] as it observes.
    ///
    /// **Shared with the observer rather than owned here, and that is a fix.**
    /// It was an `AtomicU32` this struct constructed at zero and only ever read —
    /// never stored — so every request named cell `{run_id}-0` whatever step
    /// raised it. Through 0.37.0 that mis-addressed a failure notice; from
    /// 0.38.0 it would mis-address the dialog an operator acts on, attaching the
    /// question to a call that already finished.
    step: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Where the answer will come back. Shared with the read task, which is the
    /// only thing that can settle it.
    correlator: std::sync::Arc<Correlator>,
}

impl io_harness::Approver for Consulting {
    fn decide<'a>(
        &'a self,
        request: &'a io_harness::Request,
    ) -> io_harness::approve::DecisionFuture<'a> {
        let act_name = format!("{:?}", request.act).to_lowercase();
        let act = request.act;
        let target = request.target.clone();
        let session_id = self.session_id.clone();
        let outbound = self.outbound.clone();
        let correlator = std::sync::Arc::clone(&self.correlator);
        // **The id has to be the one the client already has.** An earlier draft
        // put the target *path* here, so the update named a cell that had never
        // been announced and a conforming client had nothing to apply it to —
        // which made the "a refusal an operator can see" claim above false. The
        // id is built the same way `acp_map::call_id` builds it, from the run and
        // the step, because that is what every `tool_call` frame carried.
        let id = format!(
            "{}-{}",
            self.run_id,
            self.step.load(std::sync::atomic::Ordering::Relaxed)
        );
        Box::pin(async move {
            ask_permission(
                &correlator,
                &outbound,
                &session_id,
                act,
                &act_name,
                &target,
                &id,
            )
            .await
        })
    }
}

/// Ask the client for one permission, and wait for the answer.
///
/// **A free function and not a method, so a test can assert what it returns.**
/// The criterion this satisfies is about the `Decision` the run receives, and
/// the run receives it from an `Approver` that needs a whole session to build.
/// 0.36.0 shipped the frame builders with no production caller and gates that
/// asserted the parts against each other; a test that watched a frame go out
/// would repeat that mistake one level up. This is the thing the decision comes
/// out of, and it is callable with a correlator and a channel.
///
/// **Every failure is a denial, and there is exactly one safe direction to be
/// wrong in here.** A closed writer means the question was never asked. A
/// dropped sender means the reader stopped with this request outstanding — a
/// disconnected client, which [`Correlator::abandon`] turns into a denial rather
/// than a hang, and which is why this adapter needs no approval timeout. A
/// client that answers with a JSON-RPC error has reported a protocol failure,
/// which must not be read as a decision about a permission.
pub async fn ask_permission(
    correlator: &Correlator,
    outbound: &tokio::sync::mpsc::UnboundedSender<Outgoing>,
    session_id: &str,
    act: io_harness::Act,
    act_name: &str,
    target: &str,
    tool_call_id: &str,
) -> io_harness::Decision {
    let (request_id, answer) = correlator.issue();
    if outbound
        .send(Outgoing::Request {
            id: json!(request_id),
            method: "session/request_permission".into(),
            params: permission_params(session_id, act_name, target, tool_call_id),
        })
        .is_err()
    {
        return io_harness::Decision::deny(NOT_ROUTED);
    }

    match answer.await {
        Ok(Ok(result)) => crate::approval::decision(permission_answer(&result), act, target),
        Ok(Err(_)) | Err(_) => io_harness::Decision::deny(NOT_ROUTED),
    }
}

/// What the model is told when an ACP approval could not be routed.
///
/// Deliberately **not** `approval::REFUSED_BY_OPERATOR`. That sentence says the
/// operator denied it, and every branch that reaches this one is a branch where
/// nobody answered: the writer was gone before the question was sent, the reader
/// stopped with the question outstanding, or the client replied with a protocol
/// error rather than a decision. Telling the model a human refused would put a
/// false statement about a person into the transcript the agent then reasons
/// from.
///
/// **An operator who was asked and said no gets `REFUSED_BY_OPERATOR`**, from
/// `approval::decision`, because that is what happened. Since 0.38.0 that is the
/// ordinary path and this constant is the exception; through 0.37.0 it was the
/// only path there was.
pub const NOT_ROUTED: &str = "this interface could not route the approval, so it was refused";

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
