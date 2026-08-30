use super::*;
use serde_json::json;

fn edit_call(index: usize, call_id: &str, path: &str, old: &str, new: &str) -> ToolCall {
    ToolCall {
        index,
        call_id: call_id.into(),
        model_call_id: "model:0".into(),
        name: "StrReplace".into(),
        arguments_text: String::new(),
        arguments: json!({
            "path": path,
            "old_string": old,
            "new_string": new,
        }),
    }
}

#[tokio::test]
async fn same_path_edits_start_one_at_a_time() {
    let runtime = CursorToolRuntime::default();
    let dispatcher = ToolDispatcher::new(runtime.clone());
    let calls = [
        edit_call(0, "first", "/tmp/a.txt", "left", "LEFT"),
        edit_call(1, "second", "/tmp/a.txt", "right", "RIGHT"),
        edit_call(2, "other", "/tmp/b.txt", "other", "OTHER"),
    ];

    let dispatched = dispatcher
        .start_batch(
            &calls,
            ToolBatchState {
                completed: &HashSet::new(),
                started: &HashSet::new(),
                response_text: "",
                response_thinking: "",
            },
            &[],
            &BTreeMap::new(),
            &ExecContext::default(),
        )
        .await
        .unwrap();

    assert_eq!(dispatched.len(), 2);
    assert_eq!(exec(&dispatched[0]).exec_id, "first");
    assert_eq!(exec(&dispatched[1]).exec_id, "other");

    let mut file = "left right\n".to_string();
    let first_write = advance_read(&runtime, exec(&dispatched[0]).id, &file).await;
    file = write_text(&first_write);
    assert_eq!(file, "LEFT right\n");
    complete_write(&runtime, &first_write).await;

    let second = dispatcher
        .continue_after("first")
        .await
        .unwrap()
        .expect("second same-path edit should start after the first completes");
    assert_eq!(exec(&second).exec_id, "second");
    let second_write = advance_read(&runtime, exec(&second).id, &file).await;
    file = write_text(&second_write);
    assert_eq!(file, "LEFT RIGHT\n");
    complete_write(&runtime, &second_write).await;
    assert!(dispatcher.continue_after("second").await.unwrap().is_none());
}

fn exec(dispatched: &DispatchedTool) -> &pb::ExecServerMessage {
    dispatched
        .messages
        .iter()
        .find_map(|message| match message.message.as_ref() {
            Some(pb::agent_server_message::Message::ExecServerMessage(exec)) => Some(exec),
            _ => None,
        })
        .expect("dispatched edit should contain an Exec request")
}

async fn advance_read(
    runtime: &CursorToolRuntime,
    id: u32,
    content: &str,
) -> pb::ExecServerMessage {
    let event = codec::client_event(
        &pb::ExecClientMessage {
            id,
            message: Some(pb::exec_client_message::Message::ReadResult(
                pb::ReadResult {
                    result: Some(pb::read_result::Result::Success(pb::ReadSuccess {
                        output: Some(pb::read_success::Output::Content(content.into())),
                        ..Default::default()
                    })),
                },
            )),
            ..Default::default()
        },
        runtime,
    )
    .await
    .unwrap();
    let codec::ClientExecEvent::Message(message) = event else {
        panic!("edit read should advance to a write")
    };
    let Some(pb::agent_server_message::Message::ExecServerMessage(exec)) = message.message else {
        panic!("edit read should emit an Exec write request")
    };
    exec
}

fn write_text(exec: &pb::ExecServerMessage) -> String {
    let Some(pb::exec_server_message::Message::WriteArgs(args)) = exec.message.as_ref() else {
        panic!("expected WriteArgs")
    };
    args.file_text.clone()
}

async fn complete_write(runtime: &CursorToolRuntime, exec: &pb::ExecServerMessage) {
    let Some(pb::exec_server_message::Message::WriteArgs(args)) = exec.message.as_ref() else {
        panic!("expected WriteArgs")
    };
    let event = codec::client_event(
        &pb::ExecClientMessage {
            id: exec.id,
            message: Some(pb::exec_client_message::Message::WriteResult(
                pb::WriteResult {
                    result: Some(pb::write_result::Result::Success(pb::WriteSuccess {
                        path: args.path.clone(),
                        ..Default::default()
                    })),
                },
            )),
            ..Default::default()
        },
        runtime,
    )
    .await
    .unwrap();
    assert!(matches!(event, codec::ClientExecEvent::Completed(_)));
}
