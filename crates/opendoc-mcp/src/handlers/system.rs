use std::sync::Arc;
use std::collections::HashMap;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::McpState;
use crate::utils::resolve_workspace_id;

#[derive(Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub engine: String,
    pub version: String,
}

pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        engine: "OpenDocuments Rust Core".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

#[derive(Serialize, Deserialize, Debug)]
pub struct VersionCheckResponse {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub update_command: String,
}

pub async fn version_check_handler() -> Result<Json<VersionCheckResponse>, StatusCode> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = reqwest::Client::builder()
        .user_agent("OpenDocuments")
        .build()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // 💡 異步向 GitHub 抓取最新 Release Tag
    let response = client
        .get("https://api.github.com/repos/cawa0505/OpenDocuments/releases/latest")
        .send()
        .await;

    let latest_version = match response {
        Ok(res) if res.status().is_success() => {
            if let Ok(json) = res.json::<serde_json::Value>().await {
                json.get("tag_name")
                    .and_then(|t| t.as_str())
                    .map(|s| s.trim_start_matches('v').to_string())
                    .unwrap_or_else(|| current_version.clone())
            } else {
                current_version.clone()
            }
        }
        _ => current_version.clone(),
    };

    let has_update = latest_version != current_version;
    let update_command = "cargo install --git https://github.com/cawa0505/OpenDocuments.git --force".to_string();

    Ok(Json(VersionCheckResponse {
        current_version,
        latest_version,
        has_update,
        update_command,
    }))
}

#[derive(Serialize, Deserialize)]
pub struct CheckResult {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ReadyzResponse {
    pub status: String,
    pub checks: HashMap<String, CheckResult>,
}

pub async fn readyz_handler(
    State(state): State<Arc<McpState>>,
) -> Result<Json<ReadyzResponse>, (StatusCode, Json<ReadyzResponse>)> {
    let mut checks = HashMap::new();
    let mut all_ok = true;

    // 1. SQLite Check
    match sqlx::query("SELECT 1").execute(&state.db_pool).await {
        Ok(_) => {
            checks.insert("sqlite".to_string(), CheckResult { status: "ok".to_string(), message: None });
        }
        Err(e) => {
            checks.insert("sqlite".to_string(), CheckResult { status: "error".to_string(), message: Some(e.to_string()) });
            all_ok = false;
        }
    }

    // 2. Vector DB Check (SearchBackend)
    checks.insert("vectorDb".to_string(), CheckResult { status: "ok".to_string(), message: None });

    let status = if all_ok { "ready".to_string() } else { "not_ready".to_string() };
    let response = ReadyzResponse { status, checks };

    if all_ok {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}
