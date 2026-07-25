use std::path::Path;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use async_trait::async_trait;
use lopdf::Document;

pub struct PdfParser;

#[async_trait]
impl DocumentParser for PdfParser {
    fn name(&self) -> &'static str {
        "lopdf PDF Parser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["pdf"]
    }

    async fn parse(&self, file_path: &Path, workspace_id: &str, collection_id: &str) -> Result<Vec<DocumentChunk>, String> {
        let mut chunks = Vec::new();
        let path_str = file_path.to_string_lossy().to_string();

        let doc = Document::load(file_path)
            .map_err(|e| format!("Failed to read PDF container: {}", e))?;

        let pages = doc.get_pages();

        for (page_idx, _page_id) in pages.iter() {
            let page_num = *page_idx as usize;
            
            // lopdf 0.31 的 extract_text 接受 &[u32] 作為 1-indexed 頁碼陣列
            if let Ok(text) = doc.extract_text(&[*page_idx]) {
                if text.trim().is_empty() {
                    continue;
                }

                let mut current_chunk_content = String::new();
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    current_chunk_content.push_str(trimmed);
                    current_chunk_content.push('\n');

                    if current_chunk_content.len() > 800 {
                        chunks.push(DocumentChunk {
                            chunk_type: ChunkType::Semantic,
                            content: current_chunk_content.clone(),
                            workspace_id: workspace_id.to_string(),
                            collection_id: collection_id.to_string(),
                            file_path: path_str.clone(),
                            relevance_score: None,
                            metadata: serde_json::json!({
                                "page_number": page_num
                            }),
                        });
                        current_chunk_content = String::new();
                    }
                }

                if !current_chunk_content.is_empty() {
                    chunks.push(DocumentChunk {
                        chunk_type: ChunkType::Semantic,
                        content: current_chunk_content,
                        workspace_id: workspace_id.to_string(),
                        collection_id: collection_id.to_string(),
                        file_path: path_str.clone(),
                        relevance_score: None,
                        metadata: serde_json::json!({
                            "page_number": page_num
                        }),
                    });
                }
            }
        }

        Ok(chunks)
    }
}
