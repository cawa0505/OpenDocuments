use std::path::Path;
use std::fs;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};

pub struct JupyterParser;

#[async_trait]
impl DocumentParser for JupyterParser {
    fn name(&self) -> &'static str {
        "JupyterParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["ipynb"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read ipynb file: {}", e))?;

        let json_val: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Invalid Jupyter JSON format: {}", e))?;

        let cells = json_val.get("cells")
            .and_then(|c| c.as_array())
            .ok_or_else(|| "No cells found in jupyter notebook".to_string())?;

        let mut chunks = vec![];

        for (cell_idx, cell) in cells.iter().enumerate() {
            let cell_type = cell.get("cell_type").and_then(|t| t.as_str()).unwrap_or("code");
            let source_array = cell.get("source").and_then(|s| s.as_array());
            
            let mut source_text = String::new();
            if let Some(arr) = source_array {
                for line_val in arr {
                    if let Some(line) = line_val.as_str() {
                        source_text.push_str(line);
                    }
                }
            }

            if source_text.trim().is_empty() {
                continue;
            }

            let chunk_type = match cell_type {
                "markdown" => ChunkType::Semantic,
                _ => ChunkType::CodeAst,
            };

            chunks.push(DocumentChunk {
                chunk_type,
                content: source_text,
                workspace_id: workspace_id.to_string(),
                collection_id: collection_id.to_string(),
                file_path: file_path.to_string_lossy().into_owned(),
                relevance_score: None,
                metadata: serde_json::json!({
                    "cell_index": cell_idx,
                    "cell_type": cell_type,
                }),
            });
        }

        Ok(chunks)
    }
}
