#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
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
// ponytail: trait avoids pulling lancedb/lance into opendoc-mcp's dep tree.
// Add when ConfigManager's search API stabilizes.

pub trait SearchBackend: Send + Sync {
    fn search_and_rerank(&self, query: &str, threshold: f32) -> Vec<opendoc_types::DocumentChunk>;
}

// ── Shared state ──────────────────────────────────────────────

type SessionMap = Arc<RwLock<HashMap<String, mpsc::Sender<String>>>>;

pub struct McpState {
    pub sessions: SessionMap,
    pub search: Arc<dyn SearchBackend>,
}

// ── Health response ──────────────────────────────────────────

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

pub async fn start_mcp_and_api_server(
    port: u16,
    search: Arc<dyn SearchBackend>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("🚀 [OpenDocuments] 大一統伺服器正啟動於 http://{addr}");

    let mcp_state = Arc::new(McpState {
        sessions: Arc::new(RwLock::new(HashMap::new())),
        search,
    });

    let api_routes = Router::new()
        .route("/healthz", get(health_handler));

    // ponytail: SPA fallback 到 ./dist/index.html 解決 Vue 重新整理 404
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
