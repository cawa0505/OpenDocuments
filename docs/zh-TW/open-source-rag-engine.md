# 開源 RAG 引擎 (Data Plane)

[English](../en/open-source-rag-engine.md) | [繁體中文](open-source-rag-engine.md)

---

## 1. 執行摘要

開源 RAG 引擎提供純粹、高效能的 Data Plane（資料平面），專為檢索增強生成 (RAG) 設計。本引擎基於 LanceDB 與 Apache Arrow 建立，提供亞毫秒級的向量相似度搜尋，並結合用戶端驅動的 SQL 風格預過濾 (Pre-filtering) 能力。與整合 Control Plane（控制平面，包含 OAuth/SSO 身份驗證、零信任 RBAC 權限控管與集中式策略閘道）的企業版不同，開源 Data Plane 專注於高效向量儲存、索引與確定性過濾。本引擎專為獨立開發者、本地 AI Agent 與開源社群設計，提供輕量、零依賴的高密度文件檢索基礎，亦作為企業版 Control Plane 管理功能的底層執行核心。

---

## 2. 技術架構

### 核心組件

- **`VectorSearchQuery`**：資料結構，封裝密集查詢向量 (query vector)、結果數量限制 (limit) 與選填的 SQL 風格預過濾字串 (filter)。
- **`LanceDbEngine`**：Rust 核心執行引擎，管理 LanceDB 實體表連線、Apache Arrow 記錄批次 (RecordBatch) 轉換與向量距離計算。
- **`open_source_rag_search`**：公開的 Tauri IPC 命令處理常式，直接向前端應用程式與桌面 UI 橋接器提供高速向量檢索。
- **`LanceDbEngineWrapper`**：執行緒安全的狀態容器，以 `std::sync::Mutex` 或 `tokio::sync::Mutex` 包裹 `LanceDbEngine`，實現跨非同步任務的生命週期管理。

### 資料流

```
┌───────────────────────────┐
│        用戶端層           │
│  (查詢向量 + 過濾條件)    │
└─────────────┬─────────────┘
              │ 1. IPC / API 請求
              ▼
┌───────────────────────────┐
│   Tauri IPC / API Bridge  │
│ (open_source_rag_search)  │
└─────────────┬─────────────┘
              │ 2. 解包 VectorSearchQuery
              ▼
┌───────────────────────────┐
│     LanceDbEngine         │
│  向量搜尋 + SQL 預過濾    │
└─────────────┬─────────────┘
              │ 3. 執行 Arrow/LanceDB 掃描
              ▼
┌───────────────────────────┐
│   Vec<RAGChunkResult>     │
│  (結構化區塊 + 相關度分數)│
└───────────────────────────┘
```

1. **查詢準備**：用戶端生成查詢 Embedding 向量 (`Vec<f32>`)，並指定選填的 SQL 預過濾條件（例如：`department = 'HR' AND min_security_level <= 2`）。
2. **搜尋執行**：`LanceDbEngine` 在 LanceDB Apache Arrow 資料集的向量索引巡覽過程中套用預過濾斷言。
3. **結果結構化**：匹配的記錄批次轉換為 `Vec<RAGChunkResult>`，包含標準化相似度分數與來源詮釋資料片段。

### 整合點

- **Tauri IPC Bridge**：於 `src-tauri/src/commands/lancedb.rs` 中以 `#[tauri::command]` 暴露。
- **RAG 子系統相容性**：與現有 Embedding 流程及文件分塊解析器即插即用相容。

---

## 3. API 文件

### 開源 Tauri 命令簽名

```rust
#[tauri::command]
pub async fn open_source_rag_search(
    app: tauri::AppHandle,
    query_vector: Vec<f32>,
    top_k: usize,
    filter: Option<String>,
) -> Result<Vec<RAGChunkResult>, String>
```

#### 參數
- `app`：`tauri::AppHandle` — 應用程式上下文控制代碼，用於存取受管狀態 (`LanceDbEngineWrapper`)。
- `query_vector`：`Vec<f32>` — 輸入查詢的密集向量表示。
- `top_k`：`usize` — 回傳的最大近鄰候選數量。
- `filter`：`Option<String>` — 於向量巡覽前或巡覽期間執行的選填 SQL 風格字串過濾條件。

#### 回傳值
- `Ok(Vec<RAGChunkResult>)`：按相關度降序排列的匹配文件區塊陣列。
- `Err(String)`：若索引查詢或 SQL 過濾條件解析失敗則回傳錯誤說明。

### 支援的過濾語法

預過濾解析器針對 Arrow 欄位詮釋資料評估標準 SQL 風格運算式：

- **簡單相等**：`column = 'value'`
- **數值比較**：`column <= value`、`column > value`、`column = value`
- **邏輯運算子**：`AND`、`OR`
- **支援的詮釋資料欄位**：
  - `department` (String)
  - `min_security_level` (Integer / u8)
  - `score` (Float / f32)

#### 過濾查詢範例

```sql
department = 'HR' AND min_security_level <= 2
department = 'Engineering' OR score > 0.85
min_security_level = 1 AND department = 'Finance'
```

---

## 4. 資料結構

### `VectorSearchQuery`

```rust
pub struct VectorSearchQuery {
    pub query_vector: Vec<f32>,
    pub limit: usize,
    pub filter: Option<String>,
}
```

### `RAGChunkResult`

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RAGChunkResult {
    pub file_name: String,
    pub file_path: String,
    pub content_snippet: String,
    pub score: f32,
    pub department: Option<String>,
    pub allowed_roles: Vec<String>,
    pub allowed_users: Vec<String>,
    pub min_security_level: Option<u8>,
}
```

---

## 5. 安全模型

### 開源版 (僅 Data Plane)

- **無身份驗證執行**：引擎純粹作為資料處理管道運作，無內建身份驗證或 Token 檢查。
- **用戶端控制過濾**：預過濾條件直接由用戶端應用程式建構並提供。
- **無狀態與零憑證管理**：引擎不持久化亦不評估使用者憑證、工作階段 Token 或策略表。

> **核心設計差異**：在開源 Data Plane 中，由用戶端決定過濾條件，引擎照單執行。企業版 Control Plane 則加入伺服器端策略強制執行，於查詢送達 Data Plane 前注入強制性安全限制。

---

## 6. 效能特性

- **查詢延遲**：典型索引掃描（top-k = 5 至 50）執行延遲約 ~10ms。
- **擴充性**：由 LanceDB 磁碟原生 IVF-PQ / DiskANN 索引支援的次線性向量搜尋擴充能力。
- **記憶體足跡**：低靜態記憶體消耗；Arrow 記錄批次採用零複製記憶體對映。
- **過濾開銷**：輕量級 SQL 預過濾評估，具有極低的 AST 解析開銷。

---

## 7. 部署與設定

### Cargo 依賴設定

將以下 crate 依賴加入 `Cargo.toml`：

```toml
[dependencies]
reqwest = { version = "0.13", features = ["blocking", "json", "rustls"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

### 狀態管理設定

於 Tauri 受管狀態中註冊 `LanceDbEngineWrapper`：

```rust
use std::sync::Mutex;
use tauri::State;

pub struct LanceDbEngineWrapper(pub Mutex<LanceDbEngine>);

fn main() {
    let engine = LanceDbEngine::new("./lancedb_data").expect("Failed to initialize LanceDB");

    tauri::Builder::default()
        .manage(LanceDbEngineWrapper(Mutex::new(engine)))
        .invoke_handler(tauri::generate_handler![open_source_rag_search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## 8. 使用範例

### 基本 Rust 檢索實作

```rust
use crate::commands::lancedb::{LanceDbEngine, VectorSearchQuery};

fn execute_search(engine: &LanceDbEngine, embedding_vector: Vec<f32>) -> Result<(), Box<dyn std::error::Error>> {
    let query = VectorSearchQuery {
        query_vector: embedding_vector,
        limit: 5,
        filter: Some("department = 'HR' AND min_security_level <= 2".to_string()),
    };

    let results = engine.search_sync(query)?;

    for chunk in results {
        println!("[{}] 分數: {:.4} | 檔案: {}", chunk.file_name, chunk.score, chunk.file_path);
        println!("片段: {}\n", chunk.content_snippet);
    }

    Ok(())
}
```

---

## 9. 限制與考量

- **Data Plane 範疇**：不包含內建身份驗證、授權或使用者管理。
- **精簡版 SQL 解析器**：預過濾語法僅限於簡單邏輯運算子 (`AND`, `OR`) 與基本比較斷言。
- **欄位限制**：過濾僅支援已建立索引的詮釋資料屬性 (`department`, `min_security_level`, `score`)。
- **用戶端安全邊界**：若需要多租戶隔離，使用開源引擎的應用程式必須於用戶端或 API 閘道層自行處理安全過濾。

---

## 10. 功能對照表

| 功能 | 開源版本 (Data Plane) | 企業版本 (Control Plane) |
| :--- | :--- | :--- |
| **向量搜尋** | ✅ 高效能 LanceDB / Arrow | ✅ 高效能 LanceDB / Arrow |
| **預過濾 (Pre-filtering)** | ✅ 用戶端控制 SQL 預過濾 | ✅ 伺服器端策略強制 + SQL 預過濾 |
| **身份驗證** | ❌ 無 (僅 Data Plane) | ✅ OAuth2, OIDC, API Key Gateway |
| **RBAC / ABAC** | ❌ 用戶端責任 | ✅ 伺服器端零信任強制執行 |
| **SSO 整合** | ❌ 無 | ✅ Okta, Azure AD, SAML 2.0 |
| **價格** | 🆓 開源 / 免費 | 💼 商業訂閱 |
