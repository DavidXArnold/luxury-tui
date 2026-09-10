use anyhow::Result;
use kube::Client;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::k8s::{
    context::KubeContext, diff::DiffLine, nodes::DrainOutcome, objectmap::MapNode, resources::*,
};

pub enum AppEvent {
    Term(crossterm::event::Event),
    Tick,

    ContextsLoaded(Result<Vec<KubeContext>>),
    ClientReady(String, Client),
    ClientFailed(String),

    NamespacesLoaded(Result<Vec<String>>),
    PodsLoaded(Result<Vec<PodSummary>>),
    NodesLoaded(Result<Vec<NodeSummary>>),
    WorkloadsLoaded(&'static str, Result<Vec<WorkloadSummary>>),
    EventsLoaded(Result<Vec<EventSummary>>),

    LogLine(Result<String>),
    LogStreamEnded,

    ObjectMapLoaded(Result<MapNode>),
    DiffReady(Result<Vec<DiffLine>>),

    DrainFinished(Result<DrainOutcome>),
    ActionResult(Result<String>),
    PortForwardStarted(Result<String>),
    /// (namespace, pod, debug container name) once the ephemeral container
    /// is attached and ready to exec into.
    DebugContainerReady(Result<(String, String, String)>),
}

pub type EventSender = UnboundedSender<AppEvent>;
pub type EventReceiver = UnboundedReceiver<AppEvent>;

pub fn channel() -> (EventSender, EventReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}

/// Bridges crossterm's terminal input into the same event channel the async
/// k8s tasks report back on, so the main loop only needs one `select!`.
///
/// Returns the task's handle so the caller can `abort()` it before anything
/// else reads stdin directly — an interactive exec session, for one. Two
/// readers racing the same fd split keystrokes between them unpredictably,
/// and whatever this one grabs sits queued for the main loop to fire off,
/// all at once, the moment it gets control back. Always stop this first,
/// then start a fresh one afterward with `spawn_input_forwarder` again.
pub fn spawn_input_forwarder(tx: EventSender) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use crossterm::event::EventStream;
        use futures::StreamExt;
        let mut stream = EventStream::new();
        while let Some(Ok(evt)) = stream.next().await {
            if tx.send(AppEvent::Term(evt)).is_err() {
                break;
            }
        }
    })
}

pub fn spawn_ticker(tx: EventSender, period: std::time::Duration) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}
