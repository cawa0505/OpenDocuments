use std::sync::Arc;
use axum::{extract::State, http::StatusCode, Json};
use serde_json::json;
use crate::McpState;
use crate::utils::resolve_workspace_id;

pub async fn get_admin_stats_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    // 1. 統計 documents 與 chunks 數量
    let summary: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(chunk_count), 0) FROM documents WHERE deleted_at IS NULL AND workspace_id = ?"
    )
    .bind(&workspace)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 stats 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // 2. 統計 workspaces 總數
    let workspaces_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces")
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or(1);

    // 3. 回傳 WebUI 對齊結構 (包含對應的 stats 分布，空對應以防網頁崩潰)
    Ok(Json(json!({
        "documents": summary.0,
        "chunks": summary.1,
        "workspaces": workspaces_count,
        "plugins": 0,
        "sourceDistribution": {},
        "statusDistribution": {},
        "fileTypeDistribution": {},
    })))
}

pub async fn get_admin_plugins_handler() -> Json<serde_json::Value> {
    let version = env!("CARGO_PKG_VERSION");

    Json(json!({
        "plugins": [
            {
                "name": "document-parser",
                "type": "built-in",
                "version": version,
                "health": { "healthy": true, "message": "Built-in parser is available" },
                "metrics": {}
            },
            {
                "name": "text-chunker",
                "type": "built-in",
                "version": version,
                "health": { "healthy": true, "message": "Built-in chunker is available" },
                "metrics": {}
            },
            {
                "name": "vector-store",
                "type": "built-in",
                "version": version,
                "health": { "healthy": true, "message": "Built-in vector store is available" },
                "metrics": {}
            }
        ]
    }))
}

pub async fn get_admin_search_quality_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    // 聚合 SQL
    let summary: (i64, f64, f64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(AVG(confidence_score), 0.0), COALESCE(AVG(response_time_ms), 0.0) FROM query_logs WHERE workspace_id = ?"
    )
    .bind(&workspace)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or((0, 0.0, 0.0));

    // 統計回饋
    let feedback: (i64, i64) = sqlx::query_as(
        "SELECT SUM(CASE WHEN feedback = 'positive' THEN 1 ELSE 0 END), SUM(CASE WHEN feedback = 'negative' THEN 1 ELSE 0 END) FROM query_logs WHERE workspace_id = ?"
    )
    .bind(&workspace)
    .fetch_one(&state.db_pool)
    .await
    .unwrap_or((0, 0));

    Ok(Json(json!({
        "totalQueries": summary.0,
        "avgConfidence": (summary.1 * 100.0).round() / 100.0,
        "avgResponseTimeMs": summary.2.round() as i64,
        "intentDistribution": {},
        "routeDistribution": {},
        "feedback": { "positive": feedback.0, "negative": feedback.1 },
    })))
}

pub async fn get_admin_query_logs_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let query_rows = sqlx::query(
        "SELECT id, query, profile, confidence_score, response_time_ms, route, feedback, datetime(created_at, 'localtime') FROM query_logs WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 100"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 撈取日誌失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut logs = Vec::new();
    for r in query_rows {
        let id: i64 = sqlx::Row::get(&r, 0);
        let query: String = sqlx::Row::get(&r, 1);
        let profile: String = sqlx::Row::get(&r, 2);
        let score: Option<f64> = sqlx::Row::get(&r, 3);
        let time: Option<i64> = sqlx::Row::get(&r, 4);
        let route: Option<String> = sqlx::Row::get(&r, 5);
        let feedback: Option<String> = sqlx::Row::get(&r, 6);
        let created_at: Option<String> = sqlx::Row::get(&r, 7);

        logs.push(json!({
            "id": id,
            "query": query,
            "profile": profile,
            "confidenceScore": score,
            "responseTimeMs": time,
            "route": route,
            "feedback": feedback,
            "createdAt": created_at.unwrap_or_default(),
        }));
    }

    Ok(Json(json!({ "logs": logs })))
}

pub async fn get_admin_benchmark_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT model, metric_name, metric_value, datetime(created_at, 'localtime') FROM benchmark_runs WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 50"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 benchmark 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut runs = Vec::new();
    for r in rows {
        runs.push(json!({
            "model": sqlx::Row::get::<String, _>(&r, 0),
            "metricName": sqlx::Row::get::<String, _>(&r, 1),
            "metricValue": sqlx::Row::get::<f64, _>(&r, 2),
            "createdAt": sqlx::Row::get::<String, _>(&r, 3),
        }));
    }

    Ok(Json(json!({ "runs": runs })))
}

pub async fn get_admin_connectors_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, name, type, config, sync_interval_seconds, last_synced_at, status FROM connectors WHERE workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 connectors 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut connectors = Vec::new();
    for r in rows {
        let config_str = sqlx::Row::get::<String, _>(&r, 3);
        let config_val: serde_json::Value = serde_json::from_str(&config_str).unwrap_or(json!({}));
        let repo = config_val.get("repo").and_then(|v| v.as_str()).map(|s| s.to_string());

        connectors.push(json!({
            "connectorId": sqlx::Row::get::<String, _>(&r, 0),
            "name": sqlx::Row::get::<String, _>(&r, 1),
            "type": sqlx::Row::get::<String, _>(&r, 2),
            "config": config_str,
            "syncIntervalSeconds": sqlx::Row::get::<i64, _>(&r, 4),
            "lastSyncedAt": sqlx::Row::get::<Option<String>, _>(&r, 5).unwrap_or_default(),
            "status": sqlx::Row::get::<String, _>(&r, 6),
            "repo": repo,
        }));
    }

    Ok(Json(json!({ "connectors": connectors })))
}
