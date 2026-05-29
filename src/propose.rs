use std::io::{self, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::{ChatModel, Message, ModelError, Reply};

const MAX_CANDIDATES: usize = 5;

const SYSTEM_PROMPT: &str = "You are a shell command assistant. Given a task, reply with a JSON \
array of at most five objects, each {\"command\": <shell command>, \"explanation\": <one-line \
explanation>}, best fit first. Output only the JSON array: no prose, no markdown fences.";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Candidate {
    pub command: String,
    pub explanation: String,
}

#[derive(Debug, Error)]
pub enum ProposeError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("expected command suggestions but the model requested a tool call")]
    UnexpectedToolCalls,
    #[error("could not parse command suggestions: {0}")]
    Parse(String),
    #[error("the model returned no command suggestions")]
    Empty,
}

pub fn build_messages(query: &str) -> Vec<Message> {
    vec![Message::system(SYSTEM_PROMPT), Message::user(query)]
}

pub fn parse_candidates(text: &str) -> Result<Vec<Candidate>, ProposeError> {
    let candidates: Vec<Candidate> =
        serde_json::from_str(strip_fences(text)).map_err(|error| ProposeError::Parse(error.to_string()))?;
    match candidates.is_empty() {
        true => Err(ProposeError::Empty),
        false => Ok(candidates.into_iter().take(MAX_CANDIDATES).collect()),
    }
}

pub async fn propose<M: ChatModel>(model: &M, query: &str) -> Result<Vec<Candidate>, ProposeError> {
    match model.complete(&build_messages(query), &[]).await?.reply {
        Reply::Text(text) => parse_candidates(&text),
        Reply::ToolCalls(_) => Err(ProposeError::UnexpectedToolCalls),
    }
}

pub fn emit_command(mut out: impl Write, command: &str) -> io::Result<()> {
    writeln!(out, "{command}")
}

fn strip_fences(text: &str) -> &str {
    let trimmed = text.trim();
    let opened = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    opened.strip_suffix("```").unwrap_or(opened).trim()
}

pub struct Selection {
    candidates: Vec<Candidate>,
    cursor: usize,
}

impl Selection {
    pub fn new(candidates: Vec<Candidate>) -> Self {
        Self {
            candidates,
            cursor: 0,
        }
    }

    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn down(&mut self) {
        if self.cursor + 1 < self.candidates.len() {
            self.cursor += 1;
        }
    }

    pub fn selected(&self) -> &Candidate {
        &self.candidates[self.cursor]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Completion, FakeModel, Role, Usage};

    fn text_reply(json: &str) -> Completion {
        Completion {
            reply: Reply::Text(json.into()),
            usage: Usage::default(),
        }
    }

    fn sample(commands: &[&str]) -> Vec<Candidate> {
        commands
            .iter()
            .map(|command| Candidate {
                command: (*command).into(),
                explanation: "why".into(),
            })
            .collect()
    }

    #[test]
    fn build_messages_pairs_system_and_user() {
        let messages = build_messages("list files");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[1].content.as_deref(), Some("list files"));
    }

    #[test]
    fn parses_plain_json_array() {
        let candidates = parse_candidates(
            r#"[{"command": "ls -lS", "explanation": "why"}]"#,
        )
        .unwrap();
        assert_eq!(candidates, sample(&["ls -lS"]));
    }

    #[test]
    fn strips_markdown_fences() {
        let candidates = parse_candidates(
            "```json\n[{\"command\": \"ls\", \"explanation\": \"why\"}]\n```",
        )
        .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].command, "ls");
    }

    #[test]
    fn caps_at_five_candidates() {
        let many: String = serde_json::to_string(
            &(0..7)
                .map(|i| Candidate {
                    command: format!("c{i}"),
                    explanation: "e".into(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(parse_candidates(&many).unwrap().len(), 5);
    }

    #[test]
    fn empty_array_is_empty_error() {
        assert!(matches!(parse_candidates("[]"), Err(ProposeError::Empty)));
    }

    #[test]
    fn garbage_is_parse_error() {
        assert!(matches!(
            parse_candidates("not json"),
            Err(ProposeError::Parse(_))
        ));
    }

    #[test]
    fn selection_navigates_within_bounds() {
        let mut selection = Selection::new(sample(&["a", "b", "c"]));
        assert_eq!(selection.cursor(), 0);
        selection.up();
        assert_eq!(selection.cursor(), 0);
        selection.down();
        selection.down();
        assert_eq!(selection.selected().command, "c");
        selection.down();
        assert_eq!(selection.cursor(), 2);
        selection.up();
        assert_eq!(selection.selected().command, "b");
    }

    #[tokio::test]
    async fn propose_parses_model_text() {
        let model = FakeModel::new([text_reply(
            r#"[{"command": "du -sh *", "explanation": "sizes"}]"#,
        )]);
        let candidates = propose(&model, "disk usage").await.unwrap();
        assert_eq!(
            candidates,
            vec![Candidate {
                command: "du -sh *".into(),
                explanation: "sizes".into(),
            }]
        );
    }

    #[tokio::test]
    async fn propose_rejects_tool_calls() {
        let model = FakeModel::new([Completion {
            reply: Reply::ToolCalls(Vec::new()),
            usage: Usage::default(),
        }]);
        assert!(matches!(
            propose(&model, "x").await,
            Err(ProposeError::UnexpectedToolCalls)
        ));
    }

    #[test]
    fn emit_writes_command_with_newline() {
        let mut out = Vec::new();
        emit_command(&mut out, "ls -la").unwrap();
        assert_eq!(out, b"ls -la\n");
    }
}
