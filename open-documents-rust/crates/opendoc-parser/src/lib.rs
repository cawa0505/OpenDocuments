use std::path::Path;
use opendoc_types::{DocumentChunk, DocumentParser};
use opendoc_parser_xlsx::ExcelParser;
use opendoc_parser_docx::WordParser;
use opendoc_parser_pdf::PdfParser;

pub async fn parse_file(file_path: &Path, workspace_id: &str, collection_id: &str) -> Result<Vec<DocumentChunk>, String> {
    let ext = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();

    match ext.as_str() {
        "xlsx" | "xls" | "ods" => {
            let parser = ExcelParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "docx" => {
            let parser = WordParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "pdf" => {
            let parser = PdfParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        _ => Err(format!("No pure Rust parser plugin registered for extension: .{}", ext)),
    }
}
