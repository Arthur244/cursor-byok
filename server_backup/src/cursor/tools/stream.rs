use crate::{
    cursor::{
        interaction,
        json_stream::{JsonStringFields, StringFieldEvent},
        proto::agent::v1 as pb,
    },
    model::ToolCall,
    Result,
};

pub struct ToolCallStream {
    presentation: Presentation,
}

enum Presentation {
    Plain,
    DynamicMcp(pb::McpToolDefinition),
    Edit(EditProjection),
    CreatePlan(CreatePlanProjection),
}

struct EditProjection {
    fields: JsonStringFields,
    path_field: &'static str,
    content_field: &'static str,
    path: String,
    content: NewlineStream,
}

#[derive(Default)]
struct CreatePlanProjection {
    fields: JsonStringFields,
    name: String,
    plan: String,
    overview: String,
}

impl ToolCallStream {
    pub fn new(name: &str, dynamic_mcp: Option<&pb::McpToolDefinition>) -> Self {
        let presentation = match dynamic_mcp {
            Some(definition) => Presentation::DynamicMcp(definition.clone()),
            None => match normalized(name).as_str() {
                "write" => Presentation::Edit(EditProjection::new("path", "contents")),
                "strreplace" => Presentation::Edit(EditProjection::new("path", "new_string")),
                "editnotebook" => {
                    Presentation::Edit(EditProjection::new("target_notebook", "new_string"))
                }
                "createplan" => Presentation::CreatePlan(CreatePlanProjection::default()),
                _ => Presentation::Plain,
            },
        };
        Self { presentation }
    }

    pub fn arguments_delta(
        &mut self,
        call: &ToolCall,
        raw_delta: &str,
    ) -> Result<Vec<pb::AgentServerMessage>> {
        match &mut self.presentation {
            Presentation::Plain => Ok(vec![interaction::arguments_delta(call, raw_delta)?]),
            Presentation::DynamicMcp(definition) => {
                Ok(vec![interaction::dynamic_mcp_arguments_delta(
                    call, raw_delta, definition,
                )])
            }
            Presentation::Edit(edit) => {
                let mut messages = Vec::new();
                edit.project(call, raw_delta, &mut messages)?;
                Ok(messages)
            }
            Presentation::CreatePlan(plan) => plan.project(call, raw_delta),
        }
    }
}

impl CreatePlanProjection {
    fn project(&mut self, call: &ToolCall, raw_delta: &str) -> Result<Vec<pb::AgentServerMessage>> {
        let mut completed_field = false;
        for event in self.fields.push(raw_delta)? {
            match event {
                StringFieldEvent::Delta { name, text } => match name.as_str() {
                    "name" => self.name.push_str(&text),
                    "plan" => self.plan.push_str(&text),
                    "overview" => self.overview.push_str(&text),
                    _ => {}
                },
                StringFieldEvent::End { name }
                    if matches!(name.as_str(), "name" | "plan" | "overview") =>
                {
                    completed_field = true
                }
                _ => {}
            }
        }
        Ok(completed_field
            .then(|| interaction::create_plan_partial(call, &self.name, &self.plan, &self.overview))
            .into_iter()
            .collect())
    }
}

impl EditProjection {
    fn new(path_field: &'static str, content_field: &'static str) -> Self {
        Self {
            fields: JsonStringFields::default(),
            path_field,
            content_field,
            path: String::new(),
            content: NewlineStream::default(),
        }
    }

    fn project(
        &mut self,
        call: &ToolCall,
        raw_delta: &str,
        messages: &mut Vec<pb::AgentServerMessage>,
    ) -> Result<()> {
        for event in self.fields.push(raw_delta)? {
            match event {
                StringFieldEvent::Delta { name, text } if name == self.path_field => {
                    self.path.push_str(&text)
                }
                StringFieldEvent::End { name } if name == self.path_field => {
                    messages.push(interaction::edit_path_partial(call, &self.path));
                }
                StringFieldEvent::Delta { name, text } if name == self.content_field => {
                    let content = self.content.push(&text, false);
                    if !content.is_empty() {
                        messages.push(interaction::edit_content_delta(call, content));
                    }
                }
                StringFieldEvent::End { name } if name == self.content_field => {
                    let content = self.content.push("", true);
                    if !content.is_empty() {
                        messages.push(interaction::edit_content_delta(call, content));
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct NewlineStream {
    pending_cr: bool,
}

impl NewlineStream {
    fn push(&mut self, text: &str, finished: bool) -> String {
        let mut output = String::with_capacity(text.len());
        for character in text.chars() {
            if self.pending_cr {
                output.push('\n');
                self.pending_cr = false;
                if character == '\n' {
                    continue;
                }
            }
            if character == '\r' {
                self.pending_cr = true;
            } else {
                output.push(character);
            }
        }
        if finished && self.pending_cr {
            output.push('\n');
            self.pending_cr = false;
        }
        output
    }
}

fn normalized(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    fn call(name: &str) -> ToolCall {
        ToolCall {
            index: 0,
            call_id: "call-1".into(),
            model_call_id: "model-1".into(),
            name: name.into(),
            arguments_text: String::new(),
            arguments: Value::Null,
        }
    }

    #[test]
    fn plain_tools_only_project_raw_argument_deltas() {
        let call = call("Read");
        let mut stream = ToolCallStream::new(&call.name, None);
        assert_eq!(
            stream.arguments_delta(&call, "{\"path\":").unwrap().len(),
            1
        );
    }

    #[test]
    fn write_projects_path_and_content_without_starting_execution() {
        let call = call("Write");
        let mut stream = ToolCallStream::new(&call.name, None);
        let first = stream
            .arguments_delta(&call, "{\"path\":\"/tmp/a\",\"contents\":\"hel")
            .unwrap();
        assert_eq!(first.len(), 2);
        assert!(matches!(
            first[0].message,
            Some(pb::agent_server_message::Message::InteractionUpdate(
                pb::InteractionUpdate {
                    message: Some(pb::interaction_update::Message::PartialToolCall(_))
                }
            ))
        ));
        assert_eq!(edit_delta(&first[1]), "hel");

        let second = stream.arguments_delta(&call, "lo\\n世界\"}").unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(edit_delta(&second[0]), "lo\n世界");
    }

    #[test]
    fn str_replace_projects_only_new_string_when_path_arrives_later() {
        let mut call = call("StrReplace");
        let mut stream = ToolCallStream::new(&call.name, None);
        let first = stream
            .arguments_delta(&call, "{\"new_string\":\"new\",\"old_string\":\"old\",")
            .unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(edit_delta(&first[0]), "new");
        let second = stream
            .arguments_delta(&call, "\"path\":\"/tmp/a\"}")
            .unwrap();
        assert_eq!(second.len(), 1);
        assert!(matches!(
            second[0].message,
            Some(pb::agent_server_message::Message::InteractionUpdate(
                pb::InteractionUpdate {
                    message: Some(pb::interaction_update::Message::PartialToolCall(_))
                }
            ))
        ));

        call.arguments = json!({
            "path": "/tmp/a",
            "old_string": "old",
            "new_string": "new"
        });
        let rendered = interaction::render_tool_call(&call, false).unwrap();
        let Some(pb::tool_call::Tool::EditToolCall(edit)) = rendered.tool else {
            panic!("expected EditToolCall")
        };
        assert_eq!(edit.args.unwrap().stream_content.as_deref(), Some("new"));
    }

    #[test]
    fn edit_stream_normalizes_split_crlf_once() {
        let call = call("Write");
        let mut stream = ToolCallStream::new(&call.name, None);
        let first = stream
            .arguments_delta(&call, "{\"contents\":\"a\\r")
            .unwrap();
        let second = stream
            .arguments_delta(&call, "\\nb\\r\",\"path\":\"/tmp/a\"}")
            .unwrap();
        assert_eq!(edit_delta(&first[0]), "a");
        assert_eq!(edit_delta(&second[0]), "\nb");
        assert_eq!(edit_delta(&second[1]), "\n");
    }

    #[test]
    fn create_plan_projects_completed_fields_as_structured_partial_args() {
        let call = call("CreatePlan");
        let mut stream = ToolCallStream::new(&call.name, None);
        let name = stream
            .arguments_delta(&call, "{\"name\":\"Migration Plan\",\"plan\":\"# Move")
            .unwrap();
        assert_eq!(name.len(), 1);

        let messages = stream
            .arguments_delta(
                &call,
                " services\",\"overview\":\"Move the services safely\",\"todos\":[]}",
            )
            .unwrap();
        assert_eq!(messages.len(), 1);
        let Some(pb::agent_server_message::Message::InteractionUpdate(update)) =
            &messages[0].message
        else {
            panic!("expected InteractionUpdate")
        };
        let Some(pb::interaction_update::Message::PartialToolCall(partial)) = &update.message
        else {
            panic!("expected PartialToolCall")
        };
        assert!(partial.args_text_delta.is_empty());
        let Some(pb::tool_call::Tool::CreatePlanToolCall(plan)) = partial
            .tool_call
            .as_ref()
            .and_then(|call| call.tool.as_ref())
        else {
            panic!("expected CreatePlanToolCall")
        };
        let args = plan.args.as_ref().unwrap();
        assert_eq!(args.name, "Migration Plan");
        assert_eq!(args.plan, "# Move services");
        assert_eq!(args.overview, "Move the services safely");
        assert!(args.todos.is_empty());
    }

    fn edit_delta(message: &pb::AgentServerMessage) -> &str {
        let Some(pb::agent_server_message::Message::InteractionUpdate(update)) = &message.message
        else {
            panic!("expected InteractionUpdate")
        };
        let Some(pb::interaction_update::Message::ToolCallDelta(update)) = &update.message else {
            panic!("expected ToolCallDelta")
        };
        let Some(pb::tool_call_delta::Delta::EditToolCallDelta(delta)) = update
            .tool_call_delta
            .as_deref()
            .and_then(|delta| delta.delta.as_ref())
        else {
            panic!("expected EditToolCallDelta")
        };
        &delta.stream_content_delta
    }
}
