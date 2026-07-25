use std::path::Path;
use std::fs::File;
use std::io::Read;
use async_trait::async_trait;
use opendoc_types::{DocumentChunk, ChunkType, DocumentParser};
use zip::ZipArchive;
use quick_xml::events::Event;
use quick_xml::Reader;

pub struct PptxParser;

#[async_trait]
impl DocumentParser for PptxParser {
    fn name(&self) -> &'static str {
        "PptxParser"
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["pptx"]
    }

    async fn parse(
        &self,
        file_path: &Path,
        workspace_id: &str,
        collection_id: &str,
    ) -> Result<Vec<DocumentChunk>, String> {
        let file = File::open(file_path).map_err(|e| format!("Failed to open pptx: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {}", e))?;

        let mut chunks = vec![];
        let mut slide_index = 1;

        loop {
            let slide_file_path = format!("ppt/slides/slide{}.xml", slide_index);
            let mut slide_file = match archive.by_name(&slide_file_path) {
                Ok(f) => f,
                Err(_) => break, // 讀到最後一張 Slide，退出循環
            };

            let mut xml_content = String::new();
            slide_file.read_to_string(&mut xml_content).unwrap_or(0);

            // 💡 透過 quick-xml 高速、輕量抽取 slide 內的文本
            let mut reader = Reader::from_str(&xml_content);
            reader.trim_text(true);

            let mut slide_text = String::new();
            let mut buf = Vec::new();

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Text(e)) => {
                        let text = e.unescape().map(|c| c.into_owned()).unwrap_or_default();
                        if !text.is_empty() {
                            slide_text.push_str(&text);
                            slide_text.push(' ');
                        }
                    }
                    Ok(Event::Eof) => break,
                    Err(_) => break,
                    _ => {}
                }
                buf.clear();
            }

            if !slide_text.trim().is_empty() {
                chunks.push(DocumentChunk {
                    chunk_type: ChunkType::Semantic,
                    content: slide_text,
                    workspace_id: workspace_id.to_string(),
                    collection_id: collection_id.to_string(),
                    file_path: file_path.to_string_lossy().into_owned(),
                    relevance_score: None,
                    metadata: serde_json::json!({
                        "slide_number": slide_index,
                    }),
                });
            }

            slide_index += 1;
        }

        Ok(chunks)
    }
}
