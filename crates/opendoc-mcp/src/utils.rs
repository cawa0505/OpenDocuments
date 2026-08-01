use std::sync::Arc;
use axum::http::StatusCode;
use serde::Serialize;
use crate::McpState;

// ── Workspace resolver ────────────────────────────────────────
/// 將 X-Workspace header（id 或 name）解析成 workspace UUID id（Node getById ?? getByName 向後相容）。
/// - header 非空：`SELECT id FROM workspaces WHERE id = ? OR name = ?` → 命中回 id；未命中 → 400（嚴格，不 auto-create）
/// - header 缺/空：取 config default_workspace 名稱 → 同查詢 → 未命中 → 500（default 啟動必建，缺失 = invariant 破壞）
pub async fn resolve_workspace_id(
    state: &McpState,
    headers: &axum::http::HeaderMap,
) -> Result<String, StatusCode> {
    let header_val = headers
        .get("x-workspace")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let default_ws;
    let lookup_name = match header_val {
        Some(ws) => ws,
        None => {
            default_ws = state.config_manager.get_config().await.model.default_workspace;
            default_ws.as_str()
        }
    };

    let found: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? OR name = ? LIMIT 1"
    )
    .bind(lookup_name)
    .bind(lookup_name)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 解析工作空間失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match found {
        Some(id) => Ok(id),
        None => {
            if header_val.is_some() {
                // header 指定了未知 workspace → 嚴格 400
                Err(StatusCode::BAD_REQUEST)
            } else {
                // default workspace 啟動必建，缺失代表 invariant 破壞
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

#[derive(Serialize)]
pub struct DocumentItem {
    pub id: String,
    pub title: String,
    pub source_type: String,
    pub source_path: String,
    pub file_type: String,
    pub file_size_bytes: Option<i64>,
    pub connector_id: Option<String>,
    pub chunk_count: i64,
    pub status: String,
    pub content_hash: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub indexed_at: Option<String>,
    pub workspace_id: String,
}

/// 將 documents 資料列映射為 DocumentItem（list_documents 與 list_collection_documents 共用）
pub fn map_document_row(r: &sqlx::sqlite::SqliteRow) -> DocumentItem {
    DocumentItem {
        id: sqlx::Row::get::<String, _>(r, 0),
        title: sqlx::Row::get::<String, _>(r, 1),
        source_type: sqlx::Row::get::<String, _>(r, 2),
        source_path: sqlx::Row::get::<Option<String>, _>(r, 3).unwrap_or_default(),
        file_type: sqlx::Row::get::<Option<String>, _>(r, 4).unwrap_or_default(),
        file_size_bytes: sqlx::Row::get::<Option<i64>, _>(r, 5),
        connector_id: sqlx::Row::get::<Option<String>, _>(r, 6),
        chunk_count: sqlx::Row::get::<i64, _>(r, 7),
        status: sqlx::Row::get::<String, _>(r, 8),
        content_hash: sqlx::Row::get::<Option<String>, _>(r, 9),
        error_message: sqlx::Row::get::<Option<String>, _>(r, 10),
        created_at: sqlx::Row::get::<String, _>(r, 11),
        updated_at: sqlx::Row::get::<Option<String>, _>(r, 12).unwrap_or_default(),
        indexed_at: sqlx::Row::get::<Option<String>, _>(r, 13),
        workspace_id: sqlx::Row::get::<String, _>(r, 14),
    }
}

pub fn clean_json_markdown(input: &str) -> String {
    let mut s = input.trim();
    if s.starts_with("```") {
        if s.starts_with("```json") {
            s = &s[7..];
        } else {
            s = &s[3..];
        }
    }
    if s.ends_with("```") {
        s = &s[..s.len() - 3];
    }
    s.trim().to_string()
}
