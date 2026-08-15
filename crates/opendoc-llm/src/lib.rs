//! opendoc-llm — OpenAI-compatible chat completions client (BYOK 核心)
//!
//! 只做 OpenAI-compatible `/chat/completions` protocol（含 SSE streaming），
//! 不逐家特化。DeepSeek / Moonshot / OpenRouter / Ollama 全部掛同一 protocol。

pub mod embedding;

use futures_util::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProvider {
    /// 顯示名稱（如 "deepseek"、"ollama"）
    pub name: String,
    /// base url，如 https://api.deepseek.com/v1 或 http://localhost:11434/v1
    pub base_url: String,
    pub model: String,
    /// API key；Ollama 可為空
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompletionOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP 請求失敗: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Provider 回傳錯誤 (HTTP {status}): {body}")]
    Api { status: u16, body: String },
    #[error("SSE 解析失敗: {0}")]
    Sse(String),
    #[error("Provider 回傳空回覆")]
    EmptyResponse,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    provider: LlmProvider,
    http: reqwest::Client,
}

impl LlmClient {
    pub fn new(provider: LlmProvider) -> Self {
        Self { provider, http: reqwest::Client::new() }
    }

    fn chat_url(&self) -> String {
        let base = self.provider.base_url.trim_end_matches('/');
        format!("{base}/chat/completions")
    }

    fn build_body(&self, mut messages: Vec<ChatMessage>, opts: &CompletionOptions, stream: bool) -> serde_json::Value {
        if let Some(system_prompt) = opts.system_prompt.as_deref() {
            let already_present = messages
                .iter()
                .any(|message| message.role == "system" && message.content == system_prompt);
            if !already_present {
                messages.insert(0, ChatMessage::system(system_prompt));
            }
        }

        let mut body = serde_json::json!({
            "model": self.provider.model,
            "messages": messages,
            "stream": stream,
        });
        if let Some(t) = opts.temperature {
            body["temperature"] = serde_json::json!(t);
        }
        if let Some(m) = opts.max_tokens {
            body["max_tokens"] = serde_json::json!(m);
        }
        body
    }

    fn request(&self, messages: Vec<ChatMessage>, opts: &CompletionOptions, stream: bool) -> reqwest::RequestBuilder {
        let mut req = self.http.post(self.chat_url()).json(&self.build_body(messages, opts, stream));
        if let Some(k) = &self.provider.api_key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        req
    }

    /// 非串流完整生成
    pub async fn complete(&self, messages: Vec<ChatMessage>, opts: &CompletionOptions) -> Result<String, LlmError> {
        let resp = self.request(messages, opts, false).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status: status.as_u16(), body });
        }
        let json: serde_json::Value = resp.json().await?;
        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or(LlmError::EmptyResponse)
    }

    /// 串流生成：先送出請求，回傳 delta 內容的字串串流
    pub async fn stream(
        &self,
        messages: Vec<ChatMessage>,
        opts: &CompletionOptions,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let resp = self.request(messages, opts, true).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(LlmError::Api { status: status.as_u16(), body });
        }

        let byte_stream = resp.bytes_stream();
        let buf = String::new();
        let stream = futures_util::stream::unfold(
            (byte_stream, buf),
            |(mut byte_stream, mut buf)| async move {
                loop {
                    match byte_stream.next().await {
                        Some(Ok(chunk)) => {
                            buf.push_str(&String::from_utf8_lossy(&chunk));
                            // SSE：以 \n\n 分隔事件
                            while let Some(pos) = buf.find("\n\n") {
                                let event = buf[..pos].to_string();
                                buf = buf[pos + 2..].to_string();
                                let data = event.lines()
                                    .find_map(|l| l.strip_prefix("data:"))
                                    .map(|d| d.trim().to_string());
                                let Some(data) = data else { continue };
                                if data == "[DONE]" {
                                    return None;
                                }
                                match serde_json::from_str::<serde_json::Value>(&data) {
                                    Ok(v) => {
                                        if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                            return Some((Ok(delta.to_string()), (byte_stream, buf)));
                                        }
                                        // 非 content delta（如 role、usage）→ 繼續
                                    }
                                    Err(_) => return Some((Err(LlmError::Sse(data)), (byte_stream, buf))),
                                }
                            }
                        }
                        Some(Err(e)) => return Some((Err(LlmError::Http(e)), (byte_stream, buf))),
                        None => return None,
                    }
                }
            },
        );
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_prepends_system_prompt_once() {
        let client = LlmClient::new(LlmProvider {
            name: "test".into(),
            base_url: "http://localhost".into(),
            model: "test".into(),
            api_key: None,
        });
        let opts = CompletionOptions {
            system_prompt: Some("workspace context".into()),
            ..Default::default()
        };

        let body = client.build_body(vec![ChatMessage::user("question")], &opts, false);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0], serde_json::json!({"role": "system", "content": "workspace context"}));
        assert_eq!(messages[1], serde_json::json!({"role": "user", "content": "question"}));

        let body = client.build_body(
            vec![ChatMessage::system("workspace context"), ChatMessage::user("question")],
            &opts,
            false,
        );
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }
}
