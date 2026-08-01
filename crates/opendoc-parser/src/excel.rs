use std::path::Path;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use async_trait::async_trait;
use calamine::{Reader, open_workbook, Xlsx, Data};

pub struct ExcelParser;

#[async_trait]
impl DocumentParser for ExcelParser {
    fn name(&self) -> &'static str {
        "Calamine Excel Parser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["xlsx", "xls", "ods"]
    }

    async fn parse(&self, file_path: &Path, workspace_id: &str, collection_id: &str) -> Result<Vec<DocumentChunk>, String> {
        let mut chunks = Vec::new();
        let path_str = file_path.to_string_lossy().to_string();

        let mut workbook: Xlsx<_> = open_workbook(file_path)
            .map_err(|e| format!("Failed to open Excel workbook: {}", e))?;

        let sheet_names = workbook.sheet_names().to_vec();

        for sheet_name in sheet_names {
            if let Ok(range) = workbook.worksheet_range(&sheet_name) {
                let mut current_chunk_content = String::new();
                let mut row_start = 0;
                let mut char_count = 0;

                let max_cols = range.width();

                for (row_idx, row) in range.rows().enumerate() {
                    let mut row_str = String::new();
                    for (col_idx, cell) in row.iter().enumerate() {
                        let cell_val = match cell {
                            Data::Empty => String::new(),
                            Data::String(s) => s.clone(),
                            Data::Float(f) => f.to_string(),
                            Data::Int(i) => i.to_string(),
                            Data::Bool(b) => b.to_string(),
                            Data::Error(e) => format!("[Error: {:?}]", e),
                            _ => String::new(),
                        };
                        row_str.push_str(&cell_val);
                        if col_idx < max_cols - 1 {
                            row_str.push_str(" | ");
                        }
                    }

                    let row_len = row_str.len();
                    
                    if char_count + row_len > 1200 {
                        if !current_chunk_content.is_empty() {
                            chunks.push(DocumentChunk {
                                chunk_type: ChunkType::Table,
                                content: current_chunk_content.clone(),
                                workspace_id: workspace_id.to_string(),
                                collection_id: collection_id.to_string(),
                                file_path: path_str.clone(),
                                relevance_score: None,
                                metadata: serde_json::json!({
                                    "sheet_name": sheet_name,
                                    "row_range": format!("{}-{}", row_start, row_idx),
                                    "total_cols": max_cols
                                }),
                            });
                        }
                        current_chunk_content = String::new();
                        row_start = row_idx;
                        char_count = 0;
                    }

                    current_chunk_content.push_str(&row_str);
                    current_chunk_content.push('\n');
                    char_count += row_len + 1;
                }

                if !current_chunk_content.is_empty() {
                    chunks.push(DocumentChunk {
                        chunk_type: ChunkType::Table,
                        content: current_chunk_content,
                        workspace_id: workspace_id.to_string(),
                        collection_id: collection_id.to_string(),
                        file_path: path_str.clone(),
                        relevance_score: None,
                        metadata: serde_json::json!({
                            "sheet_name": sheet_name,
                            "row_range": format!("{}-end", row_start),
                            "total_cols": max_cols
                        }),
                    });
                }
            }
        }

        Ok(chunks)
    }
}
