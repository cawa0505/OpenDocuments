# OpenDocuments 需求規格書 — graphify-plugin-opendoc Layer 2 缺口

> 本文件列出 graphify-plugin-opendoc Layer 2（向量軟搜尋 fallback）對
> OpenDocuments 上游的具體需求。Layer 1（硬連結）已獨立 ship，零 OD 依賴；
> 本規格書只在 Layer 1 硬連結缺席時才需要 OD 提供的 soft search 能力。

## 0. 背景與現狀（已驗證事實）

| 項目 | 現狀 | 來源 |
|------|------|------|
| `opendoc-storage::search_and_rerank` | stub，回 `Vec::new()` | opendoc-storage src |
| `opendoc-mcp` SearchBackend impl | 僅 `MockSearch`（回空） | opendoc-mcp src |
| Rust crate index write path | 不存在（opendoc-mcp index tool 是 HTTP forward） | opendoc-mcp index tool src |
| Live MCP `opendocuments_search` | 回 `[]` | 實測 |
| `/api/v1/search`、`/api/v1/query` | 回傳 SPA HTML fallback（route missing） | 實測 |
| `/api/v1/documents/search` | 404 | 實測 |
| `/api/v1/documents` | 回 `[]` | 實測 |
| `/api/v1/healthz` | OK | 實測 |
| Node.js backend RAG（LanceDB + Qdrant） | 程式碼存在但 search endpoint 未接 | opendoc-mcp index tool 為 HTTP forward 至 `{server.url}/api/v1/documents/upload` |

**結論**：OD 的搜尋管線（index → embed → LanceDB → search）尚未完工。本規格書
列出 plugin Layer 2 啟用所需的最小 OD 端交付項。

## 1. 需求總覽

Layer 2只需要 OD 提供一個能力：**給定 workspace + query，回傳排序後的文件 chunk**。
硬連結（Layer 1）處理 100% 確定性 match；Layer 2 只在硬連結不存在時 fallback。

| 編號 | 需求 | 優先級 | 依賴 |
|------|------|--------|------|
| R1 | Search REST endpoint（workspace-scoped） | P0 | — |
| R2 | Search 結果含 `heading` 原文（Layer 1↔Layer 2 對映所需） | P0 | R1 |
| R3 | Index write path（file → chunks → embeddings → store） | P0 | — |
| R4 | Workspace 隔離（X-Workspace header 或 query param） | P0 | R1, R3 |
| R5 | workspace_id 為 TEXT（非 UUID 強制） | P1 | R4 |
| R6 | 同步 Rust API（供未來直連路線） | P2 | R1, R3 |

## 2. R1 — Search REST endpoint

### 2.1 動詞與路徑

```
POST /api/v1/search
Content-Type: application/json
X-Workspace: <workspace_id>
```

或等效 query param：`POST /api/v1/workspaces/<workspace_id>/search`。

兩者擇一即可；必須支援 workspace 隔離（見 R4）。

### 2.2 Request body

```json
{
  "query": "crate::auth::verify_token",
  "top_k": 10,
  "threshold": 0.0
}
```

| 欄位 | 型別 | 預設 | 說明 |
|------|------|------|------|
| `query` | string | (必填) | 搜尋文字；plugin 傳入 code symbol 或自然語言片段 |
| `top_k` | int | 10 | 回傳前 K 筆 |
| `threshold` | float | 0.0 | 最低相似度門檻；低於此值不回傳 |

### 2.3 Response（200）

```json
{
  "hits": [
    {
      "doc_path": "docs/auth.md",
      "spec_id": "docs/auth.md#token-spec",
      "heading": "Token Spec",
      "score": 0.87,
      "snippet": "...Token signature verification spec..."
    }
  ]
}
```

| 欄位 | 型別 | 說明 |
|------|------|------|
| `doc_path` | string | workspace root 相對路徑 |
| `spec_id` | string | OD 自訂識別（如 `<doc_path>#<slug>`）；plugin **不依賴**它做對映，僅作為 OD 內部排序/去重用 |
| `heading` | string | **新增**：chunk 對應的原始 heading 文字（非 slug）。Plugin 拿 `doc_path`+`heading` 自算內部 spec_id 做 1:1 對映（詳見 §3） |
| `score` | float | 相似度（0..1，越高越相關） |
| `snippet` | string | (可選) 命中片段文字，供 agent 預覽 |

### 2.4 錯誤語意

| HTTP | 意義 | plugin 行為 |
|------|------|-------------|
| 200 + `hits: []` | 無命中 | 回空（等同 NoOp） |
| 404 | workspace 不存在 | 回空 + log warning |
| 5xx | OD 内部錯誤 | 回空 + log error；不 panic |

**關鍵約束**：search 失敗時 plugin **不得**影響 Layer 1 硬連結查詢。Layer 2
永遠是 best-effort fallback。

## 3. R2 — Search 結果含 heading 原文（Layer 1↔Layer 2 對映所需）

plugin 的 `fetch_code_to_doc_context` 收到 SearchHit 後需映射回 `LinkRow`：

```rust
pub struct SearchHit {
    pub doc_path: String,
    pub spec_id: String,   // OD 自訂，plugin 不依賴它做對映
    pub heading: String,   // 新增：原始 heading 文字（非 slug）
    pub score: f64,
    pub snippet: String,
}
```

### 3.1 對映路徑（hard link ↔ soft hit）

Plugin 的 `LinkRow.spec_id` 與 OD 的 `SearchHit.spec_id` 採用不同推導規則，**不會
1:1 相同**：

| 識別 | 推導方式 | 用途 |
|------|----------|------|
| Plugin `LinkRow.spec_id` | `sha1(doc_path + heading_original)[0..12]` | hard link 持久識別 |
| plugin `block_signature` | `sha1(block_content)` 完整 40 hex | drift 偵測（內容變） |
| OD `SearchHit.spec_id`（選用） | `slug(最後 heading)` | OD 內部排序/去重 |

> 兩端的 `spec_id` 字串長得不一樣，**不要嘗試讓 OD 仿照 plugin 的 sha1 推導**——
> 那需要 plugin 內部 doc_path+heading 的 raw bytes，與 plugin spec_id 演算法耦合，
> 不利 OD 獨立演進。

對映的正確做法：OD SearchHit 額外回傳 **原始 heading 文字**（非 slug、非
transformed），plugin 收到後用 `(doc_path, heading_original)` 自算
`sha1(doc_path + heading)[0..12]` ↔ `LinkRow.spec_id`，1:1 對映。

### 3.2 `heading` 欄位規格

- 型別：string
- 內容：該 chunk 對應的 Markdown heading 原文（去除 `#` 與空白前綴，但保留 inline
  code 的反引號與原始大小寫）
- 範例：
  - `## 分塊策略` → `heading = "分塊策略"`
  - `## `verify_token`` → `heading = "verify_token"`（pulldown-cmark 解析後反引號被消去，與 plugin 端解析器一致）
  - `### Layer 1 — 硬連結` → `heading = "Layer 1 — 硬連結"`
- 若 chunk 跨多個 heading（OD 內部分塊策略與 plugin 以 heading 切 block 不同），
  回傳**該 chunk 的最後一個 heading**（即所屬 section 的標題）即可。Plugin 會以
  heading 為查詢單位重算 spec_id，自然對到位於該 heading 下的 LinkRow。

### 3.3 score 正規化

OD 回傳的 score 需為可排序的數值（越高越相關）。plugin 不假設特定分佈，只用於
top_k 排序與 threshold 過濾。

### 3.4 對 OD 無副作用

`heading` 欄位只用于 plugin 端算回內部 spec_id，不影響 OD 自家 ranking/TUI/MCP。
OD 既有 slug 路徑（`SearchHit.spec_id = slug(heading)`）保留不動。

## 4. R3 — Index write path

OD 需有可程式化觸發的 index 管線（不只是手動 TUI 操作）：

```
file → chunk → embedding → LanceDB / Qdrant → 可 search
```

### 4.1 觸發方式（擇一）

1. **REST**：`POST /api/v1/workspaces/<id>/index`（非同步，回 202 + job_id）
2. **MCP tool**：opendoc-mcp 提供可呼叫的 index tool（非 MockSearch）
3. **CLI**：`opendoc index --workspace <id> --path <dir>`（可 cron / script）

### 4.2 現狀缺口

- 目前 opendoc-mcp 的 index tool 是 HTTP forward 到 `/api/v1/documents/upload`
  （單檔上傳），**沒有**批次 / 目錄掃描 / 變更偵測。
- opendoc-storage 的 Rust 端 `search_and_rerank` 是 stub。
- 需確認 Node.js backend 是否有完整的 index→embed→store 管線，或僅有 storage schema。

### 4.3 最小交付

OD 端至少提供「給定 workspace + 目錄，完成 index」的 single shot API。plugin
不關心內部用 LanceDB 或 Qdrant，只要求事後 search 有資料回。

## 5. R4 — Workspace 隔離

### 5.1 必須

search 與 index 都必須以 workspace 為隔離邊界：

- search 僅回傳該 workspace 已索引的 chunks
- index 僅影響該 workspace 的 collection
- 跨 workspace 不互相污染

### 5.2 識別方式

OD 的 `workspace_id` 為 **TEXT**（見 R5）。plugin 透過 `set_workspace_mapping`
手動建立 `workspace_key → od_workspace_id` 對映後，在 search/index 請求帶上。

### 5.3 不可

- 不假設 1:1 workspace 對映（plugin 支援一個 graphify workspace 映射到一個 OD workspace）
- 不要求 OD 認識 graphify 的 SipHash key（對映在 plugin 端）

## 6. R5 — workspace_id 為 TEXT（非 UUID 強制）

### 6.1 現狀

opendoc-storage 的 `DocumentChunk.workspace_id` 為 `TEXT`（不強制 UUID）。

### 6.2 需求

維持 TEXT。plugin 的 `od_workspace_id` 可能是：
- OD 自產的 UUID 字串
- 使用者自訂的 slug（如 `graphify-handoff`）

兩者皆須可作為 `X-Workspace` header 值傳入。OD 不對格式做 UUID validation。

## 7. R6 — 同步 Rust API（P2，未來直連路線）

### 7.1 動機

Layer 2 目前規劃走 MCP-to-MCP 轉發（避免 libsqlite3-sys 衝突，見 design.md §6.4）。
未來若 OD 拆分搜尋為獨立 crate（無 sqlx/libsqlite3-sys 依賴），plugin 可改直連
Rust API，省一層 MCP 轉發。

### 7.2 需求

若 OD 提供 Rust crate（如 `opendoc-search`，獨立於 `opendoc-storage`），需：

```rust
pub trait SearchBackend: Send + Sync {
    fn search(&self, workspace_id: &str, query: &str, top_k: usize) -> Vec<SearchHit>;
}
```

- 同步呼叫（plugin 業務 API 為同步）
- 不依賴 `libsqlite3-sys`（避免與 plugin 的 rusqlite 衝突）
- 不依賴 tokio（plugin 本體無 async runtime）

### 7.3 優先级說明

P2 = 等 MCP 路線驗證穩定後再考慮。非 blocker。

## 8. 驗收條件（Definition of Done）

Layer 2 McpBackend 可啟用，需 OD 端滿足：

1. ✅ `POST /api/v1/search`（或等效）回傳非空 hits（R1）
2. ✅ hits 含 `doc_path` + `heading` 原文（plugin 據此算回內部 spec_id）（R2）
3. ✅ 有可程式化觸發的 index 管線（R3）
4. ✅ search/index 均以 workspace 隔離（R4）
5. ✅ workspace_id 接受 TEXT（R5）

R6（同步 Rust API）非 blocker；MCP 路線先驗證。

### 8.1 驗收測試程序（OD 可自我執行；plugin 接手檢核時照跑）

前置：伺服器 `opendoc start --port <port>` 已啟動，已知一個 workspace_id 且該
workspace 已 index 至少一份文件（`GET /api/v1/documents` 回非空）。

| # | 測試 | 指令 | 預期 |
|---|------|------|------|
| T1 | R1 search 非空 | `curl -s -X POST :<port>/api/v1/search -H "X-Workspace: <ws>" -d '{"query":"<文件內真實詞彙>","top_k":3}'` | `{"hits":[...]}` 非空，每 hit 含 doc_path/score |
| T2 | R2 heading 欄位 | 同上，`jq '.hits[0] | has("heading")'` | `true`；`heading` 為原始 heading 文字（非 slug） |
| T3 | R1 無命中 | 同上，query 用不存在的詞 | `{"hits":[]}`（200，非錯誤） |
| T4 | R4 隔離（未知 ws） | 同上，`X-Workspace: nonexistent-ws` | 非 200 或明確錯誤（不得回傳其他 ws 的 hits） |
| T5 | R4 隔離（跨 ws） | 在 ws A 查 ws B 獨有的詞 | `{"hits":[]}`（或僅 A 的 hits） |
| T6 | R3 已 index | `GET :<port>/api/v1/documents -H "X-Workspace: <ws>"` | 回非空 documents，狀態 `indexed` |
| T7 | R5 TEXT id | 用非 UUID 的 slug 當 workspace 建立/查詢 | 不因格式被拒（無 UUID validation） |

**通過標準**：T1/T2/T4/T6 必過；T3/T5/T7 為強化項（T3 回空亦符合語意）。

**plugin 接手檢核的額外項目**（需 plugin 側配合，非 OD 單獨可驗）：

- P1：`heading` 值能讓 plugin 重算 `sha1(doc_path+heading)[0..12]` 命中對應
  `LinkRow.spec_id`（端對端對映；plugin 寫 integration test 驗）
- P2：OD search 回傳的 `doc_path` 與 plugin index 時使用的相對路徑一致
  （若 OD 存絕對路徑，plugin 需 strip workspace root — 需確認 OD 端語意）

### 8.2 已知觀察（2026-08-10 實測）

實測 `POST /api/v1/search` 對已 index 的 workspace 回 `{"hits":[]}`（query 用
文件內真實詞彙 + `threshold:0.0`）。route 活、回應結構正確，但 hits 為空。
兩個 workspace（含 12-chunk 的 TASKS.md）皆如此。

可能原因（需 OD 排查，非定論）：
- 執行的 server binary 早於 index 完成（embedding 未寫入 LanceDB）
- 或 embed provider 設定與 index 時不一致（BYOK 缺 config → search 時 embed 失敗回空）
- 或 LanceDB table 與 SQLite documents 不同步

plugin 側此觀察不阻塞 Layer 1；僅 Layer 2 啟用前需確認（見 §8.1 T1/T2）。

## 9. 不在本規格書範圍

- **Layer 1 硬連結**：plugin 自有，不需 OD。`# Symbol: <name>` 的 Markdown
  解析、registry、drift audit 全在 plugin 端。
- **Graphify graph 修改**：plugin 不改 core graph，query-time merge（見 design.md §8）。
- **TOON 序列化**：plugin 複用 graphify-core 的 to_toon/from_toon。
- **向量資料庫選型**：OD 端決定（LanceDB / Qdrant / 其他），plugin 不關心。
- **embedding model 選型**：OD 端決定。

## 10. 待討論

- [ ] OD 的 index 是否需要即時（file watch）或批次即可？（plugin 可接受批次 + 手動觸發）
- [ ] OD search 是否需要支援「排除已失效文件」（doc 已刪但 index 仍存在）？plugin
      Layer 1 的 DocMissing 可偵測，但 Layer 2 若回傳已刪文件的 hit 會誤導。
- [ ] 多版本文件（docs/v1/auth.md vs docs/v2/auth.md）的 workspace 內搜尋語意？
      plugin 目前不處理版本，假設單一 doc_path 在 workspace 內唯一。