use std::path::Path;
use std::fs;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use mail_parser::MessageParser;

pub struct EmailParser;

#[async_trait]
impl DocumentParser for EmailParser {
    fn name(&self) -> &'static str {
        "EmailParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["eml", "msg"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let bytes = fs::read(file_path)
            .map_err(|e| format!("Failed to read email bytes: {}", e))?;

        let message = MessageParser::default().parse(&bytes)
            .ok_or_else(|| "Failed to parse MIME email structure".to_string())?;

        let from_str = message.from()
            .and_then(|addr| {
                if let mail_parser::Address::List(list) = addr {
                    list.first().map(|a| a.address.as_ref().map(|s| s.to_string()).unwrap_or_else(|| "Unknown".to_string()))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Unknown".to_string());

        let subject_str = message.subject().unwrap_or("No Subject").to_string();
        let body_str = message.body_text(0).unwrap_or_default().to_string();

        let header_info = format!(
            "寄件人: {}\n主題: {}\n日期: {:?}\n\n",
            from_str,
            subject_str,
            message.date().map(|d| d.to_rfc3339()).unwrap_or_else(|| "N/A".to_string())
        );

        let combined_text = format!("{}{}", header_info, body_str);

        // 普通自適應分片
        let mut chunks = vec![];
        let mut current_block = String::new();
        let mut current_chars = 0;

        for line in combined_text.lines() {
            current_block.push_str(line);
            current_block.push('\n');
            current_chars += line.len();

            if current_chars >= 800 {
                chunks.push(DocumentChunk {
                    chunk_type: ChunkType::Semantic,
                    content: current_block.clone(),
                    workspace_id: workspace_id.to_string(),
                    collection_id: collection_id.to_string(),
                    file_path: file_path.to_string_lossy().into_owned(),
                    relevance_score: None,
                    metadata: serde_json::json!({
                        "subject": subject_str,
                        "from": from_str,
                    }),
                });
                current_block.clear();
                current_chars = 0;
            }
        }

        if !current_block.is_empty() {
            chunks.push(DocumentChunk {
                chunk_type: ChunkType::Semantic,
                content: current_block,
                workspace_id: workspace_id.to_string(),
                collection_id: collection_id.to_string(),
                file_path: file_path.to_string_lossy().into_owned(),
                relevance_score: None,
                metadata: serde_json::json!({
                    "subject": subject_str,
                    "from": from_str,
                }),
            });
        }

        Ok(chunks)
    }
}
