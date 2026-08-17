use crate::McpState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct UploadResponse {
    #[serde(rename = "documentId")]
    pub document_id: String,
    pub chunks: usize,
    pub status: String,
}

pub async fn upload_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    // 1. 取得 Header 中的工作空間（id 或 name 雙查）與集合識別碼 (預設 "default")
    let raw_ws = match headers
        .get("x-workspace")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(s) => s,
        None => state
            .config_manager
            .get_config()
            .await
            .model
            .default_workspace
            .clone(),
    };

    // lenient upload: auto-create if missing
    let workspace_id: String = match sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? OR name = ? LIMIT 1",
    )
    .bind(&raw_ws)
    .bind(&raw_ws)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        Some(id) => id,
        None => {
            let new_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)",
            )
            .bind(&new_id)
            .bind(&raw_ws)
            .execute(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            new_id
        }
    };

    let collection_id = headers
        .get("x-collection")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("default")
        .to_string();

    let mut original_name = None;
    let mut file_bytes = Vec::new();

    // 2. 讀取 multipart 欄位
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(filename) = field.file_name() {
                original_name = Some(filename.to_string());
            }
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
                .to_vec();
        }
    }

    if file_bytes.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "上傳檔案內容不可為空".to_string()));
    }

    // Calculate SHA-256 hash of file content
    let mut hasher = Sha256::new();
    hasher.update(&file_bytes);
    let content_hash = format!("{:x}", hasher.finalize());

    // 2.5. 檔案大小上限 50 MiB
    const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;
    if file_bytes.len() > MAX_FILE_SIZE {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                r#"{{"error":"File too large: {:.1}MB (max 50MB)"}}"#,
                file_bytes.len() as f64 / 1024.0 / 1024.0
            ),
        ));
    }

    // 3. 在暫存目錄中建立安全的隨機檔名暫存實體檔案
    let temp_dir = std::env::temp_dir();
    let temp_file_id = Uuid::new_v4().to_string();

    // 從原名萃取小寫副檔名
    let ext_suffix = original_name
        .as_deref()
        .and_then(|name| std::path::Path::new(name).extension())
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()))
        .unwrap_or_default();

    let temp_file_path = temp_dir.join(format!("opendoc-{temp_file_id}{ext_suffix}"));

    std::fs::write(&temp_file_path, &file_bytes).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("寫入暫存檔失敗: {e}"),
        )
    })?;

    // 4. 呼叫雙軌高彈性 Parser 進行解析，回傳 chunks 向量
    let chunks = opendoc_parser::parse_file(
        &temp_file_path,
        original_name.as_deref(),
        &workspace_id,
        &collection_id,
    )
    .await
    .map_err(|e| {
        let _ = std::fs::remove_file(&temp_file_path);
        (StatusCode::BAD_REQUEST, e)
    })?;

    // 5. 刪除解析完畢的實體暫存檔案
    let _ = std::fs::remove_file(&temp_file_path);

    let chunks_count = chunks.len();

    // 6. 持久化寫入 SQLite 資料庫 documents 表
    let file_path_str = original_name
        .clone()
        .unwrap_or_else(|| format!("opendoc-{temp_file_id}"));

    // 寫入 documents 資料庫
    let ext_display = ext_suffix.trim_start_matches('.').to_uppercase();
    let source_path = headers
        .get("x-source-path")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/{}", workspace_id, file_path_str));
    let file_size = file_bytes.len() as i64;

    // 去重鍵：(workspace_id, source_path)
    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE workspace_id = ? AND source_path = ? AND deleted_at IS NULL"
    )
    .bind(&workspace_id)
    .bind(&source_path)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("查詢既有文件失敗: {e}")))?;

    let document_id = match existing_id {
        Some(id) => {
            sqlx::query(
                "UPDATE documents SET title = ?, file_type = ?, file_size_bytes = ?, chunk_count = ?, status = 'indexed', updated_at = CURRENT_TIMESTAMP, indexed_at = CURRENT_TIMESTAMP, content_hash = ? WHERE id = ?"
            )
            .bind(&file_path_str)
            .bind(&ext_display)
            .bind(file_size)
            .bind(chunks_count as i64)
            .bind(&content_hash)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("更新既有文件失敗: {e}")))?;
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO documents (id, title, source_type, source_path, file_type, file_size_bytes, status, chunk_count, workspace_id, created_at, indexed_at, content_hash) \
                 VALUES (?, ?, ?, ?, ?, ?, 'indexed', ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, ?)"
            )
            .bind(&temp_file_id)
            .bind(&file_path_str)
            .bind(&ext_display)
            .bind(&source_path)
            .bind(&ext_display)
            .bind(file_size)
            .bind(chunks_count as i64)
            .bind(&workspace_id)
            .bind(&content_hash)
            .execute(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("寫入資料庫失敗: {e}")))?;
            temp_file_id
        }
    };

    // 7. 索引 chunks → embedding → LanceDB（套件介面搜尋用）。best-effort：失敗不阻塞上傳。
    if let Err(e) = state.search.index_chunks(
        &document_id,
        &workspace_id,
        Some(&collection_id),
        &source_path,
        &chunks,
    ).await {
        eprintln!("⚠️ 向量索引寫入失敗（文件已入庫但搜尋暫時查不到）: {e}");
    }

    Ok(Json(UploadResponse {
        document_id,
        chunks: chunks_count,
        status: "indexed".to_string(),
    }))
}
