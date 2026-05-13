use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

const ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Clone)]
pub struct LlmClient {
    http: Client,
    api_key: String,
    model: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    temperature: f32,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

impl LlmClient {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    pub async fn cleanup(
        &self,
        system_prompt: &str,
        raw_transcript: &str,
        vocabulary: &[String],
    ) -> Result<String> {
        if self.api_key.is_empty() {
            return Err(anyhow!("missing OPENAI_API_KEY"));
        }
        if raw_transcript.trim().is_empty() {
            return Ok(String::new());
        }

        let mut composed_system = system_prompt.to_string();
        if !vocabulary.is_empty() {
            composed_system.push_str("\n\nPreserve these terms exactly as written: ");
            composed_system.push_str(&vocabulary.join(", "));
            composed_system.push('.');
        }

        let body = ChatRequest {
            model: &self.model,
            temperature: 0.2,
            messages: vec![
                Message {
                    role: "system",
                    content: &composed_system,
                },
                Message {
                    role: "user",
                    content: raw_transcript,
                },
            ],
        };

        let resp = self
            .http
            .post(ENDPOINT)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("POST /v1/chat/completions")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err = resp.text().await.unwrap_or_default();
            return Err(anyhow!("cleanup failed: {status} — {err}"));
        }

        let parsed: ChatResponse = resp.json().await?;
        let out = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .unwrap_or_default()
            .trim()
            .to_string();
        Ok(out)
    }
}
