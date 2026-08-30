//! Builds deterministic prompt state derived from Conversation context.
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{CanonicalMessage, MessageContent};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct DerivedState {
    pub todos: Option<Value>,
    pub plan: Option<Value>,
}

pub fn fold_derived_state(messages: &[CanonicalMessage]) -> DerivedState {
    let mut state = DerivedState::default();
    let mut calls = std::collections::HashMap::<String, (String, Value)>::new();
    for message in messages {
        match &message.content {
            MessageContent::Assistant { tool_calls, .. } => {
                for call in tool_calls {
                    calls.insert(
                        call.call_id.clone(),
                        (call.name.clone(), call.arguments.clone()),
                    );
                }
            }
            MessageContent::ToolResult(result) if !result.is_error => {
                let Some((name, input)) = calls.get(&result.call_id).cloned() else {
                    continue;
                };
                match normalize(&name).as_str() {
                    "todowrite" | "updatetodos" => {
                        state.todos = Some(apply_todo_write(state.todos.take(), input));
                    }
                    "createplan" | "updateplan" | "writeplan" => state.plan = Some(input),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    state
}

fn apply_todo_write(current: Option<Value>, mut input: Value) -> Value {
    if !input.get("merge").and_then(Value::as_bool).unwrap_or(false) {
        return input;
    }
    let mut todos = current
        .as_ref()
        .and_then(|value| value.get("todos"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let patches = input
        .get("todos")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for patch in patches {
        let existing = patch.get("id").and_then(Value::as_str).and_then(|id| {
            todos
                .iter_mut()
                .find(|todo| todo.get("id").and_then(Value::as_str) == Some(id))
        });
        match (existing, patch) {
            (Some(Value::Object(todo)), Value::Object(patch)) => todo.extend(patch),
            (_, patch) => todos.push(patch),
        }
    }
    if let Some(object) = input.as_object_mut() {
        object.insert("merge".into(), Value::Bool(false));
        object.insert("todos".into(), Value::Array(todos));
    }
    input
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
