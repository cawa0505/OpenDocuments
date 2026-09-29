//! 雲端硬碟 connector：list / fetch / export，供索引管線取回檔案位元組。
//!
//! 無狀態設計：token 一律以參數傳入，本 crate 不持久化、不記錄 token。
//! MVP 僅 Google Drive（v3）；OneDrive 走同一 trait 於後續加入。

use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// 單檔下載上限（預設 25MB；超過回 `CloudError::TooLarge`，呼叫端記 skipped 不中斷批次）。
pub const DEFAULT_MAX_BYTES: u64 = 25 * 1024 * 1024;

/// 雲端檔案條目（清單與索引共用形狀）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloudFile {
    pub id: String,
    pub name: String,
    pub mime: String,
    /// 雲端以字串回傳大小；原生 Google 文件（無固定大小）為 None。
    pub size: Option<u64>,
}

/// 分頁清單回應。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudPage {
    #[serde(default)]
    pub files: Vec<CloudFile>,
    pub next_page_token: Option<String>,
}

/// Connector 錯誤。
#[derive(Debug)]
pub enum CloudError {
    /// 傳輸層（DNS、連線、timeout）。
    Http(String),
    /// Provider 回非 2xx 或語意錯誤。
    Api(String),
    /// 檔案超過大小上限（帶實際或宣告大小）。
    TooLarge(u64),
}

impl std::fmt::Display for CloudError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(e) => write!(f, "cloud http: {e}"),
            Self::Api(e) => write!(f, "cloud api: {e}"),
            Self::TooLarge(n) => write!(f, "file exceeds size limit: {n} bytes"),
        }
    }
}

impl std::error::Error for CloudError {}

pub type Result<T> = std::result::Result<T, CloudError>;

/// 雲端硬碟 connector trait。實作必須無狀態：token 只從參數進出。
#[async_trait]
pub trait CloudDrive: Send + Sync {
    /// 列出資料夾內容（`folder_id` 為 None 時列全部未進垃圾桶檔案），支援分頁。
    async fn list_files(
        &self,
        token: &str,
        folder_id: Option<&str>,
        page_token: Option<&str>,
    ) -> Result<CloudPage>;

    /// 取回檔案位元組。原生可匯出格式（Docs）內部走 export（DOCX）；
    /// 不可匯出的原生格式（Sheets/Slides 等）直接回 `CloudError::Api`。
    /// 超過大小上限回 `CloudError::TooLarge`。
    async fn fetch_bytes(&self, token: &str, file: &CloudFile) -> Result<Vec<u8>>;
}

/// 是否為 Google 原生格式（`application/vnd.google-apps.*`）。
pub fn is_google_native(mime: &str) -> bool {
    mime.starts_with("application/vnd.google-apps.")
}

/// 原生格式 → 可索引的 export mime。目前僅 Docs → DOCX；
/// Sheets/Slides 不支援索引（None，呼叫端記 skipped）。
pub fn native_export_mime(mime: &str) -> Option<&'static str> {
    match mime {
        "application/vnd.google-apps.document" => Some(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        _ => None,
    }
}

/// Google Drive v3 connector。
pub struct GoogleDriveConnector {
    http: reqwest::Client,
    base_url: String,
    max_bytes: u64,
    /// 429/503 退避基底（指數 `<<attempt`）；測試可調小。
    backoff_ms: u64,
}

impl Default for GoogleDriveConnector {
    fn default() -> Self {
        Self::new()
    }
}

impl GoogleDriveConnector {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .expect("reqwest client"),
            base_url: "https://www.googleapis.com".to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            backoff_ms: 500,
        }
    }

    /// 測試用：指向本機 wire-shape 測試機。
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// 測試用：調小下載上限。
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// 測試用：調小退避基底。
    pub fn with_backoff_ms(mut self, backoff_ms: u64) -> Self {
        self.backoff_ms = backoff_ms;
        self
    }

    fn drive_url(&self, path: &str) -> String {
        format!("{}/drive/v3{}", self.base_url, path)
    }

    /// 送出請求；429/503 以指數退避重試（至多 3 次），其他非 2xx 直接回錯。
    async fn send_with_backoff(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let mut attempt: u32 = 0;
        loop {
            // reqwest 0.11 RequestBuilder 無 Clone；try_clone 對無 body 的 GET 恆成功
            let clone = req
                .try_clone()
                .ok_or_else(|| CloudError::Http("request not cloneable".into()))?;
            let resp = clone
                .send()
                .await
                .map_err(|e| CloudError::Http(e.to_string()))?;
            let retryable = resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                || resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE;
            if retryable && attempt < 3 {
                tokio::time::sleep(Duration::from_millis(self.backoff_ms << attempt)).await;
                attempt += 1;
                continue;
            }
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let snippet: String = body.chars().take(200).collect();
                return Err(CloudError::Api(format!("drive HTTP {status}: {snippet}")));
            }
            return Ok(resp);
        }
    }
}

#[derive(Deserialize)]
struct DriveList {
    #[serde(default)]
    files: Vec<DriveFile>,
    #[serde(rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime: String,
    size: Option<String>,
}

impl From<DriveFile> for CloudFile {
    fn from(f: DriveFile) -> Self {
        Self {
            id: f.id,
            name: f.name,
            mime: f.mime,
            size: f.size.and_then(|s| s.parse().ok()),
        }
    }
}

#[async_trait]
impl CloudDrive for GoogleDriveConnector {
    async fn list_files(
        &self,
        token: &str,
        folder_id: Option<&str>,
        page_token: Option<&str>,
    ) -> Result<CloudPage> {
        let q = match folder_id {
            Some(id) => format!("'{id}' in parents and trashed=false"),
            None => "trashed=false".to_string(),
        };
        let mut req = self
            .http
            .get(self.drive_url("/files"))
            .bearer_auth(token)
            .query(&[
                ("q", q.as_str()),
                ("pageSize", "200"),
                ("orderBy", "folder,name"),
                ("fields", "nextPageToken,files(id,name,mimeType,size)"),
            ]);
        if let Some(t) = page_token {
            req = req.query(&[("pageToken", t)]);
        }
        let resp = self.send_with_backoff(req).await?;
        let list: DriveList = resp
            .json()
            .await
            .map_err(|e| CloudError::Api(format!("bad list payload: {e}")))?;
        Ok(CloudPage {
            files: list.files.into_iter().map(Into::into).collect(),
            next_page_token: list.next_page_token,
        })
    }

    async fn fetch_bytes(&self, token: &str, file: &CloudFile) -> Result<Vec<u8>> {
        let req = if is_google_native(&file.mime) {
            let export_mime = native_export_mime(&file.mime).ok_or_else(|| {
                CloudError::Api(format!("native format not indexable: {}", file.mime))
            })?;
            self.http
                .get(self.drive_url(&format!("/files/{}", file.id)))
                .bearer_auth(token)
                .query(&[("mimeType", export_mime)])
        } else {
            self.http
                .get(self.drive_url(&format!("/files/{}", file.id)))
                .bearer_auth(token)
                .query(&[("alt", "media")])
        };
        let resp = self.send_with_backoff(req).await?;
        if let Some(len) = resp.content_length() {
            if len > self.max_bytes {
                return Err(CloudError::TooLarge(len));
            }
        }
        let mut resp = resp;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| CloudError::Http(e.to_string()))?
        {
            if buf.len() as u64 + chunk.len() as u64 > self.max_bytes {
                return Err(CloudError::TooLarge(buf.len() as u64 + chunk.len() as u64));
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, Query, State};
    use axum::http::header::CONTENT_TYPE;
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::routing::get;
    use axum::{Json, Router};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    const DOCX: &[u8] = b"PK\x03\x04fake-docx-bytes";
    const MARKDOWN: &[u8] = "# 講義\n\nRAG 測試內容".as_bytes();

    /// wire-shape 測試機狀態：請求計數＋是否首發 429（退避測試）。
    #[derive(Clone)]
    struct SrvState {
        hits: Arc<AtomicUsize>,
        four2nine_first: bool,
    }

    async fn spawn_server() -> String {
        spawn_server_with(false).await
    }

    async fn spawn_server_with(four2nine_first: bool) -> String {
        let state = SrvState {
            hits: Arc::new(AtomicUsize::new(0)),
            four2nine_first,
        };
        let app = Router::new()
            .route("/drive/v3/files", get(list_files))
            .route("/drive/v3/files/:id", get(get_file))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    async fn list_files(
        State(s): State<SrvState>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Response {
        if s.four2nine_first && s.hits.fetch_add(1, Ordering::SeqCst) == 0 {
            return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        let (files, token) = if q.get("pageToken").map(String::as_str) == Some("t2") {
            (serde_json::json!([]), serde_json::Value::Null)
        } else {
            (
                serde_json::json!([
                    {"id": "md1", "name": "講義.md", "mimeType": "text/markdown", "size": "34"},
                    {"id": "doc1", "name": "協同文件", "mimeType": "application/vnd.google-apps.document"},
                    {"id": "sheet1", "name": "試算表", "mimeType": "application/vnd.google-apps.spreadsheet"}
                ]),
                serde_json::Value::String("t2".into()),
            )
        };
        Json(serde_json::json!({"files": files, "nextPageToken": token})).into_response()
    }

    async fn get_file(
        State(_s): State<SrvState>,
        Path(id): Path<String>,
        Query(q): Query<HashMap<String, String>>,
    ) -> Response {
        if q.contains_key("mimeType") {
            // export 端點：回 DOCX 位元組
            return (
                [(
                    CONTENT_TYPE,
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                )],
                DOCX.to_vec(),
            )
                .into_response();
        }
        if q.get("alt").map(String::as_str) == Some("media") {
            return (
                [(CONTENT_TYPE, "application/octet-stream")],
                MARKDOWN.to_vec(),
            )
                .into_response();
        }
        Json(serde_json::json!({"id": id, "name": "file", "mimeType": "text/plain"})).into_response()
    }

    fn md_file() -> CloudFile {
        CloudFile {
            id: "md1".into(),
            name: "講義.md".into(),
            mime: "text/markdown".into(),
            size: Some(34),
        }
    }

    fn doc_file() -> CloudFile {
        CloudFile {
            id: "doc1".into(),
            name: "協同文件".into(),
            mime: "application/vnd.google-apps.document".into(),
            size: None,
        }
    }

    fn sheet_file() -> CloudFile {
        CloudFile {
            id: "sheet1".into(),
            name: "試算表".into(),
            mime: "application/vnd.google-apps.spreadsheet".into(),
            size: None,
        }
    }

    #[tokio::test]
    async fn list_parses_real_wire_shape() {
        let base = spawn_server().await;
        let conn = GoogleDriveConnector::new().with_base_url(base);
        let page = conn.list_files("tok", Some("root"), None).await.unwrap();
        assert_eq!(page.files.len(), 3);
        assert_eq!(page.files[0].size, Some(34));
        assert_eq!(page.files[2].mime, "application/vnd.google-apps.spreadsheet");
        assert_eq!(page.next_page_token.as_deref(), Some("t2"));
        // 第二頁：空清單、無下一頁
        let page2 = conn
            .list_files("tok", Some("root"), page.next_page_token.as_deref())
            .await
            .unwrap();
        assert!(page2.files.is_empty());
        assert!(page2.next_page_token.is_none());
    }

    #[tokio::test]
    async fn fetch_media_bytes() {
        let base = spawn_server().await;
        let conn = GoogleDriveConnector::new().with_base_url(base);
        let bytes = conn.fetch_bytes("tok", &md_file()).await.unwrap();
        assert_eq!(bytes, MARKDOWN);
    }

    #[tokio::test]
    async fn fetch_native_doc_exports_docx() {
        let base = spawn_server().await;
        let conn = GoogleDriveConnector::new().with_base_url(base);
        let bytes = conn.fetch_bytes("tok", &doc_file()).await.unwrap();
        assert_eq!(bytes, DOCX);
    }

    #[tokio::test]
    async fn fetch_unsupported_native_is_api_error() {
        let base = spawn_server().await;
        let conn = GoogleDriveConnector::new().with_base_url(base);
        match conn.fetch_bytes("tok", &sheet_file()).await {
            Err(CloudError::Api(msg)) => assert!(msg.contains("not indexable")),
            other => panic!("expected Api error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn oversized_returns_too_large() {
        let base = spawn_server().await;
        let conn = GoogleDriveConnector::new().with_base_url(base).with_max_bytes(8);
        match conn.fetch_bytes("tok", &md_file()).await {
            Err(CloudError::TooLarge(_)) => {}
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn backoff_retries_on_429() {
        let base = spawn_server_with(true).await;
        let conn = GoogleDriveConnector::new().with_base_url(base).with_backoff_ms(1);
        let page = conn.list_files("tok", Some("root"), None).await.unwrap();
        assert_eq!(page.files.len(), 3);
    }
}
