use serde::{Deserialize, Serialize};
use std::path::Path;
use async_trait::async_trait;

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
