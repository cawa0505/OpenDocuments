use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use crate::McpState;

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

    // 向 GitHub 抓取最新 Release Tag；任何失敗都視為「無更新」，不誤導使用者
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

    // 只有 latest > current 才算有更新；等於、小於（dev/rollback）或查詢失敗都隱藏
    let has_update = version_is_newer(&latest_version, &current_version);
    let update_command =
        "cargo install --git https://github.com/cawa0505/OpenDocuments.git --force".to_string();

    Ok(Json(VersionCheckResponse {
        current_version,
        latest_version,
        has_update,
        update_command,
    }))
}

/// 比較 semver「x.y.z」字串，僅當 latest > current 時回傳 true。
/// 任一版本無法解析時回傳 false（保守隱藏，避免誤導）。
fn version_is_newer(latest: &str, current: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let mut parts = v.trim().split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = match parts.next() {
            Some(s) => s.parse().ok()?,
            None => 0,
        };
        let patch = match parts.next() {
            Some(s) => s.parse().ok()?,
            None => 0,
        };
        // 超過三段的版本（如 1.2.3.4）視為無法解析
        if parts.next().is_some() {
            return None;
        }
        Some((major, minor, patch))
    }

    match (parse(latest), parse(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
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
            checks.insert(
                "sqlite".to_string(),
                CheckResult {
                    status: "ok".to_string(),
                    message: None,
                },
            );
        }
        Err(e) => {
            checks.insert(
                "sqlite".to_string(),
                CheckResult {
                    status: "error".to_string(),
                    message: Some(e.to_string()),
                },
            );
            all_ok = false;
        }
    }

    // 2. Vector DB Check (SearchBackend)
    checks.insert(
        "vectorDb".to_string(),
        CheckResult {
            status: "ok".to_string(),
            message: None,
        },
    );

    let status = if all_ok {
        "ready".to_string()
    } else {
        "not_ready".to_string()
    };
    let response = ReadyzResponse { status, checks };

    if all_ok {
        Ok(Json(response))
    } else {
        Err((StatusCode::SERVICE_UNAVAILABLE, Json(response)))
    }
}

#[cfg(test)]
mod tests {
    use super::version_is_newer;

    #[test]
    fn version_is_newer_only_when_latest_gt_current() {
        assert!(version_is_newer("0.3.0", "0.2.0"));
        assert!(version_is_newer("1.0.0", "0.9.9"));
        assert!(version_is_newer("0.2.1", "0.2.0"));
        // equal / older / unparseable → false（保守隱藏）
        assert!(!version_is_newer("0.2.0", "0.2.0"));
        assert!(!version_is_newer("0.1.0", "0.2.0"));
        assert!(!version_is_newer("latest", "0.2.0"));
        assert!(!version_is_newer("0.2.0", "not-a-version"));
    }
}
