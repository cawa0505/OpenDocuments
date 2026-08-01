use std::sync::Arc;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;
use crate::McpState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceItem {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub settings: serde_json::Value,
    pub created_at: String,
    pub is_default: bool,
}

#[derive(Deserialize)]
pub struct CreateWorkspaceReq {
    #[serde(alias = "name")]
    pub id: String,
}

pub async fn list_workspaces_handler(
    State(state): State<Arc<McpState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, name, datetime(created_at, 'localtime') FROM workspaces"
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 workspaces 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 取得設定檔中的預設工作空間名稱
    let config = state.config_manager.get_config().await;
    let default_ws_name = config.model.default_workspace.clone();

    let mut workspaces = Vec::new();
    for row in rows {
        let is_default = row.1 == default_ws_name || row.0 == default_ws_name;
        workspaces.push(WorkspaceItem {
            id: row.0,
            name: row.1,
            mode: "personal".to_string(),
            settings: json!({}),
            created_at: row.2,
            is_default,
        });
    }

    Ok(Json(json!({ "workspaces": workspaces })))
}

pub async fn create_workspace_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<CreateWorkspaceReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Node 契約：workspace id = randomUUID；name 用請求帶的 id 欄位
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT OR IGNORE INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)")
        .bind(&id)
        .bind(&payload.id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立工作空間失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "success": true })))
}

pub async fn delete_workspace_handler(
    State(state): State<Arc<McpState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // UUID 遷移後 default workspace 的 id 是 UUID（不再等於名稱），改查 id 或 name 比對
    let default_workspace = state.config_manager.get_config().await.model.default_workspace;
    let default_id: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = ?")
        .bind(&default_workspace)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢預設工作空間失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if default_id.as_ref() == Some(&id) || default_workspace == id {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 1. 軟刪除工作空間內的所有文件
    sqlx::query("UPDATE documents SET deleted_at = CURRENT_TIMESTAMP WHERE workspace_id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除工作空間關聯文件失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 2. 物理刪除工作空間本身
    sqlx::query("DELETE FROM workspaces WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除工作空間失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "success": true })))
}
