use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Hardware execution provider selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HardwareBackend {
    Cpu,
    Vulkan,
    Hip,
}

impl Default for HardwareBackend {
    fn default() -> Self {
        Self::Cpu
    }
}

/// One model instance: file + runtime backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    pub model_path: String,
    #[serde(default)]
    pub backend: HardwareBackend,
    pub device_id: Option<u32>,
    pub threads: Option<u32>,
    #[serde(default = "default_embedding_dim")]
    pub embedding_dim: usize,
}

fn default_embedding_dim() -> usize {
    1024
}

impl EngineConfig {
    pub fn new(model_path: impl Into<String>) -> Self {
        Self {
            model_path: model_path.into(),
            backend: HardwareBackend::Cpu,
            device_id: None,
            threads: None,
            embedding_dim: default_embedding_dim(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    LlamaCpp,
    FastEmbed,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Engine execution error: {0}")]
    ExecutionFailed(String),

    #[error("Model load error from path '{0}': {1}")]
    ModelLoadError(String, String),

    #[error("Operation unsupported on engine {0:?}: {1}")]
    Unsupported(EngineKind, String),

    #[error("Hardware backend unavailable: {0:?}")]
    HardwareUnavailable(HardwareBackend),

    #[error("Invalid input dimension: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },
}

/// Unified native AI backend: llama.cpp (GGUF, Vulkan/HIP/CPU) and fastembed-rs (ONNX CPU).
#[async_trait]
pub trait AiEngine: Send + Sync {
    fn engine_kind(&self) -> EngineKind;
    fn backend(&self) -> HardwareBackend;

    /// Embed a batch of texts -> dense vectors.
    async fn embed(&self, texts: Vec<String>, config: &EngineConfig) -> Result<Vec<Vec<f32>>, EngineError>;

    /// Re-rank query against candidate chunks -> (index, score) pairs, top-k.
    async fn rerank(
        &self,
        query: &str,
        candidates: &[opendoc_types::DocumentChunk],
        config: &EngineConfig,
        top_k: usize,
    ) -> Result<Vec<(usize, f32)>, EngineError>;

    /// Native SLM generation (llama.cpp only; fastembed returns Unsupported).
    async fn infer(
        &self,
        messages: Vec<opendoc_llm::ChatMessage>,
        config: &EngineConfig,
        opts: &opendoc_llm::CompletionOptions,
    ) -> Result<String, EngineError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_config_defaults() {
        let config = EngineConfig::new("models/bge-m3.onnx");
        assert_eq!(config.backend, HardwareBackend::Cpu);
        assert_eq!(config.embedding_dim, 1024);
    }

    #[test]
    fn test_engine_config_serialization() {
        let config = EngineConfig {
            model_path: "models/bge-m3.onnx".to_string(),
            backend: HardwareBackend::Vulkan,
            device_id: Some(0),
            threads: Some(4),
            embedding_dim: 1024,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: EngineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, deserialized);
    }
}
