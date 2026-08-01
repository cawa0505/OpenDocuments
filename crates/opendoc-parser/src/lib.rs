use std::path::Path;
use opendoc_types::{DocumentChunk, DocumentParser};
use opendoc_parser_xlsx::ExcelParser;
use opendoc_parser_docx::WordParser;
use opendoc_parser_pdf::PdfParser;
use opendoc_parser_text::TextParser;
use opendoc_parser_code::CodeParser;
use opendoc_parser_jupyter::JupyterParser;
use opendoc_parser_email::EmailParser;
use opendoc_parser_html::HtmlParser;
use opendoc_parser_pptx::PptxParser;

/// 💡 雙軌高彈性 Parser 路由器：支援 original_name 回退，100% 防止 WebUI 上傳暫存路徑無副檔名或大寫副檔名導致的解析失敗。
pub async fn parse_file(
    file_path: &Path,
    original_name: Option<&str>,
    workspace_id: &str,
    collection_id: &str,
) -> Result<Vec<DocumentChunk>, String> {
    // 1. 優先嘗試從實體暫存路徑中獲取副檔名
    let mut ext = file_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();

    // 2. 💡 【第一重防護】如果實體檔案沒有副檔名（常見於 multipart upload 隨機暫存 hash），
    // 或者是無效副檔名，則自動 fallback 使用 original_name！
    if (ext.is_empty() || ext.len() > 5) && original_name.is_some() {
        let orig = original_name.unwrap();
        if let Some(orig_ext) = Path::new(orig).extension().and_then(|e| e.to_str()) {
            ext = orig_ext.to_lowercase();
        }
    }

    // 3. 💡 【第二重防護】100% 強制副檔名字串轉全小寫進行強型別匹配
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
        "md" | "markdown" | "txt" => {
            let parser = TextParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "rs" | "py" | "ts" | "js" | "go" | "java" | "cpp" | "h" | "cs" | "sh" | "yaml" | "yml" | "json" | "toml" => {
            let parser = CodeParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "ipynb" => {
            let parser = JupyterParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "eml" | "msg" => {
            let parser = EmailParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "html" | "htm" => {
            let parser = HtmlParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        "pptx" => {
            let parser = PptxParser;
            parser.parse(file_path, workspace_id, collection_id).await
        }
        _ => Err(format!(
            "No pure Rust parser plugin registered for extension: .{} (Original name: {:?}, physical path: {:?})",
            ext, original_name, file_path
        )),
    }
}
