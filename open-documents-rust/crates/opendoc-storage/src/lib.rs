use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
use directories::ProjectDirs;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};

/// 遠端伺服器設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub url: String,
    pub api_key: String,
}

/// 數據庫配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

/// 模型與檢索設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub default_workspace: String,
    pub score_threshold: f32,
    pub local_reranker_path: String,
}

/// ~/.config/opendocuments/config.toml 對應的強型別結構
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub model: ModelConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                url: "http://127.0.0.1:3000".to_string(),
                api_key: "".to_string(),
            },
            database: DatabaseConfig {
                path: "~/.config/opendocuments/data".to_string(),
            },
            model: ModelConfig {
                default_workspace: "GraphifyOpt".to_string(),
                score_threshold: 0.60,
                local_reranker_path: "~/.config/opendocuments/models/bge-reranker-base.onnx".to_string(),
            },
        }
    }
}

/// 系統設定管理器 (管理 config.toml 載入、持久化與記憶體快取)
pub struct ConfigManager {
    config_path: PathBuf,
    cache: Arc<RwLock<AppConfig>>,
}

impl ConfigManager {
    /// 載入或初始化 ~/.config/opendocuments/config.toml
    pub fn load_or_init() -> Result<Self, String> {
        let home_dir = dirs::home_dir().ok_or("無法獲取使用者家目錄")?;
        let config_dir = home_dir.join(".config").join("opendocuments");
        
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir).map_err(|e| format!("無法建立設定目錄: {}", e))?;
        }
        
        let config_path = config_dir.join("config.toml");
        let initial_config = if config_path.exists() {
            let content = fs::read_to_string(&config_path).map_err(|e| format!("無法讀取設定檔: {}", e))?;
            toml::from_str(&content).map_err(|e| format!("設定檔格式錯誤: {}", e))?
        } else {
            let default_cfg = AppConfig::default();
            let content = toml::to_string_pretty(&default_cfg).map_err(|e| format!("無法序列化設定檔: {}", e))?;
            fs::write(&config_path, content).map_err(|e| format!("無法寫入預設設定檔: {}", e))?;
            default_cfg
        };

        Ok(Self {
            config_path,
            cache: Arc::new(RwLock::new(initial_config)),
        })
    }

    /// 獲取當前設定 (直讀記憶體快取，0 延遲)
    pub async fn get_config(&self) -> AppConfig {
        self.cache.read().await.clone()
    }

    /// 獲取設定檔實體路徑
    pub fn get_config_path(&self) -> &Path {
        &self.config_path
    }

    /// 儲存並同步更新記憶體與 config.toml 檔案
    pub async fn update_config(&self, new_cfg: AppConfig) -> Result<(), String> {
        let mut cache_guard = self.cache.write().await;
        *cache_guard = new_cfg.clone();

        let content = toml::to_string_pretty(&new_cfg).map_err(|e| format!("無法序列化設定檔: {}", e))?;
        fs::write(&self.config_path, content).map_err(|e| format!("無法寫入設定檔: {}", e))?;
        Ok(())
    }

    /// 初始化 SQLite 數據庫連線池 (WAL 與快取優化)
    pub async fn init_db_pool(&self) -> Result<SqlitePool, String> {
        let cfg = self.get_config().await;
        let raw_path = cfg.database.path;
        
        // 解析 ~ 符號為實體路徑
        let home_dir = dirs::home_dir().ok_or("無法獲取使用者家目錄")?;
        let db_dir_str = raw_path.replace("~", &home_dir.to_string_lossy());
        let db_dir = PathBuf::from(&db_dir_str);
        
        if !db_dir.exists() {
            fs::create_dir_all(&db_dir).map_err(|e| format!("無法建立資料庫目錄: {}", e))?;
        }
        
        let db_path = db_dir.join("opendocuments.db");
        let db_url = format!("sqlite://{}", db_path.to_string_lossy());

        let connection_options = SqliteConnectOptions::from_str(&db_url)
            .map_err(|e| e.to_string())?
            .create_if_missing(true)
            .pragma("journal_mode", "WAL")
            .pragma("synchronous", "NORMAL")
            .pragma("busy_timeout", "5000")
            .pragma("cache_size", "-2000");

        let pool = SqlitePool::connect_with(connection_options)
            .await
            .map_err(|e| e.to_string())?;

        // 初始化基本資料表 (多租戶與隔離)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        Ok(pool)
    }
}

// 補足 dirs 庫的相容性
mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}
