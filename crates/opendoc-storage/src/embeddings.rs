//! fastembed (ONNX, BGE-M3 1024 維) 離線 embedding 後端。
//! 以 `embedding-fastembed` feature 開啟。離線、本地，絕無任何外部 BYOK 端點呼叫。
//! ponytail: 模型檔首次載入自 HF repo 下載至 cache dir；之後離線。維度 1024 與 BYOK 預設 bge-m3 對齊，可共用同一張 LanceDB 表。

use std::path::PathBuf;
use std::sync::Mutex;
use async_trait::async_trait;
use fastembed::{Bgem3Embedding, Bgem3InitOptions};
use opendoc_types::EmbeddingProvider;

pub struct FastEmbedProvider {
    model: Mutex<Bgem3Embedding>,
    dim: usize,
}

impl FastEmbedProvider {
    /// `cache_dir` 為 None → fastembed 預設 cache（`~/.cache/hf`）。
    pub fn new(cache_dir: Option<PathBuf>) -> Result<Self, String> {
        let mut opts = Bgem3InitOptions::default().with_show_download_progress(false);
        if let Some(dir) = cache_dir {
            opts = opts.with_cache_dir(dir);
        }
        let model = Bgem3Embedding::try_new(opts)
            .map_err(|e| format!("fastembed 載入 BGE-M3 失敗: {e}"))?;
        Ok(Self { model: Mutex::new(model), dim: 1024 })
    }
    pub fn dim(&self) -> usize { self.dim }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let inputs: Vec<String> = texts.to_vec();
        // ponytail: BGE-M3 推理同步阻塞；block_in_place 在當前多執行緒 worker 上原地執行，避免 spawn_blocking 的 'static 所有權搬移。
        let dense = tokio::task::block_in_place(|| {
            let mut m = self.model.lock().map_err(|e| format!("embed mutex poisoned: {e}"))?;
            let out = m.embed(inputs, None).map_err(|e| format!("fastembed embed 失敗: {e}"))?;
            Ok::<_, String>(out.dense)
        })?;
        dense.into_iter().map(|v| {
            if v.len() != self.dim {
                return Err(format!("embedding 維度不符: {} ≠ {}", v.len(), self.dim));
            }
            Ok(v)
        }).collect()
    }

    fn dim(&self) -> usize { self.dim }
}