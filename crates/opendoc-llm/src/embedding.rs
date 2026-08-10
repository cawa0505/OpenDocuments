//! opendoc-llm embedding — OpenAI-compatible `/embeddings` BYOK client。
//! 與 chat client 共用 LlmProvider；獨立 provider 行（llm_providers.kind='embedding'）
//! 以避免與 chat 的 is_active 互踩。

use async_trait::async_trait;
use opendoc_types::EmbeddingProvider;
use serde::Deserialize;
use crate::LlmProvider;

pub struct ByokEmbeddingProvider {
    provider: LlmProvider,
    http: reqwest::Client,
    dim: usize,
}

impl ByokEmbeddingProvider {
    pub fn new(provider: LlmProvider, dim: usize) -> Self {
        Self { provider, http: reqwest::Client::new(), dim }
    }

    fn url(&self) -> String {
        let base = self.provider.base_url.trim_end_matches('/');
        format!("{base}/embeddings")
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for ByokEmbeddingProvider {
    fn dim(&self) -> usize { self.dim }

    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let body = serde_json::json!({
            "model": self.provider.model,
            "input": texts,
        });
        let mut req = self.http.post(self.url()).json(&body);
        if let Some(k) = &self.provider.api_key {
            req = req.header("Authorization", format!("Bearer {k}"));
        }
        let resp = req.send().await.map_err(|e| format!("embed HTTP 失敗: {e}"))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("embed 回傳錯誤 (HTTP {status}): {body}"));
        }
        let parsed: EmbeddingResponse = resp.json().await
            .map_err(|e| format!("embed 回應解析失敗: {e}"))?;
        let mut out = Vec::with_capacity(parsed.data.len());
        for item in parsed.data {
            if item.embedding.len() != self.dim {
                return Err(format!(
                    "embed 維度不一致：期望 {} 實得 {}（模型可能與 config.embedding_dim 不符）",
                    self.dim, item.embedding.len()
                ));
            }
            out.push(item.embedding);
        }
        if out.len() != texts.len() {
            return Err(format!("embed 回傳筆數 {} ≠ 輸入 {}", out.len(), texts.len()));
        }
        Ok(out)
    }
}