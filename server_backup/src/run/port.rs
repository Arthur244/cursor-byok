use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

use crate::model::{
    CanonicalMessage, RevisionId, RuntimeEvent, ToolCall, ToolResult, ToolRoundId, Usage,
};

use super::RunOutcome;

#[derive(Debug)]
pub struct MessageInsertion {
    pub messages: Vec<CanonicalMessage>,
    pub delivered: oneshot::Sender<()>,
}

#[derive(Debug)]
pub enum ClientCommand {
    ToolResult(ToolResult),
    InterruptWithMessage(CanonicalMessage),
    RuntimeEvent(RuntimeEvent),
    InsertMessages(MessageInsertion),
    ClientClosed { error: String },
    Cancel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitCause {
    InitialMessages,
    ToolRoundStarted(ToolRoundId),
    ToolResult { call_id: String, interrupted: bool },
    FinalTurn,
    Compaction { summary: String },
    RuntimeEvent { event_id: String },
}

#[derive(Debug)]
pub enum CommitBarrier {
    None,
    BeforeContinue(oneshot::Sender<std::result::Result<(), String>>),
}

impl CommitBarrier {
    pub fn before_continue() -> (Self, oneshot::Receiver<std::result::Result<(), String>>) {
        let (sender, receiver) = oneshot::channel();
        (Self::BeforeContinue(sender), receiver)
    }

    pub fn is_required(&self) -> bool {
        matches!(self, Self::BeforeContinue(_))
    }

    pub fn complete(self, result: std::result::Result<(), String>) {
        if let Self::BeforeContinue(sender) = self {
            let _ = sender.send(result);
        }
    }
}

#[derive(Debug)]
pub struct StateCommitted {
    pub revision_id: RevisionId,
    pub tool_round_version: u64,
    pub cause: CommitCause,
    pub barrier: CommitBarrier,
}

#[derive(Debug)]
pub enum ClientEvent {
    AutoCompactionStarted,
    AutoCompactionCompleted,
    TextStart,
    TextDelta(String),
    TextEnd,
    ThinkingStart,
    ThinkingDelta(String),
    ThinkingEnd {
        duration: Duration,
    },
    ToolCallStart {
        index: usize,
        call_id: String,
        name: String,
        model_call_id: String,
    },
    ToolCallArgumentsDelta {
        index: usize,
        delta: String,
    },
    ToolCallEnd {
        index: usize,
    },
    Usage(Usage),
    ExecuteToolRound {
        round_id: ToolRoundId,
        calls: Vec<ToolCall>,
    },
    StateCommitted(StateCommitted),
    Ended(RunOutcome),
}

pub struct ClientPort {
    pub commands: mpsc::Receiver<ClientCommand>,
    pub events: mpsc::Sender<ClientEvent>,
}

pub struct ClientSession {
    pub commands: mpsc::Sender<ClientCommand>,
    pub events: mpsc::Receiver<ClientEvent>,
}

pub fn session(capacity: usize) -> (ClientPort, ClientSession) {
    let (commands_tx, commands_rx) = mpsc::channel(capacity);
    let (events_tx, events_rx) = mpsc::channel(capacity);
    (
        ClientPort {
            commands: commands_rx,
            events: events_tx,
        },
        ClientSession {
            commands: commands_tx,
            events: events_rx,
        },
    )
}
