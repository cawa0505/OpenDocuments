use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json; // removed unused Serialize and Value
use uuid::Uuid;
use crate::McpState;
use crate::utils::{resolve_workspace_id, clean_json_markdown};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtractAssetRequest {
    pub title: String,
    pub asset_type: String,
    pub document_id: Option<String>,
    pub schema_definition: serde_json::Value,
    pub data_content: Option<serde_json::Value>,
    pub prompt: Option<String>,
}

async fn get_active_llm_client(db_pool: &sqlx::SqlitePool, workspace_id: &str) -> Option<opendoc_llm::LlmClient> {
    let row_res = sqlx::query(
        "SELECT name, provider, base_url, model, api_key FROM llm_providers WHERE workspace_id = ? AND is_active = 1 AND kind = 'chat' LIMIT 1"
    )
    .bind(workspace_id)
    .fetch_optional(db_pool)
    .await;

    match row_res {
        Ok(Some(row)) => {
            let name: String = sqlx::Row::get(&row, 0);
            let _provider_name: String = sqlx::Row::get(&row, 1);
            let base_url: String = sqlx::Row::get(&row, 2);
            let model: String = sqlx::Row::get(&row, 3);
            let api_key: String = sqlx::Row::get(&row, 4);

            let api_key_opt = if api_key.is_empty() { None } else { Some(api_key) };

            let provider = opendoc_llm::LlmProvider {
                name,
                base_url,
                model,
                api_key: api_key_opt,
            };
            Some(opendoc_llm::LlmClient::new(provider))
        }
        _ => None,
    }
}

pub async fn extract_asset_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<ExtractAssetRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    let asset_id = Uuid::new_v4().to_string();
    let schema_str = serde_json::to_string(&payload.schema_definition).unwrap_or_else(|_| "[]".to_string());

    let mut final_data = payload.data_content.clone().unwrap_or(json!([]));
    let mut source_chunks = json!([]);

    if payload.data_content.is_none() {
        if let Some(llm_client) = get_active_llm_client(&state.db_pool, &workspace_id).await {
            let mut context_text = String::new();
            if let Some(ref doc_id) = payload.document_id {
                let doc_row = sqlx::query("SELECT title, source_path FROM documents WHERE id = ? AND workspace_id = ?")
                    .bind(doc_id)
                    .bind(&workspace_id)
                    .fetch_optional(&state.db_pool)
                    .await
                    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("database query error: {e}") }))))?;

                if let Some(r) = doc_row {
                    let doc_title = sqlx::Row::get::<String, _>(&r, 0);
                    let chunks = state.search.search_and_rerank(&doc_title, 0.0);
                    for (i, chunk) in chunks.iter().enumerate() {
                        context_text.push_str(&format!("--- Chunk {} ---\n{}\n", i + 1, chunk.content));
                    }
                    source_chunks = json!([doc_id]);
                }
            } else {
                let chunks = state.search.search_and_rerank(&payload.title, 0.0);
                for (i, chunk) in chunks.iter().enumerate() {
                    context_text.push_str(&format!("--- Chunk {} ---\n{}\n", i + 1, chunk.content));
                }
            }

            if context_text.is_empty() {
                context_text = "No document context found. Please extract based on general knowledge.".to_string();
            }

            let schema_hint = serde_json::to_string_pretty(&payload.schema_definition).unwrap_or_default();
            let custom_prompt = payload.prompt.as_deref().unwrap_or("Extract key entities and relationships.");

            let system_prompt = format!(
                "You are an expert Structured Data Extraction Agent.\n\
                You must extract structured information from the provided Document Context strictly adhering to the JSON schema below.\n\n\
                [JSON Schema Definition]\n\
                {}\n\n\
                [Extraction Guideline]\n\
                {}\n\n\
                Respond with ONLY a valid raw JSON array of objects conforming to the schema. Do not include markdown code block backticks (e.g. ```json), explanations, or trailing prose.",
                schema_hint, custom_prompt
            );

            let messages = vec![
                opendoc_llm::ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                opendoc_llm::ChatMessage {
                    role: "user".to_string(),
                    content: format!("Document Context:\n{}", context_text),
                },
            ];

            let opts = opendoc_llm::CompletionOptions {
                temperature: Some(0.1),
                max_tokens: Some(4000),
                system_prompt: None,
            };

            match llm_client.complete(messages, &opts).await {
                Ok(raw_response) => {
                    let clean_json = clean_json_markdown(&raw_response);
                    match serde_json::from_str::<serde_json::Value>(&clean_json) {
                        Ok(parsed_json) => {
                            final_data = parsed_json;
                        }
                        Err(e) => {
                            eprintln!("💥 LLM 回傳 JSON 解析失敗: {e}. Raw: {clean_json}");
                            final_data = json!([{ "error": "LLM output parsing failed", "rawOutput": clean_json }]);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("💥 LLM 萃取完成請求失敗: {e}");
                    return Err((StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("LLM request failed: {e}") }))));
                }
            }
        } else {
            let mut fallback_row = serde_json::Map::new();
            if let Some(arr) = payload.schema_definition.as_array() {
                for col in arr {
                    if let Some(key) = col.get("key").and_then(|k| k.as_str()) {
                        let label = col.get("label").and_then(|l| l.as_str()).unwrap_or(key);
                        fallback_row.insert(key.to_string(), json!(format!("[模擬資料] 關於 {label} 的萃取內容")));
                    }
                }
            }
            if fallback_row.is_empty() {
                fallback_row.insert("info".to_string(), json!("無可用的欄位定義"));
            }
            final_data = json!([fallback_row]);
        }
    }

    let final_data_str = serde_json::to_string(&final_data).unwrap_or_else(|_| "[]".to_string());
    let source_chunks_str = serde_json::to_string(&source_chunks).unwrap_or_else(|_| "[]".to_string());

    sqlx::query(
        "INSERT INTO extracted_assets (id, workspace_id, document_id, asset_type, title, schema_definition, data_content, source_chunks) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&asset_id)
    .bind(&workspace_id)
    .bind(payload.document_id.as_deref())
    .bind(&payload.asset_type)
    .bind(&payload.title)
    .bind(&schema_str)
    .bind(&final_data_str)
    .bind(&source_chunks_str)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 寫入 extracted_assets 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "database insert failed" })))
    })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": asset_id,
            "workspaceId": workspace_id,
            "documentId": payload.document_id,
            "assetType": payload.asset_type,
            "title": payload.title,
            "schemaDefinition": payload.schema_definition,
            "dataContent": final_data,
            "sourceChunks": source_chunks,
        })),
    ))
}

pub async fn list_assets_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, workspace_id, document_id, asset_type, title, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime') FROM extracted_assets WHERE workspace_id = ? ORDER BY created_at DESC"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 extracted_assets 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut assets = Vec::new();
    for r in rows {
        assets.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "workspaceId": sqlx::Row::get::<String, _>(&r, 1),
            "documentId": sqlx::Row::get::<Option<String>, _>(&r, 2),
            "assetType": sqlx::Row::get::<String, _>(&r, 3),
            "title": sqlx::Row::get::<String, _>(&r, 4),
            "createdAt": sqlx::Row::get::<String, _>(&r, 5),
            "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 6).unwrap_or_default(),
        }));
    }

    Ok(Json(json!({ "assets": assets })))
}

pub async fn get_asset_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let row = sqlx::query(
        "SELECT id, workspace_id, document_id, asset_type, title, schema_definition, data_content, source_chunks, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime') FROM extracted_assets WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢單一 extracted_asset 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
        Some(r) => {
            let schema_str = sqlx::Row::get::<String, _>(&r, 5);
            let data_str = sqlx::Row::get::<String, _>(&r, 6);
            let chunks_str = sqlx::Row::get::<String, _>(&r, 7);

            let schema: serde_json::Value = serde_json::from_str(&schema_str).unwrap_or(json!([]));
            let data_content: serde_json::Value = serde_json::from_str(&data_str).unwrap_or(json!([]));
            let source_chunks: serde_json::Value = serde_json::from_str(&chunks_str).unwrap_or(json!([]));

            Ok(Json(json!({
                "id": sqlx::Row::get::<String, _>(&r, 0),
                "workspaceId": sqlx::Row::get::<String, _>(&r, 1),
                "documentId": sqlx::Row::get::<Option<String>, _>(&r, 2),
                "assetType": sqlx::Row::get::<String, _>(&r, 3),
                "title": sqlx::Row::get::<String, _>(&r, 4),
                "schemaDefinition": schema,
                "dataContent": data_content,
                "sourceChunks": source_chunks,
                "createdAt": sqlx::Row::get::<String, _>(&r, 8),
                "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 9).unwrap_or_default(),
            })))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn delete_asset_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query("DELETE FROM extracted_assets WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 extracted_asset 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}
