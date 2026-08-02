#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]

pub mod utils;
pub mod handlers;

use utils::{resolve_workspace_id, map_document_row, clean_json_markdown, DocumentItem};
use handlers::tags::{list_tags_handler, create_tag_handler, delete_tag_handler, tag_document_handler, untag_document_handler};
use handlers::workspaces::{list_workspaces_handler, create_workspace_handler, delete_workspace_handler};
use handlers::dictionary::{get_dictionary_handler, add_dictionary_handler, delete_dictionary_handler, import_seed_handler};
use handlers::conversations::{
    list_conversations_handler, update_conversation_handler, create_conversation_handler,
    delete_conversation_handler, list_conversation_messages_handler, share_conversation_handler,
    shared_conversation_handler
};
use handlers::admin::{
    get_admin_stats_handler, get_admin_plugins_handler, get_admin_search_quality_handler,
    get_admin_query_logs_handler, get_admin_benchmark_handler, get_admin_connectors_handler
};
use handlers::documents::{
    list_documents_handler, get_document_handler, delete_document_handler,
    list_trash_handler, restore_document_handler
};
use handlers::collections::{
    list_collections_handler, create_collection_handler, delete_collection_handler,
    add_collection_document_handler, remove_collection_document_handler,
    list_collection_documents_handler
};
use handlers::llm::{
    list_llm_providers_handler, upsert_llm_provider_handler, delete_llm_provider_handler,
    test_llm_provider_handler
};
use handlers::assets::{list_assets_handler, extract_asset_handler, get_asset_handler, delete_asset_handler};
use handlers::workbench_core::workbench_handler;
use handlers::stats::stats_handler;
use handlers::query::{query_log_handler, query_feedback_handler, chat_feedback_handler};
use handlers::upload::upload_handler;
use handlers::chat::{chat_handler, chat_stream_handler};
use handlers::system::{health_handler, version_check_handler, readyz_handler};

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post, delete},
    http::{StatusCode, Uri, header, Response},
    body::Body,
    response::IntoResponse,
    Json, Router,
};
use rust_embed::RustEmbed;
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

// ── Remaining State ──────────────────────────────────────────

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

#[derive(RustEmbed)]
#[folder = "../../apps/webui/dist/"]
struct Assets;

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    
    // 如果路徑為空（首頁），使用 index.html
    let mut asset_path = if path.is_empty() {
        "index.html"
    } else {
        path
    };

    // 嘗試獲取資源
    let mut file = Assets::get(asset_path);

    // 如果找不到資源，為了支援 React 的 SPA 前端路由，一律回退至 index.html
    if file.is_none() {
        asset_path = "index.html";
        file = Assets::get(asset_path);
    }

    match file {
        Some(content) => {
            let mime = mime_guess::from_path(asset_path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap_or_else(|_| {
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::empty())
                        .unwrap()
                })
        }
        None => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("404 Not Found"))
                .unwrap()
        }
    }
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
        .route("/admin/version-check", get(version_check_handler))
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

    let app = Router::new()
        .nest("/api/v1", api_routes)
        .route("/mcp/sse", get(sse_handler))
        .route("/api/mcp/sse", get(sse_handler))
        .route("/mcp/message", post(message_handler))
        .route("/api/mcp/message", post(message_handler))
        .fallback(static_handler)
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
            .route("/admin/version-check", get(version_check_handler))
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
    async fn test_version_check() {
        let state = build_test_state().await;
        let res = build_router(state).oneshot(Request::builder().uri("/admin/version-check").body(axum::body::Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("current_version").is_some());
        assert!(json.get("latest_version").is_some());
        assert!(json.get("update_command").is_some());
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
