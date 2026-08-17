//! The two-way seam between io-harness and the operator.
//!
//! [`crate::bridge`] is the other seam and the easy one: an observer is handed an
//! event and hands it on, and nothing waits for the interface. This one is the
//! opposite shape. `Approver::decide_in_context` runs **on the agent's own task**
//! and the run stays paused until the future it returns resolves, so this module
//! is the only place in the product where the interface can stop the agent.
//!
//! Two consequences shape everything below.
//!
//! **A question that is never answered must deny.** Both ends can vanish — the
//! whole interface (the mpsc closes) or one question it took and abandoned (the
//! oneshot closes) — and both mean the same thing to a run that cannot proceed
//! without an answer. A blocked turn is worse than a refused one: a refusal
//! reaches the model as an observation it can adapt to, and a block reaches
//! nobody. F4 asserts it on a closed channel rather than on a timeout, because a
//! deadlock asserted with a clock is a test that passes on a fast machine.
//!
//! **The rule and the layer only exist here.** `EventKind::ApprovalRequested`
//! carries the act and the target and nothing else; the glob that put the action
//! in the grey tier, the layer that glob came from, and the content a write would
//! leave behind arrive as the [`Request`] and [`ApprovalContext`] handed to this
//! trait. So the approval overlay is drawn from what this module forwards, and
//! never from the event stream.

use io_harness::approve::DecisionFuture;
use io_harness::{Act, ApprovalContext, Approver, Decision, Request};
use tokio::sync::{mpsc, oneshot};

/// What the model is told when nobody answered.
///
/// It reaches the model as an observation rather than as an error, which is the
/// point: a run told this can do something else, and a run left waiting cannot.
pub const UNANSWERED: &str = "nobody was there to approve it";

/// One question, on its way to the interface, with the way back inside it.
///
/// Answering consumes it. There is no way to hold an `Ask` and answer it twice,
/// and dropping one is a denial rather than a leak — which is the behaviour F4
/// asks for, expressed as a type rather than as a rule somebody has to remember.
pub struct Ask {
    request: Request,
    context: ApprovalContext,
    answer: oneshot::Sender<Decision>,
}

impl Ask {
    /// Answer it. The run resumes on whatever this says.
    pub fn answer(self, decision: Decision) {
        // A send error means the run ended while the question was on screen — an
        // interrupt, or a ceiling reached elsewhere. There is nobody left to tell,
        // and that is not a failure of the interface.
        let _ = self.answer.send(decision);
    }

    /// What kind of action is being asked about.
    pub fn act(&self) -> Act {
        self.request.act
    }

    /// The path, or the binary name for an exec.
    pub fn target(&self) -> &str {
        &self.request.target
    }

    /// What a write would leave behind, whole. The harness hands an approver the
    /// resulting file rather than a patch, so anything diff-shaped is this
    /// product's to compute — and is 0.3.0's, not this release's.
    pub fn content(&self) -> Option<&str> {
        self.request.content.as_deref()
    }

    /// The glob that put this action in the grey tier, or `None` when the tier
    /// default did.
    ///
    /// `None` is not "no reason". io-harness's own documentation is explicit that
    /// an unnamed action in the grey tier is the *least* vouched-for kind, so a
    /// surface that renders `None` as blank tells the reader the opposite of what
    /// happened. F8 asserts both cases.
    pub fn rule(&self) -> Option<&str> {
        self.context.rule.as_deref()
    }

    /// The policy layer the deciding rule came from, or `None` for the tier
    /// default. Layers are named after whoever wrote them, so this is the field
    /// that sends a reader to the right configuration file.
    pub fn layer(&self) -> Option<&str> {
        self.context.layer.as_deref()
    }

    /// The run's goal, in the words the operator typed.
    pub fn goal(&self) -> &str {
        &self.context.goal
    }
}

/// The approver handed to `Session::turn_steered`.
pub struct Asker {
    asks: mpsc::UnboundedSender<Ask>,
}

/// An asker and the receiver the interface drains.
///
/// Unbounded for the same reason [`crate::bridge`]'s channel is: the alternatives
/// are blocking the run and dropping a question, and a dropped question is a turn
/// that waits forever. In practice the depth is one — the run is paused from the
/// moment it asks until the moment it is answered, so a second question cannot
/// arrive from the same run while the first is outstanding.
pub fn channel() -> (Asker, mpsc::UnboundedReceiver<Ask>) {
    let (asks, rx) = mpsc::unbounded_channel();
    (Asker { asks }, rx)
}

impl Asker {
    async fn ask(&self, request: Request, context: ApprovalContext) -> Decision {
        let (answer, reply) = oneshot::channel();
        let ask = Ask {
            request,
            context,
            answer,
        };
        // One path for both ways this can fail, deliberately. A failed `send`
        // returns the `Ask` and drops it, which closes the oneshot inside it — so
        // "the interface is gone" and "the interface took the question and went
        // away" arrive here as the same closed channel. An early return for the
        // first was written, sabotaged, and found to fail no test at all: it was a
        // second spelling of this line.
        let _ = self.asks.send(ask);
        reply.await.unwrap_or_else(|_| Decision::deny(UNANSWERED))
    }
}

impl Approver for Asker {
    fn decide<'a>(&'a self, request: &'a Request) -> DecisionFuture<'a> {
        // The harness calls `decide_in_context`; this exists because the trait
        // requires it, and it must still ask rather than answer on its own — an
        // approver with two behaviours is one that ships the wrong one.
        let request = request.clone();
        Box::pin(async move { self.ask(request, ApprovalContext::default()).await })
    }

    fn decide_in_context<'a>(
        &'a self,
        request: &'a Request,
        context: &'a ApprovalContext,
    ) -> DecisionFuture<'a> {
        let request = request.clone();
        let context = context.clone();
        Box::pin(async move { self.ask(request, context).await })
    }
}
