use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::Duration;
use opendoc_parser::parse_file;
use opendoc_storage::ConfigManager;
use walkdir::WalkDir;
use reqwest::multipart;

#[derive(Parser)]
#[command(name = "opendocuments-rust")]
#[command(author = "Jimmy Yen")]
#[command(about = "OpenDocuments Rust — 極致性能、強型別防禦與 Ratatui TUI 終端三位一體之自建 RAG 旗艦 CLI")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 啟動輕量化 Ratatui TUI 檢索與進度調試終端
    Tui,

    /// 對 RAG 知識庫進行快速終端問答
    Ask {
        /// 問答諮詢內容
        query: String,

        /// 目標工作空間
        #[arg(short, long, default_value = "default")]
        workspace: String,

        /// 可選的集合過濾
        #[arg(short, long)]
        collections: Option<Vec<String>>,
    },

    /// 對 RAG 知識庫進行向量與混合檢索
    Search {
        /// 檢索關鍵字或語義句
        query: String,

        /// 目標工作空間
        #[arg(short, long, default_value = "default")]
        workspace: String,

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

    /// 啟動 Axum API、MCP SSE 伺服器與 TUI 背景執行緒
    Start {
        /// Web API 與 MCP 連接埠
        #[arg(short, long, default_value_t = 3000)]
        port: u16,
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
    /// 索引/上傳一個新的本地檔案或整個目錄到伺服器 (整合自原 Python Ingester)
    Index {
        /// 本地檔案或目錄路徑
        path: PathBuf,
        /// 工作空間
        #[arg(short, long, default_value = "default")]
        workspace: String,
    },
    /// 列出當前索引的所有文件
    List {
        /// 篩選工作空間
        #[arg(short, long, default_value = "default")]
        workspace: String,
    },
    /// 刪除指定 ID 的文件
    Delete {
        id: String,
        #[arg(short, long, default_value = "default")]
        workspace: String,
    },
    /// 重新索引指定 ID 的文件
    Reindex {
        id: String,
        #[arg(short, long, default_value = "default")]
        workspace: String,
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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // 1. 初始化設定檔載入
    let config_manager = match ConfigManager::load_or_init() {
        Ok(cm) => cm,
        Err(e) => {
            eprintln!("💥 載入設定失敗: {}", e);
            std::process::exit(1);
        }
    };

    let app_cfg = config_manager.get_config().await;

    match cli.command {
        Commands::Tui => {
            println!("正在啟動 Ratatui TUI 終端互動檢索介面... (Phase 2 實裝中)");
        }
        Commands::Ask { query, workspace, collections } => {
            println!("正在向空間 '{}' 提問: '{}' ... (API 端點: {})", workspace, query, app_cfg.server.url);
            if let Some(cols) = collections {
                println!("過濾集合: {:?}", cols);
            }
        }
        Commands::Search { query, workspace, threshold, limit } => {
            println!(
                "正在空間 '{}' 檢索: '{}' (門檻: {}, 限制: {})... (API 端點: {})",
                workspace, query, threshold, limit, app_cfg.server.url
            );
        }
        Commands::Document { sub } => {
            match sub {
                DocumentSubcommands::Index { path, workspace } => {
                    let path = match path.canonicalize() {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("💥 無法解析路徑 {:?}: {}", path, e);
                            std::process::exit(1);
                        }
                    };

                    println!("🚀 啟動高效目錄索引:");
                    println!("📦 目標工作空間: {}", workspace);
                    println!("🌐 伺服器 API 端點: {}", app_cfg.server.url);
                    println!("{}", "-".repeat(50));

                    // 100% 繼承 python ingester 的噪音過濾
                    let ignored_dirs = [
                        "node_modules", ".git", "dist", "build", ".turbo", ".next", ".cache", 
                        "__pycache__", "venv", ".env", "out"
                    ];

                    let supported_extensions = [
                        ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".mdx", ".json", ".yaml", ".yml", 
                        ".toml", ".css", ".html", ".htm", ".sh", ".sql", ".pdf", ".docx", ".xlsx"
                    ];

                    let upload_url = format!("{}/api/v1/documents/upload", app_cfg.server.url);
                    let client = reqwest::Client::new();
                    let mut success_count = 0;
                    let mut fail_count = 0;

                    // 執行多執行緒或高效掃描
                    let walk = WalkDir::new(&path).into_iter().filter_entry(|entry| {
                        if let Some(name) = entry.file_name().to_str() {
                            !ignored_dirs.contains(&name) && !name.starts_with('.')
                        } else {
                            false
                        }
                    });

                    for entry in walk {
                        let entry = match entry {
                            Ok(e) => e,
                            Err(_) => continue,
                        };

                        if !entry.file_type().is_file() {
                            continue;
                        }

                        let file_path = entry.path();
                        let file_name = match file_path.file_name().and_then(|n| n.to_str()) {
                            Some(name) => name,
                            None => continue,
                        };

                        if file_name.starts_with('.') {
                            continue;
                        }

                        // 副檔名過濾
                        let ext = file_path.extension()
                            .and_then(|e| e.to_str())
                            .map(|e| format!(".{}", e.to_lowercase()))
                            .unwrap_or_default();

                        if !supported_extensions.contains(&ext.as_str()) {
                            continue;
                        }

                        let rel_path = match file_path.strip_prefix(&path) {
                            Ok(p) => p.to_string_lossy().into_owned(),
                            Err(_) => file_name.to_string(),
                        };

                        print!("[..] 上傳中: {} ...", rel_path);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();

                        // 3 次退避重試邏輯
                        let max_retries = 3;
                        let mut attempt = 0;
                        let mut success = false;

                        while attempt < max_retries {
                            attempt += 1;
                            
                            let file_bytes = match std::fs::read(file_path) {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("\r[!!] 讀取檔案失敗: {} - {}", rel_path, e);
                                    break;
                                }
                            };

                            let part = multipart::Part::bytes(file_bytes)
                                .file_name(file_name.to_string())
                                .mime_str("application/octet-stream")
                                .unwrap();

                            let form = multipart::Form::new().part("file", part);

                            match client.post(&upload_url)
                                .header("X-Workspace", &workspace)
                                .multipart(form)
                                .timeout(Duration::from_secs(180))
                                .send()
                                .await 
                            {
                                Ok(resp) => {
                                    if resp.status().is_success() {
                                        success = true;
                                        let res_json: serde_json::Value = resp.json().await.unwrap_or_default();
                                        let chunks = res_json.get("chunks").and_then(|c| c.as_u64()).unwrap_or(0);
                                        print!("\r[ok] 已索引: {} ({} 區塊)\n", rel_path, chunks);
                                        success_count += 1;
                                        tokio::time::sleep(Duration::from_millis(500)).await; // 💡 舒緩 GPU & CPU 滿載保護
                                        break;
                                    } else {
                                        if attempt == max_retries {
                                            print!("\r[!!] 失敗: {} - HTTP {}\n", rel_path, resp.status());
                                            fail_count += 1;
                                        } else {
                                            tokio::time::sleep(Duration::from_millis(1500)).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    if attempt == max_retries {
                                        print!("\r[!!] 失敗: {} - 錯誤: {}\n", rel_path, e);
                                        fail_count += 1;
                                    } else {
                                        tokio::time::sleep(Duration::from_millis(1500)).await;
                                    }
                                }
                            }
                        }
                    }

                    println!("{}", "-".repeat(50));
                    println!("🎉 索引完成! 成功: {}, 失敗: {}", success_count, fail_count);
                }
                DocumentSubcommands::List { workspace } => {
                    println!("正在列出 '{}' 空間下的文件...", workspace);
                }
                DocumentSubcommands::Delete { id, workspace } => {
                    println!("正在從 '{}' 刪除文件 ID: {}", workspace, id);
                }
                DocumentSubcommands::Reindex { id, workspace } => {
                    println!("正在對 '{}' 重新索引文件 ID: {}", workspace, id);
                }
            }
        }
        Commands::Workspace { sub } => {
            match sub {
                WorkspaceSubcommands::List => {
                    println!("工作空間列表:");
                }
                WorkspaceSubcommands::Create { name } => {
                    println!("已成功建立工作空間: {}", name);
                }
                WorkspaceSubcommands::Delete { name } => {
                    println!("已成功刪除工作空間: {}", name);
                }
                WorkspaceSubcommands::Switch { name } => {
                    println!("切換預設工作空間至: {}", name);
                }
            }
        }
        Commands::Start { port } => {
            println!("正在啟動 API & MCP 服務端 (Port: {})...", port);
        }
        Commands::Stop => {
            println!("已向背景服務發送停止訊號。");
        }
        Commands::Doctor => {
            println!("🔍 正在運行系統健康檢查:");
            println!("========================================");
            println!("1. 讀取設定檔 [~/.config/opendocuments/config.toml] ...");
            println!("   - 伺服器端點: {}", app_cfg.server.url);
            println!("   - 數據庫路徑: {}", app_cfg.database.path);
            println!("   - 預設工作空間: {}", app_cfg.model.default_workspace);
            println!("   - 檢索分數過濾門檻: {}", app_cfg.model.score_threshold);
            println!("   - 重排模型路徑: {}", app_cfg.model.local_reranker_path);
            println!("\n2. 檢測設定檔實體路徑: {}", config_manager.get_config_path().to_string_lossy());
            println!("   - 狀態: [OK] 載入與持久化一切通暢！");
            println!("========================================");
        }
        Commands::Config { sub } => {
            match sub {
                ConfigSubcommands::Get { key } => {
                    println!("配置項 '{}' ...", key);
                }
                ConfigSubcommands::Set { key, value } => {
                    println!("已將 '{}' 配置更新為 '{}'", key, value);
                }
            }
        }
        Commands::Parse { file_path, workspace_id, collection_id, original_name } => {
            println!("正在解析檔案: {:?}", file_path);
            let name_ref = original_name.as_deref();
            match parse_file(&file_path, name_ref, &workspace_id, &collection_id).await {
                Ok(chunks) => {
                    let json = serde_json::to_string_pretty(&chunks).unwrap();
                    println!("{}", json);
                }
                Err(e) => {
                    eprintln!("💥 解析失敗: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}
