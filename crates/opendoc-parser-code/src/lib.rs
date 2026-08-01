use std::path::Path;
use std::fs;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};

pub struct CodeParser;

#[async_trait]
impl DocumentParser for CodeParser {
    fn name(&self) -> &'static str {
        "CodeParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["rs", "py", "ts", "js", "go", "java", "cpp", "h", "cs", "sh", "yaml", "yml", "json", "toml"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read source file: {}", e))?;

        let ext = file_path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt")
            .to_lowercase();

        let mut chunks = vec![];
        let mut current_block = String::new();
        let mut line_count = 0;

        for line in content.lines() {
            current_block.push_str(line);
            current_block.push('\n');
            line_count += 1;

            // 每 50 行程式碼或遇到大括號結尾的行作為自適應分片點
            if line_count >= 50 || (line_count >= 20 && (line.trim() == "}" || line.trim() == "end")) {
                chunks.push(DocumentChunk {
                    chunk_type: ChunkType::CodeAst,
                    content: current_block.clone(),
                    workspace_id: workspace_id.to_string(),
                    collection_id: collection_id.to_string(),
                    file_path: file_path.to_string_lossy().into_owned(),
                    relevance_score: None,
                    metadata: serde_json::json!({
                        "language": ext,
                        "line_range": format!("{}-{}", chunks.len() * 50 + 1, chunks.len() * 50 + line_count),
                    }),
                });
                current_block.clear();
                line_count = 0;
            }
        }

        if !current_block.is_empty() {
            chunks.push(DocumentChunk {
                chunk_type: ChunkType::CodeAst,
                content: current_block,
                workspace_id: workspace_id.to_string(),
                collection_id: collection_id.to_string(),
                file_path: file_path.to_string_lossy().into_owned(),
                relevance_score: None,
                metadata: serde_json::json!({
                    "language": ext,
                    "line_range": "final_block"
                }),
            });
        }

        Ok(chunks)
    }
}
