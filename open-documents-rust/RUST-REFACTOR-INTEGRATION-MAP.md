# OpenDocuments Rust 後端功能全貌與重構進度文件

## 1. 核心定位與架構設計 (WASM / Single Binary)
`open-documents-rust` 是一個旨在完全取代 Node.js 後端的 Rust Axum 單一進程 (Single Binary) 伺服器：
* **無 Node.js 外部依賴**：完全整合 SQLite 資料庫讀寫、與 RAG 搜尋引擎（基於內部或外部向量/傳統混合檢索）。
* **安全隔離與工作空間**：透過實作嚴格的 `resolve_ws` (X-Workspace header $\to$ `active_workspace` $\to$ `default_workspace`) 在 SQLite 中隔離租戶與工作空間，預防 FK violations。
* **WASM 擴充路線**：為使未來能夠於無伺服器 (Serverless) 或瀏覽器內 (WebAssembly) 環境運行，各模組（尤其是儲存層 `opendoc-storage`）力求低記憶體佔用、高效編譯及純 Rust 實作。

---

## 2. API 路由與合約完整盤點 (對齊 Node.js)

### 2.1 已整併與完整實作 (API Parity)
| Method | Route | Rust Handler | 狀態與修正紀錄 |
|--------|-------|--------------|----------------|
| GET | `/api/v1/health` | `health_handler` | ✅ |
| GET | `/api/v1/healthz` | `health_handler` | ✅ |
| GET | `/api/v1/workbench` | `workbench_handler` | ✅ 已依據 WebUI 契約完成聚合 |
| GET | `/api/v1/documents` | `list_documents_handler` | ✅ 補齊 `source_path`, `file_type`, `indexed_at` 等前端渲染必要欄位 |
| GET | `/api/v1/documents/:id` | `get_document_handler` | ✅ 補齊單一文件詳情路由 |
| POST | `/api/v1/documents/upload`| `upload_handler` | ✅ 支援至多 50MB 檔案，解決同名覆蓋衝突（採用 `workspace/filename` 前綴），前端正確匹配 HTTP 200 回傳格式 |
| DELETE | `/api/v1/documents/:id` | `delete_document_handler` | ✅ |
| GET | `/api/v1/workspaces` | `list_workspaces_handler` | ✅ |
| POST | `/api/v1/workspaces` | `create_workspace_handler` | ✅ |
| DELETE | `/api/v1/workspaces/:id`| `delete_workspace_handler` | ✅ |
| GET | `/api/v1/collections` | `list_collections_handler` | ✅ 補齊 `workspaceId`, `createdAt` 欄位 |
| POST | `/api/v1/collections` | `create_collection_handler` | ✅ 新增 |
| DELETE| `/api/v1/collections/:id`| `delete_collection_handler` | ✅ 新增 |
| GET | `/api/v1/collections/:id/documents` | `list_collection_documents_handler` | ✅ 新增，集合內文件列表 |
| POST | `/api/v1/collections/:id/documents/:docId` | `add_collection_document_handler` | ✅ 新增，關聯文件至集合 |
| DELETE| `/api/v1/collections/:id/documents/:docId` | `remove_collection_document_handler` | ✅ 新增 |
| GET | `/api/v1/conversations` | `list_conversations_handler` | ✅ 補齊 `limit`, `offset` 分頁及 `updated_at` 欄位 |
| POST | `/api/v1/conversations` | `create_conversation_handler` | ✅ 新增 |
| DELETE| `/api/v1/conversations/:id` | `delete_conversation_handler` | ✅ 新增 |
| PATCH | `/api/v1/conversations/:id` | `update_conversation_handler` | ✅ 新增，更名對話標題 |
| GET | `/api/v1/conversations/:id/messages` | `list_conversation_messages_handler` | ✅ 新增，載入對話訊息 |
| POST | `/api/v1/conversations/:id/share` | `share_conversation_handler` | ✅ 新增，產生 32 hex chars 分享 token |
| GET | `/shared/:token` | `shared_conversation_handler` | ✅ 新增，免 Auth 公開分享路由 |
| GET | `/api/v1/shared/:token` | `shared_conversation_handler` | ✅ 新增，免 Auth 公開分享路由 |
| GET | `/api/v1/dictionary` | `get_dictionary_handler` | ✅ |
| POST | `/api/v1/dictionary` | `add_dictionary_handler` | ✅ |
| DELETE| `/api/v1/dictionary/:id` | `delete_dictionary_handler` | ✅ |
| POST | `/api/v1/dictionary/import-seed` | `import_seed_handler` | ✅ |
| GET | `/api/v1/admin/stats` | `get_admin_stats_handler` | ✅ |
| GET | `/api/v1/admin/search-quality` | `get_admin_search_quality_handler` | ✅ |
| GET | `/api/v1/admin/benchmark` | `get_admin_benchmark_handler` | ✅ |
| GET | `/api/v1/admin/connectors` | `get_admin_connectors_handler` | ✅ |
| GET | `/api/v1/admin/query-logs` | `get_admin_query_logs_handler` | ✅ 補齊 `{ logs, total, limit, offset }` 回傳格式 |
| POST | `/api/v1/query/log` | `query_log_handler` | ✅ |
| POST | `/api/v1/chat/feedback` | `chat_feedback_handler` | ✅ 修正原 `/query/feedback` 路由不匹配前端之缺陷 |
| POST | `/api/v1/chat` | `chat_handler` | ✅ 已補齊歷史對話載入與資料庫持久化 |
| POST | `/api/v1/chat/stream` | `chat_stream_handler` | ✅ 已補齊自動建立、歷史對話載入與資料庫持久化 |
| GET | `/mcp/sse` | `sse_handler` | ✅ 整合 MCP 唯讀與寫入工具於一體 |
| POST | `/mcp/message` | `message_handler` | ✅ |

---

## 3. 資料庫結構與 DDL 變更紀錄

### 3.1 關鍵實體關聯 (SQLite ERD)
* `workspaces` (id PRIMARY KEY, name)
* `documents` (id PRIMARY KEY, title, source_type, source_path, file_type, file_size_bytes, connector_id, chunk_count, status, content_hash, error_message, workspace_id REFERENCES workspaces(id), deleted_at, created_at, updated_at, indexed_at)
* `collections` (id PRIMARY KEY, workspace_id REFERENCES workspaces(id), name, description)
* `collection_documents` (collection_id REFERENCES collections(id) ON DELETE CASCADE, document_id REFERENCES documents(id) ON DELETE CASCADE, PRIMARY KEY)
* `conversations` (id PRIMARY KEY, workspace_id, title, shared, share_token, deleted_at, created_at, updated_at)
* `messages` (id PRIMARY KEY, conversation_id REFERENCES conversations(id) ON DELETE CASCADE, role, content, sources, profile_used, confidence_score, response_time_ms, created_at)
* `query_logs` (id AUTOINCREMENT, query, profile, confidence_score, response_time_ms, route, feedback, workspace_id)

### 3.2 已驗證之 Migration 修正
* 移除所有 Mock-up DDL，改由統一的 `init_db_pool` 進行建表，與 Node 遷移腳本（001 + 002）完全一致。
* 修正 `conversations` 缺少 `updated_at` 欄位之問題。
* 修正 `messages` 關聯與級聯刪除（ON DELETE CASCADE）之完整性。

---

## 4. 驗證與測試流程規範 (MANDATORY)
所有變更必須依循下列嚴格的「測試驅動重構」流程進行：
1. **編寫單元測試**：在 `lib.rs` 的 `tests` 模組中建立對應功能（或 Bug 復現）的單元測試（寫測試 $\to$ 確認失敗 $\to$ 修復代碼）。
2. **靜態分析**：執行 `cargo check` 確保編譯與型別檢查零錯誤。
3. **單元測試驗證**：執行 `cargo test` 確保本機單元測試 100% 通過。
4. **二進位檔編譯安裝**：執行 `cargo install --path . --force`，安裝至 `~/.cargo/bin/opendoc`。
5. **啟動與重啟服務**：重啟現役 `opendoc-server` 載入最新變更。
6. **E2E 驗證**：透過 `curl` 與 WebUI 端對端交互驗證，取得實際證據。

---

## 5. 遺留之 Node.js 與 Rust 後端功能差異與未移植盤點 `[待討論]`

經由前端 API 宣告 (`api.ts`) 與 Node.js 舊路由實作比對，現階段 Rust 單一進程後端與 Node.js 尚存之功能差異如下：

### 5.1 認證與會話管理 (Auth & Session)
* **Node.js 實作**：包含 `auth-routes.ts`，支援完整的 JWT 認證、會話持久化、與 `withStoredApiKey` 輔助器。
* **Rust 現狀**：除 MCP 自帶之輕量會話（`SessionMap`）外，尚無持久化使用者註冊/登入或 JWT 認證過濾器，目前路由皆假定為內部或受信任環境直接調用。

### 5.2 動態插件安裝與卸載 (Dynamic Plugins)
* **Node.js 實作**：支援於 `/api/v1/plugins` 路由下動態安裝 (Install) 與移除 (Remove) 插件。
* **Rust 現狀**：僅提供 `get_admin_plugins_handler`（`/admin/plugins`）靜態宣告內建模組（`document-parser`, `text-chunker`, `vector-store`）的健康度，未實作運行時動態安裝/載入外部 Node.js 模組插件（此亦符合 Rust 提倡之 Single Binary WASM 演進方向）。

### 5.3 文件標籤系統 (Tags) [已實作]
* **Node.js 實作**：具備 `tags.ts` 路由，支援對 Documents 或 Chunks 進行多維度標籤分類。
* **Rust 實作**：已於 `opendoc-storage` 建立相容 tags 及 document_tags 資料表，並在 `lib.rs` 完整實作 5 個標籤管理與貼標相關之 Axum API 路由，經單元測試與 E2E 驗證 100% 通過。

### 5.4 複雜過濾與排序 (Complex Query Filtering) [已實作]
* **Node.js 實作**：文件及集合清單支援更為複雜的多參數複合查詢條件（如 `where status = 'X' AND workspace_id = 'Y'` 的動態 SQL）。
* **Rust 實作**：在 `list_documents_handler` 中成功引入 Query 提取器，支援對 status、sourceType (或 source_type) 作篩選，並能對 title、chunks (chunk_count)、updated (updated_at)、created (created_at)、indexed (indexed_at) 等多欄位進行升降冪 (asc/desc) 動態 SQL 安全排序，經測試 100% 通過。

### 5.5 儀表板佈局 (Dashboard Layout)
* **Node.js 實作**：定義了特定的 `/dashboard` 整合端點。
* **Rust 現狀**：由更寬泛的 `/workbench` 端點（透過 `workbench_handler`）實作整合式聚合。

以上盤點已記錄入檔，留待後續與你討論移植的必要性與實作路徑。
