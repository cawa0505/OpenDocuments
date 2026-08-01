use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use crate::McpState;
use crate::utils::resolve_workspace_id;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

pub async fn list_tags_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query("SELECT id, workspace_id, name, color FROM tags WHERE workspace_id = ? ORDER BY name")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 tags 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut tags = Vec::new();
    for r in rows {
        tags.push(TagItem {
            id: sqlx::Row::get::<String, _>(&r, 0),
            workspace_id: sqlx::Row::get::<String, _>(&r, 1),
            name: sqlx::Row::get::<String, _>(&r, 2),
            color: sqlx::Row::get::<Option<String>, _>(&r, 3),
        });
    }

    Ok(Json(json!({ "tags": tags })))
}

pub async fn create_tag_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Tag name required" }))));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;
    let id = Uuid::new_v4().to_string();
    let color = payload.color;

    sqlx::query("INSERT INTO tags (id, workspace_id, name, color) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&workspace_id)
        .bind(&name)
        .bind(&color)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立 tag 失敗: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "workspaceId": workspace_id,
            "name": name,
            "color": color,
        })),
    ))
}

pub async fn delete_tag_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query("DELETE FROM tags WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 tag 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

pub async fn tag_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((doc_id, tag_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO document_tags (document_id, tag_id) \
         SELECT d.id, t.id \
         FROM documents d \
         JOIN tags t ON t.id = ? \
         WHERE d.id = ? AND d.workspace_id = ? AND t.workspace_id = ?"
    )
    .bind(&tag_id)
    .bind(&doc_id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 文件貼標籤失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "tagged": true })))
}

pub async fn untag_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((doc_id, tag_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query(
        "DELETE FROM document_tags \
         WHERE document_id = ? AND tag_id = ? \
         AND EXISTS (SELECT 1 FROM documents WHERE id = ? AND workspace_id = ?) \
         AND EXISTS (SELECT 1 FROM tags WHERE id = ? AND workspace_id = ?)"
    )
    .bind(&doc_id)
    .bind(&tag_id)
    .bind(&doc_id)
    .bind(&workspace_id)
    .bind(&tag_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 移除文件標籤失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "untagged": true })))
}
