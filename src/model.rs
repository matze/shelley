#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    #[default]
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Message {
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(Role::Assistant, content)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            ..Self::default()
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            tool_calls,
            ..Self::default()
        }
    }

    fn text(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: Some(content.into()),
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolKind {
    #[default]
    Function,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: ToolKind,
    pub function: FunctionCall,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolDef {
    #[serde(rename = "type")]
    pub kind: ToolKind,
    pub function: FunctionDef,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Serialize)]
pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub messages: &'a [Message],
    #[serde(skip_serializing_if = "is_empty_slice")]
    pub tools: &'a [ToolDef],
}

fn is_empty_slice<T>(value: &&[T]) -> bool {
    value.is_empty()
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default)]
    pub total_tokens: u32,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    #[serde(default)]
    pub usage: Usage,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: Message,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Completion {
    pub reply: Reply,
    pub usage: Usage,
}

impl ChatResponse {
    pub fn into_completion(self) -> Result<Completion, ModelError> {
        let message = self
            .choices
            .into_iter()
            .next()
            .ok_or(ModelError::EmptyResponse)?
            .message;
        let reply = match message.tool_calls.is_empty() {
            true => Reply::Text(message.content.unwrap_or_default()),
            false => Reply::ToolCalls(message.tool_calls),
        };
        Ok(Completion {
            reply,
            usage: self.usage,
        })
    }
}

#[derive(Debug, Error)]
pub enum ModelError {
    #[error("model returned no choices")]
    EmptyResponse,
    #[error("fake model exhausted")]
    Exhausted,
    #[error("transport error")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("api returned status {status}: {body}")]
    Api { status: u16, body: String },
    #[error("failed to decode response: {0}")]
    Decode(String),
}

pub trait ChatModel {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<Completion, ModelError>;
}

#[cfg(test)]
pub struct FakeModel {
    replies: std::cell::RefCell<std::collections::VecDeque<Completion>>,
    calls: std::cell::RefCell<Vec<Vec<Message>>>,
}

#[cfg(test)]
impl FakeModel {
    pub fn new(replies: impl IntoIterator<Item = Completion>) -> Self {
        Self {
            replies: std::cell::RefCell::new(replies.into_iter().collect()),
            calls: std::cell::RefCell::new(Vec::new()),
        }
    }

    pub fn calls(&self) -> Vec<Vec<Message>> {
        self.calls.borrow().clone()
    }
}

#[cfg(test)]
impl ChatModel for FakeModel {
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDef],
    ) -> Result<Completion, ModelError> {
        self.calls.borrow_mut().push(messages.to_vec());
        self.replies
            .borrow_mut()
            .pop_front()
            .ok_or(ModelError::Exhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_message_serializes_minimally() {
        let message = Message::user("hi");
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            json!({"role": "user", "content": "hi"})
        );
    }

    #[test]
    fn tool_message_includes_tool_call_id() {
        let message = Message::tool("call_1", "result");
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            json!({"role": "tool", "content": "result", "tool_call_id": "call_1"})
        );
    }

    #[test]
    fn request_omits_empty_tools() {
        let messages = vec![Message::user("hi")];
        let request = ChatRequest {
            model: "m",
            messages: &messages,
            tools: &[],
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({"model": "m", "messages": [{"role": "user", "content": "hi"}]})
        );
    }

    #[test]
    fn response_with_text_becomes_text_reply() {
        let raw = json!({
            "choices": [{"message": {"role": "assistant", "content": "hello"}}],
            "usage": {"total_tokens": 7}
        });
        let completion: Completion = serde_json::from_value::<ChatResponse>(raw)
            .unwrap()
            .into_completion()
            .unwrap();
        assert_eq!(completion.reply, Reply::Text("hello".into()));
        assert_eq!(completion.usage.total_tokens, 7);
    }

    #[test]
    fn response_with_tool_calls_becomes_tool_reply() {
        let raw = json!({
            "choices": [{"message": {"role": "assistant", "tool_calls": [{
                "id": "c1",
                "type": "function",
                "function": {"name": "read_file", "arguments": "{\"path\":\"a\"}"}
            }]}}]
        });
        let reply = serde_json::from_value::<ChatResponse>(raw)
            .unwrap()
            .into_completion()
            .unwrap()
            .reply;
        match reply {
            Reply::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].function.name, "read_file");
            }
            other => panic!("expected tool calls, got {other:?}"),
        }
    }

    #[test]
    fn empty_response_is_an_error() {
        let response = ChatResponse {
            choices: Vec::new(),
            usage: Usage::default(),
        };
        assert!(matches!(
            response.into_completion(),
            Err(ModelError::EmptyResponse)
        ));
    }

    #[tokio::test]
    async fn fake_model_returns_scripted_replies_in_order() {
        let model = FakeModel::new([
            Completion {
                reply: Reply::ToolCalls(Vec::new()),
                usage: Usage::default(),
            },
            Completion {
                reply: Reply::Text("done".into()),
                usage: Usage::default(),
            },
        ]);

        let first = model.complete(&[Message::user("q")], &[]).await.unwrap();
        assert!(matches!(first.reply, Reply::ToolCalls(_)));

        let second = model.complete(&[], &[]).await.unwrap();
        assert_eq!(second.reply, Reply::Text("done".into()));

        assert!(matches!(
            model.complete(&[], &[]).await,
            Err(ModelError::Exhausted)
        ));
        assert_eq!(model.calls().len(), 3);
    }
}
