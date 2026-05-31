use std::collections::{HashMap, HashSet};

use futures_concurrency::prelude::*;
use thiserror::Error;

use crate::config::Budget;
use crate::model::{ChatModel, Message, ModelError, Reply, ToolCall, ToolDef};

const SYSTEM_PROMPT: &str = "You are a careful research assistant. Answer the user's question \
using the provided read-only tools to read local files or fetch web pages when you need facts; \
do not guess at file or page contents. When you have enough information, reply with a concise \
GitHub-flavored Markdown answer. Never request actions that modify the system.";

const DUPLICATE_NOTE: &str = "Duplicate tool call skipped; reuse the earlier result.";

pub trait ToolBox {
    fn schemas(&self) -> Vec<ToolDef>;
    async fn invoke(&self, call: &ToolCall) -> String;
}

#[derive(Debug, Error)]
pub enum AskError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error("exceeded token budget: spent {spent}, limit {limit}")]
    TokenBudget { spent: u32, limit: u32 },
    #[error("gave up after {0} tool rounds without a final answer")]
    RoundsExhausted(u32),
}

pub fn build_messages(query: &str) -> Vec<Message> {
    vec![Message::system(SYSTEM_PROMPT), Message::user(query)]
}

pub async fn ask<M, T>(
    model: &M,
    tools: &T,
    query: &str,
    budget: &Budget,
    report: &mut dyn FnMut(String),
) -> Result<String, AskError>
where
    M: ChatModel,
    T: ToolBox,
{
    let schemas = tools.schemas();
    let mut messages = build_messages(query);
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut spent = 0u32;

    report("thinking".to_string());
    for _ in 0..budget.max_rounds {
        let completion = model.complete(&messages, &schemas).await?;
        spent = spent.saturating_add(completion.usage.total_tokens);
        if spent > budget.max_tokens {
            return Err(AskError::TokenBudget {
                spent,
                limit: budget.max_tokens,
            });
        }

        match completion.reply {
            Reply::Text(answer) => return Ok(answer),
            Reply::ToolCalls(calls) => {
                report(describe_calls(&calls));
                messages.push(Message::assistant_tool_calls(calls.clone()));
                messages.extend(run_calls(tools, &calls, &mut seen).await);
            }
        }
    }

    Err(AskError::RoundsExhausted(budget.max_rounds))
}

fn describe_calls(calls: &[ToolCall]) -> String {
    calls
        .iter()
        .map(describe_call)
        .collect::<Vec<_>>()
        .join(", ")
}

fn describe_call(call: &ToolCall) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
        .ok()
        .and_then(|args| {
            args.as_object()
                .and_then(|fields| fields.values().find_map(serde_json::Value::as_str))
                .map(str::to_string)
        });
    match detail {
        Some(detail) => format!("{}: {detail}", call.function.name),
        None => call.function.name.clone(),
    }
}

async fn run_calls<T: ToolBox>(
    tools: &T,
    calls: &[ToolCall],
    seen: &mut HashSet<(String, String)>,
) -> Vec<Message> {
    let fresh: Vec<&ToolCall> = calls.iter().filter(|call| seen.insert(key(call))).collect();

    let results: HashMap<String, String> = fresh
        .iter()
        .map(|call| async move { (call.id.clone(), tools.invoke(call).await) })
        .collect::<Vec<_>>()
        .join()
        .await
        .into_iter()
        .collect();

    calls
        .iter()
        .map(|call| {
            let content = results
                .get(&call.id)
                .cloned()
                .unwrap_or_else(|| DUPLICATE_NOTE.to_string());
            Message::tool(&call.id, content)
        })
        .collect()
}

fn key(call: &ToolCall) -> (String, String) {
    (call.function.name.clone(), call.function.arguments.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Completion, FakeModel, FunctionCall, Reply, ToolKind, Usage};
    use std::cell::RefCell;

    struct FakeToolBox {
        results: HashMap<String, String>,
        invoked: RefCell<Vec<(String, String)>>,
    }

    impl FakeToolBox {
        fn new(results: &[(&str, &str)]) -> Self {
            Self {
                results: results
                    .iter()
                    .map(|(name, out)| ((*name).to_string(), (*out).to_string()))
                    .collect(),
                invoked: RefCell::new(Vec::new()),
            }
        }

        fn invocations(&self) -> Vec<(String, String)> {
            self.invoked.borrow().clone()
        }
    }

    impl ToolBox for FakeToolBox {
        fn schemas(&self) -> Vec<ToolDef> {
            Vec::new()
        }

        async fn invoke(&self, call: &ToolCall) -> String {
            self.invoked
                .borrow_mut()
                .push((call.function.name.clone(), call.function.arguments.clone()));
            self.results
                .get(&call.function.name)
                .cloned()
                .unwrap_or_else(|| format!("unknown tool: {}", call.function.name))
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            kind: ToolKind::Function,
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }

    fn text(answer: &str) -> Completion {
        Completion {
            reply: Reply::Text(answer.into()),
            usage: Usage::default(),
        }
    }

    fn tool_calls(calls: Vec<ToolCall>) -> Completion {
        Completion {
            reply: Reply::ToolCalls(calls),
            usage: Usage::default(),
        }
    }

    fn budget(max_rounds: u32, max_tokens: u32) -> Budget {
        Budget {
            max_rounds,
            max_tokens,
            ..Budget::default()
        }
    }

    #[tokio::test]
    async fn returns_text_without_invoking_tools() {
        let model = FakeModel::new([text("# done")]);
        let tools = FakeToolBox::new(&[]);
        let answer = ask(&model, &tools, "q", &budget(6, 1_000), &mut |_| {})
            .await
            .unwrap();
        assert_eq!(answer, "# done");
        assert!(tools.invocations().is_empty());
    }

    #[tokio::test]
    async fn reports_progress_for_thinking_and_tool_calls() {
        let model = FakeModel::new([
            tool_calls(vec![call("c1", "read_file", "{\"path\":\"README.md\"}")]),
            text("done"),
        ]);
        let tools = FakeToolBox::new(&[("read_file", "data")]);

        let mut messages = Vec::new();
        ask(&model, &tools, "q", &budget(6, 1_000), &mut |message| {
            messages.push(message)
        })
        .await
        .unwrap();

        assert_eq!(messages, ["thinking", "read_file: README.md"]);
    }

    #[tokio::test]
    async fn runs_a_tool_then_returns_the_answer() {
        let model = FakeModel::new([
            tool_calls(vec![call("c1", "read_file", "{\"path\":\"a\"}")]),
            text("contents summarized"),
        ]);
        let tools = FakeToolBox::new(&[("read_file", "hello world")]);

        let answer = ask(
            &model,
            &tools,
            "summarize a",
            &budget(6, 1_000),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert_eq!(answer, "contents summarized");
        assert_eq!(
            tools.invocations(),
            vec![("read_file".into(), "{\"path\":\"a\"}".into())]
        );

        let second_call = &model.calls()[1];
        assert!(matches!(
            second_call.last().unwrap().role,
            crate::model::Role::Tool
        ));
    }

    #[tokio::test]
    async fn dedupes_repeated_calls_across_and_within_rounds() {
        let model = FakeModel::new([
            tool_calls(vec![
                call("a1", "read_file", "{\"path\":\"x\"}"),
                call("a2", "read_file", "{\"path\":\"x\"}"),
            ]),
            tool_calls(vec![call("b1", "read_file", "{\"path\":\"x\"}")]),
            text("answer"),
        ]);
        let tools = FakeToolBox::new(&[("read_file", "x contents")]);

        let answer = ask(&model, &tools, "q", &budget(6, 1_000), &mut |_| {})
            .await
            .unwrap();

        assert_eq!(answer, "answer");
        assert_eq!(tools.invocations().len(), 1);
    }

    #[tokio::test]
    async fn errors_when_rounds_are_exhausted() {
        let model = FakeModel::new([
            tool_calls(vec![call("c1", "read_file", "{\"path\":\"1\"}")]),
            tool_calls(vec![call("c2", "read_file", "{\"path\":\"2\"}")]),
        ]);
        let tools = FakeToolBox::new(&[("read_file", "data")]);

        let result = ask(&model, &tools, "q", &budget(2, 1_000), &mut |_| {}).await;
        assert!(matches!(result, Err(AskError::RoundsExhausted(2))));
        assert_eq!(tools.invocations().len(), 2);
    }

    #[tokio::test]
    async fn errors_when_token_budget_is_exceeded() {
        let model = FakeModel::new([Completion {
            reply: Reply::ToolCalls(vec![call("c1", "read_file", "{}")]),
            usage: Usage {
                total_tokens: 50,
                ..Usage::default()
            },
        }]);
        let tools = FakeToolBox::new(&[("read_file", "data")]);

        let result = ask(&model, &tools, "q", &budget(6, 10), &mut |_| {}).await;
        assert!(matches!(
            result,
            Err(AskError::TokenBudget {
                spent: 50,
                limit: 10
            })
        ));
    }
}
