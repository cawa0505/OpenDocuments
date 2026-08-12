use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
// removed unused Serialize
use crate::utils::resolve_workspace_id;
use crate::McpState;
use serde_json::json;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderUpsertRequest {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn list_llm_providers_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;
    let rows = sqlx::query(
        "SELECT id, name, provider, base_url, model, is_active, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), api_key != '' as has_key \
         FROM llm_providers WHERE workspace_id = ? ORDER BY created_at"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 llm_providers 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut providers = Vec::new();
    for r in rows {
        providers.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "name": sqlx::Row::get::<String, _>(&r, 1),
            "provider": sqlx::Row::get::<String, _>(&r, 2),
            "baseUrl": sqlx::Row::get::<String, _>(&r, 3),
            "model": sqlx::Row::get::<String, _>(&r, 4),
            "isActive": sqlx::Row::get::<i64, _>(&r, 5) == 1,
            "createdAt": sqlx::Row::get::<Option<String>, _>(&r, 6).unwrap_or_default(),
            "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 7).unwrap_or_default(),
            "hasApiKey": sqlx::Row::get::<i64, _>(&r, 8) == 1,
        }));
    }

    Ok(Json(json!({ "providers": providers })))
}

pub async fn upsert_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LlmProviderUpsertRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let existing_row =
        sqlx::query("SELECT id, api_key FROM llm_providers WHERE workspace_id = ? AND name = ?")
            .bind(&workspace_id)
            .bind(&payload.name)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 查詢既有 provider 失敗: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;

    let existing = existing_row.map(|r| {
        (
            sqlx::Row::get::<String, _>(&r, 0),
            sqlx::Row::get::<String, _>(&r, 1),
        )
    });

    let id = existing
        .as_ref()
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let api_key = match payload.api_key {
        Some(k) if !k.trim().is_empty() => k,
        _ => existing
            .as_ref()
            .map(|(_, key)| key.clone())
            .unwrap_or_default(),
    };

    let is_active = if payload.is_active.unwrap_or(false) {
        1
    } else {
        0
    };

    if is_active == 1 {
        sqlx::query("UPDATE llm_providers SET is_active = 0 WHERE workspace_id = ?")
            .bind(&workspace_id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": e.to_string() })),
                )
            })?;
    }

    sqlx::query(
        "INSERT INTO llm_providers (id, workspace_id, name, provider, base_url, model, api_key, is_active) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(workspace_id, name) DO UPDATE SET \
            provider = excluded.provider, \
            base_url = excluded.base_url, \
            model = excluded.model, \
            api_key = excluded.api_key, \
            is_active = excluded.is_active, \
            updated_at = datetime('now', 'localtime')"
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&payload.name)
    .bind(&payload.provider)
    .bind(&payload.base_url)
    .bind(&payload.model)
    .bind(&api_key)
    .bind(is_active)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 Upsert provider 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "id": id,
            "name": payload.name,
            "provider": payload.provider,
            "baseUrl": payload.base_url,
            "model": payload.model,
            "isActive": is_active == 1,
            "hasApiKey": !api_key.is_empty(),
        })),
    ))
}

pub async fn delete_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let result = sqlx::query("DELETE FROM llm_providers WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 provider 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestRequest {
    pub provider_id: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

pub async fn test_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LlmTestRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let (base_url, model, api_key) = if let Some(pid) = payload.provider_id {
        let row = sqlx::query(
            "SELECT base_url, model, api_key FROM llm_providers WHERE id = ? AND workspace_id = ?",
        )
        .bind(&pid)
        .bind(&workspace_id)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;
        match row {
            Some(r) => {
                let url: String = sqlx::Row::get(&r, 0);
                let md: String = sqlx::Row::get(&r, 1);
                let key: String = sqlx::Row::get(&r, 2);
                (url, md, Some(key))
            }
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "Provider not found" })),
                ))
            }
        }
    } else {
        let url = payload.base_url.ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing base_url" })),
            )
        })?;
        let md = payload.model.ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "Missing model" })),
            )
        })?;
        let key = payload.api_key;
        (url, md, key)
    };

    let cfg = opendoc_llm::LlmProvider {
        name: "test".to_string(),
        base_url,
        model,
        api_key,
    };

    let client = opendoc_llm::LlmClient::new(cfg);
    let messages = vec![opendoc_llm::ChatMessage::user("say: pong")];
    let opts = opendoc_llm::CompletionOptions {
        temperature: Some(0.1),
        max_tokens: Some(10),
        system_prompt: None,
    };

    let start = std::time::Instant::now();
    match client.complete(messages, &opts).await {
        Ok(reply) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(Json(json!({
                "ok": true,
                "reply": reply.trim(),
                "latencyMs": latency,
            })))
        }
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "ok": false,
                "error": e.to_string(),
            })),
        )),
    }
}
