use crate::utils::resolve_workspace_id;
use crate::McpState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentQuery {
    pub query: String,
    pub profile: String,
    pub confidence_score: Option<f64>,
    pub response_time_ms: Option<i64>,
    pub route: Option<String>,
    pub created_at: String,
}

pub async fn workbench_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, (StatusCode, String)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|status| (status, "Workspace not found".to_string()))?;

    // 1. 讀取 documents 總量與 chunks
    let corpus_counts = sqlx::query(
        "SELECT COUNT(*) as doc_count, COALESCE(SUM(chunk_count), 0) as chunk_count FROM documents WHERE deleted_at IS NULL AND workspace_id = ?"
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let doc_count: i64 = sqlx::Row::get(&corpus_counts, 0);
    let chunk_count: i64 = sqlx::Row::get(&corpus_counts, 1);

    // 2. 來源分佈與狀態分佈
    let source_rows = sqlx::query(
        "SELECT source_type, COUNT(*) as count FROM documents WHERE deleted_at IS NULL AND workspace_id = ? GROUP BY source_type"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut source_distribution = HashMap::new();
    for r in source_rows {
        source_distribution.insert(
            sqlx::Row::get::<String, _>(&r, 0),
            sqlx::Row::get::<i64, _>(&r, 1),
        );
    }

    let status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count FROM documents WHERE deleted_at IS NULL AND workspace_id = ? GROUP BY status"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut status_distribution = HashMap::new();
    for r in status_rows {
        status_distribution.insert(
            sqlx::Row::get::<String, _>(&r, 0),
            sqlx::Row::get::<i64, _>(&r, 1),
        );
    }

    // 3. 查詢質量與回饋
    let quality_row = sqlx::query(
        "SELECT COUNT(*) as total_queries, AVG(confidence_score) as avg_confidence, AVG(response_time_ms) as avg_response_time FROM query_logs WHERE workspace_id = ?"
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_queries: i64 = sqlx::Row::get(&quality_row, 0);
    let avg_confidence: f64 = sqlx::Row::get::<Option<f64>, _>(&quality_row, 1).unwrap_or(0.0);
    let avg_response_time: f64 = sqlx::Row::get::<Option<f64>, _>(&quality_row, 2).unwrap_or(0.0);

    let feedback_row = sqlx::query(
        "SELECT
            SUM(CASE WHEN feedback = 'positive' THEN 1 ELSE 0 END) as positive,
            SUM(CASE WHEN feedback = 'negative' THEN 1 ELSE 0 END) as negative
         FROM query_logs WHERE feedback IS NOT NULL AND workspace_id = ?",
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let positive_fb: i64 = sqlx::Row::get::<Option<i64>, _>(&feedback_row, 0).unwrap_or(0);
    let negative_fb: i64 = sqlx::Row::get::<Option<i64>, _>(&feedback_row, 1).unwrap_or(0);

    // 4. 最近查詢紀錄
    let recent_query_rows = sqlx::query(
        "SELECT query, profile, confidence_score, response_time_ms, route, created_at FROM query_logs WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut recent_queries = Vec::new();
    for r in recent_query_rows {
        recent_queries.push(RecentQuery {
            query: sqlx::Row::get(&r, 0),
            profile: sqlx::Row::get(&r, 1),
            confidence_score: sqlx::Row::get(&r, 2),
            response_time_ms: sqlx::Row::get(&r, 3),
            route: sqlx::Row::get(&r, 4),
            created_at: sqlx::Row::get::<Option<String>, _>(&r, 5).unwrap_or_default(),
        });
    }

    // 5. 建議提問（依 X-Locale 三語；缺省時回傳繁中，與 chat.rs 一致）
    let locale = headers
        .get("x-locale")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("zh-TW");

    let empty: [&str; 3] = match locale {
        "en" => [
            "What should I upload first?",
            "How do I connect a documentation source?",
            "What can OpenDocuments answer once documents are indexed?",
        ],
        "ko" => [
            "어떤 문서를 먼저 업로드해야 하나요?",
            "문서 소스는 어떻게 연결하나요?",
            "인덱싱 후 어떤 질문에 답할 수 있나요?",
        ],
        _ => [
            "我該先上傳哪些文件？",
            "如何連接文件來源？",
            "文件建立索引後能問什麼？",
        ],
    };

    let populated: [&str; 3] = match locale {
        "en" => [
            "Summarize the most important docs in this workspace.",
            "Which source explains the current deployment process?",
            "Find policy or architecture notes related to authentication.",
        ],
        "ko" => [
            "작업 공간의 주요 문서를 요약해 주세요.",
            "현재 배포 프로세스를 설명하는 소스는 무엇인가요?",
            "인증 관련 정책 또는 아키텍처 노트를 찾아주세요.",
        ],
        _ => [
            "總結工作區重點文件",
            "哪裡說明目前的部署流程？",
            "尋找認證相關的政策文件",
        ],
    };

    let suggested_questions: Vec<String> = if doc_count == 0 {
        empty.iter().map(|s| s.to_string()).collect()
    } else {
        populated.iter().map(|s| s.to_string()).collect()
    };

    Ok(Json(json!({
        "health": {
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION").to_string(),
            "modelStatus": "ready",
            "models": 1,
        },
        "corpus": {
            "documents": doc_count,
            "chunks": chunk_count,
            "sourceDistribution": source_distribution,
            "statusDistribution": status_distribution,
        },
        "quality": {
            "totalQueries": total_queries,
            "avgConfidence": (avg_confidence * 100.0).round() / 100.0,
            "avgResponseTimeMs": avg_response_time.round() as i64,
            "feedback": {
                "positive": positive_fb,
                "negative": negative_fb,
            }
        },
        "connectors": { "total": 0, "active": 0, "recent": [] },
        "workspace": { "name": workspace_id, "mode": "single" },
        "recentQueries": recent_queries,
        "suggestedQuestions": suggested_questions,
    })))
}
