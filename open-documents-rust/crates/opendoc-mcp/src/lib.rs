#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post, delete},
    http::StatusCode,
    Json, Router,
};
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, RwLock};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use uuid::Uuid;

// ── Search backend trait ─────────────────────────────────────
// Add when ConfigManager's search API stabilizes.

pub trait SearchBackend: Send + Sync {
    fn search_and_rerank(&self, query: &str, threshold: f32) -> Vec<opendoc_types::DocumentChunk>;
}

// ── Shared state ──────────────────────────────────────────────

type SessionMap = Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>;

pub struct McpState {
    pub sessions: SessionMap,
    pub search: Arc<dyn SearchBackend>,
    pub config_manager: Arc<opendoc_storage::ConfigManager>,
    pub db_pool: sqlx::SqlitePool,
}

// ── Workspace resolver ────────────────────────────────────────
/// Resolve workspace from X-Workspace header.
/// Empty or missing header → configured default workspace.
async fn resolve_workspace(state: &McpState, headers: &axum::http::HeaderMap) -> String {
    let from_header = headers
        .get("x-workspace")
        .and_then(|h| h.to_str().ok())
        .filter(|s| !s.is_empty());
    match from_header {
        Some(ws) => ws.to_string(),
        None => state.config_manager.get_config().await.model.default_workspace.clone(),
    }
}

// ── Health response ──────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceItem {
    id: String,
    name: String,
    mode: String,
    settings: serde_json::Value,
    created_at: String,
    is_default: bool,
}

#[derive(Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
struct DictionaryEntry {
    id: String,
    workspace_id: String,
    key: String,
    value: String,
    created_at: String,
}

#[derive(Serialize)]
struct DocumentItem {
    id: String,
    title: String,
    source_type: String,
    source_path: String,
    file_type: String,
    file_size_bytes: Option<i64>,
    connector_id: Option<String>,
    chunk_count: i64,
    status: String,
    content_hash: Option<String>,
    error_message: Option<String>,
    created_at: String,
    updated_at: String,
    indexed_at: Option<String>,
    workspace_id: String,
}

async fn list_workspaces_handler(
    State(state): State<Arc<McpState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, datetime(created_at, 'localtime') FROM workspaces"
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
        let is_default = row.0 == default_ws_name;
        workspaces.push(WorkspaceItem {
            id: row.0.clone(),
            name: row.0.clone(),
            mode: "personal".to_string(),
            settings: json!({}),
            created_at: row.1,
            is_default,
        });
    }

    Ok(Json(json!({ "workspaces": workspaces })))
}

async fn list_documents_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace(&state, &headers).await;

    let rows: Vec<(String, String, String, Option<String>, Option<String>, Option<i64>, Option<String>, i64, String, Option<String>, Option<String>, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), indexed_at, workspace_id FROM documents WHERE workspace_id = ? AND deleted_at IS NULL ORDER BY created_at DESC"
    )
    .bind(&workspace)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 documents 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut documents = Vec::new();
    for r in rows {
        documents.push(DocumentItem {
            id: r.0,
            title: r.1,
            source_type: r.2,
            source_path: r.3.unwrap_or_default(),
            file_type: r.4.unwrap_or_default(),
            file_size_bytes: r.5,
            connector_id: r.6,
            chunk_count: r.7,
            status: r.8,
            content_hash: r.9,
            error_message: r.10,
            created_at: r.11,
            updated_at: r.12.unwrap_or_default(),
            indexed_at: r.13,
            workspace_id: r.14,
        });
    }

    Ok(Json(json!({ "documents": documents })))
}

async fn get_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let row = sqlx::query(
        "SELECT id, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, datetime(created_at, 'localtime') as created_at, datetime(updated_at, 'localtime') as updated_at, indexed_at, workspace_id FROM documents WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢文件失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match row {
Some(r) => {
             let doc = serde_json::json!({
                 "id": sqlx::Row::get::<String, _>(&r, 0),
                 "title": sqlx::Row::get::<String, _>(&r, 1),
                 "source_type": sqlx::Row::get::<String, _>(&r, 2),
                 "source_path": sqlx::Row::get::<Option<String>, _>(&r, 3).unwrap_or_default(),
                 "file_type": sqlx::Row::get::<Option<String>, _>(&r, 4),
                 "file_size_bytes": sqlx::Row::get::<Option<i64>, _>(&r, 5),
                 "connector_id": sqlx::Row::get::<Option<String>, _>(&r, 6),
                 "chunk_count": sqlx::Row::get::<i64, _>(&r, 7),
                 "status": sqlx::Row::get::<String, _>(&r, 8),
                 "content_hash": sqlx::Row::get::<Option<String>, _>(&r, 9),
                 "error_message": sqlx::Row::get::<Option<String>, _>(&r, 10),
                 "created_at": sqlx::Row::get::<String, _>(&r, 11),
                 "updated_at": sqlx::Row::get::<Option<String>, _>(&r, 12).unwrap_or_default(),
                 "indexed_at": sqlx::Row::get::<Option<String>, _>(&r, 13).unwrap_or_default(),
                 "workspace_id": sqlx::Row::get::<String, _>(&r, 14),
             });
            Ok(Json(doc))
        }
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn list_collections_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let rows = sqlx::query("SELECT id, name, description FROM collections WHERE workspace_id = ?")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 collections 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut collections = Vec::new();
    for r in rows {
        collections.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "name": sqlx::Row::get::<String, _>(&r, 1),
            "description": sqlx::Row::get::<Option<String>, _>(&r, 2).unwrap_or_default(),
        }));
    }

    Ok(Json(json!({ "collections": collections })))
}

async fn list_conversations_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let rows = sqlx::query(
        "SELECT id, title, shared, created_at FROM conversations WHERE workspace_id = ? AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 100"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversations 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut conversations = Vec::new();
    for r in rows {
        conversations.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "title": sqlx::Row::get::<String, _>(&r, 1),
            "shared": sqlx::Row::get::<i64, _>(&r, 2) == 1,
            "createdAt": sqlx::Row::get::<String, _>(&r, 3),
        }));
    }

    Ok(Json(json!({ "conversations": conversations })))
}

#[derive(Deserialize)]
struct CreateWorkspaceReq {
    id: String,
}

async fn create_workspace_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<CreateWorkspaceReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    sqlx::query("INSERT OR IGNORE INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)")
        .bind(&payload.id)
        .bind(&payload.id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立工作空間失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "success": true })))
}

async fn delete_workspace_handler(
    State(state): State<Arc<McpState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let default_workspace = state.config_manager.get_config().await.model.default_workspace;
    if id == default_workspace {
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

async fn get_dictionary_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let rows = sqlx::query("SELECT id, workspace_id, key, value, datetime(created_at, 'localtime') FROM dictionary WHERE workspace_id = ?")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 dictionary 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut entries = Vec::new();
    for r in rows {
        entries.push(DictionaryEntry {
            id: sqlx::Row::get(&r, 0),
            workspace_id: sqlx::Row::get(&r, 1),
            key: sqlx::Row::get(&r, 2),
            value: sqlx::Row::get(&r, 3),
            created_at: sqlx::Row::get::<Option<String>, _>(&r, 4).unwrap_or_default(),
        });
    }

    Ok(Json(json!({ "entries": entries })))
}

#[derive(Deserialize)]
struct AddEntryReq {
    key: String,
    value: String,
}

async fn add_dictionary_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<AddEntryReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let trimmed_key = req.key.trim().to_string();
    let trimmed_value = req.value.trim().to_string();

    if trimmed_key.is_empty() || trimmed_value.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 檢查 key 是否在目前工作空間中已存在
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM dictionary WHERE workspace_id = ? AND key = ?")
        .bind(&workspace_id)
        .bind(&trimmed_key)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 檢查 dictionary 鍵失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let entry = if let Some((id,)) = existing {
        sqlx::query("UPDATE dictionary SET value = ?, created_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&trimmed_value)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 更新 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        DictionaryEntry {
            id,
            workspace_id,
            key: trimmed_key,
            value: trimmed_value,
            created_at: now,
        }
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value, created_at) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)")
            .bind(&id)
            .bind(&workspace_id)
            .bind(&trimmed_key)
            .bind(&trimmed_value)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 插入 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        DictionaryEntry {
            id,
            workspace_id,
            key: trimmed_key,
            value: trimmed_value,
            created_at: now,
        }
    };

    Ok(Json(json!(entry)))
}

async fn delete_dictionary_handler(
    State(state): State<Arc<McpState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    sqlx::query("DELETE FROM dictionary WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 dictionary 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Deserialize)]
struct ImportSeedReq {
    language: String,
}

async fn import_seed_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ImportSeedReq>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let glossary: std::collections::HashMap<&str, &str> = if req.language == "zh-TW" {
        vec![
            ("認證", "authentication"), ("設定", "configuration"), ("部署", "deployment"),
            ("安裝", "installation"), ("資料庫", "database"), ("伺服器", "server"),
            ("客戶端", "client"), ("使用者", "user"), ("管理員", "admin"),
            ("安全性", "security"), ("權限", "permission"), ("登入", "login"),
            ("密碼", "password"), ("搜尋", "search"), ("文檔", "document"),
            ("檔案", "file"), ("上傳", "upload"), ("下載", "download"),
            ("錯誤", "error"), ("除錯", "debugging"), ("修復", "fix"),
            ("測試", "test"), ("建置", "build"), ("執行", "run"),
            ("函式", "function"), ("變數", "variable"), ("型態", "type"),
            ("模組", "module"), ("套件", "package"), ("程式庫", "library"),
            ("框架", "framework"), ("元件", "component"), ("介面", "interface"),
            ("環境變數", "environment variable"), ("快取", "cache"), ("佇列", "queue"),
            ("架構", "architecture"), ("微服務", "microservice"), ("設計", "design"),
            ("模式", "pattern"), ("依賴", "dependency"), ("擴充性", "scalability"),
            ("中介軟體", "middleware"), ("端點", "endpoint"), ("路由", "routing"),
            ("閘道", "gateway"), ("代理", "proxy"), ("負載平衡", "load balancer"),
            ("服務網格", "service mesh"), ("單體", "monolithic"), ("後端", "backend"),
            ("前端", "frontend"), ("容器", "container"), ("管線", "pipeline"),
            ("監控", "monitoring"), ("基礎設施", "infrastructure"), ("雲端", "cloud"),
            ("健康檢查", "health check"), ("命名空間", "namespace"), ("節點", "node"),
            ("服務", "service"), ("資料卷", "volume"), ("機密", "secret"),
            ("備份", "backup"), ("還原", "restore"), ("查詢", "query"),
            ("索引", "index"), ("交易", "transaction"), ("向量", "vector"),
            ("嵌入", "embedding"), ("相似度", "similarity"), ("連線池", "connection pool"),
            ("機器學習", "machine learning"), ("推論", "inference"), ("微調", "fine-tuning"),
            ("模型", "model"), ("詞記", "token"), ("檢索增強生成", "retrieval augmented generation"),
            ("切片", "chunking"), ("重排", "reranking")
        ].into_iter().collect()
    } else if req.language == "ko-KR" {
        vec![
            ("인증", "authentication"), ("설정", "configuration"), ("배포", "deployment"),
            ("설치", "installation"), ("데이터베이스", "database"), ("서버", "server"),
            ("클라이언트", "client"), ("사용자", "user"), ("관리자", "admin"),
            ("보안", "security"), ("권한", "permission"), ("로그인", "login"),
            ("비밀번호", "password"), ("검색", "search"), ("문서", "document"),
            ("파일", "file"), ("업로드", "upload"), ("다운로드", "download"),
            ("오류", "error"), ("디버깅", "debugging"), ("수정", "fix"),
            ("테스트", "test"), ("빌드", "build"), ("실행", "run"),
            ("함수", "function"), ("변수", "variable"), ("타입", "type"),
            ("모듈", "module"), ("패키지", "package"), ("라이브러리", "library"),
            ("프레임워크", "framework"), ("컴포넌트", "component"), ("인터페이스", "interface"),
            ("환경 변수", "environment variable"), ("캐시", "cache"), ("큐", "queue"),
            ("아키텍처", "architecture"), ("마이크로서비스", "microservice"), ("디자인", "design"),
            ("패턴", "pattern"), ("의존성", "dependency"), ("확장성", "scalability"),
            ("미들웨어", "middleware"), ("엔드포인트", "endpoint"), ("라우팅", "routing"),
            ("게이트웨이", "gateway"), ("프록시", "proxy"), ("로드 밸런서", "load balancer"),
            ("서비스 메시", "service mesh"), ("모놀리식", "monolithic"), ("백엔드", "backend"),
            ("프론트엔드", "frontend"), ("컨테이너", "container"), ("파이프라인", "pipeline"),
            ("모니터링", "monitoring"), ("인프라", "infrastructure"), ("클라우드", "cloud"),
            ("헬스 체크", "health check"), ("네임스페이스", "namespace"), ("노드", "node"),
            ("서비스", "service"), ("볼륨", "volume"), ("비밀", "secret"),
            ("백업", "backup"), ("복구", "restore"), ("질의", "query"),
            ("인덱스", "index"), ("트랜잭션", "transaction"), ("벡터", "vector"),
            ("임베딩", "embedding"), ("유사도", "similarity"), ("커넥션 풀", "connection pool"),
            ("머신 러닝", "machine learning"), ("추론", "inference"), ("파인 튜닝", "fine-tuning"),
            ("모델", "model"), ("토큰", "token"), ("검색 증강 생성", "retrieval augmented generation"),
            ("청킹", "chunking"), ("리랭킹", "reranking")
        ].into_iter().collect()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // 使用 Transaction 進行原子批量 UPSERT 寫入
    let mut tx = state.db_pool.begin().await.map_err(|e| {
        eprintln!("💥 無法開啟 Transaction: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    for (k, v) in glossary {
        let trimmed_key = k.trim().to_string();
        let trimmed_value = v.trim().to_string();

        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM dictionary WHERE workspace_id = ? AND key = ?")
            .bind(&workspace_id)
            .bind(&trimmed_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                eprintln!("💥 交易中查詢 dictionary 失敗: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if let Some((id,)) = existing {
            sqlx::query("UPDATE dictionary SET value = ?, created_at = CURRENT_TIMESTAMP WHERE id = ?")
                .bind(&trimmed_value)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    eprintln!("💥 交易中更新 dictionary 失敗: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        } else {
            let id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value, created_at) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)")
                .bind(&id)
                .bind(&workspace_id)
                .bind(&trimmed_key)
                .bind(&trimmed_value)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    eprintln!("💥 交易中插入 dictionary 失敗: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?;
        }
    }

    tx.commit().await.map_err(|e| {
        eprintln!("💥 提交 Transaction 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "imported": true })))
}

async fn get_admin_stats_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace(&state, &headers).await;

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

async fn get_admin_search_quality_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace(&state, &headers).await;

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

async fn get_admin_query_logs_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

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

async fn get_admin_benchmark_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

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

async fn get_admin_connectors_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    let rows = sqlx::query(
        "SELECT name, type, config, sync_interval_seconds, last_synced_at, status FROM connectors WHERE workspace_id = ? AND deleted_at IS NULL"
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
        connectors.push(json!({
            "name": sqlx::Row::get::<String, _>(&r, 0),
            "type": sqlx::Row::get::<String, _>(&r, 1),
            "config": sqlx::Row::get::<String, _>(&r, 2),
            "syncIntervalSeconds": sqlx::Row::get::<i64, _>(&r, 3),
            "lastSyncedAt": sqlx::Row::get::<Option<String>, _>(&r, 4).unwrap_or_default(),
            "status": sqlx::Row::get::<String, _>(&r, 5),
        }));
    }

    Ok(Json(json!({ "connectors": connectors })))
}

async fn delete_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace(&state, &headers).await;

    sqlx::query("UPDATE documents SET deleted_at = CURRENT_TIMESTAMP WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub engine: String,
    pub version: String,
}

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        engine: "OpenDocuments Rust Core".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// ── Workbench handler and structs ──────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct UploadResponse {
    #[serde(rename = "documentId")]
    pub document_id: String,
    pub chunks: usize,
    pub status: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchResponse {
    pub health: WorkbenchHealth,
    pub corpus: WorkbenchCorpus,
    pub quality: WorkbenchQuality,
    pub connectors: WorkbenchConnectors,
    pub workspace: WorkbenchWorkspace,
    pub recent_queries: Vec<RecentQuery>,
    pub suggested_questions: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchHealth {
    pub status: String,
    pub version: String,
    pub model_status: String,
    pub models: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchCorpus {
    pub documents: i64,
    pub chunks: i64,
    pub source_distribution: HashMap<String, i64>,
    pub status_distribution: HashMap<String, i64>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchQuality {
    pub total_queries: i64,
    pub avg_confidence: f64,
    pub avg_response_time_ms: i64,
    pub feedback: WorkbenchFeedback,
}

#[derive(Serialize, Deserialize)]
pub struct WorkbenchFeedback {
    pub positive: i64,
    pub negative: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchConnectors {
    pub total: usize,
    pub active: usize,
    pub recent: Vec<RecentConnector>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentConnector {
    pub name: String,
    pub r#type: String,
    pub status: String,
    pub last_synced_at: Option<String>,
    pub repo: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchWorkspace {
    pub name: String,
    pub mode: String,
}

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

async fn workbench_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let workspace_id = resolve_workspace(&state, &headers).await;

    // 1. 讀取 documents 總量與 chunks
    let corpus_counts = sqlx::query(
        "SELECT COUNT(*) as doc_count, COALESCE(SUM(chunk_count), 0) as chunk_count FROM documents WHERE deleted_at IS NULL AND workspace_id = ?"
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let doc_count: i64 = sqlx::Row::get(&corpus_counts, 0);
    let chunk_count: i64 = sqlx::Row::get(&corpus_counts, 1);

    // 2. 來源分佈與狀態分佈
    let source_rows = sqlx::query(
        "SELECT source_type, COUNT(*) as count FROM documents WHERE deleted_at IS NULL AND workspace_id = ? GROUP BY source_type"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut source_distribution = HashMap::new();
    for r in source_rows {
        let st: String = sqlx::Row::get(&r, 0);
        let count: i64 = sqlx::Row::get(&r, 1);
        source_distribution.insert(st, count);
    }

    let status_rows = sqlx::query(
        "SELECT status, COUNT(*) as count FROM documents WHERE deleted_at IS NULL AND workspace_id = ? GROUP BY status"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut status_distribution = HashMap::new();
    for r in status_rows {
        let status: String = sqlx::Row::get(&r, 0);
        let count: i64 = sqlx::Row::get(&r, 1);
        status_distribution.insert(status, count);
    }

    // 3. 查詢質量與回饋
    let quality_row = sqlx::query(
        "SELECT COUNT(*) as total_queries, AVG(confidence_score) as avg_confidence, AVG(response_time_ms) as avg_response_time FROM query_logs WHERE workspace_id = ?"
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let total_queries: i64 = sqlx::Row::get(&quality_row, 0);
    let avg_confidence: f64 = sqlx::Row::get::<Option<f64>, _>(&quality_row, 1).unwrap_or(0.0);
    let avg_response_time: f64 = sqlx::Row::get::<Option<f64>, _>(&quality_row, 2).unwrap_or(0.0);

    let feedback_row = sqlx::query(
        "SELECT
            SUM(CASE WHEN feedback = 'positive' THEN 1 ELSE 0 END) as positive,
            SUM(CASE WHEN feedback = 'negative' THEN 1 ELSE 0 END) as negative
         FROM query_logs WHERE feedback IS NOT NULL AND workspace_id = ?"
    )
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let positive_fb: i64 = sqlx::Row::get::<Option<i64>, _>(&feedback_row, 0).unwrap_or(0);
    let negative_fb: i64 = sqlx::Row::get::<Option<i64>, _>(&feedback_row, 1).unwrap_or(0);

    // 4. 最近查詢紀錄
    let recent_query_rows = sqlx::query(
        "SELECT query, profile, confidence_score, response_time_ms, route, created_at FROM query_logs WHERE workspace_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut recent_queries = Vec::new();
    for r in recent_query_rows {
        let q: String = sqlx::Row::get(&r, 0);
        let prof: String = sqlx::Row::get(&r, 1);
        let conf: Option<f64> = sqlx::Row::get(&r, 2);
        let resp_t: Option<i64> = sqlx::Row::get(&r, 3);
        let rt: Option<String> = sqlx::Row::get(&r, 4);
        let created: Option<String> = sqlx::Row::get(&r, 5);

        recent_queries.push(RecentQuery {
            query: q,
            profile: prof,
            confidence_score: conf,
            response_time_ms: resp_t,
            route: rt,
            created_at: created.unwrap_or_default(),
        });
    }

    // 5. 建議提問：基於有無文件提供不同之 fallback 建議
    let suggested_questions = if doc_count == 0 {
        vec![
            "What should I upload first?".to_string(),
            "How do I connect a documentation source?".to_string(),
            "What can OpenDocuments answer once documents are indexed?".to_string(),
        ]
    } else {
        vec![
            "Summarize the most important docs in this workspace.".to_string(),
            "Which source explains the current deployment process?".to_string(),
            "Find policy or architecture notes related to authentication.".to_string(),
        ]
    };

    let response = json!({
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
        "connectors": {
            "total": 0,
            "active": 0,
            "recent": []
        },
        "workspace": {
            "name": workspace_id,
            "mode": "single"
        },
        "recentQueries": recent_queries,
        "suggestedQuestions": suggested_questions,
    });

    Ok(Json(response))
}

// ── Multipart / Upload Handler ─────────────────────────────────

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

async fn query_log_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<QueryLogRequest>,
) -> Result<Json<QueryLogResponse>, (axum::http::StatusCode, String)> {
    let workspace_id = resolve_workspace(&state, &headers).await;

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
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("寫入查詢紀錄失敗: {e}")))?;

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

async fn query_feedback_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<QueryFeedbackRequest>,
) -> Result<Json<QueryFeedbackResponse>, (axum::http::StatusCode, String)> {
    if payload.feedback != "positive" && payload.feedback != "negative" {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
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
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("更新回饋紀錄失敗: {e}")))?;

    Ok(Json(QueryFeedbackResponse { success: true }))
}

// ── Multipart / Upload Handler ─────────────────────────────────

async fn upload_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<UploadResponse>, (axum::http::StatusCode, String)> {
    // 1. 取得 Header 中的工作空間 (預設 "default") 與集合識別碼 (預設 "default")
    let workspace_id = resolve_workspace(&state, &headers).await;

    let collection_id = headers
        .get("x-collection")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("default")
        .to_string();

    let mut original_name = None;
    let mut file_bytes = Vec::new();

    // 2. 讀取 multipart 欄位
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        if name == "file" {
            if let Some(filename) = field.file_name() {
                original_name = Some(filename.to_string());
            }
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| (axum::http::StatusCode::BAD_REQUEST, e.to_string()))?
                .to_vec();
        }
    }

    if file_bytes.is_empty() {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "上傳檔案內容不可為空".to_string(),
        ));
    }

    // 3. 💡 【防護機制】在暫存目錄中建立安全的隨機檔名暫存實體檔案，並加上副檔名，確保解析不爆記憶體
    let temp_dir = std::env::temp_dir();
    let temp_file_id = uuid::Uuid::new_v4().to_string();
    
    // 從原名萃取小寫副檔名
    let ext_suffix = original_name
        .as_deref()
        .and_then(|name| std::path::Path::new(name).extension())
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{}", ext.to_lowercase()))
        .unwrap_or_default();

    let temp_file_path = temp_dir.join(format!("opendoc-{temp_file_id}{ext_suffix}"));

    std::fs::write(&temp_file_path, &file_bytes)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("寫入暫存檔失敗: {e}")))?;

    // 4. 呼叫雙軌高彈性 Parser 進行解析，回傳 chunks 向量
    let chunks = opendoc_parser::parse_file(
        &temp_file_path,
        original_name.as_deref(),
        &workspace_id,
        &collection_id,
    )
    .await
    .map_err(|e| {
        let _ = std::fs::remove_file(&temp_file_path);
        (axum::http::StatusCode::BAD_REQUEST, e)
    })?;

    // 5. 💡 刪除解析完畢的實體暫存檔案
    let _ = std::fs::remove_file(&temp_file_path);

    let chunks_count = chunks.len();

    // 6. 持久化寫入 SQLite 資料庫 documents 表與 lancedb 向量庫 (或模擬)
    //    同時確保 WAL 與並行防護，100% 避免 Node.js 與 Rust 重複衝突

    // 確保 workspace 存在，避免 FK constraint 失敗
    sqlx::query(
        "INSERT OR IGNORE INTO workspaces (id, name, mode, is_default, created_at) VALUES (?, ?, 'personal', 0, CURRENT_TIMESTAMP)"
    )
    .bind(&workspace_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("確保工作空間失敗: {e}")))?;

    let file_path_str = original_name.clone().unwrap_or_else(|| format!("opendoc-{temp_file_id}"));

    // 寫入 documents 資料庫
    let ext_display = ext_suffix.trim_start_matches('.').to_uppercase();
    // source_path 用 workspace/檔名 確保跨 workspace 可區分
    let source_path = format!("{}/{}", workspace_id, file_path_str);
    let file_size = file_bytes.len() as i64;

    sqlx::query(
        "INSERT INTO documents (id, title, source_type, source_path, file_type, file_size_bytes, status, chunk_count, workspace_id, created_at, indexed_at)
         VALUES (?, ?, ?, ?, ?, ?, 'indexed', ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
         title = excluded.title,
         source_path = excluded.source_path,
         file_type = excluded.file_type,
         file_size_bytes = excluded.file_size_bytes,
         chunk_count = excluded.chunk_count,
         indexed_at = CURRENT_TIMESTAMP"
    )
    .bind(&temp_file_id)
    .bind(&file_path_str)
    .bind(&ext_display)
    .bind(&source_path)
    .bind(&ext_display)
    .bind(file_size)
    .bind(chunks_count as i64)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("寫入資料庫失敗: {e}")))?;

    Ok(Json(UploadResponse {
        document_id: temp_file_id.clone(),
        chunks: chunks_count,
        status: "indexed".to_string(),
    }))
}

// ── MCP SSE endpoint ────────────────────────────────────────

pub async fn sse_handler(
    State(state): State<Arc<McpState>>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let session_id = Uuid::new_v4().to_string();
    let (tx, mut rx) = mpsc::channel::<String>(100);

    state.sessions.write().await.insert(session_id.clone(), tx);

    let endpoint_event = Event::default()
        .event("endpoint")
        .data(format!("/api/mcp/message?sessionId={session_id}"));

    let session_id_clone = session_id.clone();
    let sessions = Arc::clone(&state.sessions);

    let stream = async_stream::stream! {
        yield Ok(endpoint_event);

        while let Some(msg) = rx.recv().await {
            yield Ok(Event::default().event("message").data(msg));
        }

        sessions.write().await.remove(&session_id_clone);
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(std::time::Duration::from_secs(30))
            .text("ping"),
    )
}

// ── MCP message endpoint (JSON-RPC over POST) ────────────────

pub async fn message_handler(
    State(state): State<Arc<McpState>>,
    Query(params): Query<HashMap<String, String>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let session_id = params.get("sessionId").cloned().unwrap_or_default();
    let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let id = request.get("id").cloned().unwrap_or(json!(null));

    let response = match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "opendocuments-rust", "version": "0.1.0" }
            }
        }),

        "notifications/initialized" => return Json(json!({})),

        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "opendocuments_search",
                        "description": "Search documents in OpenDocuments RAG knowledge base",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "query": { "type": "string", "description": "Search query" },
                                "workspace": { "type": "string", "description": "Workspace name", "default": "default" },
                                "limit": { "type": "integer", "description": "Max results to return", "default": 5 }
                            },
                            "required": ["query"]
                        }
                    },
                    {
                        "name": "opendocuments_index_path",
                        "description": "Index a local file or directory into the document store",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Absolute path to a file or directory to index" },
                                "workspace": { "type": "string", "description": "Workspace/project name to index into (optional)" }
                            },
                            "required": ["path"]
                        }
                    },
                    {
                        "name": "opendocuments_healthz",
                        "description": "Check OpenDocuments server status",
                        "inputSchema": { "type": "object", "properties": {} }
                    }
                ]
            }
        }),

        "tools/call" => {
            let tool_name = request["params"]["name"].as_str().unwrap_or("");
            let args = &request["params"]["arguments"];

            match tool_name {
                "opendocuments_search" => {
                    let query_str = args["query"].as_str().unwrap_or("");
                    let limit = args["limit"].as_u64().unwrap_or(5) as usize;

                    let results = state.search.search_and_rerank(query_str, 0.60);
                    let limited: Vec<_> = results.into_iter().take(limit).collect();
                    let text = serde_json::to_string_pretty(&limited).unwrap_or_else(|_| "[]".to_string());

                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": text }]
                        }
                    })
                }
                "opendocuments_index_path" => {
                    let path_str = args["path"].as_str().unwrap_or("");
                    let workspace = args["workspace"].as_str().unwrap_or("default");

                    let path_buf = std::path::PathBuf::from(path_str);
                    let results = if path_buf.exists() {
                        let canon_path = path_buf.canonicalize().unwrap_or_else(|_| path_buf.clone());
                        let ignored_dirs = [
                            "node_modules", ".git", "dist", "build", ".turbo", ".next", ".cache", 
                            "__pycache__", "venv", ".env", "out"
                        ];
                        let supported_extensions = [
                            ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".mdx", ".json", ".yaml", ".yml", 
                            ".toml", ".css", ".html", ".htm", ".sh", ".sql", ".pdf", ".docx", ".xlsx"
                        ];

                        let app_cfg = match tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                state.config_manager.get_config().await
                            })
                        }) {
                            c => c,
                        };

                        let upload_url = format!("{}/api/v1/documents/upload", app_cfg.server.url);
                        let client = reqwest::Client::new();
                        let mut success_count = 0;
                        let mut fail_count = 0;

                        let mut files = Vec::new();
                        if canon_path.is_file() {
                            files.push(canon_path.clone());
                        } else {
                            for entry in walkdir::WalkDir::new(&canon_path).into_iter().filter_entry(|entry| {
                                if let Some(name) = entry.file_name().to_str() {
                                    !ignored_dirs.contains(&name) && !name.starts_with('.')
                                } else {
                                    false
                                }
                            }) {
                                if let Ok(entry) = entry {
                                    if entry.file_type().is_file() {
                                        let file_path = entry.path();
                                        if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
                                            if !file_name.starts_with('.') {
                                                let ext = file_path.extension()
                                                    .and_then(|e| e.to_str())
                                                    .map(|e| format!(".{}", e.to_lowercase()))
                                                    .unwrap_or_default();
                                                if supported_extensions.contains(&ext.as_str()) {
                                                    files.push(file_path.to_path_buf());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        for file_path in files {
                            let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                            let file_bytes = match std::fs::read(&file_path) {
                                Ok(b) => b,
                                Err(_) => continue,
                            };
                            let part = match reqwest::multipart::Part::bytes(file_bytes)
                                .file_name(file_name.clone())
                                .mime_str("application/octet-stream") 
                            {
                                Ok(p) => p,
                                Err(_) => continue,
                            };
                            let form = reqwest::multipart::Form::new().part("file", part);

                            let req_res = tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    client.post(&upload_url)
                                        .header("X-Workspace", workspace)
                                        .multipart(form)
                                        .timeout(std::time::Duration::from_secs(180))
                                        .send()
                                        .await
                                })
                            });

                            match req_res {
                                Ok(resp) if resp.status().is_success() => {
                                    success_count += 1;
                                }
                                _ => {
                                    fail_count += 1;
                                }
                            }
                        }
                        format!("Success: Indexed {success_count} files, failed: {fail_count} files in workspace '{workspace}'")
                    } else {
                        format!("Error: Path not found: {path_str}")
                    };

                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": results }]
                        }
                    })
                }
                "opendocuments_healthz" => {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{ "type": "text", "text": "{\"status\":\"healthy\",\"engine\":\"OpenDocuments Rust Core\"}" }]
                        }
                    })
                }
                _ => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("Unknown tool: {tool_name}") }
                }),
            }
        }

        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": format!("Method not found: {method}") }
        }),
    };

    // Forward response back via the matching SSE session channel
    if let Some(tx) = state.sessions.read().await.get(&session_id) {
        let _ = tx.send(serde_json::to_string(&response).unwrap_or_default()).await;
    }

    Json(json!({}))
}

// ── Server entry point ───────────────────────────────────────

pub async fn run_mcp_stdio_server(
    search: Arc<dyn SearchBackend>,
    config_manager: Arc<opendoc_storage::ConfigManager>,
    db_pool: sqlx::SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use futures::StreamExt;
    use tokio::io::{stdin, stdout, AsyncBufReadExt, BufReader};

    let mcp_state = Arc::new(McpState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        search,
        config_manager,
        db_pool,
    });

    let mut lines = BufReader::new(stdin()).lines();
    let mut out = stdout();

    while let Some(line) = lines.next_line().await? {
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(json!(null));

        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": { "name": "opendocuments-rust", "version": "0.1.0" }
                }
            }),

            "notifications/initialized" => continue,

            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        {
                            "name": "opendocuments_search",
                            "description": "Search documents in OpenDocuments RAG knowledge base",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "query": { "type": "string", "description": "Search query" },
                                    "workspace": { "type": "string", "description": "Workspace name", "default": "default" },
                                    "limit": { "type": "integer", "description": "Max results to return", "default": 5 }
                                },
                                "required": ["query"]
                            }
                        },
                        {
                            "name": "opendocuments_index_path",
                            "description": "Index a local file or directory into the document store",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "path": { "type": "string", "description": "Absolute path to a file or directory to index" },
                                    "workspace": { "type": "string", "description": "Workspace/project name to index into (optional)" }
                                },
                                "required": ["path"]
                            }
                        },
                        {
                            "name": "opendocuments_healthz",
                            "description": "Check OpenDocuments server status",
                            "inputSchema": { "type": "object", "properties": {} }
                        }
                    ]
                }
            }),

            "tools/call" => {
                let tool_name = request["params"]["name"].as_str().unwrap_or("");
                let args = &request["params"]["arguments"];

                match tool_name {
                    "opendocuments_search" => {
                        let query_str = args["query"].as_str().unwrap_or("");
                        let limit = args["limit"].as_u64().unwrap_or(5) as usize;

                        let results = mcp_state.search.search_and_rerank(query_str, 0.60);
                        let limited: Vec<_> = results.into_iter().take(limit).collect();
                        let text = serde_json::to_string_pretty(&limited).unwrap_or_else(|_| "[]".to_string());

                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": text }]
                            }
                        })
                    }
                    "opendocuments_index_path" => {
                        let path_str = args["path"].as_str().unwrap_or("");
                        let workspace = args["workspace"].as_str().unwrap_or("default");

                        let path_buf = std::path::PathBuf::from(path_str);
                        let results = if path_buf.exists() {
                            let canon_path = path_buf.canonicalize().unwrap_or_else(|_| path_buf.clone());
                            let ignored_dirs = [
                                "node_modules", ".git", "dist", "build", ".turbo", ".next", ".cache", 
                                "__pycache__", "venv", ".env", "out"
                            ];
                            let supported_extensions = [
                                ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".mdx", ".json", ".yaml", ".yml", 
                                ".toml", ".css", ".html", ".htm", ".sh", ".sql", ".pdf", ".docx", ".xlsx"
                            ];

                            let app_cfg = match tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    mcp_state.config_manager.get_config().await
                                })
                            }) {
                                c => c,
                            };

                            let upload_url = format!("{}/api/v1/documents/upload", app_cfg.server.url);
                            let client = reqwest::Client::new();
                            let mut success_count = 0;
                            let mut fail_count = 0;

                            let mut files = Vec::new();
                            if canon_path.is_file() {
                                files.push(canon_path.clone());
                            } else {
                                for entry in walkdir::WalkDir::new(&canon_path).into_iter().filter_entry(|entry| {
                                    if let Some(name) = entry.file_name().to_str() {
                                        !ignored_dirs.contains(&name) && !name.starts_with('.')
                                    } else {
                                        false
                                    }
                                }) {
                                    if let Ok(entry) = entry {
                                        if entry.file_type().is_file() {
                                            let file_path = entry.path();
                                            if let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) {
                                                if !file_name.starts_with('.') {
                                                    let ext = file_path.extension()
                                                        .and_then(|e| e.to_str())
                                                        .map(|e| format!(".{}", e.to_lowercase()))
                                                        .unwrap_or_default();
                                                    if supported_extensions.contains(&ext.as_str()) {
                                                        files.push(file_path.to_path_buf());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            for file_path in files {
                                let file_name = file_path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                                let file_bytes = match std::fs::read(&file_path) {
                                    Ok(b) => b,
                                    Err(_) => continue,
                                };
                                let part = match reqwest::multipart::Part::bytes(file_bytes)
                                    .file_name(file_name.clone())
                                    .mime_str("application/octet-stream") 
                                {
                                    Ok(p) => p,
                                    Err(_) => continue,
                                };
                                let form = reqwest::multipart::Form::new().part("file", part);

                                let req_res = tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(async {
                                        client.post(&upload_url)
                                            .header("X-Workspace", workspace)
                                            .multipart(form)
                                            .timeout(std::time::Duration::from_secs(180))
                                            .send()
                                            .await
                                    })
                                });

                                match req_res {
                                    Ok(resp) if resp.status().is_success() => {
                                        success_count += 1;
                                    }
                                    _ => {
                                        fail_count += 1;
                                    }
                                }
                            }
                            format!("Success: Indexed {success_count} files, failed: {fail_count} files in workspace '{workspace}'")
                        } else {
                            format!("Error: Path not found: {path_str}")
                        };

                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": results }]
                            }
                        })
                    }
                    "opendocuments_healthz" => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{ "type": "text", "text": "{\"status\":\"healthy\",\"engine\":\"OpenDocuments Rust Core\"}" }]
                            }
                        })
                    }
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": format!("Unknown tool: {tool_name}") }
                    }),
                }
            }

            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        };

        let out_line = serde_json::to_string(&response).unwrap_or_default();
        tokio::io::AsyncWriteExt::write_all(&mut out, format!("{}\n", out_line).as_bytes()).await?;
        tokio::io::AsyncWriteExt::flush(&mut out).await?;
    }

    Ok(())
}

// ── Chat SSE handler ─────────────────────────────────────────
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatStreamRequest {
    query: String,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default, rename = "conversationId")]
    conversation_id: Option<String>,
}

// ponytail: 直接用 search_and_rerank + 組裝答案；LLM streaming 加 when 需要
async fn chat_stream_handler(
    State(state): State<Arc<McpState>>,
    axum::http::request::Parts { headers, .. }: axum::http::request::Parts,
    Json(req): Json<ChatStreamRequest>,
) -> Sse<impl Stream<Item = Result<Event, std::convert::Infallible>>> {
    let workspace = resolve_workspace(&state, &headers).await;
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let conversation_id = req.conversation_id;
    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    let results = state.search.search_and_rerank(&req.query, threshold);
    let limited: Vec<_> = results.into_iter().take(10).collect();

    let answer = if limited.is_empty() {
        "在現有文獻中未找到相關內容。".to_string()
    } else {
        limited.iter().enumerate().map(|(i, r)| {
            format!("[{}] {}", i + 1, r.content.chars().take(500).collect::<String>())
        }).collect::<Vec<_>>().join("\n\n")
    };

    let total_score: f32 = if limited.is_empty() { 0.0 }
        else { limited.iter().map(|r| r.relevance_score.unwrap_or(0.0)).sum::<f32>() / limited.len() as f32 };
    let (level, reason) = if total_score >= 0.75 { ("high", "多個高相關性片段") }
        else if total_score >= 0.55 { ("medium", "找到部分相關內容") }
        else if total_score >= 0.35 { ("low", "僅找到少量模糊匹配") }
        else { ("none", "未找到明確相關內容") };

    // Map DocumentChunk → frontend SearchResult shape
    let sources_mapped: Vec<serde_json::Value> = limited.iter().map(|r| {
        let source_path = r.metadata.get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&r.file_path);
        json!({
            "chunkId": format!("{}:{}", r.file_path, r.content.len()),
            "content": r.content,
            "score": r.relevance_score.unwrap_or(0.0),
            "documentId": r.file_path,
            "chunkType": format!("{:?}", r.chunk_type),
            "headingHierarchy": [],
            "sourcePath": source_path,
            "sourceType": "file",
        })
    }).collect();
    let sources_json = serde_json::to_string(&sources_mapped).unwrap_or_else(|_| "[]".to_string());
    let confidence_json = serde_json::to_string(&json!({
        "score": total_score, "level": level, "reason": reason
    })).unwrap_or_default();
    let done_json = serde_json::to_string(&json!({
        "queryId": query_id, "route": "rag", "profile": profile,
        "conversationId": conversation_id,
    })).unwrap_or_default();

    let stream = futures_util::stream::iter(vec![
        Ok::<_, std::convert::Infallible>(Event::default().event("sources").data(sources_json)),
        Ok(Event::default().event("confidence").data(confidence_json)),
        Ok(Event::default().event("chunk").data(answer)),
        Ok(Event::default().event("done").data(done_json)),
    ]);

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Non-streaming chat endpoint
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    query: String,
    #[serde(default)]
    profile: Option<String>,
}

async fn chat_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace(&state, &headers).await;
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    let results = state.search.search_and_rerank(&req.query, threshold);
    let limited: Vec<_> = results.into_iter().take(10).collect();

    let answer = if limited.is_empty() {
        "在現有文獻中未找到相關內容。".to_string()
    } else {
        limited.iter().enumerate().map(|(i, r)| {
            format!("[{}] {}", i + 1, r.content.chars().take(500).collect::<String>())
        }).collect::<Vec<_>>().join("\n\n")
    };

    let total_score: f32 = if limited.is_empty() { 0.0 }
        else { limited.iter().map(|r| r.relevance_score.unwrap_or(0.0)).sum::<f32>() / limited.len() as f32 };
    let (level, reason) = if total_score >= 0.75 { ("high", "多個高相關性片段") }
        else if total_score >= 0.55 { ("medium", "找到部分相關內容") }
        else if total_score >= 0.35 { ("low", "僅找到少量模糊匹配") }
        else { ("none", "未找到明確相關內容") };

    let sources_mapped: Vec<serde_json::Value> = limited.iter().map(|r| {
        let source_path = r.metadata.get("source_path")
            .and_then(|v| v.as_str())
            .unwrap_or(&r.file_path);
        json!({
            "chunkId": format!("{}:{}", r.file_path, r.content.len()),
            "content": r.content,
            "score": r.relevance_score.unwrap_or(0.0),
            "documentId": r.file_path,
            "chunkType": format!("{:?}", r.chunk_type),
            "headingHierarchy": [],
            "sourcePath": source_path,
            "sourceType": "file",
        })
    }).collect();

    let confidence_json = json!({
        "score": total_score, "level": level, "reason": reason
    });

    Ok(Json(json!({
        "queryId": query_id,
        "answer": answer,
        "sources": sources_mapped,
        "confidence": confidence_json,
        "route": "rag",
        "profile": profile,
    })))
}

pub async fn start_mcp_and_api_server(
    port: u16,
    search: Arc<dyn SearchBackend>,
    config_manager: Arc<opendoc_storage::ConfigManager>,
    db_pool: sqlx::SqlitePool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 [OpenDocuments] 大一統伺服器正啟動於 http://{addr}");

    let mcp_state = Arc::new(McpState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        search,
        config_manager,
        db_pool,
    });

    let api_routes = Router::new()
        .route("/healthz", get(health_handler))
        .route("/health", get(health_handler))
        .route("/workbench", get(workbench_handler))
        .route("/documents", get(list_documents_handler))
        .route("/documents/upload", post(upload_handler))
        .route("/documents/:id", delete(delete_document_handler))
        .route("/workspaces", get(list_workspaces_handler).post(create_workspace_handler))
        .route("/workspaces/:id", delete(delete_workspace_handler))
        .route("/collections", get(list_collections_handler))
        .route("/conversations", get(list_conversations_handler))
        .route("/dictionary", get(get_dictionary_handler).post(add_dictionary_handler))
        .route("/dictionary/:id", delete(delete_dictionary_handler))
        .route("/dictionary/import-seed", post(import_seed_handler))
        .route("/admin/stats", get(get_admin_stats_handler))
        .route("/admin/search-quality", get(get_admin_search_quality_handler))
        .route("/admin/benchmark", get(get_admin_benchmark_handler))
        .route("/admin/connectors", get(get_admin_connectors_handler))
        .route("/admin/query-logs", get(get_admin_query_logs_handler))
        .route("/query/log", post(query_log_handler))
        .route("/query/feedback", post(query_feedback_handler))
        .route("/chat", post(chat_handler))
        .route("/chat/stream", post(chat_stream_handler));

    let serve_dir = ServeDir::new("./dist")
        .not_found_service(tower_http::services::ServeFile::new("./dist/index.html"));

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .route("/mcp/sse", get(sse_handler))
        .route("/api/mcp/sse", get(sse_handler))
        .route("/mcp/message", post(message_handler))
        .route("/api/mcp/message", post(message_handler))
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .with_state(mcp_state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn build_test_state() -> Arc<McpState> {
        let db_pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();

        // 建立所有需要的資料表（對齊 init_db_pool 的 schema）
        sqlx::query(
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE documents (id TEXT PRIMARY KEY, title TEXT NOT NULL, source_type TEXT NOT NULL, source_path TEXT, file_type TEXT, file_size_bytes INTEGER, connector_id TEXT, chunk_count INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL, content_hash TEXT, error_message TEXT, workspace_id TEXT NOT NULL, deleted_at DATETIME, created_at DATETIME DEFAULT CURRENT_TIMESTAMP, updated_at DATETIME, indexed_at DATETIME)"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE query_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, query TEXT NOT NULL, profile TEXT NOT NULL, confidence_score REAL, response_time_ms INTEGER, route TEXT, feedback TEXT, workspace_id TEXT NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE dictionary (id TEXT PRIMARY KEY, workspace_id TEXT, key TEXT NOT NULL, value TEXT NOT NULL, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE connectors (id TEXT PRIMARY KEY, workspace_id TEXT, name TEXT NOT NULL, type TEXT NOT NULL, config TEXT NOT NULL DEFAULT '{}', sync_interval_seconds INTEGER DEFAULT 300, last_synced_at TEXT, status TEXT DEFAULT 'active', deleted_at TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE collections (id TEXT PRIMARY KEY, workspace_id TEXT, name TEXT NOT NULL, description TEXT, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY, workspace_id TEXT, user_id TEXT, title TEXT, shared INTEGER DEFAULT 0, deleted_at TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE benchmark_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, workspace_id TEXT NOT NULL, model TEXT NOT NULL, metric_name TEXT NOT NULL, metric_value REAL NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)"
        ).execute(&db_pool).await.unwrap();

        // 插入測試資料
        sqlx::query("INSERT INTO workspaces (id, name) VALUES ('homelab', 'homelab')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO documents (id, title, source_type, source_path, status, chunk_count, workspace_id) VALUES ('doc1', 'test.md', 'markdown', 'docs/test.md', 'indexed', 5, 'homelab')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, feedback, workspace_id) VALUES ('hello', 'hybrid', 0.85, 120, 'semantic', 'positive', 'homelab')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO dictionary (workspace_id, key, value) VALUES ('homelab', 'domain', 'opendoc')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO connectors (workspace_id, name, type, config) VALUES ('homelab', 'github', 'git', '{}')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO collections (workspace_id, name, description) VALUES ('homelab', 'default', 'default collection')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (workspace_id, title) VALUES ('homelab', 'First chat')").execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO benchmark_runs (workspace_id, model, metric_name, metric_value) VALUES ('homelab', 'ollama', 'recall', 0.92)").execute(&db_pool).await.unwrap();

        Arc::new(McpState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            search: Arc::new(MockSearch),
            config_manager: Arc::new(opendoc_storage::ConfigManager::load_or_init().unwrap()),
            db_pool,
        })
    }

    struct MockSearch;
    impl SearchBackend for MockSearch {
        fn search_and_rerank(&self, _query: &str, _threshold: f32) -> Vec<opendoc_types::DocumentChunk> {
            vec![]
        }
    }

    fn build_router(state: Arc<McpState>) -> Router {
        Router::new()
            .route("/health", get(health_handler))
            .route("/workspaces", get(list_workspaces_handler))
            .route("/documents", get(list_documents_handler))
            .route("/collections", get(list_collections_handler))
            .route("/conversations", get(list_conversations_handler))
            .route("/dictionary", get(get_dictionary_handler).post(add_dictionary_handler))
            .route("/dictionary/:id", delete(delete_dictionary_handler))
            .route("/dictionary/import-seed", post(import_seed_handler))
            .route("/admin/stats", get(get_admin_stats_handler))
            .route("/admin/search-quality", get(get_admin_search_quality_handler))
            .route("/admin/benchmark", get(get_admin_benchmark_handler))
            .route("/admin/connectors", get(get_admin_connectors_handler))
            .route("/admin/query-logs", get(get_admin_query_logs_handler))
            .route("/documents/:id", get(get_document_handler))
            .route("/chat", post(chat_handler))
            .route("/workbench", get(workbench_handler))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_200() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/health").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_workspaces_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/workspaces").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let workspaces = json.get("workspaces").and_then(|v| v.as_array()).unwrap();
        assert!(!workspaces.is_empty(), "workspaces 應該包含至少一個預設工作空間");
        let ws = &workspaces[0];
        assert!(ws.get("createdAt").is_some(), "createdAt 應該是真實資料庫時間戳 (camelCase)");
        assert!(ws.get("created_at").is_none(), "不應該存在 snake_case 的 created_at");
        assert_eq!(ws.get("name").and_then(|v| v.as_str()).unwrap(), "homelab");
        assert_eq!(ws.get("isDefault").and_then(|v| v.as_bool()).unwrap(), true, "預設工作空間 isDefault 應該為 true");
    }

    #[tokio::test]
    async fn test_list_documents_200() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/documents").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert!(!docs.is_empty(), "documents 應該包含測試資料");
        assert_eq!(docs[0].get("title").and_then(|v| v.as_str()).unwrap(), "test.md");
        assert_eq!(docs[0].get("source_path").and_then(|v| v.as_str()).unwrap(), "docs/test.md");
    }

    #[tokio::test]
    async fn test_collections_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/collections").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let cols = json.get("collections").and_then(|v| v.as_array()).unwrap();
        assert!(!cols.is_empty(), "collections 應該包含測試資料");
    }

    #[tokio::test]
    async fn test_conversations_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/conversations").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let convs = json.get("conversations").and_then(|v| v.as_array()).unwrap();
        assert!(!convs.is_empty(), "conversations 應該包含測試資料");
    }

    #[tokio::test]
    async fn test_dictionary_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/dictionary").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let entries = json.get("entries").and_then(|v| v.as_array()).unwrap();
        assert!(!entries.is_empty(), "dictionary entries 應該不為空");
        let entry = &entries[0];
        assert_eq!(entry.get("key").and_then(|v| v.as_str()).unwrap(), "domain");
        assert_eq!(entry.get("value").and_then(|v| v.as_str()).unwrap(), "opendoc");
        assert!(entry.get("createdAt").is_some(), "應有駝峰式 createdAt 欄位");
        assert!(entry.get("workspaceId").is_some(), "應有駝峰式 workspaceId 欄位");
    }

    #[tokio::test]
    async fn test_add_dictionary_entry() {
        let state = build_test_state().await;
        let req_body = serde_json::json!({
            "key": "AI",
            "value": "人工智能"
        });
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/dictionary")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap()
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let entry: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(entry.get("key").and_then(|v| v.as_str()).unwrap(), "AI");
        assert_eq!(entry.get("value").and_then(|v| v.as_str()).unwrap(), "人工智能");
        assert!(entry.get("id").is_some(), "新增後應有 id 欄位");
    }

    #[tokio::test]
    async fn test_delete_dictionary_entry() {
        let state = build_test_state().await;
        // 先新增一個詞彙，在 db_pool 中，我們可以直接手動插入一個
        let db_pool = state.db_pool.clone();
        sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value) VALUES ('test-id-999', 'homelab', 'API', '應用程式介面')").execute(&db_pool).await.unwrap();

        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/dictionary/test-id-999")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("deleted").and_then(|v| v.as_bool()).unwrap(), true);
    }

    #[tokio::test]
    async fn test_import_dictionary_seed() {
        let state = build_test_state().await;
        let req_body = serde_json::json!({
            "language": "zh-TW"
        });
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/dictionary/import-seed")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap()
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("imported").and_then(|v| v.as_bool()).unwrap(), true);
    }

    #[tokio::test]
    async fn test_admin_stats_200() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/stats").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("documents").and_then(|v| v.as_i64()).unwrap(), 1);
        assert_eq!(json.get("workspaces").and_then(|v| v.as_i64()).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_admin_search_quality_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/search-quality").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("totalQueries").and_then(|v| v.as_i64()).unwrap(), 1);
        assert_eq!(json.get("feedback").and_then(|v| v.get("positive")).and_then(|v| v.as_i64()).unwrap(), 1);
    }

    #[tokio::test]
    async fn test_admin_benchmark_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/benchmark").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let runs = json.get("runs").and_then(|v| v.as_array()).unwrap();
        assert!(!runs.is_empty(), "benchmark runs 應該包含測試資料");
    }

    #[tokio::test]
    async fn test_admin_connectors_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/connectors").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let connectors = json.get("connectors").and_then(|v| v.as_array()).unwrap();
        assert!(!connectors.is_empty(), "connectors 應該包含測試資料");
    }

    #[tokio::test]
    async fn test_admin_query_logs_returns_real_data() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/query-logs").header("X-Workspace", "homelab").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let logs = json.get("logs").and_then(|v| v.as_array()).unwrap();
        assert!(!logs.is_empty(), "query logs 應該包含測試資料");
    }

    #[tokio::test]
    async fn test_workbench_handler_returns_full_structure() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/workbench")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        
        // 驗證是否有對齊前端 SettingsPage 所需的 connectors 欄位與子屬性
        let connectors = json.get("connectors").expect("必須包含 connectors 欄位");
         assert!(connectors.get("active").is_some(), "connectors 必須包含 active 屬性");
         assert!(connectors.get("total").is_some(), "connectors 必須包含 total 屬性");
     }
 
     #[tokio::test]
     async fn test_upload_response_matches_frontend_contract() {
         // 驗證 UploadResponse 序列化符合前端期望 { documentId, chunks, status }
         let resp = UploadResponse {
             document_id: "test-uuid".to_string(),
             chunks: 50,
             status: "indexed".to_string(),
         };
         let json = serde_json::to_value(&resp).unwrap();
 
         assert!(json.get("documentId").is_some(), "前端期望 documentId 欄位");
         assert!(json.get("chunks").is_some(), "前端期望 chunks 欄位");
         assert!(json.get("status").is_some(), "前端期望 status 欄位");
assert!(json.get("success").is_none(), "不應該有 success 欄位（前端不認識）");
          assert!(json.get("message").is_none(), "不應該有 message 欄位（前端不認識）");
      }

    #[tokio::test]
    async fn test_chat_handler_returns_query_result() {
        let state = build_test_state().await;

        let query = serde_json::json!({ "query": "測試問題" });
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/chat")
                    .method("POST")
                    .header("content-type", "application/json")
                    .header("x-workspace", "homelab")
                    .body(axum::body::Body::from(query.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&body).unwrap();

        // Frontend expects QueryResult shape
        assert!(json.get("queryId").is_some(), "前端期望 queryId 欄位");
        assert!(json.get("answer").is_some(), "前端期望 answer 欄位");
        assert!(json.get("sources").is_some(), "前端期望 sources 欄位");
        assert!(json.get("confidence").is_some(), "前端期望 confidence 欄位");
        assert!(json.get("route").is_some(), "前端期望 route 欄位");
        assert!(json.get("profile").is_some(), "前端期望 profile 欄位");
    }

     #[tokio::test]
     async fn test_get_document_by_id_returns_full_document() {
         let state = build_test_state().await;
         let res = build_router(state)
             .oneshot(
                 Request::builder()
                     .uri("/documents/doc1")
                     .header("X-Workspace", "homelab")
                     .body(axum::body::Body::empty())
                     .unwrap(),
             )
             .await
             .unwrap();
         assert_eq!(res.status(), StatusCode::OK);
         let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
         let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
         assert_eq!(json.get("id").and_then(|v| v.as_str()).unwrap(), "doc1");
         assert_eq!(json.get("title").and_then(|v| v.as_str()).unwrap(), "test.md");
         assert_eq!(json.get("source_path").and_then(|v| v.as_str()).unwrap(), "docs/test.md");
         assert_eq!(json.get("status").and_then(|v| v.as_str()).unwrap(), "indexed");
         assert_eq!(json.get("chunk_count").and_then(|v| v.as_i64()).unwrap(), 5);
     }

     #[tokio::test]
     async fn test_get_document_by_id_returns_404_for_missing() {
         let state = build_test_state().await;
         let res = build_router(state)
             .oneshot(
                 Request::builder()
                     .uri("/documents/nonexistent")
                     .header("X-Workspace", "homelab")
                     .body(axum::body::Body::empty())
                     .unwrap(),
             )
             .await
             .unwrap();
         assert_eq!(res.status(), StatusCode::NOT_FOUND);
     }
 }
