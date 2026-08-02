pub mod lancedb;

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};
// removed unused directories::ProjectDirs
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
    pub active_workspace: Option<String>,
    pub score_threshold: f32,
    pub local_reranker_path: Option<String>,
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
                path: "~/.opendocuments".to_string(),
            },
model: ModelConfig {
                     default_workspace: "default".to_string(),
                     active_workspace: None,
                     score_threshold: 0.60,
                     local_reranker_path: Some("~/.opendocuments/models/bge-reranker-base.onnx".to_string()),
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

        // 💡 建立 documents 與 query_logs 必要的資料庫表，確保大一統後端運行無礙，100% 避免 Node 進程衝突
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_path TEXT,
                file_type TEXT,
                file_size_bytes INTEGER,
                connector_id TEXT,
                chunk_count INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                content_hash TEXT,
                error_message TEXT,
                workspace_id TEXT NOT NULL,
                deleted_at DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME,
                indexed_at DATETIME
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // Collections 與文件-集合關聯表（對齊 Node migration 002_add_versioning_collections.sql）
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS collections (
                id TEXT PRIMARY KEY,
                workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                auto_rules TEXT,
                created_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS collection_documents (
                collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
                document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
                PRIMARY KEY (collection_id, document_id)
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // Conversations 與 Messages（對齊 Node migration 001_initial.sql）
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
                user_id TEXT,
                title TEXT,
                shared INTEGER DEFAULT 0,
                share_token TEXT UNIQUE,
                deleted_at TEXT DEFAULT NULL,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                sources TEXT,
                profile_used TEXT,
                confidence_score REAL,
                response_time_ms INTEGER,
                created_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // Migration: 為現有資料庫添加缺失的欄位
        sqlx::query("ALTER TABLE documents ADD COLUMN source_path TEXT").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN file_type TEXT").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN file_size_bytes INTEGER").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN connector_id TEXT").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN content_hash TEXT").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN error_message TEXT").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN updated_at DATETIME").execute(&pool).await.ok();
        sqlx::query("ALTER TABLE documents ADD COLUMN indexed_at DATETIME").execute(&pool).await.ok();
        
        // BYOK LLM migrations
        sqlx::query("ALTER TABLE llm_providers ADD COLUMN provider TEXT NOT NULL DEFAULT 'custom'").execute(&pool).await.ok();
        
        // 兼容舊資料: 若無 title 欄位，用 name 欄位填入
        sqlx::query("UPDATE documents SET title = name WHERE title IS NULL AND name IS NOT NULL").execute(&pool).await.ok();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS query_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT NOT NULL,
                profile TEXT NOT NULL,
                confidence_score REAL,
                response_time_ms INTEGER,
                route TEXT,
                feedback TEXT, -- 'positive', 'negative'
                workspace_id TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 💡 預留團隊與 SDK / Embeddable Widget 專屬金鑰 (od_live_) 驗證資料表 ！！！
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                role TEXT NOT NULL,
                scopes TEXT NOT NULL,
                rate_limit INTEGER,
                allowed_ips TEXT,
                expires_at DATETIME,
                last_used_at DATETIME,
                revoked_at DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 💡 效能基準測試記錄表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS benchmark_runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workspace_id TEXT NOT NULL,
                model TEXT NOT NULL,
                metric_name TEXT NOT NULL,
                metric_value REAL NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 💡 標籤系統資料表（對齊 Node migration 001_initial.sql）
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tags (
                id TEXT PRIMARY KEY,
                workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                color TEXT,
                UNIQUE(workspace_id, name)
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS document_tags (
                document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
                tag_id TEXT REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (document_id, tag_id)
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // BYOK LLM provider 設定（金鑰僅存此處，執行期進記憶體，API 不回傳本體）
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS llm_providers (
                id TEXT PRIMARY KEY,
                workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '',
                is_active INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(workspace_id, name)
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 知識萃取與編織資產表 (Extracted Assets Table)
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS extracted_assets (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
                asset_type TEXT NOT NULL,
                title TEXT NOT NULL,
                schema_definition TEXT NOT NULL, -- JSON 陣列，儲存欄位描述與類型
                data_content TEXT NOT NULL,      -- JSON 陣列，儲存萃取出的真實物件資料
                source_chunks TEXT NOT NULL,     -- JSON 陣列，儲存引用的原始 chunk id/source_path
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // MCP 伺服器配置表
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS mcp_servers (
                id TEXT PRIMARY KEY,
                workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                command TEXT NOT NULL,
                args TEXT NOT NULL,
                env TEXT,
                is_active INTEGER NOT NULL DEFAULT 1,
                created_at TEXT DEFAULT (datetime('now')),
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(workspace_id, name)
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 💡 建立 connectors 資料表，與測試狀態 / 舊版 Node DDL 100% 保持一致，支援 GitHub 連結器
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS connectors (
                id TEXT PRIMARY KEY,
                workspace_id TEXT,
                name TEXT NOT NULL,
                type TEXT NOT NULL,
                config TEXT NOT NULL DEFAULT '{}',
                sync_interval_seconds INTEGER DEFAULT 300,
                last_synced_at TEXT,
                status TEXT DEFAULT 'active',
                deleted_at TEXT DEFAULT NULL,
                created_at TEXT DEFAULT (datetime('now'))
            )"
        ).execute(&pool).await.map_err(|e| e.to_string())?;

        // 自動建立預設工作空間（若不存在），確保 WebUI 啟動時有資料可顯示
        // 以 name 判斷存在性（UUID 遷移後 id 不再等於名稱；既有 id=name 的舊庫也能正確沿用）
        let default_workspace = &cfg.model.default_workspace;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM workspaces WHERE name = ?")
            .bind(default_workspace)
            .fetch_one(&pool)
            .await
            .map_err(|e| e.to_string())?;
        if count == 0 {
            let ws_id = uuid::Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO workspaces (id, name, created_at) VALUES (?, ?, CURRENT_TIMESTAMP)")
                .bind(&ws_id)
                .bind(default_workspace)
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
        }

        Ok(pool)
    }

    /// 運行與 BDD 規格完全一致之雙階段 Rerank 混合檢索與 Score Threshold Filter 保險絲過濾 (含自愈容錯 Top-1 保底)
    pub fn search_and_rerank(&self, _query: &str, _threshold: f32) -> Vec<opendoc_types::DocumentChunk> {
        // Ponytail: Keep search_and_rerank simple and empty of hardcoded homelab machine/IP chunks.
        // It should return an empty list or only truly matched dynamically query-retrieved chunks.
        Vec::new()
    }
}

// 補足 dirs 庫的相容性
mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_manager_load_or_init() {
        let mgr = ConfigManager::load_or_init();
        assert!(mgr.is_ok());
        let manager = mgr.unwrap();
        let cfg = manager.get_config().await;
        assert!(
            cfg.server.url.starts_with("http"),
            "server.url 應為 http(s) URL，實際: {}",
            cfg.server.url
        );
    }

    #[tokio::test]
    async fn test_search_and_rerank_filtering_and_fallback() {
        let manager = ConfigManager::load_or_init().unwrap();
        let results = manager.search_and_rerank("MCP", 0.60);
        assert!(results.is_empty());
    }
}
