use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::McpState;
use crate::utils::resolve_workspace_id;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryLogRequest {
    pub query: String,
    pub profile: String,
    pub confidence_score: Option<f64>,
    pub response_time_ms: Option<i64>,
    pub route: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryLogResponse {
    pub success: bool,
    pub log_id: i64,
}

pub async fn query_log_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<QueryLogRequest>,
) -> Result<Json<QueryLogResponse>, (StatusCode, String)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|status| (status, "Workspace not found".to_string()))?;

    let res = sqlx::query(
        "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"
    )
    .bind(&payload.query)
    .bind(&payload.profile)
    .bind(payload.confidence_score)
    .bind(payload.response_time_ms)
    .bind(&payload.route)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("寫入查詢紀錄失敗: {e}")))?;

    let last_id = res.last_insert_rowid();

    Ok(Json(QueryLogResponse {
        success: true,
        log_id: last_id,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryFeedbackRequest {
    pub log_id: i64,
    pub feedback: String, // 'positive' or 'negative'
}

#[derive(Serialize)]
pub struct QueryFeedbackResponse {
    pub success: bool,
}

pub async fn query_feedback_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<QueryFeedbackRequest>,
) -> Result<Json<QueryFeedbackResponse>, (StatusCode, String)> {
    if payload.feedback != "positive" && payload.feedback != "negative" {
        return Err((
            StatusCode::BAD_REQUEST,
            "feedback 欄位必須為 'positive' 或 'negative'".to_string(),
        ));
    }

    sqlx::query(
        "UPDATE query_logs SET feedback = ? WHERE id = ?"
    )
    .bind(&payload.feedback)
    .bind(payload.log_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("更新回饋紀錄失敗: {e}")))?;

    Ok(Json(QueryFeedbackResponse { success: true }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFeedbackRequest {
    pub query_id: String,
    pub feedback: String, // 'positive' or 'negative'
}

pub async fn chat_feedback_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<ChatFeedbackRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if payload.feedback != "positive" && payload.feedback != "negative" {
        return Err((
            StatusCode::BAD_REQUEST,
            "feedback must be 'positive' or 'negative'".to_string(),
        ));
    }

    sqlx::query("UPDATE query_logs SET feedback = ? WHERE id = ?")
        .bind(&payload.feedback)
        .bind(&payload.query_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("更新回饋紀錄失敗: {e}")))?;

    Ok(Json(json!({ "saved": true })))
}
