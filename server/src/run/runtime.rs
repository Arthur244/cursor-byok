use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    model::{CanonicalMessage, ConversationId, PreparedRun, RunId},
    provider::Provider,
    store::Store,
};

use super::{ClientCommand, ClientPort, MessageInsertion, RunEngine};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunFailure {
    Protocol(String),
    Provider(String),
    Store(String),
    Client(String),
}

impl RunFailure {
    pub fn category(&self) -> &'static str {
        match self {
            Self::Protocol(_) => "protocol",
            Self::Provider(_) => "provider",
            Self::Store(_) => "store",
            Self::Client(_) => "client",
        }
    }
}

impl From<crate::Error> for RunFailure {
    fn from(error: crate::Error) -> Self {
        use crate::Error;
        match error {
            Error::Protocol(message) | Error::Config(message) => Self::Protocol(message),
            Error::Provider(message) => Self::Provider(message),
            Error::Store(message) => Self::Store(message),
            Error::Cancelled => Self::Client("run was cancelled".into()),
            Error::Http(error) => Self::Provider(error.to_string()),
            Error::Database(error) => Self::Store(error.to_string()),
            Error::Migration(error) => Self::Store(error.to_string()),
            Error::Io(error) => Self::Store(error.to_string()),
            Error::Decode(error) => Self::Protocol(error.to_string()),
            Error::Encode(error) => Self::Protocol(error.to_string()),
            Error::Json(error) => Self::Protocol(error.to_string()),
            Error::RunNotFound(run_id) => Self::Store(format!("run not found: {run_id}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Cancelled,
    Failed(RunFailure),
}

#[derive(Clone, Default)]
pub struct RunRegistry {
    active: Arc<Mutex<HashMap<ConversationId, ActiveRun>>>,
}

struct ActiveRun {
    run_id: RunId,
    cancellation: CancellationToken,
    commands: tokio::sync::mpsc::Sender<ClientCommand>,
}

impl RunRegistry {
    pub async fn activate(
        &self,
        conversation_id: ConversationId,
        run_id: RunId,
        cancellation: CancellationToken,
        commands: tokio::sync::mpsc::Sender<ClientCommand>,
    ) {
        let previous = self.active.lock().await.insert(
            conversation_id,
            ActiveRun {
                run_id: run_id.clone(),
                cancellation,
                commands,
            },
        );
        if let Some(previous) = previous.filter(|previous| previous.run_id != run_id) {
            previous.cancellation.cancel();
        }
    }

    pub async fn insert_messages(
        &self,
        conversation_id: &ConversationId,
        messages: Vec<CanonicalMessage>,
    ) -> bool {
        if messages.is_empty() {
            return true;
        }
        let commands = self
            .active
            .lock()
            .await
            .get(conversation_id)
            .map(|run| run.commands.clone());
        let Some(commands) = commands else {
            return false;
        };
        let (delivered, delivery) = tokio::sync::oneshot::channel();
        if commands
            .send(ClientCommand::InsertMessages(MessageInsertion {
                messages,
                delivered,
            }))
            .await
            .is_err()
        {
            return false;
        }
        delivery.await.is_ok()
    }

    pub async fn release(&self, conversation_id: &ConversationId, run_id: &RunId) {
        let mut active = self.active.lock().await;
        if active
            .get(conversation_id)
            .is_some_and(|current| &current.run_id == run_id)
        {
            active.remove(conversation_id);
        }
    }

    pub async fn shutdown(&self) {
        let active = std::mem::take(&mut *self.active.lock().await);
        for run in active.into_values() {
            run.cancellation.cancel();
        }
    }
}

#[derive(Clone)]
pub struct RunActor {
    store: Store,
    provider: Arc<dyn Provider>,
    registry: RunRegistry,
}

impl RunActor {
    pub fn new(store: Store, provider: Arc<dyn Provider>, registry: RunRegistry) -> Self {
        Self {
            store,
            provider,
            registry,
        }
    }

    pub async fn spawn(
        &self,
        prepared: PreparedRun,
        client: ClientPort,
        commands: tokio::sync::mpsc::Sender<ClientCommand>,
        cancellation: CancellationToken,
    ) -> tokio::task::JoinHandle<RunOutcome> {
        let run_id = prepared.run_id.clone();
        let conversation_id = prepared.conversation_id.clone();
        self.registry
            .activate(
                conversation_id.clone(),
                run_id.clone(),
                cancellation.clone(),
                commands,
            )
            .await;
        let actor = self.clone();
        tokio::spawn(async move {
            let outcome = RunEngine::new(actor.store, actor.provider)
                .run(prepared, client, cancellation)
                .await;
            actor.registry.release(&conversation_id, &run_id).await;
            outcome
        })
    }
}
