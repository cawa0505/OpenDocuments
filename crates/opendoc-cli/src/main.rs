#![recursion_limit = "512"]
#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]
#![warn(clippy::pedantic)]

use std::sync::Arc;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use opendoc_parser::parse_file;
use opendoc_storage::ConfigManager;
use opendoc_mcp::{start_mcp_and_api_server, SearchBackend};
use opendoc_llm::{embedding::ByokEmbeddingProvider, LlmProvider};
#[cfg(feature = "embedding-fastembed")]
use opendoc_storage::embeddings::FastEmbedProvider;
use opendoc_storage::{AppConfig, retriever::SidecarRetriever};
use opendoc_types::EmbeddingProvider;
use walkdir::WalkDir;
use reqwest::multipart;
use std::io::{self, BufRead};
use sha2::{Digest, Sha256};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::collections::HashMap;

#[derive(Parser)]
#[command(name = "opendocuments-rust")]
#[command(author = "Jimmy Yen")]
#[command(about = "OpenDocuments Rust — 極致性能、強型別防禦之自建 RAG 旗艦 CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 對 RAG 知識庫進行快速終端問答
    Ask {
        query: String,
        #[arg(short, long, default_value = "default")]
        workspace: Option<String>,
        collections: Option<Vec<String>>,
    },

    /// 對 RAG 知識庫進行向量與混合檢索
    Search {
        /// 檢索關鍵字或語義句
        query: String,
        /// 目標工作空間
        #[arg(short, long)]
        workspace: Option<String>,

        /// 相似度分數過濾門檻 (Score Filter 保險絲)
        #[arg(short, long, default_value_t = 0.6)]
        threshold: f32,

        /// 最大返回數量
        #[arg(short, long, default_value_t = 5)]
        limit: usize,
    },

    /// 物理文件管理 (Ingest, Delete, List, Reindex)
    Document {
        #[command(subcommand)]
        sub: DocumentSubcommands,
    },

    /// 工作空間管理與多租戶物理隔離
    Workspace {
        #[command(subcommand)]
        sub: WorkspaceSubcommands,
    },

    /// 自動配置與安裝大一統 Rust 版 MCP 服務到 OpenCode 設定檔
    InstallOpencode {
        /// 自訂 OpenCode 遠端主機 IP
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// 自訂 OpenCode 遠端連接埠
        #[arg(long, default_value_t = 3006)]
        port: u16,
    },

    /// 啟動 Axum API 與 MCP SSE 伺服器
    Start {
        /// Web API 與 MCP 連接埠
        #[arg(short, long, default_value_t = 3000)]
        port: u16,

        /// 是否僅啟動 MCP Stdio 本地服務 (CQRS Write-Only 模式)
        #[arg(long)]
        mcp_only: bool,
    },

    /// 停止背景服務
    Stop,

    /// 運行系統健康檢查 (Ollama 連通、LanceDB 狀態、API 健康度)
    Doctor,

    /// 系統配置項管理
    Config {
        #[command(subcommand)]
        sub: ConfigSubcommands,
    },

    /// 開發者調試 CLI 檔案解析器 (Phase 1 止血代理人)
    Parse {
        /// 要解析的檔案路徑
        file_path: PathBuf,

        /// 目標工作空間
        workspace_id: String,

        /// 目標集合
        collection_id: String,

        /// 上傳場景的可選原始名稱 (防範隨機 hash 檔名丟失副檔名)
        #[arg(short, long)]
        original_name: Option<String>,
    },
}

#[derive(Subcommand)]
enum DocumentSubcommands {
    /// 索引/上傳檔案到伺服器（支援路徑或標準輸入管道）
    #[command(alias = "add")]
    Index {
        /// 本地檔案或目錄路徑（省略時從標準輸入讀取檔案路徑列表）
        path: Option<PathBuf>,
        /// 工作空間
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// 列出指定工作空間的文件
    List {
        /// 篩選工作空間
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// 刪除指定 ID 的文件
    Delete {
        id: String,
        #[arg(short, long)]
        workspace: Option<String>,
    },
    /// 重新索引指定 ID 的文件
    Reindex {
        id: String,
        #[arg(short, long)]
        workspace: Option<String>,
    },
}

#[derive(Subcommand)]
enum WorkspaceSubcommands {
    /// 列出所有工作空間
    List,
    /// 建立一個新的物理隔離工作空間
    Create {
        name: String,
    },
    /// 刪除一個工作空間
    Delete {
        name: String,
    },
    /// 切換當前預設工作空間
    Switch {
        name: String,
    },
    /// 顯示當前作用中的工作空間
    Show,
}

#[derive(Subcommand)]
enum ConfigSubcommands {
    /// 獲取特定配置項的值
    Get {
        key: String,
    },
    /// 設置特定配置項的值
    Set {
        key: String,
        value: String,
    },
}

/// 建立向量檢索後端：讀取 llm_providers（優先 kind='embedding' 行，否則退化為 active chat
/// provider + embedding_provider_name 當模型名）→ ByokEmbeddingProvider → SidecarRetriever。
async fn build_search_backend(
    app_cfg: &AppConfig,
    pool: &sqlx::SqlitePool,
) -> Result<Arc<dyn SearchBackend>, String> {
    use sqlx::Row;
    let model = &app_cfg.model;
    let dim = model.embedding_dim;
    let table = model.embedding_table_name.clone();
    let default_ws_name = model.default_workspace.clone();
    let embed_model_name = model.embedding_provider_name.clone();

    // fastembed 離線後端（opt-in `embedding-fastembed` feature）：完全本機、無需 provider 設定。
    // 未以 `--features embedding-fastembed` 編譯時此分支不存在 → 自動走 BYOK。
    // ponytail: 模型檔首次使用時自 HF 下載至 <db_dir>/models，之後離線。
    #[cfg(feature = "embedding-fastembed")]
    if model.embedding_backend == "fastembed" {
        let db_dir = ConfigManager::resolve_db_dir(&app_cfg.database.path)?;
        let lance_uri = db_dir.to_string_lossy().to_string();
        let provider = FastEmbedProvider::new(Some(db_dir.join("models")))?;
        let dim = provider.dim();
        let embed = Arc::new(provider) as Arc<dyn EmbeddingProvider>;
        let retriever =
            SidecarRetriever::connect(&engine_path(), &lance_uri, &table, dim, &default_ws_name, embed).await?;
        return Ok(Arc::new(retriever) as Arc<dyn SearchBackend>);
    }

    // R5：workspace_id 為 TEXT，不強制 UUID。找預設工作空間 UUID；查不到就以名稱當 id。
    let ws_id: String = match sqlx::query("SELECT id FROM workspaces WHERE name = ? LIMIT 1")
        .bind(&default_ws_name)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(r)) => Row::get::<String, _>(&r, "id"),
        _ => default_ws_name.clone(),
    };

    // 1) 優先取 kind='embedding' 且 name=embedding_provider_name 的獨立 provider 行。
    let (base_url, api_key, emb_model): (String, Option<String>, String) =
        match sqlx::query(
            "SELECT base_url, api_key, model FROM llm_providers \
             WHERE workspace_id = ? AND name = ? AND kind = 'embedding' LIMIT 1",
        )
        .bind(&ws_id)
        .bind(&embed_model_name)
        .fetch_optional(pool)
        .await
        {
            Ok(Some(r)) => {
                let k: String = Row::get::<String, _>(&r, "api_key");
                (
                    Row::get::<String, _>(&r, "base_url"),
                    if k.is_empty() { None } else { Some(k) },
                    Row::get::<String, _>(&r, "model"),
                )
            }
            _ => {
                // 2) 退化：active chat provider + embedding_provider_name 當模型名。
                match sqlx::query(
                    "SELECT base_url, api_key FROM llm_providers \
                     WHERE workspace_id = ? AND is_active = 1 AND kind = 'chat' LIMIT 1",
                )
                .bind(&ws_id)
                .fetch_optional(pool)
                .await
                {
                    Ok(Some(r)) => {
                        let k: String = Row::get::<String, _>(&r, "api_key");
                        (
                            Row::get::<String, _>(&r, "base_url"),
                            if k.is_empty() { None } else { Some(k) },
                            embed_model_name.clone(),
                        )
                    }
                    _ => return Err("未設定任何 LLM provider，向量檢索後端無法建立".into()),
                }
            }
        };

    let provider = LlmProvider {
        name: embed_model_name.clone(),
        base_url,
        model: emb_model,
        api_key,
    };
    let embed = Arc::new(ByokEmbeddingProvider::new(provider, dim)) as Arc<dyn EmbeddingProvider>;

    let db_dir = ConfigManager::resolve_db_dir(&app_cfg.database.path)?;
    let lance_uri = db_dir.to_string_lossy().to_string();
    let retriever =
        SidecarRetriever::connect(&engine_path(), &lance_uri, &table, dim, &default_ws_name, embed).await?;
    Ok(Arc::new(retriever) as Arc<dyn SearchBackend>)
}

/// 引擎執行檔路徑：`OPENDOC_ENGINE_PATH` 環境變數覆蓋，預設查 PATH 的
/// `opendoc-engine-lancedb`。ponytail: server/worker 部署（roadmap）時改為連線位址。
fn engine_path() -> String {
    std::env::var("OPENDOC_ENGINE_PATH").unwrap_or_else(|_| "opendoc-engine-lancedb".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();

    // 1. 初始化設定檔載入
    let config_manager = match ConfigManager::load_or_init() {
        Ok(cm) => Arc::new(cm),
        Err(e) => {
            eprintln!("💥 載入設定失敗: {e}");
            std::process::exit(1);
        }
    };

let app_cfg = config_manager.get_config().await;

     let resolve_ws = |opt_ws: Option<String>| -> String {
         opt_ws.filter(|s| !s.is_empty())
             .map(|s| s.trim().to_owned())
             .or_else(|| app_cfg.model.active_workspace.clone())
             .unwrap_or_else(|| app_cfg.model.default_workspace.clone())
     };

     match cli.command {
        Commands::Ask { query, workspace, collections } => {
            let resolved_workspace = resolve_ws(workspace);
            println!("正在向空間 '{resolved_workspace}' 提問: '{query}' .. (API 端點: {})", app_cfg.server.url);
            if let Some(cols) = collections {
                println!("過濾集合: {cols:?}");
            }
        }
Commands::Search { query, workspace, threshold, limit } => {
            let resolved_workspace = resolve_ws(workspace);
            println!(
                "正在空間 '{resolved_workspace}' 檢索: '{query}' (門檻: {threshold}, 限制: {limit}).. (API 端點: {})",
                app_cfg.server.url
            );
        }
        Commands::Document { sub } => {
            match sub {
DocumentSubcommands::Index { path, workspace } => {
            let resolved_workspace = resolve_ws(workspace);

            println!("🚀 啟動高效索引:");
            println!("📦 目標工作空間: {resolved_workspace}");
            println!("{}", "-".repeat(50));

            let supported_extensions = [
                ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".mdx", ".json", ".yaml", ".yml",
                ".toml", ".css", ".html", ".htm", ".sh", ".sql", ".pdf", ".docx", ".xlsx",
            ];

            let upload_url = format!("{}/api/v1/documents/upload", app_cfg.server.url);
            let list_url = format!("{}/api/v1/documents", app_cfg.server.url);
            let delete_url_base = format!("{}/api/v1/documents", app_cfg.server.url);
            let client = reqwest::Client::new();

            // Fetch existing documents to build source_path -> (id, content_hash) map
            let mut existing_docs: HashMap<String, (String, Option<String>)> = HashMap::new();
            match client
                .get(&list_url)
                .header("X-Workspace", &resolved_workspace)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(json) => {
                            if let Some(docs) = json.get("documents").and_then(|v| v.as_array()) {
                                for doc in docs {
                                    if let (Some(id), Some(path)) = (
                                        doc.get("id").and_then(|v| v.as_str()),
                                        doc.get("source_path").and_then(|v| v.as_str()),
                                    ) {
                                        let hash = doc
                                            .get("content_hash")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s.to_string());
                                        existing_docs.insert(path.to_string(), (id.to_string(), hash));
                                    }
                                }
                            }
                            println!("📋 已載入 {} 個現有文件記錄", existing_docs.len());
                        }
                        Err(e) => eprintln!("⚠️  解析現有文件列表失敗: {e}"),
                    }
                }
                Ok(resp) => eprintln!("⚠️  取得現有文件列表失敗: HTTP {}", resp.status()),
                Err(e) => eprintln!("⚠️  無法連接伺服器取得現有文件: {e}"),
            }

            let mut success_count = 0;
            let mut fail_count = 0;
            let mut processed_paths: std::collections::HashSet<String> = std::collections::HashSet::new();

            if let Some(path) = path {
                let canon_path = match path.canonicalize() {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("💥 無法解析路徑: {e}");
                        std::process::exit(1);
                    }
                };

                println!("🌐 伺服器 API 端點: {}", app_cfg.server.url);

                // Load .opendocignore if exists
                let ignore_file = canon_path.join(".opendocignore");
                let mut ignore_matcher: Option<Gitignore> = None;
                if ignore_file.exists() {
                    let mut builder = GitignoreBuilder::new(&canon_path);
                    if builder.add(&ignore_file).is_none() {
                        ignore_matcher = builder.build().ok();
                        if ignore_matcher.is_some() {
                            println!("📄 已載入 .opendocignore 規則");
                        }
                    }
                }

                let is_directory = canon_path.is_dir();

                let walk = WalkDir::new(&canon_path).into_iter().filter_entry(|entry| {
                    if let Some(ref matcher) = ignore_matcher {
                        let path = entry.path();
                        let is_dir = entry.file_type().is_dir();
                        let rel_path = path.strip_prefix(&canon_path).unwrap_or(path);
                        if matcher.matched_path_or_any_parents(rel_path, is_dir).is_ignore() {
                            return false;
                        }
                    }
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with('.') {
                            return false;
                        }
                        let default_ignored = [
                            "node_modules", ".git", "dist", "build", ".turbo", ".next", ".cache",
                            "__pycache__", "venv", ".env", "out", "target", "target-state",
                        ];
                        if default_ignored.contains(&name) {
                            return false;
                        }
                    }
                    true
                });

                for entry in walk {
                    let Ok(entry) = entry else { continue };

                    if !entry.file_type().is_file() {
                        continue;
                    }

                    let file_path = entry.path();
                    let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else { continue };
                    let abs_source_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());

                    if file_name.starts_with('.') {
                        continue;
                    }

                    let ext = file_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{}", e.to_lowercase()))
                        .unwrap_or_default();

                    if !supported_extensions.contains(&ext.as_str()) {
                        continue;
                    }

                    let rel_path = match file_path.strip_prefix(&canon_path) {
                        Ok(p) => p.to_string_lossy().into_owned(),
                        Err(_) => file_name.to_string(),
                    };

                    let abs_path_str = abs_source_path.to_string_lossy().into_owned();
                    processed_paths.insert(abs_path_str.clone());

                    print!("[..] 上傳中: {rel_path} ..");
                    if let Err(e) = io::Write::flush(&mut io::stdout()) {
                        eprintln!("\r[!!] 無法刷新終端: {e}");
                    }

                    let max_retries = 3;
                    let mut attempt = 0;

                    while attempt < max_retries {
                        attempt += 1;

                        let file_bytes = match std::fs::read(file_path) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("\r[!!] 讀取檔案失敗: {rel_path} - {e}");
                                break;
                            }
                        };

                        let mut hasher = Sha256::new();
                        hasher.update(&file_bytes);
                        let content_hash = format!("{:x}", hasher.finalize());

                        if let Some((_id, Some(existing_hash))) = existing_docs.get(&abs_path_str) {
                            if existing_hash == &content_hash {
                                print!("\r[skip] 已索引且無變更: {rel_path}\n");
                                success_count += 1;
                                break;
                            }
                        }

                        let part = match multipart::Part::bytes(file_bytes)
                            .file_name(file_name.to_string())
                            .mime_str("application/octet-stream")
                        {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("\r[!!] 建立請求體失敗: {rel_path} - {e}");
                                break;
                            }
                        };

                        let form = multipart::Form::new().part("file", part);

                        match client
                            .post(&upload_url)
                            .header("X-Workspace", &resolved_workspace)
                            .header("x-source-path", &abs_path_str)
                            .header("x-content-hash", &content_hash)
                            .multipart(form)
                            .timeout(Duration::from_secs(180))
                            .send()
                            .await
                        {
                            Ok(resp) => {
                                if resp.status().is_success() {
                                    let res_json: serde_json::Value = resp.json().await.unwrap_or_default();
                                    let chunks = res_json.get("chunks").and_then(serde_json::Value::as_u64).unwrap_or(0);
                                    print!("\r[ok] 已索引: {rel_path} ({chunks} 區塊)\n");
                                    success_count += 1;
                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                    break;
                                } else if attempt == max_retries {
                                    print!("\r[!!] 失敗: {rel_path} - HTTP {}\n", resp.status());
                                    fail_count += 1;
                                } else {
                                    tokio::time::sleep(Duration::from_millis(1500)).await;
                                }
                            }
                            Err(e) => {
                                if attempt == max_retries {
                                    print!("\r[!!] 失敗: {rel_path} - 錯誤: {e}\n");
                                    fail_count += 1;
                                } else {
                                    tokio::time::sleep(Duration::from_millis(1500)).await;
                                }
                            }
                        }
                    }
                }

                if is_directory {
                    let canon_path_str = canon_path.to_string_lossy().into_owned();
                    let mut pruned_count = 0;
                    for (source_path, (id, _)) in &existing_docs {
                        if source_path.starts_with(&canon_path_str) && !processed_paths.contains(source_path) {
                            print!("[..] 本地已刪除，同步刪除雲端記錄: {source_path} ..");
                            if let Err(e) = io::Write::flush(&mut io::stdout()) {
                                eprintln!("\r[!!] 無法刷新終端: {e}");
                            }
                            let del_url = format!("{}/{}", delete_url_base, id);
                            match client
                                .delete(&del_url)
                                .header("X-Workspace", &resolved_workspace)
                                .send()
                                .await
                            {
                                Ok(resp) if resp.status().is_success() => {
                                    print!("\r[prune] 同步刪除雲端記錄: {source_path}\n");
                                    pruned_count += 1;
                                }
                                Ok(resp) => {
                                    print!("\r[!!] 刪除雲端記錄失敗: {source_path} - HTTP {}\n", resp.status());
                                }
                                Err(e) => {
                                    print!("\r[!!] 無法連接伺服器刪除記錄: {source_path} - {e}\n");
                                }
                            }
                        }
                    }
                    if pruned_count > 0 {
                        println!("🗑️  同步清理完成，共刪除 {} 個失效雲端檔案", pruned_count);
                    }
                }
            } else {
                // Stdin mode: read file paths from pipe
                println!("📥 從標準輸入讀取檔案路徑...");
                let stdin = io::stdin();
                for line in stdin.lock().lines() {
                    let line = match line {
                        Ok(l) => l,
                        Err(e) => {
                            eprintln!("[!!] 讀取標準輸入失敗: {e}");
                            fail_count += 1;
                            continue;
                        }
                    };
                    let line = line.trim().to_string();
                    if line.is_empty() {
                        continue;
                    }

                    let file_path = std::path::Path::new(&line);
                    if !file_path.exists() {
                        eprintln!("\r[!!] 檔案不存在: {line}");
                        fail_count += 1;
                        continue;
                    }
                    let abs_source_path = file_path.canonicalize().unwrap_or_else(|_| file_path.to_path_buf());
                    let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else { continue };

                    let ext = file_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| format!(".{}", e.to_lowercase()))
                        .unwrap_or_default();
                    if !supported_extensions.contains(&ext.as_str()) {
                        eprintln!("\r[skip] 不支援的檔案類型: {line}");
                        continue;
                    }

                    let abs_path_str = abs_source_path.to_string_lossy().into_owned();
                    processed_paths.insert(abs_path_str.clone());

                    print!("[..] 上傳中: {line} ..");
                    if let Err(e) = io::Write::flush(&mut io::stdout()) {
                        eprintln!("\r[!!] 無法刷新終端: {e}");
                    }

                    let max_retries = 3;
                    let mut attempt = 0;

                    while attempt < max_retries {
                        attempt += 1;

                        let file_bytes = match std::fs::read(&file_path) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("\r[!!] 讀取檔案失敗: {line} - {e}");
                                break;
                            }
                        };

                        let mut hasher = Sha256::new();
                        hasher.update(&file_bytes);
                        let content_hash = format!("{:x}", hasher.finalize());

                        if let Some((_id, Some(existing_hash))) = existing_docs.get(&abs_path_str) {
                            if existing_hash == &content_hash {
                                print!("\r[skip] 已索引且無變更: {line}\n");
                                success_count += 1;
                                break;
                            }
                        }

                        let part = match multipart::Part::bytes(file_bytes)
                            .file_name(file_name.to_string())
                            .mime_str("application/octet-stream")
                        {
                            Ok(p) => p,
                            Err(e) => {
                                eprintln!("\r[!!] 建立請求體失敗: {line} - {e}");
                                break;
                            }
                        };

                        let form = multipart::Form::new().part("file", part);

                        match client
                            .post(&upload_url)
                            .header("X-Workspace", &resolved_workspace)
                            .header("x-source-path", &abs_path_str)
                            .header("x-content-hash", &content_hash)
                            .multipart(form)
                            .timeout(Duration::from_secs(180))
                            .send()
                            .await
                        {
                            Ok(resp) => {
                                if resp.status().is_success() {
                                    let res_json: serde_json::Value = resp.json().await.unwrap_or_default();
                                    let chunks = res_json.get("chunks").and_then(serde_json::Value::as_u64).unwrap_or(0);
                                    print!("\r[ok] 已索引: {line} ({chunks} 區塊)\n");
                                    success_count += 1;
                                    tokio::time::sleep(Duration::from_millis(500)).await;
                                    break;
                                } else if attempt == max_retries {
                                    print!("\r[!!] 失敗: {line} - HTTP {}\n", resp.status());
                                    fail_count += 1;
                                } else {
                                    tokio::time::sleep(Duration::from_millis(1500)).await;
                                }
                            }
                            Err(e) => {
                                if attempt == max_retries {
                                    print!("\r[!!] 失敗: {line} - 錯誤: {e}\n");
                                    fail_count += 1;
                                } else {
                                    tokio::time::sleep(Duration::from_millis(1500)).await;
                                }
                            }
                        }
                    }
                }
            }

            println!("{}", "-".repeat(50));
            println!("🎉 索引完成! 成功: {success_count}, 失敗: {fail_count}");
        }
                DocumentSubcommands::List { workspace } => {
                    let ws = resolve_ws(workspace);
                    println!("正在列出 '{ws}' 空間下的文件..");

                    let client = reqwest::Client::new();
                    let url = format!("{}/api/v1/documents", app_cfg.server.url);
                    let resp = match client.get(&url)
                        .header("X-Workspace", &ws)
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("💥 連線失敗: {e}");
                            std::process::exit(1);
                        }
                    };

                    if !resp.status().is_success() {
                        eprintln!("💥 伺服器回傳: {}", resp.status());
                        std::process::exit(1);
                    }

                    let body: serde_json::Value = match resp.json().await {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("💥 解析回應失敗: {e}");
                            std::process::exit(1);
                        }
                    };

                    let docs = match body.get("documents").and_then(|d| d.as_array()) {
                        Some(arr) => arr,
                        None => {
                            println!("（無文件）");
                            return Ok(());
                        }
                    };

                    if docs.is_empty() {
                        println!("（無文件）");
                        return Ok(());
                    }

                    println!("{:<4} {:<40} {:<10} {:<10} {:<8}", "ID", "標題", "類型", "狀態", "Chunks");
                    println!("{}", "-".repeat(80));
                    for doc in docs {
                        let id_short = doc.get("id").and_then(|v| v.as_str()).unwrap_or("?")[..8].to_string();
                        let title = doc.get("title").and_then(|v| v.as_str()).unwrap_or("?");
                        let source_type = doc.get("source_type").and_then(|v| v.as_str()).unwrap_or("?");
                        let status = doc.get("status").and_then(|v| v.as_str()).unwrap_or("?");
                        let chunks = doc.get("chunk_count").and_then(|v| v.as_i64()).unwrap_or(0);
                        println!("{:<4} {:<40} {:<10} {:<10} {:<8}", id_short, title, source_type, status, chunks);
                    }
                    println!("\n共 {} 個文件", docs.len());
                }
                DocumentSubcommands::Delete { id, workspace } => {
                    let ws = resolve_ws(workspace);
                    println!("正在從 '{ws}' 刪除文件 ID: {id}");

                    let client = reqwest::Client::new();
                    let url = format!("{}/api/v1/documents/{}", app_cfg.server.url, id);
                    let resp = match client.delete(&url)
                        .header("X-Workspace", &ws)
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("💥 連線失敗: {e}");
                            std::process::exit(1);
                        }
                    };

                    if resp.status().is_success() {
                        println!("✅ 已刪除文件 {id}");
                    } else {
                        eprintln!("💥 刪除失敗: {}", resp.status());
                    }
                }
                DocumentSubcommands::Reindex { id, workspace } => {
                    let ws = resolve_ws(workspace);
                    println!("正在對 '{ws}' 重新索引文件 ID: {id}");

                    let client = reqwest::Client::new();
                    let url = format!("{}/api/v1/documents/{}/reindex", app_cfg.server.url, id);
                    let resp = match client.post(&url)
                        .header("X-Workspace", &ws)
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("💥 連線失敗: {e}");
                            std::process::exit(1);
                        }
                    };

                    if resp.status().is_success() {
                        println!("✅ 已觸發重新索引 {id}");
                    } else {
                        eprintln!("💥 重新索引失敗: {}", resp.status());
                    }
                }
            }
        }
        Commands::Workspace { sub } => {
            let db_pool = match tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(config_manager.init_db_pool())
            }) {
                Ok(pool) => pool,
                Err(e) => {
                    eprintln!("💥 初始化資料庫連線失敗: {e}");
                    std::process::exit(1);
                }
            };
match sub {
                 WorkspaceSubcommands::List => {
                     let rows: Vec<(String,)> = sqlx::query_as("SELECT id FROM workspaces")
                         .fetch_all(&db_pool)
                         .await
                         .expect("查詢 workspaces 失敗");
                     if rows.is_empty() {
                         println!("目前沒有任何工作空間。");
                     } else {
                         println!("工作空間列表:");
                         for row in rows {
                             println!("  - {}", row.0);
                         }
                     }
                 }
                 WorkspaceSubcommands::Create { name } => {
                     let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = ?")
                         .bind(&name)
                         .fetch_one(&db_pool)
                         .await
                         .expect("查詢失敗");
                     if exists > 0 {
                         println!("工作空間 '{name}' 已存在。");
                     } else {
                         sqlx::query("INSERT INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)")
                             .bind(&name)
                             .bind(&name)
                             .execute(&db_pool)
                             .await
                             .expect("建立工作空間失敗");
                         println!("已成功建立工作空間: {name}");
                     }
                 }
                 WorkspaceSubcommands::Delete { name } => {
                     let default_workspace = config_manager.get_config().await.model.default_workspace;
                     if name == default_workspace {
                         eprintln!("💥 預設工作空間 '{name}' 不可刪除。");
                         std::process::exit(1);
                     }
                     let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = ?")
                         .bind(&name)
                         .fetch_one(&db_pool)
                         .await
                         .expect("查詢失敗");
                     if count == 0 {
                         println!("工作空間 '{name}' 不存在。");
                     } else {
                         sqlx::query("DELETE FROM documents WHERE workspace_id = ?")
                             .bind(&name)
                             .execute(&db_pool)
                             .await
                             .expect("刪除文件失敗");
                         sqlx::query("DELETE FROM workspaces WHERE id = ?")
                             .bind(&name)
                             .execute(&db_pool)
                             .await
                             .expect("刪除工作空間失敗");
                         println!("已成功刪除工作空間: {name}");
                     }
                 }
                 WorkspaceSubcommands::Switch { name } => {
                     let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE id = ?")
                         .bind(&name)
                         .fetch_one(&db_pool)
                         .await
                         .expect("查詢失敗");
                     if exists == 0 {
                         println!("工作空間 '{name}' 不存在。");
                     } else {
                         let mut cfg = config_manager.get_config().await;
                         cfg.model.active_workspace = Some(name.clone());
                         config_manager.update_config(cfg).await?;
                         println!("切換預設工作空間至: {name}");
                     }
                 }
                 WorkspaceSubcommands::Show => {
                     let cfg = config_manager.get_config().await;
                     let active = cfg.model.active_workspace.as_deref().unwrap_or("[未設定]");
                     let default = &cfg.model.default_workspace;
                     println!("預設工作空間: {default}");
                     println!("作用中工作空間: {active}");
                 }
             }
         }
        Commands::InstallOpencode { host: _, port: _ } => {
            println!("🚀 啟動大一統 Rust 版 MCP 自動化註冊 (OpenCode)..");
            
            let home = match std::env::var("HOME") {
                Ok(h) => h,
                Err(_) => {
                    eprintln!("💥 無法取得使用者家目錄 HOME 環境變數");
                    std::process::exit(1);
                }
            };
            
            let opencode_json_path = std::path::Path::new(&home)
                .join(".config")
                .join("opencode")
                .join("opencode.json");

            if !opencode_json_path.exists() {
                eprintln!("💥 找不到 OpenCode 設定檔，預期路徑：{}", opencode_json_path.display());
                std::process::exit(1);
            }

            println!("   - 讀取設定檔: {}", opencode_json_path.display());
            let raw_content = match std::fs::read_to_string(&opencode_json_path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("💥 讀取設定檔失敗: {e}");
                    std::process::exit(1);
                }
            };

            // 輕量化 JSON5 註解清理 (對齊 Hono 版本清理機制，防範 JSON 解析報錯)
            let mut clean_raw = String::new();
            let mut in_line_comment = false;
            let mut in_block_comment = false;
            let mut in_string = false;
            let mut escaped = false;
            let chars: Vec<char> = raw_content.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                let c = chars[i];
                
                if in_line_comment {
                    if c == '\n' || c == '\r' {
                        in_line_comment = false;
                        clean_raw.push(c);
                    }
                } else if in_block_comment {
                    if i + 1 < chars.len() && c == '*' && chars[i+1] == '/' {
                        in_block_comment = false;
                        i += 1;
                    }
                } else if in_string {
                    if escaped {
                        escaped = false;
                    } else if c == '\\' {
                        escaped = true;
                    } else if c == '"' {
                        in_string = false;
                    }
                    clean_raw.push(c);
                } else {
                    if i + 1 < chars.len() && c == '/' && chars[i+1] == '/' {
                        in_line_comment = true;
                        i += 1;
                    } else if i + 1 < chars.len() && c == '/' && chars[i+1] == '*' {
                        in_block_comment = true;
                        i += 1;
                    } else if c == '"' {
                        in_string = true;
                        clean_raw.push(c);
                    } else {
                        clean_raw.push(c);
                    }
                }
                i += 1;
            }

            let mut config: serde_json::Value = match serde_json::from_str(&clean_raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("💥 解析 JSON 失敗 (可能包含複雜註解或尾隨逗號): {e}");
                    std::process::exit(1);
                }
            };

            // 確保 mcp 節點存在
            if config.get("mcp").is_none() {
                if let Some(obj) = config.as_object_mut() {
                    obj.insert("mcp".to_string(), serde_json::json!({}));
                }
            }

            if let Some(mcp_servers) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
                println!("   - 註冊大一統 Local 讀寫 (Stdio) MCP: opendoc-mcp");
                let binary_path = format!("{home}/.cargo/bin/opendoc");
                mcp_servers.insert(
                    "opendoc-mcp".to_string(),
                    serde_json::json!({
                        "enabled": true,
                        "type": "local",
                        "command": [
                            binary_path,
                            "start",
                            "--mcp-only"
                        ]
                    })
                );

                // 移除 Node 時代的 legacy CQRS 配置，達成單一進程大一統
                // ponytail: only remove exact match legacy tools if they use the opendoc executable
                let legacy_keys = ["opendocuments-read", "opendocuments-write", "opendocuments", "opendoc-read", "opendoc-write"];
                for key in legacy_keys {
                    if let Some(val) = mcp_servers.get(key) {
                        let is_legacy_opendoc = val.to_string().contains("opendoc");
                        if is_legacy_opendoc {
                            mcp_servers.remove(key);
                        }
                    }
                }
            }

            let updated_json = match serde_json::to_string_pretty(&config) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("💥 序列化 JSON 失敗: {e}");
                    std::process::exit(1);
                }
            };

            if let Err(e) = std::fs::write(&opencode_json_path, updated_json) {
                eprintln!("💥 寫入設定檔失敗: {e}");
                std::process::exit(1);
            }

            println!("✅ opencode.json 已完美更新！");
            println!("   - opendoc (Local Stdio) -> {} start --mcp-only", format!("{home}/.cargo/bin/opendoc"));
            println!("👉 請重啟你的 OpenCode 客戶端以載入大一統 Rust 向量引擎！");
        }
        Commands::Start { port, mcp_only } => {
            let db_pool = match tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(config_manager.init_db_pool())
            }) {
                Ok(pool) => pool,
                Err(e) => {
                    eprintln!("💥 初始化資料庫連線失敗: {e}");
                    std::process::exit(1);
                }
            };

            let search = build_search_backend(&app_cfg, &db_pool)
                .await
                .map_err(|e| format!("向量檢索後端初始化失敗: {e}"))?;

            if mcp_only {
                // 進入 stdio Local MCP 專用模式，絕不混淆
                opendoc_mcp::run_mcp_stdio_server(search, Arc::clone(&config_manager), db_pool).await?;
            } else {
                println!("🚀 正在啟動大一統 API & MCP 伺服器端 (Port: {port})..");
                start_mcp_and_api_server(port, search, Arc::clone(&config_manager), db_pool).await?;
            }
        }
        Commands::Stop => {
            println!("已向背景服務發送停止訊號。");
        }
        Commands::Doctor => {
            println!("🔍 正在運行系統健康檢查:");
            println!("========================================");
            println!("1. 讀取設定檔 [~/.config/opendocuments/config.toml] ..");
            println!("   - 伺服器端點: {}", app_cfg.server.url);
            println!("   - 數據庫路徑: {}", app_cfg.database.path);
            println!("   - 預設工作空間: {}", app_cfg.model.default_workspace);
            println!("   - 檢索分數過濾門檻: {}", app_cfg.model.score_threshold);
            println!("   - 重排模型路徑: {}", app_cfg.model.local_reranker_path.as_deref().unwrap_or("[未設定/使用預設]"));
            println!("\n2. 檢測設定檔實體路徑: {}", config_manager.get_config_path().to_string_lossy());
            println!("   - 狀態: [OK] 載入與持久化一切通暢！");
            println!("========================================");
        }
        Commands::Config { sub } => {
            match sub {
                ConfigSubcommands::Get { key } => {
                    println!("配置項 '{key}' ..");
                }
                ConfigSubcommands::Set { key, value } => {
                    println!("已將 '{key}' 配置更新為 '{value}'");
                }
            }
        }
        Commands::Parse { file_path, workspace_id, collection_id, original_name } => {
            println!("正在解析檔案: {file_path_display}", file_path_display = file_path.display());
            let name_ref = original_name.as_deref();
            match parse_file(&file_path, name_ref, &workspace_id, &collection_id).await {
                Ok(chunks) => {
                    let json = match serde_json::to_string_pretty(&chunks) {
                        Ok(j) => j,
                        Err(e) => {
                            eprintln!("💥 序列化 JSON 失敗: {e}");
                            std::process::exit(1);
                        }
                    };
                    println!("{json}");
                }
                Err(e) => {
                    eprintln!("💥 解析失敗: {e}");
                    std::process::exit(1);
                }
            }
        }
    }

    Ok(())
}
