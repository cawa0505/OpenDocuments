use crate::utils::{map_document_row, resolve_workspace_id, DocumentItem};
use crate::McpState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

pub async fn list_collections_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query("SELECT id, name, description FROM collections WHERE workspace_id = ?")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 collections 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut collections = Vec::new();
    for r in rows {
        collections.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "name": sqlx::Row::get::<String, _>(&r, 1),
            "description": sqlx::Row::get::<Option<String>, _>(&r, 2).unwrap_or_default(),
        }));
    }

    Ok(Json(json!({ "collections": collections })))
}

pub async fn create_collection_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Collection name required" })),
        ));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;
    let id = Uuid::new_v4().to_string();
    let description = payload.description;

    sqlx::query(
        "INSERT INTO collections (id, workspace_id, name, description) VALUES (?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&name)
    .bind(&description)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 建立 collection 失敗: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal error" })),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "name": name, "description": description.unwrap_or_default() })),
    ))
}

pub async fn delete_collection_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query("DELETE FROM collections WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 collection 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

pub async fn add_collection_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((collection_id, document_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if collection_id.trim().is_empty() || document_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Collection and document ids required" })),
        ));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    sqlx::query(
        "INSERT OR IGNORE INTO collection_documents (collection_id, document_id) SELECT c.id, d.id FROM collections c JOIN documents d ON d.id = ? WHERE c.id = ? AND c.workspace_id = ? AND d.workspace_id = ?"
    )
    .bind(&document_id)
    .bind(&collection_id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 加入文件至集合失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    Ok(Json(json!({ "added": true })))
}

pub async fn remove_collection_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((collection_id, document_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if collection_id.trim().is_empty() || document_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Collection and document ids required" })),
        ));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    sqlx::query(
        "DELETE FROM collection_documents WHERE collection_id = ? AND document_id = ? AND EXISTS (SELECT 1 FROM collections WHERE id = ? AND workspace_id = ?) AND EXISTS (SELECT 1 FROM documents WHERE id = ? AND workspace_id = ?)"
    )
    .bind(&collection_id)
    .bind(&document_id)
    .bind(&collection_id)
    .bind(&workspace_id)
    .bind(&document_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 移除文件自集合失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    Ok(Json(json!({ "removed": true })))
}

pub async fn list_collection_documents_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    let collection_row = sqlx::query(
        "SELECT id, name, description, datetime(created_at, 'localtime') FROM collections WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 collection 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    let Some(cr) = collection_row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Collection not found" })),
        ));
    };

    let rows = sqlx::query(
        "SELECT d.id, d.title, d.source_type, d.source_path, d.file_type, d.file_size_bytes, d.connector_id, d.chunk_count, d.status, d.content_hash, d.error_message, datetime(d.created_at, 'localtime'), datetime(d.updated_at, 'localtime'), d.indexed_at, d.workspace_id FROM collection_documents cd JOIN collections c ON c.id = cd.collection_id JOIN documents d ON d.id = cd.document_id WHERE cd.collection_id = ? AND c.workspace_id = ? AND d.workspace_id = ? AND d.deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢集合文件失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    let documents: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();

    Ok(Json(json!({
        "collection": {
            "id": sqlx::Row::get::<String, _>(&cr, 0),
            "name": sqlx::Row::get::<String, _>(&cr, 1),
            "description": sqlx::Row::get::<Option<String>, _>(&cr, 2).unwrap_or_default(),
            "createdAt": sqlx::Row::get::<String, _>(&cr, 3),
        },
        "documents": documents,
    })))
}
