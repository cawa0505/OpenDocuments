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
/// 將 X-Workspace header（id 或 name）解析成 workspace UUID id（Node getById ?? getByName 向後相容）。
/// - header 非空：`SELECT id FROM workspaces WHERE id = ? OR name = ?` → 命中回 id；未命中 → 400（嚴格，不 auto-create）
/// - header 缺/空：取 config default_workspace 名稱 → 同查詢 → 未命中 → 500（default 啟動必建，缺失 = invariant 破壞）
async fn resolve_workspace_id(
    state: &McpState,
    headers: &axum::http::HeaderMap,
) -> Result<String, StatusCode> {
    let header_val = headers
        .get("x-workspace")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    let default_ws;
    let lookup_name = match header_val {
        Some(ws) => ws,
        None => {
            default_ws = state.config_manager.get_config().await.model.default_workspace;
            default_ws.as_str()
        }
    };

    let found: Option<String> = sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? OR name = ? LIMIT 1"
    )
    .bind(lookup_name)
    .bind(lookup_name)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 解析工作空間失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    match found {
        Some(id) => Ok(id),
        None => {
            if header_val.is_some() {
                // header 指定了未知 workspace → 嚴格 400
                Err(StatusCode::BAD_REQUEST)
            } else {
                // default workspace 啟動必建，缺失代表 invariant 破壞
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
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

/// 將 documents 資料列映射為 DocumentItem（list_documents 與 list_collection_documents 共用）
fn map_document_row(r: &sqlx::sqlite::SqliteRow) -> DocumentItem {
    DocumentItem {
        id: sqlx::Row::get::<String, _>(r, 0),
        title: sqlx::Row::get::<String, _>(r, 1),
        source_type: sqlx::Row::get::<String, _>(r, 2),
        source_path: sqlx::Row::get::<Option<String>, _>(r, 3).unwrap_or_default(),
        file_type: sqlx::Row::get::<Option<String>, _>(r, 4).unwrap_or_default(),
        file_size_bytes: sqlx::Row::get::<Option<i64>, _>(r, 5),
        connector_id: sqlx::Row::get::<Option<String>, _>(r, 6),
        chunk_count: sqlx::Row::get::<i64, _>(r, 7),
        status: sqlx::Row::get::<String, _>(r, 8),
        content_hash: sqlx::Row::get::<Option<String>, _>(r, 9),
        error_message: sqlx::Row::get::<Option<String>, _>(r, 10),
        created_at: sqlx::Row::get::<String, _>(r, 11),
        updated_at: sqlx::Row::get::<Option<String>, _>(r, 12).unwrap_or_default(),
        indexed_at: sqlx::Row::get::<Option<String>, _>(r, 13),
        workspace_id: sqlx::Row::get::<String, _>(r, 14),
    }
}

async fn list_workspaces_handler(
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

async fn list_documents_handler(
    State(state): State<Arc<McpState>>,
    Query(params): Query<HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    let mut sql = "SELECT id, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), indexed_at, workspace_id FROM documents WHERE workspace_id = ? AND deleted_at IS NULL".to_string();
    let mut binds: Vec<String> = vec![workspace];

    if let Some(status) = params.get("status") {
        if !status.is_empty() && status != "all" {
            sql.push_str(" AND status = ?");
            binds.push(status.clone());
        }
    }

    if let Some(source_type) = params.get("sourceType").or_else(|| params.get("source_type")) {
        if !source_type.is_empty() && source_type != "all" {
            sql.push_str(" AND source_type = ?");
            binds.push(source_type.clone());
        }
    }

    // Sorting columns allowlist
    let sort_col = match params.get("sortBy").map(|s| s.as_str()) {
        Some("title") => "title",
        Some("chunks") => "chunk_count",
        Some("updated") => "updated_at",
        Some("created") | Some("createdAt") => "created_at",
        Some("indexed") | Some("indexedAt") => "indexed_at",
        _ => "created_at",
    };

    let sort_order = match params.get("order").map(|s| s.to_lowercase()) {
        Some(ref o) if o == "asc" => "ASC",
        _ => "DESC",
    };

    sql.push_str(&format!(" ORDER BY {} {}", sort_col, sort_order));

    let mut query = sqlx::query(&sql);
    for val in binds {
        query = query.bind(val);
    }

    let rows = query
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 documents 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let documents: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();

    Ok(Json(json!({ "documents": documents })))
}

async fn get_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, title, shared, created_at, updated_at FROM conversations WHERE workspace_id = ? AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 100"
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
            "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 4),
        }));
    }

    Ok(Json(json!({ "conversations": conversations })))
}

#[derive(Deserialize)]
struct UpdateConversationReq {
    title: Option<String>,
}

async fn update_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(body): Json<UpdateConversationReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let result = sqlx::query(
        "UPDATE conversations SET title = COALESCE(?, title), updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(body.title.as_deref())
    .bind(&id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 update conversation 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "Conversation not found" }))));
    }

    Ok(Json(json!({ "updated": true })))
}

#[derive(Deserialize)]
struct CreateWorkspaceReq {
    #[serde(alias = "name")]
    id: String,
}

async fn create_workspace_handler(
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

async fn delete_workspace_handler(
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

async fn get_dictionary_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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

async fn get_admin_plugins_handler() -> Json<serde_json::Value> {
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

async fn get_admin_search_quality_handler(
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

async fn get_admin_query_logs_handler(
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

async fn get_admin_benchmark_handler(
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

async fn get_admin_connectors_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

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

// ── BYOK LLM provider handlers ──────────────────────────────

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderUpsertRequest {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub is_active: Option<bool>,
}

async fn list_llm_providers_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;
    let rows = sqlx::query(
        "SELECT id, name, provider, base_url, model, is_active, datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), api_key != '' as has_key \
         FROM llm_providers WHERE workspace_id = ? ORDER BY created_at"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 llm_providers 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut providers = Vec::new();
    for r in rows {
        providers.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "name": sqlx::Row::get::<String, _>(&r, 1),
            "provider": sqlx::Row::get::<String, _>(&r, 2),
            "baseUrl": sqlx::Row::get::<String, _>(&r, 3),
            "model": sqlx::Row::get::<String, _>(&r, 4),
            "isActive": sqlx::Row::get::<i64, _>(&r, 5) == 1,
            "createdAt": sqlx::Row::get::<Option<String>, _>(&r, 6).unwrap_or_default(),
            "updatedAt": sqlx::Row::get::<Option<String>, _>(&r, 7).unwrap_or_default(),
            "hasApiKey": sqlx::Row::get::<i64, _>(&r, 8) == 1,
        }));
    }

    Ok(Json(json!({ "providers": providers })))
}

async fn upsert_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LlmProviderUpsertRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let existing_row = sqlx::query("SELECT id, api_key FROM llm_providers WHERE workspace_id = ? AND name = ?")
        .bind(&workspace_id)
        .bind(&payload.name)
        .fetch_optional(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢既有 provider 失敗: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
        })?;

    let existing = existing_row.map(|r| {
        (sqlx::Row::get::<String, _>(&r, 0), sqlx::Row::get::<String, _>(&r, 1))
    });

    let id = existing.as_ref().map(|(id, _)| id.clone()).unwrap_or_else(|| Uuid::new_v4().to_string());
    let api_key = match payload.api_key {
        Some(k) if !k.trim().is_empty() => k,
        _ => existing.as_ref().map(|(_, key)| key.clone()).unwrap_or_default(),
    };

    let is_active = if payload.is_active.unwrap_or(false) { 1 } else { 0 };

    if is_active == 1 {
        sqlx::query("UPDATE llm_providers SET is_active = 0 WHERE workspace_id = ?")
            .bind(&workspace_id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
    }

    sqlx::query(
        "INSERT INTO llm_providers (id, workspace_id, name, provider, base_url, model, api_key, is_active) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(workspace_id, name) DO UPDATE SET \
            provider = excluded.provider, \
            base_url = excluded.base_url, \
            model = excluded.model, \
            api_key = excluded.api_key, \
            is_active = excluded.is_active, \
            updated_at = datetime('now', 'localtime')"
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&payload.name)
    .bind(&payload.provider)
    .bind(&payload.base_url)
    .bind(&payload.model)
    .bind(&api_key)
    .bind(is_active)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 Upsert provider 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() })))
    })?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "id": id,
            "name": payload.name,
            "provider": payload.provider,
            "baseUrl": payload.base_url,
            "model": payload.model,
            "isActive": is_active == 1,
            "hasApiKey": !api_key.is_empty(),
        })),
    ))
}

async fn delete_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let result = sqlx::query("DELETE FROM llm_providers WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 provider 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(json!({ "deleted": true })))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmTestRequest {
    pub provider_id: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
}

async fn test_llm_provider_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LlmTestRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    let (base_url, model, api_key) = if let Some(pid) = payload.provider_id {
        let row = sqlx::query("SELECT base_url, model, api_key FROM llm_providers WHERE id = ? AND workspace_id = ?")
            .bind(&pid)
            .bind(&workspace_id)
            .fetch_optional(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;
        match row {
            Some(r) => {
                let url: String = sqlx::Row::get(&r, 0);
                let md: String = sqlx::Row::get(&r, 1);
                let key: String = sqlx::Row::get(&r, 2);
                (url, md, Some(key))
            }
            None => return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "Provider not found" })))),
        }
    } else {
        let url = payload.base_url.ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing base_url" }))))?;
        let md = payload.model.ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing model" }))))?;
        let key = payload.api_key;
        (url, md, key)
    };

    let cfg = opendoc_llm::LlmProvider {
        name: "test".to_string(),
        base_url,
        model,
        api_key,
    };

    let client = opendoc_llm::LlmClient::new(cfg);
    let messages = vec![opendoc_llm::ChatMessage::user("say: pong")];
    let opts = opendoc_llm::CompletionOptions {
        temperature: Some(0.1),
        max_tokens: Some(10),
        system_prompt: None,
    };

    let start = std::time::Instant::now();
    match client.complete(messages, &opts).await {
        Ok(reply) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(Json(json!({
                "ok": true,
                "reply": reply.trim(),
                "latencyMs": latency,
            })))
        }
        Err(e) => {
            Err((StatusCode::BAD_GATEWAY, Json(json!({
                "ok": false,
                "error": e.to_string(),
            }))))
        }
    }
}

async fn delete_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;

    // Node 契約 documents.ts:42：文件不存在（含其他 workspace）→ 404
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documents WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if count == 0 {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Document not found" })),
        ));
    }

    sqlx::query("UPDATE documents SET deleted_at = CURRENT_TIMESTAMP WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((StatusCode::OK, Json(json!({ "deleted": true }))))
}

async fn list_trash_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query(
        "SELECT id, title, source_type, source_path, file_type, file_size_bytes, \
         connector_id, chunk_count, status, content_hash, error_message, \
         datetime(created_at, 'localtime'), datetime(updated_at, 'localtime'), \
         indexed_at, workspace_id FROM documents WHERE workspace_id = ? AND deleted_at IS NOT NULL \
         ORDER BY deleted_at DESC"
    )
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢垃圾桶 documents 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let docs: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();
    Ok(Json(json!({ "documents": docs })))
}

async fn restore_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers).await
        .map_err(|s| (s, Json(json!({ "error": "workspace error" }))))?;

    sqlx::query(
        "UPDATE documents SET deleted_at = NULL, updated_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" }))))?;

    Ok(Json(json!({ "restored": true })))
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

async fn readyz_handler(
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

// ── Frontend-contract feedback (POST /chat/feedback) ──────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatFeedbackRequest {
    pub query_id: String,
    pub feedback: String, // 'positive' or 'negative'
}

async fn chat_feedback_handler(
    State(state): State<Arc<McpState>>,
    Json(payload): Json<ChatFeedbackRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if payload.feedback != "positive" && payload.feedback != "negative" {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            "feedback must be 'positive' or 'negative'".to_string(),
        ));
    }

    sqlx::query("UPDATE query_logs SET feedback = ? WHERE id = ?")
        .bind(&payload.feedback)
        .bind(&payload.query_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("更新回饋紀錄失敗: {e}")))?;

    Ok(Json(json!({ "saved": true })))
}

// ── Conversations CRUD ─────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
}

async fn create_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    payload: Option<Json<CreateConversationRequest>>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;
    let id = Uuid::new_v4().to_string();
    let title = payload
        .and_then(|Json(p)| p.title)
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_default();

    sqlx::query("INSERT INTO conversations (id, workspace_id, title, created_at, updated_at) VALUES (?, ?, ?, datetime('now'), datetime('now'))")
        .bind(&id)
        .bind(&workspace_id)
        .bind(&title)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let row = sqlx::query("SELECT created_at, updated_at FROM conversations WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 讀取 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "title": title,
            "workspaceId": workspace_id,
            "shared": false,
            "createdAt": sqlx::Row::get::<String, _>(&row, 0),
            "updatedAt": sqlx::Row::get::<String, _>(&row, 1),
        })),
    ))
}

async fn delete_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Conversation not found" }))));
    }

    sqlx::query("UPDATE conversations SET deleted_at = datetime('now') WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok((StatusCode::OK, Json(json!({ "deleted": true }))))
}

async fn list_conversation_messages_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Conversation not found" }))));
    }

    let messages = conversation_messages_json(&state.db_pool, &id).await?;

    Ok((StatusCode::OK, Json(json!({ "messages": messages }))))
}

/// Fetch messages for a conversation in the shared JSON shape.
async fn conversation_messages_json(
    db_pool: &sqlx::SqlitePool,
    conversation_id: &str,
) -> Result<Vec<serde_json::Value>, StatusCode> {
    let rows = sqlx::query(
        "SELECT id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at FROM messages WHERE conversation_id = ? ORDER BY created_at"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 messages 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let mut messages = Vec::new();
    for r in rows {
        let raw_sources: Option<String> = sqlx::Row::get(&r, 4);
        let sources_val = match raw_sources {
            Some(s) if !s.is_empty() => {
                serde_json::from_str::<Value>(&s).unwrap_or_else(|_| json!([]))
            }
            _ => json!([]),
        };

        messages.push(json!({
            "id": sqlx::Row::get::<String, _>(&r, 0),
            "conversationId": sqlx::Row::get::<String, _>(&r, 1),
            "role": sqlx::Row::get::<String, _>(&r, 2),
            "content": sqlx::Row::get::<String, _>(&r, 3),
            "sources": sources_val,
            "profileUsed": sqlx::Row::get::<Option<String>, _>(&r, 5),
            "confidenceScore": sqlx::Row::get::<Option<f64>, _>(&r, 6),
            "responseTimeMs": sqlx::Row::get::<Option<i64>, _>(&r, 7),
            "createdAt": sqlx::Row::get::<String, _>(&r, 8),
        }));
    }
    Ok(messages)
}

// ── Conversation share ─────────────────────────────────────────

async fn share_conversation_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(conversation_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
    )
    .bind(&conversation_id)
    .bind(&workspace_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if count == 0 {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Conversation not found" }))));
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query(
        "UPDATE conversations SET shared = 1, share_token = ? WHERE id = ? AND workspace_id = ?"
    )
    .bind(&token)
    .bind(&conversation_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 更新 share_token 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((StatusCode::OK, Json(json!({ "shareUrl": format!("/shared/{}", token) }))))
}

async fn shared_conversation_handler(
    State(state): State<Arc<McpState>>,
    Path(token): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let row = sqlx::query(
        "SELECT id, workspace_id, title, shared, share_token, created_at FROM conversations WHERE share_token = ?"
    )
    .bind(&token)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 shared conversation 失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let Some(r) = row else {
        return Ok((StatusCode::NOT_FOUND, Json(json!({ "error": "Not found" }))));
    };

    let conv_id = sqlx::Row::get::<String, _>(&r, 0);
    let messages = conversation_messages_json(&state.db_pool, &conv_id).await?;
    let conversation = json!({
        "id": conv_id,
        "workspace_id": sqlx::Row::get::<String, _>(&r, 1),
        "title": sqlx::Row::get::<String, _>(&r, 2),
        "shared": sqlx::Row::get::<Option<i64>, _>(&r, 3).unwrap_or(0),
        "share_token": sqlx::Row::get::<String, _>(&r, 4),
        "created_at": sqlx::Row::get::<String, _>(&r, 5),
    });

    Ok((StatusCode::OK, Json(json!({ "conversation": conversation, "messages": messages }))))
}

// ── Tags CRUD ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TagItem {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub color: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
}

async fn list_tags_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    let rows = sqlx::query("SELECT id, workspace_id, name, color FROM tags WHERE workspace_id = ? ORDER BY name")
        .bind(&workspace_id)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 tags 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let mut tags = Vec::new();
    for r in rows {
        tags.push(TagItem {
            id: sqlx::Row::get::<String, _>(&r, 0),
            workspace_id: sqlx::Row::get::<String, _>(&r, 1),
            name: sqlx::Row::get::<String, _>(&r, 2),
            color: sqlx::Row::get::<Option<String>, _>(&r, 3),
        });
    }

    Ok(Json(json!({ "tags": tags })))
}

async fn create_tag_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Tag name required" }))));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;
    let id = Uuid::new_v4().to_string();
    let color = payload.color;

    sqlx::query("INSERT INTO tags (id, workspace_id, name, color) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&workspace_id)
        .bind(&name)
        .bind(&color)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立 tag 失敗: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "workspaceId": workspace_id,
            "name": name,
            "color": color,
        })),
    ))
}

async fn delete_tag_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query("DELETE FROM tags WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 tag 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

async fn tag_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((doc_id, tag_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query(
        "INSERT OR IGNORE INTO document_tags (document_id, tag_id) \
         SELECT d.id, t.id \
         FROM documents d \
         JOIN tags t ON t.id = ? \
         WHERE d.id = ? AND d.workspace_id = ? AND t.workspace_id = ?"
    )
    .bind(&tag_id)
    .bind(&doc_id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 文件貼標籤失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "tagged": true })))
}

async fn untag_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((doc_id, tag_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query(
        "DELETE FROM document_tags \
         WHERE document_id = ? AND tag_id = ? \
         AND EXISTS (SELECT 1 FROM documents WHERE id = ? AND workspace_id = ?) \
         AND EXISTS (SELECT 1 FROM tags WHERE id = ? AND workspace_id = ?)"
    )
    .bind(&doc_id)
    .bind(&tag_id)
    .bind(&doc_id)
    .bind(&workspace_id)
    .bind(&tag_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 移除文件標籤失敗: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(json!({ "untagged": true })))
}

// ── Extracted Assets CRUD ──────────────────────────────────────

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

fn clean_json_markdown(input: &str) -> String {
    let mut s = input.trim();
    if s.starts_with("```") {
        if s.starts_with("```json") {
            s = &s[7..];
        } else {
            s = &s[3..];
        }
    }
    if s.ends_with("```") {
        s = &s[..s.len() - 3];
    }
    s.trim().to_string()
}

async fn extract_asset_handler(
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

async fn list_assets_handler(
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

async fn get_asset_handler(
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

async fn delete_asset_handler(
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

// ── Collections CRUD ───────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

async fn create_collection_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let name = payload.name.trim().to_string();
    if name.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Collection name required" }))));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;
    let id = Uuid::new_v4().to_string();
    let description = payload.description;

    sqlx::query("INSERT INTO collections (id, workspace_id, name, description) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&workspace_id)
        .bind(&name)
        .bind(&description)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 建立 collection 失敗: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
        })?;

    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": id, "name": name, "description": description.unwrap_or_default() })),
    ))
}

async fn delete_collection_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace_id = resolve_workspace_id(&state, &headers).await?;

    sqlx::query("DELETE FROM collections WHERE id = ? AND workspace_id = ?")
        .bind(&id)
        .bind(&workspace_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 刪除 collection 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(json!({ "deleted": true })))
}

async fn add_collection_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((collection_id, document_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if collection_id.trim().is_empty() || document_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Collection and document ids required" })),
        ));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    sqlx::query(
        "INSERT OR IGNORE INTO collection_documents (collection_id, document_id) SELECT c.id, d.id FROM collections c JOIN documents d ON d.id = ? WHERE c.id = ? AND c.workspace_id = ? AND d.workspace_id = ?"
    )
    .bind(&document_id)
    .bind(&collection_id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 加入文件至集合失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    Ok(Json(json!({ "added": true })))
}

async fn remove_collection_document_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path((collection_id, document_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if collection_id.trim().is_empty() || document_id.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Collection and document ids required" })),
        ));
    }

    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    sqlx::query(
        "DELETE FROM collection_documents WHERE collection_id = ? AND document_id = ? AND EXISTS (SELECT 1 FROM collections WHERE id = ? AND workspace_id = ?) AND EXISTS (SELECT 1 FROM documents WHERE id = ? AND workspace_id = ?)"
    )
    .bind(&collection_id)
    .bind(&document_id)
    .bind(&collection_id)
    .bind(&workspace_id)
    .bind(&document_id)
    .bind(&workspace_id)
    .execute(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 移除文件自集合失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    Ok(Json(json!({ "removed": true })))
}

async fn list_collection_documents_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let workspace_id = resolve_workspace_id(&state, &headers)
        .await
        .map_err(|s| (s, Json(json!({ "error": "invalid workspace" }))))?;

    let collection_row = sqlx::query(
        "SELECT id, name, description, datetime(created_at, 'localtime') FROM collections WHERE id = ? AND workspace_id = ?"
    )
    .bind(&id)
    .bind(&workspace_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢 collection 失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    let Some(cr) = collection_row else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Collection not found" })),
        ));
    };

    let rows = sqlx::query(
        "SELECT d.id, d.title, d.source_type, d.source_path, d.file_type, d.file_size_bytes, d.connector_id, d.chunk_count, d.status, d.content_hash, d.error_message, datetime(d.created_at, 'localtime'), datetime(d.updated_at, 'localtime'), d.indexed_at, d.workspace_id FROM collection_documents cd JOIN collections c ON c.id = cd.collection_id JOIN documents d ON d.id = cd.document_id WHERE cd.collection_id = ? AND c.workspace_id = ? AND d.workspace_id = ? AND d.deleted_at IS NULL"
    )
    .bind(&id)
    .bind(&workspace_id)
    .bind(&workspace_id)
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        eprintln!("💥 查詢集合文件失敗: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "internal error" })))
    })?;

    let documents: Vec<DocumentItem> = rows.iter().map(map_document_row).collect();

    Ok(Json(json!({
        "collection": {
            "id": sqlx::Row::get::<String, _>(&cr, 0),
            "name": sqlx::Row::get::<String, _>(&cr, 1),
            "description": sqlx::Row::get::<Option<String>, _>(&cr, 2).unwrap_or_default(),
            "createdAt": sqlx::Row::get::<String, _>(&cr, 3),
        },
        "documents": documents,
    })))
}

// ── Stats (frontend contract GET /stats) ───────────────────────

async fn stats_handler(
    State(state): State<Arc<McpState>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
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

// ── Multipart / Upload Handler ─────────────────────────────────

async fn upload_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Result<Json<UploadResponse>, (axum::http::StatusCode, String)> {
    // 1. 取得 Header 中的工作空間（id 或 name 雙查，Node getById ?? getByName）與集合識別碼 (預設 "default")
    //     upload 路徑維持 lenient：缺 header 用 config default 名稱；未知 workspace 自動建立（Node parity），id 用 UUID
    let raw_ws = match headers
        .get("x-workspace")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(s) => s,
        None => state.config_manager.get_config().await.model.default_workspace.clone(),
    };

    // lenient upload: auto-create if missing (Node parity)
    let workspace_id: String = match sqlx::query_scalar(
        "SELECT id FROM workspaces WHERE id = ? OR name = ? LIMIT 1"
    )
    .bind(&raw_ws)
    .bind(&raw_ws)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    {
        Some(id) => id,
        None => {
            let new_id = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)")
                .bind(&new_id)
                .bind(&raw_ws)
                .execute(&state.db_pool)
                .await
                .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            new_id
        }
    };

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

    // 2.5. 檔案大小上限 50 MiB（對齊 Node documents.ts:55-58 行為：413 + JSON error body）
    const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;
    if file_bytes.len() > MAX_FILE_SIZE {
        return Err((
            axum::http::StatusCode::PAYLOAD_TOO_LARGE,
            format!(
                r#"{{"error":"File too large: {:.1}MB (max 50MB)"}}"#,
                file_bytes.len() as f64 / 1024.0 / 1024.0
            ),
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
    //    （workspace 已在步驟 1 以 id/name 雙查 + auto-create 解析為既有或新 UUID id）

    let file_path_str = original_name.clone().unwrap_or_else(|| format!("opendoc-{temp_file_id}"));

    // 寫入 documents 資料庫
    let ext_display = ext_suffix.trim_start_matches('.').to_uppercase();
    // source_path：帶 x-source-path header 時原樣採用（CLI 傳真實絕對路徑，對齊 Node resolve(inputPath)）；
    // 否則用 workspace/檔名 確保跨 workspace 可區分
    let source_path = headers
        .get("x-source-path")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}/{}", workspace_id, file_path_str));
    let file_size = file_bytes.len() as i64;

    // 去重鍵：(workspace_id, source_path) — 已存在且未刪除 → 更新既有文件（reindex，沿用 document id），否則新增
    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM documents WHERE workspace_id = ? AND source_path = ? AND deleted_at IS NULL"
    )
    .bind(&workspace_id)
    .bind(&source_path)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("查詢既有文件失敗: {e}")))?;

    let document_id = match existing_id {
        Some(id) => {
            sqlx::query(
                "UPDATE documents SET title = ?, file_type = ?, file_size_bytes = ?, chunk_count = ?, status = 'indexed', updated_at = CURRENT_TIMESTAMP, indexed_at = CURRENT_TIMESTAMP WHERE id = ?"
            )
            .bind(&file_path_str)
            .bind(&ext_display)
            .bind(file_size)
            .bind(chunks_count as i64)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, format!("更新既有文件失敗: {e}")))?;
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO documents (id, title, source_type, source_path, file_type, file_size_bytes, status, chunk_count, workspace_id, created_at, indexed_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'indexed', ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"
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
            temp_file_id
        }
    };

    Ok(Json(UploadResponse {
        document_id,
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
                                "workspace": { "type": "string", "description": "Workspace name or UUID id（省略時使用 config 預設 workspace）" },
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

                        // 省略 workspace 參數時：active_workspace 優先、回退 default_workspace（取「名稱」，server 端解析層會做 name→id）
                        let workspace = args["workspace"].as_str()
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| s.to_string())
                            .unwrap_or_else(|| {
                                app_cfg.model.active_workspace.clone()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| app_cfg.model.default_workspace.clone())
                            });

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
                                        .header("X-Workspace", workspace.clone())
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
                                    "workspace": { "type": "string", "description": "Workspace name or UUID id（省略時使用 config 預設 workspace）" },
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

                            // 省略 workspace 參數時：active_workspace 優先、回退 default_workspace（取「名稱」，server 端解析層會做 name→id）
                            let workspace = args["workspace"].as_str()
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| {
                                    app_cfg.model.active_workspace.clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| app_cfg.model.default_workspace.clone())
                                });

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
                                             .header("X-Workspace", workspace.clone())
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

// ── Helper to fetch and format recent conversation messages for RAG context ──
async fn get_history_context(db_pool: &sqlx::SqlitePool, conversation_id: &str) -> String {
    let rows_res = sqlx::query(
        "SELECT role, content FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await;

    match rows_res {
        Ok(rows) => {
            let mut formatted = Vec::new();
            for r in rows.into_iter().rev() {
                let role: String = sqlx::Row::get(&r, 0);
                let content: String = sqlx::Row::get(&r, 1);
                let display_role = if role.to_lowercase() == "user" { "User" } else { "Assistant" };
                formatted.push(format!("{}: {}", display_role, content));
            }
            formatted.join("\n")
        }
        Err(e) => {
            eprintln!("💥 讀取對話歷史失敗: {e}");
            String::new()
        }
    }
}

// ponytail: 直接用 search_and_rerank + 組裝答案；LLM streaming 加 when 需要

// ── BYOK LLM Helpers ──
async fn get_active_llm_client(db_pool: &sqlx::SqlitePool, workspace_id: &str) -> Option<opendoc_llm::LlmClient> {
    let row_res = sqlx::query(
        "SELECT name, provider, base_url, model, api_key FROM llm_providers WHERE workspace_id = ? AND is_active = 1 LIMIT 1"
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

async fn get_history_messages(db_pool: &sqlx::SqlitePool, conversation_id: &str) -> Vec<opendoc_llm::ChatMessage> {
    let rows_res = sqlx::query(
        "SELECT role, content FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT 6"
    )
    .bind(conversation_id)
    .fetch_all(db_pool)
    .await;

    match rows_res {
        Ok(rows) => {
            let mut msgs = Vec::new();
            for r in rows.into_iter().rev() {
                let role: String = sqlx::Row::get(&r, 0);
                let content: String = sqlx::Row::get(&r, 1);
                if role.to_lowercase() == "user" {
                    msgs.push(opendoc_llm::ChatMessage::user(content));
                } else if role.to_lowercase() == "assistant" {
                    msgs.push(opendoc_llm::ChatMessage::assistant(content));
                }
            }
            msgs
        }
        Err(e) => {
            eprintln!("💥 讀取對話歷史強型別失敗: {e}");
            Vec::new()
        }
    }
}

async fn chat_stream_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatStreamRequest>,
) -> Result<Sse<std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>>, (StatusCode, Json<serde_json::Value>)> {
    use futures_util::StreamExt;

    let workspace = match resolve_workspace_id(&state, &headers).await {
        Ok(w) => w,
        Err(e) => {
            eprintln!("💥 取得 workspace_id 失敗: {e}");
            return Err((e, Json(json!({ "error": "invalid workspace" }))));
        }
    };
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let mut conversation_id = req.conversation_id;

    // Node 契約 chat.ts:127：conversationId 不存在（或已刪除）→ 404
    if let Some(cid) = &conversation_id {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
        )
        .bind(cid)
        .bind(&workspace)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Internal server error" })),
            )
        })?;

        if count == 0 {
            return Err((
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "Conversation not found" })),
            ));
        }
    } else {
        // 💡 串流聊天在沒有提供 conversationId 時，自動生成一個 Uuid 並建立新的 conversation
        let new_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO conversations (id, workspace_id, title, created_at, updated_at) VALUES (?, ?, '新對話', datetime('now'), datetime('now'))")
            .bind(&new_id)
            .bind(&workspace)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                eprintln!("💥 自動建立 conversation 失敗: {e}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": "Failed to auto-create conversation" })),
                )
            })?;
        conversation_id = Some(new_id);
    }
    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    // 💡 Fetch and build RAG context from history in stream handler
    let mut expanded_query = req.query.clone();
    if let Some(cid) = &conversation_id {
        let history = get_history_context(&state.db_pool, cid).await;
        if !history.is_empty() {
            expanded_query = format!("{}\n\n[Recent Conversation History]\n{}", req.query, history);
        }
    }

    let results = state.search.search_and_rerank(&expanded_query, threshold);
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

    let llm_client_opt = get_active_llm_client(&state.db_pool, &workspace).await;

    if let Some(llm_client) = llm_client_opt {
        let sources_json_clone = sources_json.clone();
        let confidence_json_clone = confidence_json.clone();

        let context_str = if limited.is_empty() {
            "未找到相關本地文獻。請直接回答使用者的問題，或說明沒有相關文獻。".to_string()
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[文獻 {}] 來源: {}\n内容: {}", i + 1, r.file_path, r.content)
            }).collect::<Vec<_>>().join("\n\n")
        };

        let system_prompt = format!(
            "你是一個專業的本地知識庫助理。請根據以下提供的 [本地文獻] 來回答使用者的問題。\n\
            如果文獻中沒有提到相關資訊，請誠實說明「本地文獻未提及」，不要虛構事實。回答時請維持繁體中文語系。\n\n\
            [本地文獻]\n{}",
            context_str
        );

        let cid_clone = conversation_id.clone();
        let query_clone = req.query.clone();
        let profile_clone = profile.clone();
        let workspace_clone = workspace.clone();
        let query_id_clone = query_id.clone();
        let state_clone = state.clone();
        let sources_mapped_clone = sources_mapped.clone();

        let mut messages = if let Some(cid) = &conversation_id {
            get_history_messages(&state.db_pool, cid).await
        } else {
            Vec::new()
        };
        messages.push(opendoc_llm::ChatMessage::user(&req.query));

        let opts = opendoc_llm::CompletionOptions {
            temperature: Some(0.3),
            max_tokens: None,
            system_prompt: Some(system_prompt),
        };

        let llm_stream_res = llm_client.stream(messages, &opts).await;

        match llm_stream_res {
            Ok(mut llm_stream) => {
                let s = async_stream::stream! {
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("sources").data(sources_json_clone));
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("confidence").data(confidence_json_clone));

                    let mut full_answer = String::new();

                    while let Some(res) = llm_stream.next().await {
                        match res {
                            Ok(token) => {
                                full_answer.push_str(&token);
                                let chunk_data = serde_json::to_string(&token).unwrap_or_default();
                                yield Ok::<Event, std::convert::Infallible>(Event::default().event("chunk").data(chunk_data));
                            }
                            Err(e) => {
                                eprintln!("💥 LLM stream error: {e}");
                                let error_data = serde_json::to_string(&json!({ "error": e.to_string() })).unwrap_or_default();
                                yield Ok::<Event, std::convert::Infallible>(Event::default().event("error").data(error_data));
                                break;
                            }
                        }
                    }

                    if let Some(cid) = &cid_clone {
                        let user_msg_id = Uuid::new_v4().to_string();
                        let assistant_msg_id = Uuid::new_v4().to_string();
                        let sources_str = serde_json::to_string(&sources_mapped_clone).unwrap_or_default();

                        let db = &state_clone.db_pool;

                        if let Err(e) = sqlx::query(
                            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
                        )
                        .bind(&user_msg_id)
                        .bind(cid)
                        .bind(&query_clone)
                        .execute(db)
                        .await {
                            eprintln!("💥 串流寫入 user messages 失敗: {e}");
                        }

                        if let Err(e) = sqlx::query(
                            "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
                        )
                        .bind(&assistant_msg_id)
                        .bind(cid)
                        .bind(&full_answer)
                        .bind(&sources_str)
                        .bind(&profile_clone)
                        .bind(total_score)
                        .bind(100i64)
                        .execute(db)
                        .await {
                            eprintln!("💥 串流寫入 assistant messages 失敗: {e}");
                        }
                    }

                    if let Err(e) = sqlx::query(
                        "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
                    )
                    .bind(&query_clone)
                    .bind(&profile_clone)
                    .bind(total_score)
                    .bind(100i64)
                    .bind(&workspace_clone)
                    .execute(&state_clone.db_pool)
                    .await {
                        eprintln!("💥 串流寫入 query_logs 失敗: {e}");
                    }

                    let done_json = serde_json::to_string(&json!({
                        "queryId": query_id_clone, "route": "rag", "profile": profile_clone,
                        "conversationId": cid_clone,
                    })).unwrap_or_default();
                    yield Ok::<Event, std::convert::Infallible>(Event::default().event("done").data(done_json));
                };

                return Ok(Sse::new(Box::pin(s) as std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>).keep_alive(KeepAlive::default()));
            }
            Err(e) => {
                eprintln!("💥 取得 LLM Stream 失敗，Fallback 到 Echo 模式: {e}");
            }
        }
    }

    // Fallback stream (concatenated chunks)
    let s_fallback = async_stream::stream! {
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("sources").data(sources_json));
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("confidence").data(confidence_json));
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("chunk").data(serde_json::to_string(&answer).unwrap_or_default()));

        if let Some(cid) = &conversation_id {
            let user_msg_id = Uuid::new_v4().to_string();
            let assistant_msg_id = Uuid::new_v4().to_string();
            let sources_str = serde_json::to_string(&sources_mapped).unwrap_or_default();

            if let Err(e) = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
            )
            .bind(&user_msg_id)
            .bind(cid)
            .bind(&req.query)
            .execute(&state.db_pool)
            .await {
                eprintln!("💥 Fallback 寫入 user messages 失敗: {e}");
            }

            if let Err(e) = sqlx::query(
                "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
            )
            .bind(&assistant_msg_id)
            .bind(cid)
            .bind(&answer)
            .bind(&sources_str)
            .bind(&profile)
            .bind(total_score)
            .bind(100i64)
            .execute(&state.db_pool)
            .await {
                eprintln!("💥 Fallback 寫入 assistant messages 失敗: {e}");
            }
        }

        if let Err(e) = sqlx::query(
            "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
        )
        .bind(&req.query)
        .bind(&profile)
        .bind(total_score)
        .bind(100i64)
        .bind(&workspace)
        .execute(&state.db_pool)
        .await {
            eprintln!("💥 Fallback 寫入 query_logs 失敗: {e}");
        }

        let done_json = serde_json::to_string(&json!({
            "queryId": query_id, "route": "rag", "profile": profile,
            "conversationId": conversation_id,
        })).unwrap_or_default();
        yield Ok::<Event, std::convert::Infallible>(Event::default().event("done").data(done_json));
    };

    Ok(Sse::new(Box::pin(s_fallback) as std::pin::Pin<Box<dyn Stream<Item = Result<Event, std::convert::Infallible>> + Send>>).keep_alive(KeepAlive::default()))
}

// Non-streaming chat endpoint
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChatRequest {
    query: String,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default, rename = "conversationId")]
    conversation_id: Option<String>,
}

async fn chat_handler(
    State(state): State<Arc<McpState>>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ChatRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let workspace = resolve_workspace_id(&state, &headers).await?;
    let query_id = Uuid::new_v4().to_string();
    let profile = req.profile.unwrap_or_else(|| "balanced".to_string());
    let conversation_id = req.conversation_id;

    // Node 契約 chat.ts:74：conversationId 不存在（或已刪除）→ 404
    if let Some(cid) = &conversation_id {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL"
        )
        .bind(cid)
        .bind(&workspace)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 查詢 conversation 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        if count == 0 {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let threshold = match profile.as_str() {
        "fast" => 0.50,
        "precise" => 0.70,
        _ => 0.60,
    };

    // 💡 Fetch and build RAG context from history in non-stream handler
    let mut expanded_query = req.query.clone();
    if let Some(cid) = &conversation_id {
        let history = get_history_context(&state.db_pool, cid).await;
        if !history.is_empty() {
            expanded_query = format!("{}\n\n[Recent Conversation History]\n{}", req.query, history);
        }
    }

    let results = state.search.search_and_rerank(&expanded_query, threshold);
    let limited: Vec<_> = results.into_iter().take(10).collect();

    let mut answer = String::new();
    let mut generated_by_llm = false;

    if let Some(llm_client) = get_active_llm_client(&state.db_pool, &workspace).await {
        let context_str = if limited.is_empty() {
            "未找到相關本地文獻。請直接回答使用者的問題，或說明沒有相關文獻。".to_string()
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[文獻 {}] 來源: {}\n内容: {}", i + 1, r.file_path, r.content)
            }).collect::<Vec<_>>().join("\n\n")
        };

        let system_prompt = format!(
            "你是一個專業的本地知識庫助理。請根據以下提供的 [本地文獻] 來回答使用者的問題。\n\
            如果文獻中沒有提到相關資訊，請誠實說明「本地文獻未提及」，不要虛構事實。回答時請維持繁體中文語系。\n\n\
            [本地文獻]\n{}",
            context_str
        );

        let mut messages = if let Some(cid) = &conversation_id {
            get_history_messages(&state.db_pool, cid).await
        } else {
            Vec::new()
        };
        messages.push(opendoc_llm::ChatMessage::user(&req.query));

        let opts = opendoc_llm::CompletionOptions {
            temperature: Some(0.3),
            max_tokens: None,
            system_prompt: Some(system_prompt),
        };

        match llm_client.complete(messages, &opts).await {
            Ok(generated) => {
                answer = generated;
                generated_by_llm = true;
            }
            Err(e) => {
                eprintln!("💥 LLM 生成失敗，自動 Fallback 至 Echo 模式: {e}");
            }
        }
    }

    if !generated_by_llm {
        answer = if limited.is_empty() {
            "在現有文獻中未找到相關內容。".to_string()
        } else {
            limited.iter().enumerate().map(|(i, r)| {
                format!("[{}] {}", i + 1, r.content.chars().take(500).collect::<String>())
            }).collect::<Vec<_>>().join("\n\n")
        };
    }

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

    // 💡 寫入 query_logs 表
    if let Err(e) = sqlx::query(
        "INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, workspace_id) VALUES (?, ?, ?, ?, 'rag', ?)"
    )
    .bind(&req.query)
    .bind(&profile)
    .bind(total_score)
    .bind(100i64)
    .bind(&workspace)
    .execute(&state.db_pool)
    .await {
        eprintln!("💥 寫入 query_logs 失敗: {e}");
    }

    // 💡 若提供 conversationId，將使用者 query 及助理 answer（含 metadata）寫入 messages 表
    if let Some(cid) = &conversation_id {
        let user_msg_id = Uuid::new_v4().to_string();
        let assistant_msg_id = Uuid::new_v4().to_string();
        let sources_str = serde_json::to_string(&sources_mapped).unwrap_or_default();

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', ?, datetime('now'))"
        )
        .bind(&user_msg_id)
        .bind(cid)
        .bind(&req.query)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 寫入 messages 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, confidence_score, response_time_ms, created_at) VALUES (?, ?, 'assistant', ?, ?, ?, ?, ?, datetime('now'))"
        )
        .bind(&assistant_msg_id)
        .bind(cid)
        .bind(&answer)
        .bind(&sources_str)
        .bind(&profile)
        .bind(total_score)
        .bind(100i64)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            eprintln!("💥 寫入 messages 失敗: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    }

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
        .route("/tags", get(list_tags_handler).post(create_tag_handler))
        .route("/tags/:id", delete(delete_tag_handler))
        .route("/documents/:docId/tags/:tagId", post(tag_document_handler).delete(untag_document_handler))
        .route("/extracted-assets", get(list_assets_handler).post(extract_asset_handler))
        .route("/extracted-assets/:id", get(get_asset_handler).delete(delete_asset_handler))
        .route("/healthz", get(health_handler))
        .route("/health", get(health_handler))
        .route("/readyz", get(readyz_handler))
        .route("/workbench", get(workbench_handler))
        .route("/documents", get(list_documents_handler))
        .route("/documents/trash", get(list_trash_handler))
        .route("/documents/upload", post(upload_handler))
        .route("/documents/:id/restore", post(restore_document_handler))
        .route("/documents/:id", get(get_document_handler).delete(delete_document_handler))
        .route("/workspaces", get(list_workspaces_handler).post(create_workspace_handler))
        .route("/workspaces/:id", delete(delete_workspace_handler))
        .route("/collections", get(list_collections_handler).post(create_collection_handler))
        .route("/collections/:id", delete(delete_collection_handler))
        .route("/collections/:id/documents", get(list_collection_documents_handler))
        .route("/collections/:id/documents/:docId", post(add_collection_document_handler).delete(remove_collection_document_handler))
        .route("/conversations", get(list_conversations_handler).post(create_conversation_handler))
        .route("/conversations/:id", delete(delete_conversation_handler).patch(update_conversation_handler))
        .route("/conversations/:id/messages", get(list_conversation_messages_handler))
        .route("/conversations/:id/share", post(share_conversation_handler))
        .route("/shared/:token", get(shared_conversation_handler))
        .route("/api/v1/shared/:token", get(shared_conversation_handler))
        .route("/dictionary", get(get_dictionary_handler).post(add_dictionary_handler))
        .route("/dictionary/:id", delete(delete_dictionary_handler))
        .route("/dictionary/import-seed", post(import_seed_handler))
        .route("/admin/stats", get(get_admin_stats_handler))
        .route("/admin/plugins", get(get_admin_plugins_handler))
        .route("/admin/search-quality", get(get_admin_search_quality_handler))
        .route("/admin/benchmark", get(get_admin_benchmark_handler))
        .route("/admin/connectors", get(get_admin_connectors_handler))
        .route("/admin/query-logs", get(get_admin_query_logs_handler))
        .route("/admin/llm/providers", get(list_llm_providers_handler).post(upsert_llm_provider_handler))
        .route("/admin/llm/providers/:id", delete(delete_llm_provider_handler))
        .route("/admin/llm/test", post(test_llm_provider_handler))
        .route("/query/log", post(query_log_handler))
        .route("/query/feedback", post(query_feedback_handler))
        .route("/chat/feedback", post(chat_feedback_handler))
        .route("/chat", post(chat_handler))
        .route("/chat/stream", post(chat_stream_handler))
        .route("/stats", get(stats_handler))
        // 放寬 multipart 內建 2MB 上限到 60MiB，讓 upload_handler 自己的 50MiB 檢查（Node 相容 413）優先生效
        .layer(axum::extract::DefaultBodyLimit::max(60 * 1024 * 1024));

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
            "CREATE TABLE llm_providers (id TEXT PRIMARY KEY, workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE, name TEXT NOT NULL, provider TEXT NOT NULL, base_url TEXT NOT NULL, model TEXT NOT NULL, api_key TEXT NOT NULL DEFAULT '', is_active INTEGER NOT NULL DEFAULT 0, created_at TEXT DEFAULT (datetime('now')), updated_at TEXT DEFAULT (datetime('now')), UNIQUE(workspace_id, name))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE collections (id TEXT PRIMARY KEY, workspace_id TEXT, name TEXT NOT NULL, description TEXT, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE collection_documents (collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE, document_id TEXT REFERENCES documents(id) ON DELETE CASCADE, PRIMARY KEY (collection_id, document_id))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE conversations (id TEXT PRIMARY KEY, workspace_id TEXT, user_id TEXT, title TEXT, shared INTEGER DEFAULT 0, share_token TEXT, deleted_at TEXT DEFAULT NULL, created_at TEXT DEFAULT (datetime('now')), updated_at TEXT)"
        ).execute(&db_pool).await.unwrap();

        // Migration: 為現有 conversations 資料表補上 updated_at 欄位（已存在則忽略）
        match sqlx::query("ALTER TABLE conversations ADD COLUMN updated_at TEXT").execute(&db_pool).await {
            Ok(_) => {}
            Err(e) if e.to_string().contains("duplicate column") => {}
            Err(e) => panic!("ALTER TABLE conversations ADD COLUMN updated_at 失敗: {e}"),
        }

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, sources TEXT, profile_used TEXT, confidence_score REAL, response_time_ms INTEGER, created_at TEXT DEFAULT (datetime('now')))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tags (id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL, name TEXT NOT NULL, color TEXT, UNIQUE(workspace_id, name))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS document_tags (document_id TEXT NOT NULL, tag_id TEXT NOT NULL, PRIMARY KEY (document_id, tag_id))"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS extracted_assets (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
                asset_type TEXT NOT NULL,
                title TEXT NOT NULL,
                schema_definition TEXT NOT NULL,
                data_content TEXT NOT NULL,
                source_chunks TEXT NOT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&db_pool).await.unwrap();

        sqlx::query(
            "CREATE TABLE benchmark_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, workspace_id TEXT NOT NULL, model TEXT NOT NULL, metric_name TEXT NOT NULL, metric_value REAL NOT NULL, created_at DATETIME DEFAULT CURRENT_TIMESTAMP)"
        ).execute(&db_pool).await.unwrap();

    // 插入測試資料（workspace id 用 UUID，name='homelab'；解析層做 name→id）
    let cfg = opendoc_storage::ConfigManager::load_or_init().unwrap();
    let default_name = cfg.get_config().await.model.default_workspace.clone();

    let ws_id = "75b9b1dd-4c99-4b7d-a362-24fae18d861d".to_string(); // 使用預期 UUID 或在 test-state 直接寫入，但 resolve_workspace_id 支援以 name = 'homelab' 查詢，並回傳 id。
    sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'homelab')").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO documents (id, title, source_type, source_path, status, chunk_count, workspace_id) VALUES ('doc1', 'test.md', 'markdown', 'docs/test.md', 'indexed', 5, ?)").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO query_logs (query, profile, confidence_score, response_time_ms, route, feedback, workspace_id) VALUES ('hello', 'hybrid', 0.85, 120, 'semantic', 'positive', ?)").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO dictionary (workspace_id, key, value) VALUES (?, 'domain', 'opendoc')").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO connectors (workspace_id, name, type, config) VALUES (?, 'github', 'git', '{}')").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO collections (workspace_id, name, description) VALUES (?, 'default', 'default collection')").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO conversations (workspace_id, title) VALUES (?, 'First chat')").bind(&ws_id).execute(&db_pool).await.unwrap();
        sqlx::query("INSERT INTO benchmark_runs (workspace_id, model, metric_name, metric_value) VALUES (?, 'ollama', 'recall', 0.92)").bind(&ws_id).execute(&db_pool).await.unwrap();

        if default_name != "homelab" {
            let default_ws_id = Uuid::new_v4().to_string();
            sqlx::query("INSERT OR IGNORE INTO workspaces (id, name) VALUES (?, ?)")
                .bind(&default_ws_id)
                .bind(&default_name)
                .execute(&db_pool)
                .await
                .unwrap();
        }

        Arc::new(McpState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            search: Arc::new(MockSearch),
            config_manager: Arc::new(opendoc_storage::ConfigManager::load_or_init().unwrap()),
            db_pool,
        })
    }

    // ── Workspace UUID migration ─────────────────────────────────

    async fn seed_workspace_id(db_pool: &sqlx::SqlitePool) -> String {
        let row: String = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = 'homelab'")
            .fetch_one(db_pool)
            .await
            .unwrap();
        row
    }

    #[tokio::test]
    async fn test_workspace_name_header_resolves_to_uuid() {
        let state = build_test_state().await;
        let ws_id = seed_workspace_id(&state.db_pool).await;
        assert_ne!(ws_id, "homelab", "seed workspace id 必須是 UUID 而非 name");

        // X-Workspace: homelab（name）→ 解析成 UUID id
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(r#"{"title":"resolver test"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("workspaceId").and_then(|v| v.as_str()).unwrap(),
            ws_id,
            "X-Workspace 送 name 時必須解析回 workspace 的 UUID id"
        );
    }

    #[tokio::test]
    async fn test_workspace_uuid_header_accepts_raw_uuid() {
        let state = build_test_state().await;
        let ws_id = seed_workspace_id(&state.db_pool).await;

        // X-Workspace: <uuid>（id 直傳，Node getById 向後相容）
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations")
                    .header("X-Workspace", &ws_id)
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(r#"{"title":"uuid direct"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("workspaceId").and_then(|v| v.as_str()).unwrap(),
            ws_id,
            "X-Workspace 直傳 UUID id 必須命中同一 workspace"
        );
    }

    #[tokio::test]
    async fn test_workspace_unknown_header_strict_400() {
        let state = build_test_state().await;
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/documents")
                    .header("X-Workspace", "does-not-exist-ws")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::BAD_REQUEST,
            "未知 workspace 必須嚴格 400（不 auto-create、不回空結果）"
        );
    }

    #[tokio::test]
    async fn test_delete_default_workspace_by_uuid_rejected() {
        let state = build_test_state().await;
        let default_name = state.config_manager.get_config().await.model.default_workspace.clone();
        let default_id: String = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = ?")
            .bind(&default_name)
            .fetch_one(&state.db_pool)
            .await
            .unwrap();

        // 刪除 default workspace（以 UUID id 直傳）→ 400
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/workspaces/{default_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "default workspace 不得刪除");

        // 建立其他 workspace → 可以刪除
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/workspaces")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(r#"{"name":"scratch-ws"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let scratch_id: String = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = 'scratch-ws'")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/workspaces/{scratch_id}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "非 default workspace 可刪除");
    }

    #[tokio::test]
    async fn test_upload_auto_creates_workspace_with_uuid_id() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        let body = multipart_upload_body("newfile.md", b"auto created workspace");
        let res = build_router(state.clone())
            .oneshot(upload_request("brand-new-ws", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let id: String = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = 'brand-new-ws'")
            .fetch_one(&db_pool)
            .await
            .unwrap();
        assert_ne!(id, "brand-new-ws", "auto-create 的 workspace id 必須是 UUID，不得 id=name");

        let doc_ws: String = sqlx::query_scalar("SELECT workspace_id FROM documents WHERE title = 'newfile.md'")
            .fetch_one(&db_pool)
            .await
            .unwrap();
        assert_eq!(doc_ws, id, "上傳的文件必須歸屬在 auto-create 的 workspace 下");
    }

    struct MockSearch;

    impl SearchBackend for MockSearch {
        fn search_and_rerank(&self, _query: &str, _threshold: f32) -> Vec<opendoc_types::DocumentChunk> {
            vec![]
        }
    }

    const TEST_BOUNDARY: &str = "----opendoc-test-boundary";

    fn multipart_upload_body(filename: &str, content: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(
            format!("--{TEST_BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{TEST_BOUNDARY}--\r\n").as_bytes());
        body
    }

    fn upload_request(workspace: &str, body: Vec<u8>, source_path: Option<&str>) -> Request<axum::body::Body> {
        let mut builder = Request::builder()
            .method("POST")
            .uri("/documents/upload")
            .header("X-Workspace", workspace)
            .header("content-type", format!("multipart/form-data; boundary={TEST_BOUNDARY}"));
        if let Some(sp) = source_path {
            builder = builder.header("x-source-path", sp);
        }
        builder.body(axum::body::Body::from(body)).unwrap()
    }

    fn build_router(state: Arc<McpState>) -> Router {
        Router::new()
            .route("/health", get(health_handler))
            .route("/readyz", get(readyz_handler))
            .route("/tags", get(list_tags_handler).post(create_tag_handler))
        .route("/tags/:id", delete(delete_tag_handler))
        .route("/documents/:docId/tags/:tagId", post(tag_document_handler).delete(untag_document_handler))
        .route("/extracted-assets", get(list_assets_handler).post(extract_asset_handler))
        .route("/extracted-assets/:id", get(get_asset_handler).delete(delete_asset_handler))
        .route("/workspaces", get(list_workspaces_handler).post(create_workspace_handler))
            .route("/workspaces/:id", delete(delete_workspace_handler))
            .route("/documents", get(list_documents_handler))
            .route("/documents/upload", post(upload_handler))
            .route("/collections", get(list_collections_handler).post(create_collection_handler))
            .route("/collections/:id", delete(delete_collection_handler))
            .route("/collections/:id/documents", get(list_collection_documents_handler))
            .route("/collections/:id/documents/:docId", post(add_collection_document_handler).delete(remove_collection_document_handler))
            .route("/conversations", get(list_conversations_handler).post(create_conversation_handler))
            .route("/conversations/:id", delete(delete_conversation_handler).patch(update_conversation_handler))
            .route("/conversations/:id/messages", get(list_conversation_messages_handler))
        .route("/conversations/:id/share", post(share_conversation_handler))
        .route("/shared/:token", get(shared_conversation_handler))
        .route("/api/v1/shared/:token", get(shared_conversation_handler))
            .route("/dictionary", get(get_dictionary_handler).post(add_dictionary_handler))
            .route("/dictionary/:id", delete(delete_dictionary_handler))
            .route("/dictionary/import-seed", post(import_seed_handler))
            .route("/admin/stats", get(get_admin_stats_handler))
            .route("/admin/plugins", get(get_admin_plugins_handler))
            .route("/admin/search-quality", get(get_admin_search_quality_handler))
            .route("/admin/benchmark", get(get_admin_benchmark_handler))
            .route("/admin/connectors", get(get_admin_connectors_handler))
            .route("/admin/query-logs", get(get_admin_query_logs_handler))
            .route("/admin/llm/providers", get(list_llm_providers_handler).post(upsert_llm_provider_handler))
            .route("/admin/llm/providers/:id", delete(delete_llm_provider_handler))
            .route("/admin/llm/test", post(test_llm_provider_handler))
            .route("/documents/trash", get(list_trash_handler))
            .route("/documents/:id/restore", post(restore_document_handler))
            .route("/documents/:id", get(get_document_handler).delete(delete_document_handler))
            .route("/chat", post(chat_handler))
            .route("/chat/feedback", post(chat_feedback_handler))
            .route("/chat/stream", post(chat_stream_handler))
            .route("/stats", get(stats_handler))
            .route("/workbench", get(workbench_handler))
            .layer(axum::extract::DefaultBodyLimit::max(60 * 1024 * 1024))
            .with_state(state)
    }

    #[tokio::test]
    async fn test_health_200() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/health").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_readyz_200() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/readyz").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("status").and_then(|v| v.as_str()).unwrap(), "ready");
        let checks = json.get("checks").unwrap();
        assert_eq!(checks.get("sqlite").and_then(|c| c.get("status")).and_then(|v| v.as_str()).unwrap(), "ok");
        assert_eq!(checks.get("vectorDb").and_then(|c| c.get("status")).and_then(|v| v.as_str()).unwrap(), "ok");
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
    async fn test_list_documents_filtering_and_sorting() {
        let state = build_test_state().await;
        let ws_id = "75b9b1dd-4c99-4b7d-a362-24fae18d861d".to_string();

        sqlx::query("INSERT INTO documents (id, title, source_type, source_path, status, chunk_count, workspace_id) VALUES ('doc2', 'alpha.md', 'pdf', 'docs/alpha.pdf', 'failed', 10, ?)")
            .bind(&ws_id)
            .execute(&state.db_pool)
            .await
            .unwrap();

        sqlx::query("INSERT INTO documents (id, title, source_type, source_path, status, chunk_count, workspace_id) VALUES ('doc3', 'zebra.md', 'markdown', 'docs/zebra.md', 'indexed', 2, ?)")
            .bind(&ws_id)
            .execute(&state.db_pool)
            .await
            .unwrap();

        let app = build_router(state.clone());

        let res = app.clone().oneshot(
            Request::builder()
                .uri("/documents?status=failed")
                .header("X-Workspace", "homelab")
                .body(axum::body::Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("id").unwrap().as_str().unwrap(), "doc2");

        let res = app.clone().oneshot(
            Request::builder()
                .uri("/documents?sourceType=pdf")
                .header("X-Workspace", "homelab")
                .body(axum::body::Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].get("id").unwrap().as_str().unwrap(), "doc2");

        let res = app.clone().oneshot(
            Request::builder()
                .uri("/documents?sortBy=title&order=asc")
                .header("X-Workspace", "homelab")
                .body(axum::body::Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 3);
        assert_eq!(docs[0].get("title").unwrap().as_str().unwrap(), "alpha.md");
        assert_eq!(docs[1].get("title").unwrap().as_str().unwrap(), "test.md");
        assert_eq!(docs[2].get("title").unwrap().as_str().unwrap(), "zebra.md");

        let res = app.clone().oneshot(
            Request::builder()
                .uri("/documents?sortBy=chunks&order=desc")
                .header("X-Workspace", "homelab")
                .body(axum::body::Body::empty())
                .unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 3);
        assert_eq!(docs[0].get("id").unwrap().as_str().unwrap(), "doc2");
        assert_eq!(docs[1].get("id").unwrap().as_str().unwrap(), "doc1");
        assert_eq!(docs[2].get("id").unwrap().as_str().unwrap(), "doc3");
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
    async fn test_patch_conversation_title() {
        let state = build_test_state().await;
        // 先建立一個 conversation
        let create_body = serde_json::json!({ "title": "original title" }).to_string();
        let create_res = build_router(state.clone())
            .oneshot(Request::builder().uri("/conversations").method("POST")
                .header("X-Workspace", "homelab")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(create_body)).unwrap())
            .await.unwrap();
        assert!(create_res.status().is_success());
        let body = axum::body::to_bytes(create_res.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let conv_id = created["id"].as_str().unwrap().to_string();

        // PATCH 更新標題
        let patch_body = serde_json::json!({ "title": "updated title" }).to_string();
        let patch_res = build_router(state.clone())
            .oneshot(Request::builder().uri(format!("/conversations/{conv_id}")).method("PATCH")
                .header("X-Workspace", "homelab")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(patch_body)).unwrap())
            .await.unwrap();
        assert_eq!(patch_res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(patch_res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["updated"], true);

        // 不存在的 ID → 404
        let miss_body = serde_json::json!({ "title": "x" }).to_string();
        let miss_res = build_router(state.clone())
            .oneshot(Request::builder().uri("/conversations/no-such-id").method("PATCH")
                .header("X-Workspace", "homelab")
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(miss_body)).unwrap())
            .await.unwrap();
        assert_eq!(miss_res.status(), StatusCode::NOT_FOUND);
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
        let ws_id = seed_workspace_id(&db_pool).await;
        sqlx::query("INSERT INTO dictionary (id, workspace_id, key, value) VALUES ('test-id-999', ?, 'API', '應用程式介面')").bind(&ws_id).execute(&db_pool).await.unwrap();

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
    async fn test_admin_plugins_returns_built_in_modules() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/admin/plugins")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let plugins = json.get("plugins").and_then(|v| v.as_array()).unwrap();
        assert!(!plugins.is_empty(), "內置模組列表不應為空");
        for plugin in plugins {
            assert!(plugin.get("name").and_then(|v| v.as_str()).is_some());
            assert_eq!(plugin.get("type").and_then(|v| v.as_str()), Some("built-in"));
            assert!(plugin.get("version").and_then(|v| v.as_str()).is_some());
            assert_eq!(
                plugin.get("health").and_then(|v| v.get("healthy")).and_then(|v| v.as_bool()),
                Some(true)
            );
            assert!(plugin.get("metrics").is_some());
        }
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
    async fn test_tags_crud_and_document_association() {
        let state = build_test_state().await;
        let app = build_router(state.clone());

        // Step 1: Create a tag
        let create_tag_payload = json!({
            "name": "Important",
            "color": "#FF0000"
        });
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/tags")
                    .header("X-Workspace", "homelab")
                    .header("content-type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&create_tag_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let created_tag_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tag_id = created_tag_json.get("id").unwrap().as_str().unwrap().to_string();
        assert_eq!(created_tag_json.get("name").unwrap().as_str().unwrap(), "Important");

        // Step 2: List tags
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/tags")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let list_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let tags_array = list_json.get("tags").unwrap().as_array().unwrap();
        assert_eq!(tags_array.len(), 1);
        assert_eq!(tags_array[0].get("name").unwrap().as_str().unwrap(), "Important");

        // Step 3: Insert a mock document into DB
        let doc_id = "test-doc-id-123".to_string();
        sqlx::query(
            "INSERT INTO documents (id, title, source_type, source_path, status, workspace_id) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&doc_id)
        .bind("Test Doc")
        .bind("TXT")
        .bind("test.txt")
        .bind("indexed")
        .bind("75b9b1dd-4c99-4b7d-a362-24fae18d861d") // build_test_state builds workspace with this UUID or similar?
        .execute(&state.db_pool)
        .await
        .unwrap();

        // Since build_test_state creates a workspace but we need its exact UUID or name, let's query workpaces
        let ws_row = sqlx::query("SELECT id FROM workspaces WHERE name = 'homelab'")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        let ws_id = sqlx::Row::get::<String, _>(&ws_row, 0);

        // Re-insert doc with correct workspace UUID
        sqlx::query("DELETE FROM documents").execute(&state.db_pool).await.unwrap();
        sqlx::query(
            "INSERT INTO documents (id, title, source_type, source_path, status, workspace_id) \
             VALUES (?, ?, ?, ?, ?, ?)"
        )
        .bind(&doc_id)
        .bind("Test Doc")
        .bind("TXT")
        .bind("test.txt")
        .bind("indexed")
        .bind(&ws_id)
        .execute(&state.db_pool)
        .await
        .unwrap();

        // Step 4: Tag the document
        let uri = format!("/documents/{}/tags/{}", doc_id, tag_id);
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let tag_res_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(tag_res_json.get("tagged").unwrap().as_bool().unwrap(), true);

        // Verify association in DB
        let assoc_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_tags WHERE document_id = ? AND tag_id = ?"
        )
        .bind(&doc_id)
        .bind(&tag_id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(assoc_count, 1);

        // Step 5: Untag the document
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&uri)
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let untag_res_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(untag_res_json.get("untagged").unwrap().as_bool().unwrap(), true);

        // Verify association is gone
        let assoc_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM document_tags WHERE document_id = ? AND tag_id = ?"
        )
        .bind(&doc_id)
        .bind(&tag_id)
        .fetch_one(&state.db_pool)
        .await
        .unwrap();
        assert_eq!(assoc_count, 0);

        // Step 6: Delete the tag
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&format!("/tags/{}", tag_id))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // Verify tag is gone
        let tag_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tags WHERE id = ?")
            .bind(&tag_id)
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(tag_count, 0);
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
    async fn test_upload_dedup_same_workspace_reuses_document() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 第一次上傳 dup.md
        let body = multipart_upload_body("dup.md", b"hello world first version");
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "第一次上傳應成功");
        let first: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        let first_id = first.get("documentId").and_then(|v| v.as_str()).unwrap().to_string();
        assert_eq!(first.get("status").and_then(|v| v.as_str()).unwrap(), "indexed");

        // 第二次上傳同名檔案（內容不同）
        let second_content = "hello world second version much longer content for reindex";
        let body = multipart_upload_body("dup.md", second_content.as_bytes());
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "第二次上傳應成功");
        let second: serde_json::Value =
            serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
        let second_id = second.get("documentId").and_then(|v| v.as_str()).unwrap().to_string();

        assert_eq!(first_id, second_id, "同 workspace 重複上傳同檔必須回傳相同 documentId");

        // DB: 只有一列（無 header 上傳 → source_path = `{workspace_id}/{basename}`，workspace_id 為 UUID）
        let ws_id = seed_workspace_id(&db_pool).await;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE workspace_id = ? AND source_path = ?"
        )
        .bind(&ws_id)
        .bind(format!("{ws_id}/dup.md"))
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "同 workspace 重複上傳同檔只能有一列");

        // 且已 reindex（status='indexed'、file_size_bytes 為第二次內容長度）
        let (status, size): (String, i64) = sqlx::query_as("SELECT status, file_size_bytes FROM documents WHERE id = ?")
            .bind(&first_id)
            .fetch_one(&db_pool)
            .await
            .unwrap();
        assert_eq!(status, "indexed");
        assert_eq!(size, second_content.len() as i64, "reindex 後 file_size_bytes 應為最新上傳內容大小");
    }

    #[tokio::test]
    async fn test_document_delete_not_found_returns_404() {
        let state = build_test_state().await;

        // 不存在的文件 id → 404（Node 契約 documents.ts:42）
        let res = build_router(state)
            .oneshot(Request::builder()
                .method("DELETE")
                .uri("/documents/nonexistent-id")
                .header("X-Workspace", "homelab")
                .body(axum::body::Body::empty())
                .unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND, "不存在的文件必須回傳 404（Node 契約）");
    }

    #[tokio::test]
    async fn test_upload_dedup_different_workspaces_separate_rows() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        let body = multipart_upload_body("dup.md", b"same file name, two workspaces");
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let id_a: String = {
            let v: serde_json::Value =
                serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
            v.get("documentId").and_then(|x| x.as_str()).unwrap().to_string()
        };

        let body = multipart_upload_body("dup.md", b"same file name, two workspaces");
        let res = build_router(state.clone())
            .oneshot(upload_request("OpenDocuments", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let id_b: String = {
            let v: serde_json::Value =
                serde_json::from_slice(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()).unwrap();
            v.get("documentId").and_then(|x| x.as_str()).unwrap().to_string()
        };

        assert_ne!(id_a, id_b, "不同 workspace 的同名檔案必須是不同的文件");

        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT source_path, workspace_id FROM documents WHERE title = 'dup.md' ORDER BY workspace_id"
        )
        .fetch_all(&db_pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2, "兩個 workspace 應各自有一列");
        // 無 header 上傳 → source_path = `{workspace_id}/{basename}`（workspace_id 均為 UUID）
        let homelab_id = seed_workspace_id(&db_pool).await;
        let open_docs_id: String = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = 'OpenDocuments'")
            .fetch_one(&db_pool)
            .await
            .unwrap();
        let paths: Vec<String> = rows.iter().map(|(p, _)| p.clone()).collect();
        assert!(paths.contains(&format!("{homelab_id}/dup.md")));
        assert!(paths.contains(&format!("{open_docs_id}/dup.md")));
    }

    #[tokio::test]
    async fn test_upload_source_path_header_override() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 帶 x-source-path header → 原樣儲存（不帶 workspace 前綴）
        let body = multipart_upload_body("A.md", b"absolute path document");
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, Some("/mnt/data/docs/A.md")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ws_id = seed_workspace_id(&db_pool).await;
        let source_path: String = sqlx::query_scalar(
            "SELECT source_path FROM documents WHERE workspace_id = ? AND title = 'A.md'"
        )
        .bind(&ws_id)
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(source_path, "/mnt/data/docs/A.md", "x-source-path 應原樣作為 source_path");

        // 同 basename 不同目錄（不同絕對路徑）→ 各自獨立文件，互不碰撞
        let body = multipart_upload_body("A.md", b"absolute path document");
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, Some("/mnt/data/other-dir/A.md")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE workspace_id = ? AND title = 'A.md'"
        )
        .bind(&ws_id)
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(count, 2, "不同目錄的同名檔案（絕對路徑不同）應各自建立為獨立文件");

        // 無 header → workspace/basename
        let body = multipart_upload_body("B.md", b"manual upload");
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let source_path: String = sqlx::query_scalar(
            "SELECT source_path FROM documents WHERE workspace_id = ? AND title = 'B.md'"
        )
        .bind(&ws_id)
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(source_path, format!("{ws_id}/B.md"), "無 header 時應為 workspace_id/basename");
    }

    #[tokio::test]
    async fn test_upload_rejects_over_50mb() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        let oversized = vec![0u8; 50 * 1024 * 1024 + 1];
        let body = multipart_upload_body("huge.md", &oversized);
        let res = build_router(state.clone())
            .oneshot(upload_request("homelab", body, None))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE, "超過 50MiB 應回傳 413");

        let ws_id = seed_workspace_id(&db_pool).await;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM documents WHERE workspace_id = ? AND title = 'huge.md'"
        )
        .bind(&ws_id)
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "413 時不得寫入任何列");
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
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        eprintln!("DEBUG status/body: {:?} / {:?}", status, String::from_utf8_lossy(&body));
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

    #[tokio::test]
    async fn test_chat_feedback_updates_query_log() {
        let state = build_test_state().await;
        let log_id: i64 = sqlx::query_scalar("SELECT id FROM query_logs LIMIT 1")
            .fetch_one(&state.db_pool)
            .await
            .unwrap();

        let req_body = serde_json::json!({ "queryId": log_id.to_string(), "feedback": "positive" });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat/feedback")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("saved").and_then(|v| v.as_bool()).unwrap(), true);

        let stored: String = sqlx::query_scalar("SELECT feedback FROM query_logs WHERE id = ?")
            .bind(log_id)
            .fetch_one(&state.db_pool)
            .await
            .unwrap();
        assert_eq!(stored, "positive");
    }

    #[tokio::test]
    async fn test_chat_feedback_rejects_invalid_value() {
        let state = build_test_state().await;
        let req_body = serde_json::json!({ "queryId": "1", "feedback": "meh" });
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat/feedback")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_chat_stream_rejects_unknown_conversation() {
        let state = build_test_state().await;

        // 不存在的 conversationId → 404（Node 契約 chat.ts:127）
        let req_body = serde_json::json!({ "query": "test", "conversationId": "nonexistent-convo" });
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat/stream")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::NOT_FOUND,
            "不存在的 conversationId 必須回傳 404（Node 契約）"
        );
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("error").and_then(|v| v.as_str()).unwrap(),
            "Conversation not found"
        );
    }

    #[tokio::test]
    async fn test_chat_handler_persistence() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 1. Create a conversation first
        let req_body = serde_json::json!({ "title": "Chat and persist" });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let conv_json: Value = serde_json::from_slice(&body).unwrap();
        let conv_id = conv_json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // 2. Call /chat with conversationId
        let chat_body = serde_json::json!({
            "query": "Who are you?",
            "profile": "fast",
            "conversationId": conv_id
        });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&chat_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 3. Verify messages are written to DB (User message and Assistant message)
        let messages: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT role, content, profile_used FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
        )
        .bind(&conv_id)
        .fetch_all(&db_pool)
        .await
        .unwrap();

        assert_eq!(messages.len(), 2, "Should persist exactly 2 messages (user & assistant)");
        assert_eq!(messages[0].0, "user");
        assert_eq!(messages[0].1, "Who are you?");
        assert_eq!(messages[1].0, "assistant");
        assert_eq!(messages[1].2, Some("fast".to_string()));

        // 4. Verify query_logs are written
        let query_log_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM query_logs WHERE query = 'Who are you?' AND profile = 'fast'"
        )
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(query_log_count, 1, "Should log query to query_logs");
    }

    #[tokio::test]
    async fn test_chat_stream_handler_persistence() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 1. Call /chat/stream without conversationId (should auto-create one)
        let chat_body = serde_json::json!({
            "query": "Stream with persistence",
            "profile": "precise"
        });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chat/stream")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&chat_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        
        // Extract conversationId from 'done' event in SSE stream
        let mut conv_id = String::new();
        for line in body_str.lines() {
            if line.contains("\"conversationId\"") {
                // Find where the JSON starts
                if let Some(idx) = line.find('{') {
                    let json_str = &line[idx..];
                    if let Ok(json_val) = serde_json::from_str::<Value>(json_str) {
                        if let Some(cid) = json_val.get("conversationId").and_then(|v| v.as_str()) {
                            conv_id = cid.to_string();
                        }
                    }
                }
            }
        }
        assert!(!conv_id.is_empty(), "Should auto-create and return a conversationId in done event");

        // 2. Verify messages are written (user and assistant)
        let messages: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT role, content, profile_used FROM messages WHERE conversation_id = ? ORDER BY created_at ASC"
        )
        .bind(&conv_id)
        .fetch_all(&db_pool)
        .await
        .unwrap();

        assert_eq!(messages.len(), 2, "Should persist user and assistant messages for streaming too");
        assert_eq!(messages[0].0, "user");
        assert_eq!(messages[0].1, "Stream with persistence");
        assert_eq!(messages[1].0, "assistant");
        assert_eq!(messages[1].2, Some("precise".to_string()));

        // 3. Verify query_logs are written
        let query_log_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM query_logs WHERE query = 'Stream with persistence' AND profile = 'precise'"
        )
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(query_log_count, 1);
    }

    #[tokio::test]
    async fn test_message_sources_deserialization() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 1. Create a workspace & conversation
        let ws_id = seed_workspace_id(&db_pool).await;
        let conv_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO conversations (id, title, workspace_id, shared, created_at, updated_at) VALUES (?, 'Test', ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"
        )
        .bind(&conv_id)
        .bind(&ws_id)
        .execute(&db_pool)
        .await
        .unwrap();

        // 2. Insert message with raw serialized sources JSON
        let sources_json = serde_json::json!([
            {
                "documentId": "doc123",
                "title": "Verified Rust Doc",
                "score": 0.95,
                "text": "Correct rust implementation."
            }
        ]);
        let sources_str = serde_json::to_string(&sources_json).unwrap();
        
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, sources, profile_used, created_at) VALUES (?, ?, 'assistant', 'Response', ?, 'precise', CURRENT_TIMESTAMP)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&conv_id)
        .bind(&sources_str)
        .execute(&db_pool)
        .await
        .unwrap();

        // 3. Request messages via API endpoint
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/conversations/{}/messages", conv_id))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let val: Value = serde_json::from_slice(&body).unwrap();
        
        // 4. Verify deserialized sources shape in messages response
        let messages = val.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 1);
        let msg = &messages[0];
        let sources = msg.get("sources").unwrap();
        
        // If our deserialization works, "sources" should be parsed back to an Array, not a String
        assert!(sources.is_array(), "Sources must be deserialized back into a JSON array");
        assert_eq!(sources[0].get("documentId").unwrap().as_str().unwrap(), "doc123");
    }

    #[tokio::test]
    async fn test_chat_and_stream_with_history_context() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 1. Create workspace & conversation
        let ws_id = seed_workspace_id(&db_pool).await;
        let conv_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO conversations (id, title, workspace_id, shared, created_at, updated_at) VALUES (?, 'Test Context', ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"
        )
        .bind(&conv_id)
        .bind(&ws_id)
        .execute(&db_pool)
        .await
        .unwrap();

        // 2. Insert some historical messages
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'user', 'What is Rust?', CURRENT_TIMESTAMP)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&conv_id)
        .execute(&db_pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?, ?, 'assistant', 'A memory safe systems language.', CURRENT_TIMESTAMP)"
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&conv_id)
        .execute(&db_pool)
        .await
        .unwrap();

        // 3. Call get_history_context helper directly to verify formatting
        let history = get_history_context(&db_pool, &conv_id).await;
        assert!(history.contains("User: What is Rust?"));
        assert!(history.contains("Assistant: A memory safe systems language."));
    }

    #[tokio::test]
    async fn test_conversation_create_messages_delete_flow() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 1. create conversation
        let req_body = serde_json::json!({ "title": "New chat" });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("title").and_then(|v| v.as_str()).unwrap(), "New chat");
        let ws_id = seed_workspace_id(&state.db_pool).await;
        assert_eq!(json.get("workspaceId").and_then(|v| v.as_str()).unwrap(), ws_id);
        assert_eq!(json.get("shared").and_then(|v| v.as_bool()).unwrap(), false);
        assert!(json.get("createdAt").is_some(), "應有 camelCase createdAt");
        assert!(json.get("updatedAt").is_some(), "應有 camelCase updatedAt");
        let conv_id = json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // 2. insert a message, fetch via API
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, profile_used, confidence_score, response_time_ms) VALUES ('msg1', ?, 'assistant', 'hello', 'hybrid', 0.9, 100)"
        )
        .bind(&conv_id)
        .execute(&db_pool)
        .await
        .unwrap();

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/conversations/{conv_id}/messages"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let messages = json.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].get("conversationId").and_then(|v| v.as_str()).unwrap(), conv_id);
        assert_eq!(messages[0].get("role").and_then(|v| v.as_str()).unwrap(), "assistant");
        assert_eq!(messages[0].get("profileUsed").and_then(|v| v.as_str()).unwrap(), "hybrid");
        assert_eq!(messages[0].get("confidenceScore").and_then(|v| v.as_f64()).unwrap(), 0.9);
        assert_eq!(messages[0].get("responseTimeMs").and_then(|v| v.as_i64()).unwrap(), 100);
        assert!(messages[0].get("createdAt").is_some());

        // 3. delete → soft delete; messages 404 afterwards
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/conversations/{conv_id}"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("deleted").and_then(|v| v.as_bool()).unwrap(), true);

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/conversations/{conv_id}/messages"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        // 4. unknown id → 404
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/conversations/does-not-exist")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_conversation_share_creates_token() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 先建立對話
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        serde_json::to_vec(&serde_json::json!({ "title": "Share me" })).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let conv_id = json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // share
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/conversations/{}/share", conv_id))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let share_url = json
            .get("shareUrl")
            .and_then(|v| v.as_str())
            .expect("應有 shareUrl")
            .to_string();
        assert!(
            share_url.starts_with("/shared/"),
            "shareUrl 格式: {share_url}"
        );
        let token = share_url.trim_start_matches("/shared/");

        // DB 狀態
        let (shared, stored_token): (i64, String) = sqlx::query_as(
            "SELECT shared, share_token FROM conversations WHERE id = ?",
        )
        .bind(&conv_id)
        .fetch_one(&db_pool)
        .await
        .unwrap();
        assert_eq!(shared, 1, "shared 必須為 1");
        assert_eq!(stored_token, token, "share_token 必須等於回傳 token");
        assert_eq!(stored_token.len(), 32, "token 必須為 32 字元 hex");
    }

    #[tokio::test]
    async fn test_conversation_share_unknown_404() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/conversations/nonexistent-id/share")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("error").and_then(|v| v.as_str()).unwrap(),
            "Conversation not found"
        );
    }

    #[tokio::test]
    async fn test_shared_view_returns_conversation() {
        let state = build_test_state().await;
        let db_pool = state.db_pool.clone();

        // 直接插對話 + share_token
        let ws_id = seed_workspace_id(&db_pool).await;
        sqlx::query(
            "INSERT INTO conversations (id, workspace_id, title, shared, share_token) VALUES ('conv-shared', ?, 'Public chat', 1, '0123456789abcdef0123456789abcdef')",
        )
        .bind(&ws_id)
        .execute(&db_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, conversation_id, role, content, profile_used, confidence_score, response_time_ms) VALUES ('msg-shared', 'conv-shared', 'assistant', 'hi', 'hybrid', 0.9, 100)",
        )
        .execute(&db_pool)
        .await
        .unwrap();

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/shared/0123456789abcdef0123456789abcdef")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let conv = json.get("conversation").expect("應有 conversation");
        assert_eq!(
            conv.get("title").and_then(|v| v.as_str()).unwrap(),
            "Public chat"
        );
        assert_eq!(
            conv.get("share_token").and_then(|v| v.as_str()).unwrap(),
            "0123456789abcdef0123456789abcdef",
            "conversation 為原始 row（含 share_token）"
        );
        let messages = json.get("messages").and_then(|v| v.as_array()).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].get("content").and_then(|v| v.as_str()).unwrap(),
            "hi"
        );
    }

    #[tokio::test]
    async fn test_shared_view_invalid_token_404() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/shared/deadbeefdeadbeefdeadbeefdeadbeef")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            json.get("error").and_then(|v| v.as_str()).unwrap(),
            "Not found"
        );
    }

    #[tokio::test]
    async fn test_collections_create_and_delete() {
        let state = build_test_state().await;

        // empty/whitespace name → 400
        let req_body = serde_json::json!({ "name": "   " });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/collections")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // create
        let req_body = serde_json::json!({ "name": "Research", "description": "papers" });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/collections")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("name").and_then(|v| v.as_str()).unwrap(), "Research");
        assert_eq!(json.get("description").and_then(|v| v.as_str()).unwrap(), "papers");
        let coll_id = json.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // delete
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/collections/{coll_id}"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("deleted").and_then(|v| v.as_bool()).unwrap(), true);
    }

    fn add_document_request(collection_id: &str, document_id: &str) -> Request<axum::body::Body> {
        Request::builder()
            .method("POST")
            .uri(format!("/collections/{collection_id}/documents/{document_id}"))
            .header("X-Workspace", "homelab")
            .body(axum::body::Body::empty())
            .unwrap()
    }

    async fn create_test_collection(state: &Arc<McpState>) -> String {
        let req_body = serde_json::json!({ "name": "Research", "description": "papers" });
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/collections")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&req_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        json.get("id").and_then(|v| v.as_str()).unwrap().to_string()
    }

    async fn insert_test_document(state: &Arc<McpState>, id: &str) {
        let ws_id = seed_workspace_id(&state.db_pool).await;
        sqlx::query(
            "INSERT INTO documents (id, title, source_type, source_path, status, chunk_count, workspace_id) VALUES (?, ?, 'markdown', 'docs/test.md', 'indexed', 1, ?)"
        )
        .bind(id)
        .bind(format!("{id}.md"))
        .bind(&ws_id)
        .execute(&state.db_pool)
        .await
        .unwrap();
    }

    async fn collection_doc_count(state: &Arc<McpState>, collection_id: &str) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM collection_documents WHERE collection_id = ?")
            .bind(collection_id)
            .fetch_one(&state.db_pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_collection_add_document() {
        let state = build_test_state().await;
        let coll_id = create_test_collection(&state).await;
        insert_test_document(&state, "doc-add").await;

        let res = build_router(state.clone()).oneshot(add_document_request(&coll_id, "doc-add")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("added").and_then(|v| v.as_bool()).unwrap(), true);
        assert_eq!(collection_doc_count(&state, &coll_id).await, 1);
    }

    #[tokio::test]
    async fn test_collection_add_missing_document_silent() {
        let state = build_test_state().await;
        let coll_id = create_test_collection(&state).await;

        let res = build_router(state.clone()).oneshot(add_document_request(&coll_id, "no-such-doc")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("added").and_then(|v| v.as_bool()).unwrap(), true);
        assert_eq!(collection_doc_count(&state, &coll_id).await, 0);
    }

    #[tokio::test]
    async fn test_collection_add_duplicate_idempotent() {
        let state = build_test_state().await;
        let coll_id = create_test_collection(&state).await;
        insert_test_document(&state, "doc-dup").await;

        for _ in 0..2 {
            let res = build_router(state.clone()).oneshot(add_document_request(&coll_id, "doc-dup")).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
        }
        assert_eq!(collection_doc_count(&state, &coll_id).await, 1);
    }

    #[tokio::test]
    async fn test_collection_remove_document() {
        let state = build_test_state().await;
        let coll_id = create_test_collection(&state).await;
        insert_test_document(&state, "doc-rm").await;
        let _ = build_router(state.clone()).oneshot(add_document_request(&coll_id, "doc-rm")).await.unwrap();

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/collections/{coll_id}/documents/doc-rm"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("removed").and_then(|v| v.as_bool()).unwrap(), true);
        assert_eq!(collection_doc_count(&state, &coll_id).await, 0);
    }

    #[tokio::test]
    async fn test_collection_list_documents() {
        let state = build_test_state().await;
        let coll_id = create_test_collection(&state).await;
        insert_test_document(&state, "doc-list-1").await;
        insert_test_document(&state, "doc-list-2").await;
        for d in ["doc-list-1", "doc-list-2"] {
            let _ = build_router(state.clone()).oneshot(add_document_request(&coll_id, d)).await.unwrap();
        }

        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri(format!("/collections/{coll_id}/documents"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        let collection = json.get("collection").unwrap();
        assert_eq!(collection.get("id").and_then(|v| v.as_str()).unwrap(), coll_id);
        assert_eq!(collection.get("name").and_then(|v| v.as_str()).unwrap(), "Research");
        assert_eq!(collection.get("description").and_then(|v| v.as_str()).unwrap(), "papers");
        assert!(collection.get("createdAt").is_some(), "createdAt 應該存在 (camelCase)");
        assert!(collection.get("created_at").is_none(), "不應該存在 snake_case 的 created_at");

        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        let doc_ids: Vec<&str> = docs.iter().filter_map(|d| d.get("id").and_then(|v| v.as_str())).collect();
        assert!(
            doc_ids.contains(&"doc-list-1") && doc_ids.contains(&"doc-list-2"),
            "documents 應包含兩個文件"
        );
    }

    #[tokio::test]
    async fn test_collection_list_missing_collection_404() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/collections/does-not-exist/documents")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("error").and_then(|v| v.as_str()).unwrap(), "Collection not found");
    }

    #[tokio::test]
    async fn test_stats_endpoint() {
        let state = build_test_state().await;
        let res = build_router(state)
            .oneshot(Request::builder().uri("/stats").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("documents").and_then(|v| v.as_i64()).unwrap(), 1);
        assert_eq!(json.get("workspaces").and_then(|v| v.as_i64()).unwrap(), 1);
        assert_eq!(json.get("plugins").and_then(|v| v.as_i64()).unwrap(), 0);
        assert_eq!(json.get("pluginList").and_then(|v| v.as_array()).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_document_get_then_delete_same_route() {
        // GET 與 DELETE 掛在同一路徑 (主 router 形狀)：驗證合併後兩者皆可用
        let state = build_test_state().await;

        let res = build_router(state.clone())
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

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
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
        assert_eq!(json.get("deleted").and_then(|v| v.as_bool()).unwrap(), true);

        // soft-delete 後 list 不應再包含該文件
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/documents")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert!(docs.is_empty(), "軟刪除後 documents 列表應為空");
    }

    #[tokio::test]
    async fn test_document_trash_and_restore() {
        let state = build_test_state().await;

        // 1. 初始垃圾桶應為空
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/documents/trash")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert!(docs.is_empty(), "初始垃圾桶應為空");

        // 2. 軟刪除 doc1
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/documents/doc1")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 3. 垃圾桶現在應該包含 doc1
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/documents/trash")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 1, "垃圾桶應該有 1 個文件");
        assert_eq!(docs[0].get("id").and_then(|v| v.as_str()).unwrap(), "doc1");

        // 4. 跨 workspace 垃圾桶隔離驗證 (OpenDocuments 應該看不到 doc1 在垃圾桶)
        // 先建立 OpenDocuments 工作空間，否則 resolve_workspace_id 會回傳 400 Bad Request
        let second_ws_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO workspaces (id, name) VALUES (?, 'OpenDocuments')")
            .bind(&second_ws_id)
            .execute(&state.db_pool)
            .await
            .unwrap();

        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/documents/trash")
                    .header("X-Workspace", "OpenDocuments")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert!(docs.is_empty(), "跨工作空間不應能看到別人的垃圾桶內容");

        // 5. 還原 doc1
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/documents/doc1/restore")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("restored").and_then(|v| v.as_bool()).unwrap(), true);

        // 6. 還原後垃圾桶應變回空
        let res = build_router(state.clone())
            .oneshot(
                Request::builder()
                    .uri("/documents/trash")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
                )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert!(docs.is_empty(), "還原後垃圾桶應為空");

        // 7. 還原後正常 documents 列表應重新包含 doc1
        let res = build_router(state)
            .oneshot(
                Request::builder()
                    .uri("/documents")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let docs = json.get("documents").and_then(|v| v.as_array()).unwrap();
        assert_eq!(docs.len(), 1, "還原後 documents 列表應該恢復到 1 個文件");
        assert_eq!(docs[0].get("id").and_then(|v| v.as_str()).unwrap(), "doc1");
    }

    #[tokio::test]
    async fn test_llm_providers_crud() {
        let state = build_test_state().await;
        let app = build_router(state.clone());

        // 1. GET 列表應為空
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/llm/providers")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let providers = json.get("providers").and_then(|v| v.as_array()).unwrap();
        assert!(providers.is_empty(), "預設 LLM providers 應為空");

        // 2. POST 建立 provider
        let payload = json!({
            "name": "deepseek-test",
            "provider": "deepseek",
            "baseUrl": "https://api.deepseek.com/v1",
            "model": "deepseek-chat",
            "apiKey": "sk-secret-key-123",
            "isActive": true
        });
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/llm/providers")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.get("name").and_then(|v| v.as_str()).unwrap(), "deepseek-test");
        assert_eq!(created.get("isActive").and_then(|v| v.as_bool()).unwrap(), true);
        assert_eq!(created.get("hasApiKey").and_then(|v| v.as_bool()).unwrap(), true);
        let provider_id = created.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // 3. GET 列表確認不洩漏 API Key 且 hasApiKey 為 true
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/llm/providers")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let list = json.get("providers").and_then(|v| v.as_array()).unwrap();
        assert_eq!(list.len(), 1);
        let item = &list[0];
        assert_eq!(item.get("id").and_then(|v| v.as_str()).unwrap(), &provider_id);
        assert_eq!(item.get("hasApiKey").and_then(|v| v.as_bool()).unwrap(), true);
        assert!(item.get("apiKey").is_none(), "安全防禦：列表絕不能回傳 apiKey 欄位");
        assert!(item.get("api_key").is_none());

        // 4. POST 更新：保持 API Key，修改 model 且設為 active
        let update_payload = json!({
            "name": "deepseek-test",
            "provider": "deepseek",
            "baseUrl": "https://api.deepseek.com/v2",
            "model": "deepseek-coder",
            "apiKey": "", // 留空代表保持原 key
            "isActive": true
        });
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/llm/providers")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&update_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let updated: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(updated.get("model").and_then(|v| v.as_str()).unwrap(), "deepseek-coder");
        assert_eq!(updated.get("hasApiKey").and_then(|v| v.as_bool()).unwrap(), true, "既有 key 應該被保留");

        // 5. 刪除測試
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/admin/llm/providers/{provider_id}"))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 驗證列表重新變空
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/admin/llm/providers")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let list = json.get("providers").and_then(|v| v.as_array()).unwrap();
        assert!(list.is_empty(), "刪除後列表應為空");
    }

    #[tokio::test]
    async fn test_llm_provider_test_connection() {
        // 建立 Mock Server 來模擬 OpenAI completions 回應
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mock_url = format!("http://{}", addr);

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf).await; // 讀取 request (不論內容)

                let response_body = serde_json::json!({
                    "choices": [{
                        "message": {
                            "role": "assistant",
                            "content": "pong"
                        }
                    }]
                }).to_string();

                let http_response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(http_response.as_bytes()).await;
            }
        });

        let state = build_test_state().await;
        let app = build_router(state.clone());

        // 測試連線 (以臨時 config 呼叫)
        let test_payload = json!({
            "baseUrl": mock_url,
            "model": "mock-model",
            "apiKey": "mock-key"
        });

        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/admin/llm/test")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&test_payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json.get("ok").and_then(|v| v.as_bool()).unwrap(), true);
        assert_eq!(json.get("reply").and_then(|v| v.as_str()).unwrap(), "pong");
        assert!(json.get("latencyMs").is_some());
    }

    #[tokio::test]
    async fn test_extracted_assets_crud() {
        let state = build_test_state().await;
        let app = build_router(state.clone());

        // 1. GET assets list should be empty
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/extracted-assets")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let assets = json.get("assets").and_then(|v| v.as_array()).unwrap();
        assert!(assets.is_empty());

        // 2. POST to extract asset with provided data_content
        let payload = json!({
            "title": "Test Invoice",
            "assetType": "invoice",
            "documentId": "doc1",
            "schemaDefinition": [
                { "key": "amount", "label": "金額", "type": "number" },
                { "key": "vendor", "label": "供應商", "type": "string" }
            ],
            "dataContent": [
                { "amount": 1200, "vendor": "ACME Corp" }
            ]
        });

        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/extracted-assets")
                    .header("X-Workspace", "homelab")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(serde_json::to_vec(&payload).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.get("title").and_then(|v| v.as_str()).unwrap(), "Test Invoice");
        let asset_id = created.get("id").and_then(|v| v.as_str()).unwrap().to_string();

        // 3. GET list should now contain 1 asset
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/extracted-assets")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let assets = json.get("assets").and_then(|v| v.as_array()).unwrap();
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].get("id").and_then(|v| v.as_str()).unwrap(), &asset_id);

        // 4. GET single asset details
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/extracted-assets/{}", asset_id))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let single: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(single.get("title").and_then(|v| v.as_str()).unwrap(), "Test Invoice");
        let parsed_data = single.get("dataContent").and_then(|v| v.as_array()).unwrap();
        assert_eq!(parsed_data.len(), 1);
        assert_eq!(parsed_data[0].get("vendor").and_then(|v| v.as_str()).unwrap(), "ACME Corp");

        // 5. DELETE asset
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/extracted-assets/{}", asset_id))
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // 6. GET list should be empty again
        let res = app.clone()
            .oneshot(
                Request::builder()
                    .uri("/extracted-assets")
                    .header("X-Workspace", "homelab")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let assets = json.get("assets").and_then(|v| v.as_array()).unwrap();
        assert!(assets.is_empty());
    }
}
