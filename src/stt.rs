use anyhow::{anyhow, Context, Result};
use reqwest::multipart::{Form, Part};
use reqwest::Client;

const ENDPOINT: &str = "https://api.openai.com/v1/audio/transcriptions";

#[derive(Clone)]
pub struct SttClient {
    http: Client,
    api_key: String,
    model: String,
}

impl SttClient {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            api_key: api_key.into(),
            model: model.into(),
        }
    }

    pub async fn transcribe(&self, wav: Vec<u8>, prompt: Option<&str>) -> Result<String> {
        if self.api_key.is_empty() {
            return Err(anyhow!("missing OPENAI_API_KEY"));
        }

        let file = Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")?;

        let mut form = Form::new()
            .text("model", self.model.clone())
            .text("response_format", "text")
            .part("file", file);

        if let Some(p) = prompt.filter(|p| !p.is_empty()) {
            form = form.text("prompt", p.to_string());
        }

        let resp = self
            .http
            .post(ENDPOINT)
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .context("POST /v1/audio/transcriptions")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(anyhow!("transcription failed: {status} — {body}"));
        }

        let text = resp.text().await?.trim().to_string();
        Ok(text)
    }
}
