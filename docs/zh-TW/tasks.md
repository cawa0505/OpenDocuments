# 📋 OpenDocuments 待開發任務清單

[English](../en/tasks.md) | **繁體中文**

本文件依據架構地圖與當前研發狀態，追蹤所有工程任務的執行進度與具體驗證標準。

---

## 🚨 當前執行任務 (Phase 1)

### 1.1 WebUI 與 RAG 串流優化

- [x] **1.1.1 BYOK 設定介面**：完成 `SettingsPage.tsx` 供應商管理介面與連線測試功能。
- [x] **1.1.2 預設 Light Mode**：鎖定高質感明亮模式，並清潔 DOM 主題初始化邏輯。
- [ ] **1.1.3 Markdown 代碼高亮與 Copy 按鈕**：
  - [ ] 整合輕量級 Markdown 解析器，支援程式碼區塊語法高亮。
  - [ ] 於每個程式碼區塊右上角新增浮動「複製 (Copy)」按鈕。
  - *驗證方式*：確認 `rust`、`json`、`javascript` 程式碼有正確渲染語法色彩；點擊「複製」按鈕後，剪貼簿內容與原始代碼完全一致。
- [ ] **1.1.4 互動式 Citation 連結**：
  - [ ] 動態解析 SSE 串流內文中的 `[1]`、`[2]` 出處標記。
  - [ ] 將靜態文字標籤轉換為可點擊的超連結按鈕，點擊時會將對應的來源文獻卡片平滑滾動（smooth-scroll）至畫面中，並套用高亮外框。
  - *驗證方式*：執行多 chunk 檢索問答；點擊 LLM 回覆中的 `[1]` 標記；確認右側來源卡片被高亮顯示並自動滾動到可視區域。
- [ ] **1.1.5 RAG 檢索偏好 (Query Profiles)**：
  - [ ] 在 Chat 聊天介面中新增檢索偏好選擇器（`Fast`、`Balanced`、`Precise`）。
  - [ ] 實作後端路由邏輯：`Fast` (FTS5 Top-5)、`Balanced` (LanceDB Top-10)、`Precise` (混合 + 重度 Reranker Top-15)。
  - *驗證方式*：驗證前端發送的 REST 請求帶有對應的 Profile 標頭，且後端日誌中顯示正確的檢索策略與 Chunk 數量。

### 1.2 CLI 優化

- [x] **1.2.1 工作區切換持久化**：`opendoc workspace switch <name>` 正確將選擇寫入 `config.toml` 的 `active_workspace`。
- [x] **1.2.2 一鍵跨平台安裝腳本**：`install.sh` 腳本支援 Linux 與 macOS (x86_64/aarch64)。

### 1.3 GA 前契約稽核

- [x] **1.3.1 工作區卡片來源稽核**：`/workbench`、`/admin/stats` 與 `/admin/connectors` 都依 `X-Workspace` 查詢；未發現卡片直接誤用 `default_workspace`。
- [x] **1.3.2 CLI index 同步程式碼稽核**：已確認 SHA-256 去重、內容變更重傳、目錄內本機刪除同步刪除，以及 `X-Workspace` 傳遞。
- [ ] **1.3.3 CLI index 同步整合測試**：補驗證空目錄、巢狀路徑與跨 workspace 不互刪。
- [ ] **1.3.4 GitHub connector 契約**：WebUI 呼叫 `/admin/connectors/github` 與 `/admin/connectors/github/sync`，但 Rust router 尚未提供兩條 route；完成 connector 實作前不得標記 workspace 隔離完成。
- [x] **1.3.5 活動日誌 workspace 讀取稽核**：統計、workbench 與 query-log 讀取都有 `workspace_id` 條件。
- [x] **1.3.6 活動日誌完整功能**：修正總數／分頁／DTO 對齊、feedback workspace 條件，並補刪除 API 與 UI。

---

## 📋 規劃中執行任務 (Phase 2 — 任務執行層與原生 AI 引擎)

規格：[`openspec/specs/task-execution-ai-engines/spec.md`](../../openspec/specs/task-execution-ai-engines/spec.md)
參考：[`docs/ref/zh-TW/task-execution-ai-engines-verification.md`](../ref/zh-TW/task-execution-ai-engines-verification.md)

### 2.0 Phase 0 — 基線強化（不改變行為）

- [ ] **2.0.1 稽核 `search_and_rerank` call sites**：在 async 簽名變更前，列出所有同步 `SearchBackend` trait 的呼叫點（mcp `lib.rs:187`/`:441`、CLI `SearchWrapper` main.rs:30、storage stub `lib.rs:394`）。
- [ ] **2.0.2 async `SearchBackend` 失敗測試**：撰寫 trait 方法由 `fn search_and_rerank(...) -> Vec<DocumentChunk>` 改為 `async fn ... -> Vec<DocumentChunk>` 的失敗單元測試（所有 call sites 改為 `.await`）。
- [ ] **2.0.3 `[ai]`/`[task]` 設定解析**：以 `#[serde(default)]` 在 `AppConfig` 新增段落，使既有 `config.toml` 檔案原樣載入（向後相容）。
  - *驗證方式*：`cargo check` 零警告；既有設定可載入；含 `[ai]`/`[task]` 的新設定可解析。

### 2.1 Phase 1 — Task 與 AI 抽象層（純 Rust，CPU）

- [ ] **2.1.1 `opendoc-task` crate**：`TaskEnvelope`/`TaskResult`/`TaskType`、`TaskExecutor` trait、`InProcessExecutor`。
- [ ] **2.1.2 `opendoc-ai` crate**：`AiEngine` trait、`EngineConfig`、`HardwareBackend` probe（Vulkan→HIP→CPU）。
- [ ] **2.1.3 `opendoc-ai-fastembed` crate**：bge-m3 embed + reranker 於 ONNX CPU（dim 1024）。
- [ ] **2.1.4 上傳管線**：parse → embed（fastembed CPU）→ LanceDB 寫入（compat schema）。
- [ ] **2.1.5 真實 `LanceDbRetriever`**：向量 + FTS5 + RRF + threshold 取代 stub `search_and_rerank`。
  - *驗證方式*：真實文件往返——索引後查詢回傳實際 chunks；無匹配時回傳空 `Vec::new()`。
