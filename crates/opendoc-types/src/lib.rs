use serde::{Deserialize, Serialize};
use std::path::Path;
use async_trait::async_trait;

pub mod protocol;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ChunkType {
    Semantic,
    CodeAst,
    Table,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DocumentChunk {
    pub chunk_type: ChunkType,
    pub content: String,
    pub workspace_id: String,
    pub collection_id: String,
    pub file_path: String,
    pub relevance_score: Option<f32>, // Reranker 二次重排後的分數
    pub metadata: serde_json::Value,  // 存放 docx 父標題、xlsx 行列範圍、pdf 頁碼
}

/// 檢索核心與 Reranker 的參數規格
#[derive(Debug, Deserialize)]
pub struct SearchQueryParams {
    pub query: String,
    pub workspace_id: String,
    pub collection_ids: Option<Vec<String>>,
    pub score_threshold: f32, // Score Filter 保險絲門檻
    pub limit: usize,
}

#[async_trait]
pub trait DocumentParser {
    fn name(&self) -> &'static str;
    fn supported_extensions(&self) -> Vec<&'static str>;
    async fn parse(&self, file_path: &Path, workspace_id: &str, collection_id: &str) -> Result<Vec<DocumentChunk>, String>;
}

/// 向量化供應商抽象：BYOK HTTP 與 in-process ONNX (fastembed) 兩條實作共用此契約。
/// ponytail: 只暴露 embed + dim；模型選擇與傳輸細節由各 impl 私有。
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 回傳向量維度；LanceDB 表的 FixedSizeList 維度必須與此一致。
    fn dim(&self) -> usize;
    /// 批次嵌入；回傳向量大小的順序與輸入 texts 嚴格對齊。
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}
