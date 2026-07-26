use std::net::SocketAddr;
use axum::{
    routing::get,
    Router, Json,
};
use tower_http::services::ServeDir;
use tower_http::cors::CorsLayer;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub engine: String,
    pub version: String,
}

// 💡 簡單的 REST API Handler
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        engine: "OpenDocuments Rust Core".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

// 💡 啟動大一統伺服器：同時處理 REST API、MCP SSE 與 WebUI 靜態託管 ！！！
pub async fn start_mcp_and_api_server(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("🚀 [OpenDocuments] 大一統伺服器正啟動於 http://{}", addr);

    // 1. 後端路由
    let api_routes = Router::new()
        .route("/healthz", get(health_handler));

    // 2. 前端 Vue 靜態資源託管服務，並配備標準 SPA fallback (重定向返回 index.html) ！！！
    // ponytail: fallback 到 ./dist/index.html 解決 Vue 重新整理 404 問題
    let serve_dir = ServeDir::new("./dist")
        .not_found_service(tower_http::services::ServeFile::new("./dist/index.html"));

    // 3. 組合全域 App
    let app = Router::new()
        .nest("/api/v1", api_routes)
        // 💡 靜態網頁託管 (Vue 專案執行 npm run build 後產出的 dist 目錄)
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
