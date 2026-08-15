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
- [x] **1.1.5 RAG 檢索偏好 (Query Profiles)**：
  - [x] 在 Chat 聊天介面中新增檢索偏好選擇器（`Fast`、`Balanced`、`Precise`）。
  - [x] 實作後端路由邏輯：`Fast` (FTS5 Top-5)、`Balanced` (LanceDB Top-10)、`Precise` (混合 + 重度 Reranker Top-15)。
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

### 1.4 GA 前 WebUI 收尾

- [ ] **1.4.1 Tailwind CSS v4 遷移**：`tailwindcss ^4` + `@tailwindcss/vite`（vite.config.ts），移除 `postcss.config.js` / `autoprefixer`；設定改 CSS-first（`@import "tailwindcss"`、`@custom-variant dark`、`@theme`、`@plugin "@tailwindcss/typography"`）。*驗證方式*：`npm run typecheck` 零錯誤、`make install`、頁面正常 render（非 stub）、暗色模式正常、modal/markdown 外觀無視覺漂移。
- [ ] **1.4.2 default workspace hardcoding 修正**：`DictionaryPage.tsx:20` 與 `Sidebar.tsx:192` 目前 `localStorage.getItem('active-workspace') || 'default'` 會把字面 `default` 當成 workspace 名稱帶入；正解是 WebUI 啟動時呼叫 `getWorkbench()` 取得後端解析的真實 workspace 名稱（active → default_workspace 回退），存入 appStore，Sidebar/DictionaryPage 從 store 讀取，不再各自猜 localStorage。*驗證方式*：清空 localStorage 開新瀏覽器，Sidebar 顯示設定中 `default_workspace` 的名稱（非字面 `default`）。
- [x] **1.4.3 LLM 公版回答修正**：`chat.rs` system prompt 未注入工作區資訊，檢索模糊命中時 LLM 不知自身工作區與文件範圍，回覆公版內容（「因為您沒有提供具體的專案名稱…」）；需在 prompt 注入工作區名稱與已索引文件範圍，檢索無相關時直接聲明。*驗證方式*：對部署流程類問題以 fast profile 詢問，回覆應直接引用工作區文件或明確聲明「該工作區無相關文件」，不得給出公版泛泛回答。
- [x] **1.4.4 Chat 版面重構（底部輸入框 + 處理中狀態）**：現行 ChatPage 輸入框在上、訊息往下長，不符合主流 chat UI（Open WebUI 風格：訊息區在上可捲動、輸入框釘在底部）；且送出後到第一個 SSE chunk 之間無任何後端處理中回饋。需改為 flex column 版面（訊息區 `flex-1 overflow-y-auto`、輸入框釘底），並在 `isStreaming && !currentStreamText` 時顯示思考指示器（lazy-load 狀態）。*驗證方式*：瀏覽器送出問題後立即看到思考指示器；訊息區獨立捲動、輸入框固定底部；新訊息自動滾到底。
- [x] **1.4.5 工作區內容與 LLM 上下文／受控工具調研**：確認 chat 檢索到的文件內容、來源與工作區範圍是否完整傳入 LLM；若仍不足，設計只允許讀取目前 workspace、透過既有後端資料層執行的文件清單／內容工具，再評估是否需要 tool calling。不得讓 LLM 直接讀取任意實體路徑，也不得繞過 workspace 隔離。*驗證方式*：以真實工作區文件提問，檢查 outbound LLM request 的上下文與回答引用；無相關內容時仍明確拒答，不使用通用知識補答。*結論*：1.4.3 修正後檢索內容已完整進入 LLM 上下文（system prompt 注入工作區文件數與範圍），GA 維持 Chat-centric RAG，受控文件工具延後至 post-GA。
- [x] **1.4.6 會話標題重新命名**：在既有 conversation update API 上補 WebUI 的重新命名互動，標題更新必須維持 workspace scope、輸入驗證與錯誤回饋，不新增第二套 conversation 狀態。*驗證方式*：瀏覽器重新命名後重新整理，側邊欄與會話標題一致；跨 workspace 不得更新其他 workspace 的會話。
- [x] **1.4.7 文件 Detail 資訊框寬度溢出修正**：文件 detail 中「資料無損與完整性校驗」資訊框會被長文字撐寬，破壞原有欄寬並可能產生橫向捲軸；依 `AGENTS.md` §9.4 修正承載文字的 flex/grid child 與斷行規則，不得以縮小整體字級掩蓋問題。*驗證方式*：以「資料無損與完整性校驗」、長英文單字、URL 與窄 viewport 測試；資訊框維持原欄寬、文字正常換行，頁面無非預期橫向捲軸。
- [x] **1.4.8 WebUI `font-semibold` 字重調整**：從 Tailwind 共用字重 token 的單一來源將 `font-semibold` 由 600 調為 500，未逐頁散落覆寫。此變更影響所有使用 `font-semibold` 的段落、標題、按鈕與標籤，保留可辨識的資訊階層。*驗證方式*：`npm run typecheck` 通過；以繁中段落、`h3`～`h5`、按鈕及狀態標籤檢查，並比對英文與韓文介面。
- [x] **1.4.9 Modal 支援 Escape 關閉**：盤點所有 WebUI modal／dialog／overlay，讓可關閉的 modal 支援按 `Escape` 關閉，並維持 `role="dialog"`、`aria-modal="true"`、焦點與 busy 狀態契約；執行危險動作期間不得因 Escape 中斷。*驗證方式*：逐一開啟各 modal，按 `Escape` 可關閉且焦點回到觸發元件；busy 時 Escape 不關閉；背景頁面不誤觸發快捷鍵。
- [x] **1.4.10 BYOK 編輯按鈕改用明確編輯圖示**：BYOK Provider 列表的「編輯」按鈕原以 `+` 圖示呈現，與「新增 Provider」混淆；改用鉛筆圖示（`Pencil`）明確表達編輯語意，維持既有按鈕尺寸與 i18n 標籤。*驗證方式*：設定頁 BYOK 區塊「編輯 Provider」按鈕顯示鉛筆圖示（非 `+`），typecheck 通過。
- [ ] **1.4.11 Chat 文件來源預覽 Modal 內 Markdown 樣式一致性**：Chat 文件來源預覽 modal 內的 Markdown／程式碼區塊目前有些黑底內容會被白色框線包住、有些不會，需統一相同內容層級的背景、邊框與內距規則；不得順帶修改 modal 寬度、外部文件卡或其他未指定的版面樣式。*驗證方式*：開啟含一般 Markdown、程式碼區塊與長文字的真實文件來源預覽 modal，確認所有同類區塊的黑底與白框呈現一致，且三語介面與窄 viewport 不產生非預期橫向捲軸。

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
