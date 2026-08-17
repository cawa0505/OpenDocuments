//! POST /api/v1/search — R1/R2/R4 契約。
//! body: `{ query, top_k?, threshold? }`；X-Workspace header 解析工作空間（R4）。
//! 回 `{ hits: [{ doc_path, spec_id, score, snippet }] }`（R2）。
//! 同步呼叫 `SearchBackend::search_hits`（內部以 block_in_place+block_on 跑非同步 lancedb）。

use crate::utils::resolve_workspace_id;
use crate::McpState;
use crate::SearchHit;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub top_k: Option<usize>,
    pub threshold: Option<f32>,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
}

pub async fn search_handler(
    State(state): State<Arc<McpState>>,
    headers: HeaderMap,
    Json(body): Json<SearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, String)> {
    let query = body.query.trim();
    if query.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "query 不可為空".to_string()));
    }
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, "workspace 解析失敗".to_string()))?;
    if !state.search.engine_available() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "engine_unavailable: LanceDB 引擎未啟動或已崩潰".to_string(),
        ));
    }
    let cfg = state.config_manager.get_config().await;
    let top_k = body.top_k.unwrap_or(10);
    let threshold = body.threshold.unwrap_or(cfg.model.score_threshold);
    let hits = state
        .search
        .search_hits(query, top_k, threshold, &workspace_id)
        .await;
    Ok(Json(SearchResponse { hits }))
}
