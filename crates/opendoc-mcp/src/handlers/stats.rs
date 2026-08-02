use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use crate::McpState;

pub async fn stats_handler(
    State(state): State<Arc<McpState>>,
) -> Result<Json<Value>, StatusCode> {
    let documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE deleted_at IS NULL")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 統計 documents 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let workspaces: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 統計 workspaces 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({
        "documents": documents,
        "workspaces": workspaces,
        "plugins": 0,
        "pluginList": [],
    })))
}
