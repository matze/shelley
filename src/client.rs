use reqwest::Client as HttpClient;

use crate::config::Config;
use crate::model::{
    ChatModel, ChatRequest, ChatResponse, Completion, Message, ModelError, ToolDef,
};

pub struct OpenAiClient {
    http: HttpClient,
    base_url: String,
    model: String,
    api_key: String,
}

impl OpenAiClient {
    pub fn new(config: &Config) -> Result<Self, ModelError> {
        HttpClient::builder()
            .build()
            .map(|http| Self {
                http,
                base_url: config.base_url.clone(),
                model: config.model.clone(),
                api_key: config.api_key.clone(),
            })
            .map_err(transport)
    }
}

impl ChatModel for OpenAiClient {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[ToolDef],
    ) -> Result<Completion, ModelError> {
        let request = ChatRequest {
            model: &self.model,
            messages,
            tools,
        };
        let response = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .map_err(transport)?;

        let status = response.status();
        let body = response.text().await.map_err(transport)?;
        if !status.is_success() {
            return Err(ModelError::Api {
                status: status.as_u16(),
                body,
            });
        }

        serde_json::from_str::<ChatResponse>(&body)
            .map_err(|error| ModelError::Decode(error.to_string()))?
            .into_completion()
    }
}

fn transport(error: impl std::error::Error + Send + Sync + 'static) -> ModelError {
    ModelError::Transport(Box::new(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Provider, Sandbox};
    use crate::model::Reply;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn config_for(server: &MockServer, provider: Provider) -> Config {
        let mut config = Config::new(provider, None, None, "secret".into(), Sandbox::Disabled);
        config.base_url = server.uri();
        config
    }

    #[tokio::test]
    async fn posts_to_chat_completions_with_auth_and_parses_text() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer secret"))
            .and(body_partial_json(json!({"model": "gpt-4o"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"role": "assistant", "content": "hi"}}],
                "usage": {"total_tokens": 3}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&config_for(&server, Provider::OpenAi)).unwrap();
        let completion = client.complete(&[Message::user("q")], &[]).await.unwrap();

        assert_eq!(completion.reply, Reply::Text("hi".into()));
        assert_eq!(completion.usage.total_tokens, 3);
    }

    #[tokio::test]
    async fn sends_deepseek_default_model_to_configured_base_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(json!({"model": "deepseek-v4-pro"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"role": "assistant", "content": "ok"}}]
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&config_for(&server, Provider::DeepSeek)).unwrap();
        let completion = client.complete(&[Message::user("q")], &[]).await.unwrap();

        assert_eq!(completion.reply, Reply::Text("ok".into()));
    }

    #[tokio::test]
    async fn parses_tool_calls() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"role": "assistant", "tool_calls": [{
                    "id": "c1",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": "{}"}
                }]}}]
            })))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&config_for(&server, Provider::OpenAi)).unwrap();
        match client
            .complete(&[Message::user("q")], &[])
            .await
            .unwrap()
            .reply
        {
            Reply::ToolCalls(calls) => assert_eq!(calls[0].function.name, "read_file"),
            other => panic!("expected tool calls, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn non_success_status_becomes_api_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&config_for(&server, Provider::OpenAi)).unwrap();
        match client.complete(&[Message::user("q")], &[]).await {
            Err(ModelError::Api { status, body }) => {
                assert_eq!(status, 500);
                assert_eq!(body, "boom");
            }
            other => panic!("expected api error, got {other:?}"),
        }
    }
}
