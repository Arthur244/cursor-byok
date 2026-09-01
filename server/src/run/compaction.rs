//! Decides when to compact provider-visible context and builds a stable fallback summary.

use std::collections::HashSet;

use crate::model::{estimate_context_tokens, CanonicalMessage, PreparedRun, ProjectedMessage};

const FALLBACK_CHARS: usize = 12_000;

pub(super) const RESERVE_TOKENS: u64 = 10_000;
pub(super) const OUTPUT_TOKENS: u64 = 4_096;
pub(super) const INSTRUCTIONS: &str = "Summarize the conversation for the next model turn. Preserve goals, constraints, decisions, files, commands, errors, results, and unfinished work. Do not call tools. Return only the concise durable summary.";

pub(super) fn input_budget(prepared: &PreparedRun) -> Option<u64> {
    prepared
        .model
        .context_window_tokens
        .map(|window| window.saturating_sub(RESERVE_TOKENS))
}

pub(super) fn estimated_tokens(
    prepared: &PreparedRun,
    projected_messages: &[ProjectedMessage],
) -> u64 {
    estimate_context_tokens(&prepared.prompt, projected_messages)
}

pub(super) fn should_compact(
    prepared: &PreparedRun,
    projected_messages: &[ProjectedMessage],
) -> bool {
    let Some(budget) = input_budget(prepared) else {
        return false;
    };
    estimated_tokens(prepared, projected_messages) > budget
}

pub(super) fn validate_compacted(
    prepared: &PreparedRun,
    projected_messages: &[ProjectedMessage],
) -> std::result::Result<u64, String> {
    let estimated = estimated_tokens(prepared, projected_messages);
    let Some(budget) = input_budget(prepared) else {
        return Ok(estimated);
    };
    if estimated <= budget {
        return Ok(estimated);
    }
    Err(format!(
        "context overflow after compaction: estimated input {estimated} tokens exceeds budget {budget} tokens"
    ))
}

pub(super) fn partition(
    messages: &[CanonicalMessage],
    current_ids: &HashSet<&str>,
) -> (Vec<CanonicalMessage>, Option<CanonicalMessage>) {
    let latest_request_context = messages
        .iter()
        .rposition(|message| message.message_id.starts_with("request-context:"));
    let compactable = messages
        .iter()
        .enumerate()
        .filter(|(index, message)| {
            Some(*index) != latest_request_context
                && !current_ids.contains(message.message_id.as_str())
        })
        .map(|(_, message)| message.clone())
        .collect();
    let retained = latest_request_context
        .and_then(|index| messages.get(index))
        .filter(|message| !current_ids.contains(message.message_id.as_str()))
        .cloned();
    (compactable, retained)
}

pub(super) fn fallback_summary(messages: &[CanonicalMessage]) -> String {
    let serialized = serde_json::to_string(messages).unwrap_or_default();
    let start = serialized
        .char_indices()
        .rev()
        .nth(FALLBACK_CHARS.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    format!(
        "Durable recent conversation state:\n{}",
        &serialized[start..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        project_messages, CheckpointId, ConversationId, ModelSpec, Origin, PromptSpec, Role,
        RunAction, RunId, RunKind,
    };

    fn prepared(context_window_tokens: u64) -> PreparedRun {
        let mut model = ModelSpec::new("model");
        model.context_window_tokens = Some(context_window_tokens);
        PreparedRun {
            run_id: RunId::new("run"),
            cursor_request_id: None,
            conversation_id: ConversationId::new("conversation"),
            kind: RunKind::Root,
            model,
            prompt: PromptSpec {
                instructions: String::new(),
                tools: Vec::new(),
            },
            initial_messages: Vec::new(),
            action: RunAction::Start,
            base_checkpoint_id: CheckpointId(1),
        }
    }

    #[test]
    fn automatic_compaction_uses_fixed_reserve_for_every_action() {
        let messages = vec![CanonicalMessage::text(
            "user",
            Role::User,
            Origin::Runtime,
            "x".repeat(40_000),
        )];
        let projected = project_messages(&messages).unwrap();
        let estimated = estimate_context_tokens(&prepared(1).prompt, &projected);
        let mut prepared = prepared(estimated + RESERVE_TOKENS);

        assert!(!should_compact(&prepared, &projected));
        prepared.model.context_window_tokens = Some(estimated + RESERVE_TOKENS - 1);
        assert!(should_compact(&prepared, &projected));

        prepared.action = RunAction::Resume {
            pending_tool_round: None,
        };
        assert!(should_compact(&prepared, &projected));
    }

    #[test]
    fn compacted_history_is_validated_against_the_same_budget() {
        let messages = vec![CanonicalMessage::text(
            "user",
            Role::User,
            Origin::Runtime,
            "x".repeat(40_000),
        )];
        let projected = project_messages(&messages).unwrap();
        let estimated = estimate_context_tokens(&prepared(1).prompt, &projected);

        assert_eq!(
            validate_compacted(&prepared(estimated + RESERVE_TOKENS), &projected),
            Ok(estimated)
        );
        assert!(
            validate_compacted(&prepared(estimated + RESERVE_TOKENS - 1), &projected)
                .unwrap_err()
                .contains("context overflow after compaction")
        );
    }
}
