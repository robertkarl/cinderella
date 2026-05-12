use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
    max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

pub struct CompletionResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

pub struct McpLlmClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl McpLlmClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn complete(
        &self,
        system_prompt: &str,
        user_content: &str,
        max_tokens: u32,
    ) -> Result<CompletionResult, String> {
        let messages = vec![
            ChatMessage { role: "system".into(), content: system_prompt.into() },
            ChatMessage { role: "user".into(), content: user_content.into() },
        ];

        let body = ChatRequest {
            model: self.model.clone(),
            messages,
            temperature: 0.1,
            max_tokens,
        };

        let response = self.client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to llama-server: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("llama-server returned {}: {}", status, body));
        }

        let resp: ChatResponse = response.json().await
            .map_err(|e| format!("Failed to parse llama-server response: {}", e))?;

        let text = resp.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let (input_tokens, output_tokens) = resp.usage
            .map(|u| (u.prompt_tokens, u.completion_tokens))
            .unwrap_or((0, 0));

        Ok(CompletionResult { text, input_tokens, output_tokens })
    }

    pub async fn health_check(&self) -> Result<(), String> {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| format!("Health check failed: {}", e))?
            .error_for_status()
            .map_err(|e| format!("Health check returned error: {}", e))?;
        Ok(())
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_request_serialization() {
        let body = ChatRequest {
            model: "qwen3.5-9b".into(),
            messages: vec![
                ChatMessage { role: "system".into(), content: "Be concise.".into() },
                ChatMessage { role: "user".into(), content: "Hello".into() },
            ],
            temperature: 0.1,
            max_tokens: 1024,
        };
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(json["model"], "qwen3.5-9b");
        assert_eq!(json["messages"].as_array().unwrap().len(), 2);
        assert_eq!(json["temperature"], 0.1);
    }

    #[test]
    fn test_parse_chat_response() {
        let json = r#"{
            "choices": [{"message": {"content": "BUILD SUCCEEDED"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 3200, "completion_tokens": 2, "total_tokens": 3202}
        }"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices[0].message.content.as_deref(), Some("BUILD SUCCEEDED"));
        assert_eq!(resp.usage.unwrap().prompt_tokens, 3200);
    }

    #[test]
    fn test_parse_response_without_usage() {
        let json = r#"{"choices": [{"message": {"content": "ok"}, "finish_reason": "stop"}]}"#;
        let resp: ChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
    }
}
