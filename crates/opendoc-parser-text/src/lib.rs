use std::path::Path;
use std::fs;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};

pub struct TextParser;

#[async_trait]
impl DocumentParser for TextParser {
    fn name(&self) -> &'static str {
        "TextParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["txt", "md", "markdown"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let is_markdown = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .map(|ext| ext == "md" || ext == "markdown")
            .unwrap_or(false);

        if is_markdown {
            Ok(self.parse_markdown(&content, workspace_id, collection_id, file_path.to_string_lossy().as_ref()))
        } else {
            Ok(self.parse_plain_text(&content, workspace_id, collection_id, file_path.to_string_lossy().as_ref()))
        }
    }
}

impl TextParser {
    fn parse_markdown(
        &self,
        content: &str,
        workspace_id: &str,
        collection_id: &str,
        file_path: &str,
    ) -> Vec<DocumentChunk> {
        let mut chunks = vec![];
        let mut header_stack: Vec<String> = vec![];
        let mut current_chunk = String::new();
        let mut current_char_count = 0;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // 💡 標題層疊 (H1-H6)
            if trimmed.starts_with('#') {
                let header_level = trimmed.chars().take_while(|&c| c == '#').count();
                if header_level >= 1 && header_level <= 6 {
                    let header_text = trimmed[header_level..].trim().to_string();
                    
                    // 壓入標題堆疊，清除更深層標題
                    if header_stack.len() >= header_level {
                        header_stack.truncate(header_level - 1);
                    }
                    header_stack.push(header_text);

                    // 遇到新標題，且目前 Chunk 有內容時，立刻分片
                    if !current_chunk.is_empty() {
                        chunks.push(DocumentChunk {
                            chunk_type: ChunkType::Semantic,
                            content: current_chunk.clone(),
                            workspace_id: workspace_id.to_string(),
                            collection_id: collection_id.to_string(),
                            file_path: file_path.to_string(),
                            relevance_score: None,
                            metadata: serde_json::json!({
                                "headers": header_stack.clone(),
                            }),
                        });
                        current_chunk.clear();
                        current_char_count = 0;
                    }
                    continue;
                }
            }

            // 自適應 Character 大小分片 (500 字元區間)
            current_chunk.push_str(line);
            current_chunk.push('\n');
            current_char_count += line.len();

            if current_char_count >= 600 {
                chunks.push(DocumentChunk {
                    chunk_type: ChunkType::Semantic,
                    content: current_chunk.clone(),
                    workspace_id: workspace_id.to_string(),
                    collection_id: collection_id.to_string(),
                    file_path: file_path.to_string(),
                    relevance_score: None,
                    metadata: serde_json::json!({
                        "headers": header_stack.clone(),
                    }),
                });
                current_chunk.clear();
                current_char_count = 0;
            }
        }

        // 剩餘殘片
        if !current_chunk.is_empty() {
            chunks.push(DocumentChunk {
                chunk_type: ChunkType::Semantic,
                content: current_chunk,
                workspace_id: workspace_id.to_string(),
                collection_id: collection_id.to_string(),
                file_path: file_path.to_string(),
                relevance_score: None,
                metadata: serde_json::json!({
                    "headers": header_stack,
                }),
            });
        }

        chunks
    }

    fn parse_plain_text(
        &self,
        content: &str,
        workspace_id: &str,
        collection_id: &str,
        file_path: &str,
    ) -> Vec<DocumentChunk> {
        let mut chunks = vec![];
        let mut current_chunk = String::new();
        let mut current_char_count = 0;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            current_chunk.push_str(line);
            current_chunk.push('\n');
            current_char_count += line.len();

            if current_char_count >= 800 {
                chunks.push(DocumentChunk {
                    chunk_type: ChunkType::Semantic,
                    content: current_chunk.clone(),
                    workspace_id: workspace_id.to_string(),
                    collection_id: collection_id.to_string(),
                    file_path: file_path.to_string(),
                    relevance_score: None,
                    metadata: serde_json::json!({
                        "type": "plain_text"
                    }),
                });
                current_chunk.clear();
                current_char_count = 0;
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(DocumentChunk {
                chunk_type: ChunkType::Semantic,
                content: current_chunk,
                workspace_id: workspace_id.to_string(),
                collection_id: collection_id.to_string(),
                file_path: file_path.to_string(),
                relevance_score: None,
                metadata: serde_json::json!({
                    "type": "plain_text"
                }),
            });
        }

        chunks
    }
}
