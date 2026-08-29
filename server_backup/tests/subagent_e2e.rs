#[path = "support/fake_provider.rs"]
mod fake_provider;
#[path = "support/fixtures.rs"]
mod fixtures;

use std::{sync::Arc, time::Duration};

use cursor_server::{
    cursor::{
        connect,
        prompting::{PromptAssets, PromptCompiler},
        proto::agent::v1 as pb,
        CursorCommand, CursorSessionHandle, CursorSessionRegistry,
    },
    provider::{FinishReason, ModelEvent},
};
use prost::Message;

#[tokio::test]
async fn every_bidi_run_resolves_and_persists_its_own_subagent_model_and_background_state() {
    let (_directory, store) = fixtures::temp_store().await;
    let provider = fake_provider::FakeProvider::default();
    for suffix in ["a", "b"] {
        provider.push(task_response(suffix));
        provider.push(stop_response(suffix));
    }
    let assets = PromptAssets::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("prompt/cursor")
            .as_path(),
    )
    .unwrap();
    let registry = CursorSessionRegistry::new(
        store,
        Arc::new(provider),
        PromptCompiler::new(assets),
        Default::default(),
    );

    let first = registry.get_or_create("subagent-run-a").await.unwrap();
    let first_checkpoint = drive(
        &first,
        run_request("subagent-run-a", "user-a", "model-a", None),
        "model-a",
        "child-a",
    )
    .await;

    let second = registry.get_or_create("subagent-run-b").await.unwrap();
    drive(
        &second,
        run_request(
            "subagent-run-b",
            "user-b",
            "model-b",
            Some(first_checkpoint),
        ),
        "model-b",
        "child-b",
    )
    .await;
}

async fn drive(
    handle: &CursorSessionHandle,
    request: pb::AgentClientMessage,
    expected_model: &str,
    child_id: &str,
) -> pb::ConversationStateStructure {
    let mut output = handle.subscribe();
    handle
        .command(CursorCommand::Append {
            seqno: 0,
            message: Box::new(request),
        })
        .await
        .unwrap();

    let mut seqno = 1;
    let mut saw_started = false;
    let mut saw_exec = false;
    let mut saw_completed = false;
    let mut checkpoint = None;
    loop {
        let frame = tokio::time::timeout(Duration::from_secs(5), output.recv())
            .await
            .unwrap()
            .expect("RunSSE closed before EndStream");
        let (flags, payload) = connect::decode_frames(&frame).unwrap().pop().unwrap();
        if flags & connect::END_STREAM_FLAG != 0 {
            break;
        }
        let server = pb::AgentServerMessage::decode(payload).unwrap();
        match server.message {
            Some(pb::agent_server_message::Message::KvServerMessage(kv)) => {
                handle
                    .command(CursorCommand::Append {
                        seqno,
                        message: Box::new(kv_ack(kv.id)),
                    })
                    .await
                    .unwrap();
                seqno += 1;
            }
            Some(pb::agent_server_message::Message::ExecServerMessage(exec)) => {
                let Some(pb::exec_server_message::Message::SubagentArgs(args)) = exec.message
                else {
                    continue;
                };
                assert_eq!(args.model_id, expected_model);
                assert_eq!(args.run_in_background, Some(true));
                saw_exec = true;
                handle
                    .command(CursorCommand::Append {
                        seqno,
                        message: Box::new(subagent_result(exec.id, child_id)),
                    })
                    .await
                    .unwrap();
                seqno += 1;
            }
            Some(pb::agent_server_message::Message::InteractionUpdate(update)) => {
                match update.message {
                    Some(pb::interaction_update::Message::ToolCallStarted(started)) => {
                        let task = task(started.tool_call.as_ref().unwrap());
                        assert_eq!(
                            task.args.as_ref().unwrap().model.as_deref(),
                            Some(expected_model)
                        );
                        saw_started = true;
                    }
                    Some(pb::interaction_update::Message::ToolCallCompleted(completed)) => {
                        let task = task(completed.tool_call.as_ref().unwrap());
                        let Some(pb::task_result::Result::Success(success)) = task
                            .result
                            .as_ref()
                            .and_then(|result| result.result.as_ref())
                        else {
                            panic!("expected Task success")
                        };
                        assert_eq!(
                            task.args.as_ref().unwrap().model.as_deref(),
                            Some(expected_model)
                        );
                        assert!(success.is_background);
                        assert_eq!(success.agent_id.as_deref(), Some(child_id));
                        saw_completed = true;
                    }
                    _ => {}
                }
            }
            Some(pb::agent_server_message::Message::ConversationCheckpointUpdate(state))
                if state.pending_tool_calls.is_empty() =>
            {
                checkpoint = Some(state);
            }
            _ => {}
        }
    }

    assert!(saw_started && saw_exec && saw_completed);
    let checkpoint = checkpoint.expect("settled checkpoint");
    let state = checkpoint
        .subagent_states
        .get(child_id)
        .expect("background subagent persisted state");
    assert_eq!(state.model_id.as_deref(), Some(expected_model));
    let run = checkpoint
        .subagent_runs_by_parent_tool_call_id
        .get(&format!("task-{child_id}"))
        .expect("background subagent run state");
    assert_eq!(run.status, pb::SubagentRunStatus::Backgrounded as i32);
    checkpoint
}

fn task(call: &pb::ToolCall) -> &pb::TaskToolCall {
    let Some(pb::tool_call::Tool::TaskToolCall(task)) = call.tool.as_ref() else {
        panic!("expected TaskToolCall")
    };
    task
}

fn run_request(
    request_id: &str,
    user_id: &str,
    subagent_model: &str,
    conversation_state: Option<pb::ConversationStateStructure>,
) -> pb::AgentClientMessage {
    pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::RunRequest(
            pb::AgentRunRequest {
                action: Some(pb::ConversationAction {
                    action: Some(pb::conversation_action::Action::UserMessageAction(
                        pb::UserMessageAction {
                            user_message: Some(pb::UserMessage {
                                text: format!("start {request_id}"),
                                message_id: user_id.into(),
                                mode: pb::AgentMode::Multitask as i32,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    )),
                    ..Default::default()
                }),
                conversation_id: Some("subagent-e2e-conversation".into()),
                run_id: Some(request_id.into()),
                requested_model: Some(pb::RequestedModel {
                    model_id: "parent-model".into(),
                    ..Default::default()
                }),
                conversation_state,
                subagent_model_overrides: vec![pb::SubagentModelOverride {
                    subagent_type: "generalPurpose".into(),
                    selection: Some(pb::subagent_model_override::Selection::Model(
                        pb::RequestedModel {
                            model_id: subagent_model.into(),
                            ..Default::default()
                        },
                    )),
                }],
                ..Default::default()
            },
        )),
    }
}

fn task_response(suffix: &str) -> Vec<ModelEvent> {
    let child_id = format!("child-{suffix}");
    let arguments = serde_json::json!({
        "description": format!("background {suffix}"),
        "prompt": "inspect",
        "subagent_type": "generalPurpose",
        "run_in_background": true
    })
    .to_string();
    vec![
        ModelEvent::Start {
            model_call_id: format!("model-call-{suffix}"),
        },
        ModelEvent::ToolCallStart {
            index: 0,
            call_id: format!("task-{child_id}"),
            name: "Task".into(),
        },
        ModelEvent::ToolCallArgumentsDelta {
            index: 0,
            delta: arguments,
        },
        ModelEvent::ToolCallEnd { index: 0 },
        ModelEvent::Done(FinishReason::ToolUse),
    ]
}

fn stop_response(suffix: &str) -> Vec<ModelEvent> {
    vec![
        ModelEvent::Start {
            model_call_id: format!("final-{suffix}"),
        },
        ModelEvent::TextStart,
        ModelEvent::TextDelta("background task started".into()),
        ModelEvent::TextEnd,
        ModelEvent::Done(FinishReason::Stop),
    ]
}

fn subagent_result(id: u32, child_id: &str) -> pb::AgentClientMessage {
    pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::ExecClientMessage(
            pb::ExecClientMessage {
                id,
                message: Some(pb::exec_client_message::Message::SubagentResult(
                    pb::SubagentResult {
                        result: Some(pb::subagent_result::Result::Success(pb::SubagentSuccess {
                            agent_id: child_id.into(),
                            final_message: Some("running in background".into()),
                            background_reason: pb::SubagentBackgroundReason::AgentRequest as i32,
                            ..Default::default()
                        })),
                    },
                )),
                ..Default::default()
            },
        )),
    }
}

fn kv_ack(id: u32) -> pb::AgentClientMessage {
    pb::AgentClientMessage {
        message: Some(pb::agent_client_message::Message::KvClientMessage(
            pb::KvClientMessage {
                id,
                message: Some(pb::kv_client_message::Message::SetBlobResult(
                    pb::SetBlobResult { error: None },
                )),
            },
        )),
    }
}
