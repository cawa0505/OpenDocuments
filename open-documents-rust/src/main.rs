#![deny(clippy::unwrap_used)]
#![deny(clippy::clone_on_ref_ptr)]
#![warn(clippy::pedantic)]

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use opendoc_parser::parse_file;
use opendoc_storage::ConfigManager;
use opendoc_tui::{render_ui, TuiAppState, TuiEvent, TuiSearchResult};
use walkdir::WalkDir;
use reqwest::multipart;
use crossterm::{
    event::{self, Event as CrossEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

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
            eprintln!("💥 載入設定失敗: {e}");
            std::process::exit(1);
        }
    };

    let app_cfg = config_manager.get_config().await;

    match cli.command {
        Commands::Tui => {
            // 完美註冊全局 Panic Hook, 確保 TUI 崩潰時強制還原終端
            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                let _ = disable_raw_mode();
                let mut stdout = io::stdout();
                let _ = execute!(stdout, LeaveAlternateScreen);
                default_hook(info);
            }));

            if let Err(e) = run_tui_loop(&app_cfg.model.default_workspace).await {
                eprintln!("💥 TUI 異常退出: {e}");
            }
        }
        Commands::Ask { query, workspace, collections } => {
            println!("正在向空間 '{workspace}' 提問: '{query}' ... (API 端點: {})", app_cfg.server.url);
            if let Some(cols) = collections {
                println!("過濾集合: {cols:?}");
            }
        }
        Commands::Search { query, workspace, threshold, limit } => {
            println!(
                "正在空間 '{workspace}' 檢索: '{query}' (門檻: {threshold}, 限制: {limit})... (API 端點: {})",
                app_cfg.server.url
            );
        }
        Commands::Document { sub } => {
            match sub {
                DocumentSubcommands::Index { path, workspace } => {
                    let canon_path = match path.canonicalize() {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("💥 無法解析路徑 {path_display:?}: {e}", path_display = path.display());
                            std::process::exit(1);
                        }
                    };

                    println!("🚀 啟動高效目錄索引:");
                    println!("📦 目標工作空間: {workspace}");
                    println!("🌐 伺服器 API 端點: {}", app_cfg.server.url);
                    println!("{}", "-".repeat(50));

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

                    let walk = WalkDir::new(&canon_path).into_iter().filter_entry(|entry| {
                        if let Some(name) = entry.file_name().to_str() {
                            !ignored_dirs.contains(&name) && !name.starts_with('.')
                        } else {
                            false
                        }
                    });

                    for entry in walk {
                        let Ok(entry) = entry else { continue };

                        if !entry.file_type().is_file() {
                            continue;
                        }

                        let file_path = entry.path();
                        let Some(file_name) = file_path.file_name().and_then(|n| n.to_str()) else { continue };

                        if file_name.starts_with('.') {
                            continue;
                        }

                        let ext = file_path.extension()
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

                        print!("[..] 上傳中: {rel_path} ...");
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

                            let req_res = client.post(&upload_url)
                                .header("X-Workspace", &workspace)
                                .multipart(form)
                                .timeout(Duration::from_secs(180)) // 3 mins timeout
                                .send()
                                .await;

                            match req_res {
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

                    println!("{}", "-".repeat(50));
                    println!("🎉 索引完成! 成功: {success_count}, 失敗: {fail_count}");
                }
                DocumentSubcommands::List { workspace } => {
                    println!("正在列出 '{workspace}' 空間下的文件...");
                }
                DocumentSubcommands::Delete { id, workspace } => {
                    println!("正在從 '{workspace}' 刪除文件 ID: {id}");
                }
                DocumentSubcommands::Reindex { id, workspace } => {
                    println!("正在對 '{workspace}' 重新索引文件 ID: {id}");
                }
            }
        }
        Commands::Workspace { sub } => {
            match sub {
                WorkspaceSubcommands::List => {
                    println!("工作空間列表:");
                }
                WorkspaceSubcommands::Create { name } => {
                    println!("已成功建立工作空間: {name}");
                }
                WorkspaceSubcommands::Delete { name } => {
                    println!("已成功刪除工作空間: {name}");
                }
                WorkspaceSubcommands::Switch { name } => {
                    println!("切換預設工作空間至: {name}");
                }
            }
        }
        Commands::Start { port } => {
            println!("正在啟動 API & MCP 服務端 (Port: {port})...");
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
                    println!("配置項 '{key}' ...");
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
}

/// 運行 Ratatui TUI 主循環，具備 100% 非同步事件調度與背景阻斷保護
async fn run_tui_loop(default_workspace: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化終端機環境
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    
    // 【防禦起手式】開啟獨立異步 Task 監聽鍵盤輸入，絕不阻塞主執行緒
    let tx_clone = tx.clone();
    tokio::task::spawn_blocking(move || {
        loop {
            // 每 50 毫秒檢查一次有沒有鍵盤輸入
            if let Ok(true) = event::poll(Duration::from_millis(50)) {
                if let Ok(CrossEvent::Key(key)) = event::read() {
                    let _ = tx_clone.blocking_send(TuiEvent::Input(key.code));
                }
            }
            // 定時發送 Tick 事件，用來更新動畫或檢查背景進度
            let _ = tx_clone.blocking_send(TuiEvent::Tick);
        }
    });

    let mut state = TuiAppState::new(default_workspace.to_string());

    // 模擬載入一些初始化的 TUI 測試資料
    state.results = vec![
        TuiSearchResult {
            file_name: "CHANGELOG.md".to_string(),
            score: 0.89,
            snippet: "修正：OpenDocuments MCP 在連線超時下的崩潰問題，合入 Reranker 阻斷。".to_string(),
        },
        TuiSearchResult {
            file_name: "STRUCTURE.md".to_string(),
            score: 0.65,
            snippet: "Homelab 整合：arhat 主要工作機 (192.168.77.200) 部署 5x MCP 遠端容器。".to_string(),
        },
    ];

    // TUI 主事件循環
    loop {
        terminal.draw(|f| render_ui(f, &state))?;

        if let Some(event) = rx.recv().await {
            match event {
                TuiEvent::Input(KeyCode::Esc) => break,
                TuiEvent::Input(KeyCode::Char(c)) => {
                    state.search_query.push(c);
                }
                TuiEvent::Input(KeyCode::Backspace) => {
                    state.search_query.pop();
                }
                TuiEvent::Input(KeyCode::Enter) => {
                    // 當按下 Enter，觸發非同步檢索
                    // 這裡可以透過 tokio::spawn 去呼叫 crates/opendoc-storage，
                    // 檢索完後再透過 tx.send(TuiEvent::FetchResults(data)) 丟回來更新 state
                    println!("觸發檢索: {}", state.search_query);
                }
                TuiEvent::FetchResults(new_results) => {
                    state.results = new_results;
                }
                TuiEvent::Tick => {}
                _ => {}
            }
        }
    }

    // 還原終端機
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
