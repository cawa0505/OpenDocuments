use std::sync::Arc;
use axum::{
    extract::{Path, State, Query},
    http::StatusCode,
    Json,
};
use serde_json::json;
use std::collections::HashMap;
use crate::McpState;
use crate::utils::{resolve_workspace_id, map_document_row, DocumentItem};

pub async fn list_documents_handler(
    State(state): State<Arc<McpState>>,
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    let mut sql = "SELECT id, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), indexed_at, workspace_id FROM documents WHERE workspace_id = ? AND deleted_at IS NULL".to_string();
    let mut binds: Vec<String> = vec![workspace];

    if let Some(status) = params.get("status") {
        if !status.is_empty() && status != "all" {
            sql.push_str(" AND status = ?");
            binds.push(status.clone());
        }
    }

    if let Some(source_type) = params.get("sourceType").or_else(|| params.get("source_type")) {
        if !source_type.is_empty() && source_type != "all" {
            sql.push_str(" AND source_type = ?");
            binds.push(source_type.clone());
        }
    }

    // Sorting columns allowlist
    let sort_col = match params.get("sortBy").map(|s| s.as_str()) {
        Some("title") => "title",
        Some("chunks") => "chunk_count",
        Some("updated") => "updated_at",
        Some("created") | Some("createdAt") => "created_at",
        Some("indexed") | Some("indexedAt") => "indexed_at",
        _ => "created_at",
    };

    let sort_order = match params.get("order").map(|s| s.to_lowercase()) {
        Some(ref o) if o == "asc" => "ASC",
        _ => "DESC",
    };

    sql.push_str(&format!(" ORDER BY {} {}", sort_col, sort_order));

    let mut query = sqlx::query(&sql);
    for val in binds {
        query = query.bind(val);
    }

    let rows = query
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 documents 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let documents: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();

    Ok(Json(json!({ "documents": documents })))
}

pub async fn get_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let row = sqlx::query(
        "SELECT id, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, datetime(created_at, 'localtime') as created_at, datetime(updated_at, 'localtime') as updated_at, indexed_at, workspace_id FROM documents WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢文件失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
        Some(r) => {
            let doc = serde_json::json!({
                "id": sqlx::Row::get::<String, _>(&r, 0),
                "title": sqlx::Row::get::<String, _>(&r, 1),
                "source_type": sqlx::Row::get::<String, _>(&r, 2),
                "source_path": sqlx::Row::get::<Option<String>, _>(&r, 3).unwrap_or_default(),
                "file_type": sqlx::Row::get::<Option<String>, _>(&r, 4),
                "file_size_bytes": sqlx::Row::get::<Option<i64>, _>(&r, 5),
                "connector_id": sqlx::Row::get::<Option<String>, _>(&r, 6),
                "chunk_count": sqlx::Row::get::<i64, _>(&r, 7),
                "status": sqlx::Row::get::<String, _>(&r, 8),
                "content_hash": sqlx::Row::get::<Option<String>, _>(&r, 9),
                "error_message": sqlx::Row::get::<Option<String>, _>(&r, 10),
                "created_at": sqlx::Row::get::<String, _>(&r, 11),
                "updated_at": sqlx::Row::get::<Option<String>, _>(&r, 12).unwrap_or_default(),
                "indexed_at": sqlx::Row::get::<Option<String>, _>(&r, 13).unwrap_or_default(),
                "workspace_id": sqlx::Row::get::<String, _>(&r, 14),
            });
            Ok(Json(doc))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    // Node 契約 documents.ts:42：文件不存在（含其他 workspace）→ 404
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documents WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Document not found" })),
        ));
    }

    sqlx::query("UPDATE documents SET deleted_at = CURRENT_TIMESTAMP WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::OK, Json(json!({ "deleted": true }))))
}

pub async fn list_trash_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, title, source_type, source_path, file_type, file_size_bytes, \
         connector_id, chunk_count, status, content_hash, error_message, \
         datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), \
         indexed_at, workspace_id FROM documents WHERE workspace_id = ? AND deleted_at IS NOT NULL \
         ORDER BY deleted_at DESC"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢垃圾桶 documents 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let docs: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();
    Ok(Json(json!({ "documents": docs })))
}

pub async fn restore_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    sqlx::query(
        "UPDATE documents SET deleted_at = NULL, updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))))?;

    Ok(Json(json!({ "restored": true })))
}
