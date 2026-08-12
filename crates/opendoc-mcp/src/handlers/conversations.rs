use crate::utils::resolve_workspace_id;
use crate::McpState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct UpdateConversationReq {
    pub title: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
}

pub async fn list_conversations_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, title, shared, created_at, updated_at FROM conversations WHERE workspace_id = ? AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 100"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversations 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut conversations = Vec::new();
    for r in rows {
        conversations.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "title": sqlx::Row::get::<String, _>(&r, 1),
            "shared": sqlx::Row::get::<i64, _>(&r, 2) == 1,
            "createdAt": sqlx::Row::get::<String, _>(&r, 3),
            "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 4),
        }));
    }

    Ok(Json(json!({ "conversations": conversations })))
}

pub async fn update_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UpdateConversationReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let result = sqlx::query(
        "UPDATE conversations SET title = COALESCE(?, title), updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL",
    )
    .bind(body.title.as_deref())
    .bind(&id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 update conversation 失敗: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal error" })),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Conversation not found" })),
        ));
    }

    Ok(Json(json!({ "updated": true })))
}

pub async fn create_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    payload: Option<Json<CreateConversationRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;
    let id = Uuid::new_v4().to_string();
    let title = payload
        .and_then(|Json(p)| p.title)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_default();

    sqlx::query("INSERT INTO conversations (id, workspace_id, title, created_at, updated_at) VALUES (?, ?, ?, datetime('now'), datetime('now'))")
        .bind(&id)
        .bind(&workspace_id)
        .bind(&title)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let row = sqlx::query("SELECT created_at, updated_at FROM conversations WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 讀取 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "title": title,
            "workspaceId": workspace_id,
            "shared": false,
            "createdAt": sqlx::Row::get::<String, _>(&row, 0),
            "updatedAt": sqlx::Row::get::<String, _>(&row, 1),
        })),
    ))
}

pub async fn delete_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Conversation not found" })),
        ));
    }

    sqlx::query("UPDATE conversations SET deleted_at = datetime('now') WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::OK, Json(json!({ "deleted": true }))))
}

pub async fn list_conversation_messages_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Conversation not found" })),
        ));
    }

    let messages = conversation_messages_json(&state.db_pool, &id).await?;

    Ok((StatusCode::OK, Json(json!({ "messages": messages }))))
}

/// Fetch messages for a conversation in the shared JSON shape.
pub async fn conversation_messages_json(
    db_pool: &sqlx::SqlitePool,
    conversation_id: &str,
) -> Result<Vec<serde_json::Value>, StatusCode> {
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at FROM messages WHERE conversation_id = ? ORDER BY created_at"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 messages 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut messages = Vec::new();
    for r in rows {
        let raw_sources: Option<String> = sqlx::Row::get(&r, 4);
        let sources_val = match raw_sources {
            Some(s) if !s.is_empty() => {
                serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!([]))
            }
            _ => json!([]),
        };

        messages.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "conversationId": sqlx::Row::get::<String, _>(&r, 1),
            "role": sqlx::Row::get::<String, _>(&r, 2),
            "content": sqlx::Row::get::<String, _>(&r, 3),
            "sources": sources_val,
            "profileUsed": sqlx::Row::get::<Option<String>, _>(&r, 5),
            "confidenceScore": sqlx::Row::get::<Option<f64>, _>(&r, 6),
            "responseTimeMs": sqlx::Row::get::<Option<i64>, _>(&r, 7),
            "createdAt": sqlx::Row::get::<String, _>(&r, 8),
        }));
    }
    Ok(messages)
}

pub async fn share_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(conversation_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&conversation_id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Conversation not found" })),
        ));
    }

    let token = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "UPDATE conversations SET shared = 1, share_token = ? WHERE id = ? AND workspace_id = ?",
    )
    .bind(&token)
    .bind(&conversation_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 更新 share_token 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({ "shareUrl": format!("/shared/{}", token) })),
    ))
}

pub async fn shared_conversation_handler(
    State(state): State<Arc<McpState>>,
    Path(token): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let row = sqlx::query(
        "SELECT id, workspace_id, title, shared, share_token, created_at FROM conversations WHERE share_token = ?"
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 shared conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let Some(r) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))));
    };

    let conv_id = sqlx::Row::get::<String, _>(&r, 0);
    let messages = conversation_messages_json(&state.db_pool, &conv_id).await?;
    let conversation = json!({
        "id": conv_id,
        "workspace_id": sqlx::Row::get::<String, _>(&r, 1),
        "title": sqlx::Row::get::<String, _>(&r, 2),
        "shared": sqlx::Row::get::<Option<i64>, _>(&r, 3).unwrap_or(0),
        "share_token": sqlx::Row::get::<String, _>(&r, 4),
        "created_at": sqlx::Row::get::<String, _>(&r, 5),
    });

    Ok((
        StatusCode::OK,
        Json(json!({ "conversation": conversation, "messages": messages })),
    ))
}
