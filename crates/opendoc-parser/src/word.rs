use std::path::Path;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use async_trait::async_trait;
use docx_rs::{read_docx, DocumentChild, ParagraphChild, RunChild};

pub struct WordParser;

#[async_trait]
impl DocumentParser for WordParser {
    fn name(&self) -> &'static str {
        "docx-rs Word Parser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["docx"]
    }

    async fn parse(&self, file_path: &Path, workspace_id: &str, collection_id: &str) -> Result<Vec<DocumentChunk>, String> {
        let mut chunks = Vec::new();
        let path_str = file_path.to_string_lossy().to_string();

        let file_data = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read Docx file bytes: {}", e))?;

        let docx = read_docx(&file_data)
            .map_err(|e| format!("Failed to parse DOCX container: {}", e))?;

        let mut heading_stack: Vec<String> = vec![String::new(); 6];
        let mut current_chunk_content = String::new();
        let mut current_heading_chain: Vec<String> = Vec::new();

        for child in docx.document.children {
            match child {
                DocumentChild::Paragraph(p) => {
                    let mut paragraph_text = String::new();
                    for p_child in p.children {
                        if let ParagraphChild::Run(run) = p_child {
                            for r_child in &run.children {
                                if let RunChild::Text(t) = r_child {
                                    paragraph_text.push_str(&t.text);
                                }
                            }
                        }
                    }

                    if paragraph_text.trim().is_empty() {
                        continue;
                    }

                    let style = p.property.style.map(|s| s.val).unwrap_or_default().to_lowercase();
                    
                    let heading_level = if style.contains("heading 1") || style == "h1" {
                        Some(0)
                    } else if style.contains("heading 2") || style == "h2" {
                        Some(1)
                    } else if style.contains("heading 3") || style == "h3" {
                        Some(2)
                    } else if style.contains("heading 4") || style == "h4" {
                        Some(3)
                    } else if style.contains("heading 5") || style == "h5" {
                        Some(4)
                    } else if style.contains("heading 6") || style == "h6" {
                        Some(5)
                    } else {
                        None
                    };

                    if let Some(level) = heading_level {
                        if !current_chunk_content.is_empty() {
                            chunks.push(DocumentChunk {
                                chunk_type: ChunkType::Semantic,
                                content: current_chunk_content.clone(),
                                workspace_id: workspace_id.to_string(),
                                collection_id: collection_id.to_string(),
                                file_path: path_str.clone(),
                                relevance_score: None,
                                metadata: serde_json::json!({
                                    "heading_chain": current_heading_chain.clone()
                                }),
                            });
                            current_chunk_content = String::new();
                        }

                        heading_stack[level] = paragraph_text.clone();
                        for i in (level + 1)..6 {
                            heading_stack[i] = String::new();
                        }

                        current_heading_chain = heading_stack.iter()
                            .filter(|h| !h.is_empty())
                            .cloned()
                            .collect();
                    } else {
                        current_chunk_content.push_str(&paragraph_text);
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
                                    "heading_chain": current_heading_chain.clone()
                                }),
                            });
                            current_chunk_content = String::new();
                        }
                    }
                }
                _ => {}
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
                    "heading_chain": current_heading_chain
                }),
            });
        }

        Ok(chunks)
    }
}
