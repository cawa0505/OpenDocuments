use std::path::Path;
use std::fs;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use scraper::{Html, Selector};

pub struct HtmlParser;

#[async_trait]
impl DocumentParser for HtmlParser {
    fn name(&self) -> &'static str {
        "HtmlParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["html", "htm"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read html file: {}", e))?;

        let document = Html::parse_document(&content);
        
        // 排除掉不屬於正文的主流噪聲標記
        let script_style_sel = Selector::parse("script, style, iframe, footer, nav, header").unwrap();
        let mut clean_doc = document.clone();
        
        // 我們這裡用一個簡潔的高速正文文本抽取策略
        let body_sel = Selector::parse("body, main, article").unwrap();
        let mut body_text = String::new();
        
        // 如果有 body 則提取 body 裡的文字，否則提取全域文字
        let targets = if clean_doc.select(&body_sel).count() > 0 {
            clean_doc.select(&body_sel).collect::<Vec<_>>()
        } else {
            vec![clean_doc.root_element()]
        };

        for elem in targets {
            for text_node in elem.text() {
                let trimmed = text_node.trim();
                if !trimmed.is_empty() {
                    body_text.push_str(trimmed);
                    body_text.push('\n');
                }
            }
        }

        // 普通自適應分片
        let mut chunks = vec![];
        let mut current_block = String::new();
        let mut current_chars = 0;

        for line in body_text.lines() {
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
                        "type": "webpage"
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
                    "type": "webpage"
                }),
            });
        }

        Ok(chunks)
    }
}
