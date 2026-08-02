# 📋 OpenDocuments 待開發任務清單 (TASKS.md)

本文件依據 `STRUCTURE.md` 與當前開發狀態，整理出後續所有待開發任務，作為後續執行的追蹤基準。

---

## 🚨 核心優先任務 (High Priority)
- [x] **0.1 WebUI LLM Provider 設定介面與 BYOK 整合** (對齊 `llm_providers` 表)
  - [x] 於 WebUI `SettingsPage.tsx` 補上完整的 LLM Provider 設定介面（支援自訂名稱、Provider 種類、Base URL、Model Name、API Key 遮罩輸入）。
  - [x] 串接後端 `GET /api/v1/admin/llm/providers` / `POST /api/v1/admin/llm/providers` / `DELETE` 與 `POST /api/v1/admin/llm/providers/test`，提供「一鍵連線測試」與「啟用開關」。
  - [x] 確保使用者設定並啟用 BYOK LLM 後，Chat 與 Chat Stream 徹底告別 Echo / 拼接 `STRUCTURE.md` 的 Fallback 模式，並與 SQLite 完美同步。
- [x] **0.2 RAG 深度整合 (引用與對話織入)**
  - [x] 後端：強化 `chat.rs` 中的 System Prompt，依前端傳遞之 `Accept-Language` (或 `X-Locale`) 標頭動態調整 System Prompt 所要求的回答語系（不再寫死繁體中文）。強烈規範 LLM 回答時必須在文獻對應句末精確標記出處（例如 `[1]`、`[2]`），且文獻不足時誠實說明。
  - [x] 前端：於 `sse.ts` 與 `api.ts` 請求標頭中攜帶目前選擇的語系（自 `localStorage` 取得），並將 Markdown 渲染中的 `[1]`、`[2]` 標記轉化為可點擊的互動 Citation 標籤，點擊可滑動/聚焦到對應的 Source Card 並提供 Tooltip 預覽。
- [x] **0.3 opendoc CLI 索引同步與排除優化**
  - [x] 支援 `.opendocignore` 檔案排除過濾，規則相容 `.gitignore` 通配符與預設忽略清單（node_modules、.git 等）。
  - [x] 同步優化（Update / Sync 模式）：實作檔案內容雜湊值（MD5/SHA256）比對。若伺服器已有相同路徑且內容雜湊一致的檔案，則跳過上傳與重新向量化。
  - [x] 確保更新檔案（同路徑但內容改變）時，後端正確更新 `source_path` 對應的 `content_hash` 與 `updated_at`，並重置 chunk 與索引，而非重複新增。
  - [x] 同步刪除（Sync Prune）：當對目錄進行 `index` 或同步時，若雲端資料庫中屬於該目錄之 `source_path` 的檔案，在本地已被刪除（不存在），應自動偵測並同步刪除雲端對應之 document，達到完全的單向目錄狀態同步。
- [ ] **0.4 RAG 檢索偏好優化 (Query Profile) 方案重新規劃**
  - [ ] 針對 Rust 新架構與向量混合檢索特性重新設計 `fast`、`balanced`、`precise` 三種方案的實質底層配置（例如：`fast` 僅使用 SQLite FTS5 全文檢索 + 輕量 Top-5 Chunk 且關閉 Reranker；`balanced` 啟用 LanceDB 向量 + Reranker 並回傳 Top-10；`precise` 啟用 FTS5 + LanceDB 雙路混合檢索 + 重度 Reranker 並回傳 Top-15 以求最高資訊覆蓋率）。
  - [ ] 修改後端 `chat.rs` 與 RAG 檢索引擎以落實此差異化行為。
  - [ ] 前端在介面上滑鼠懸停至 Fast/Balanced/Precise 選擇器時，呈現對應台灣在地化口吻的精準技術說明（不再僅是一個名詞）。

---

## 🎨 Phase 1: ChatGPT-Aligned WebUI & 漸進式 SSE 串流
- [ ] **1.1 WebUI Markdown 渲染與代碼高亮**
  - [ ] 支援標準 Markdown 代碼區塊高亮。
  - [ ] 程式碼區塊右上角添加一鍵「Copy」複製按鈕。
- [ ] **1.2 Axum chat_stream_handler 串流事件規範化**
  - [ ] 統一 SSE 輸出格式為 `StreamEvent`：包含 `Thought`（思考鏈）、`Text`（文本內容）、`Status`（狀態事件）。
- [ ] **1.3 前端打字機與思考鏈 UI 實作**
  - [ ] 實作打字機流流暢渲染（Typewriter effect）。
  - [ ] `Thought`（思考鏈）區塊之展開/摺疊 UI 與精緻動畫。

---

## 🎛️ Phase 2: 三欄式「行政工作艙」Tauri 2.0 桌面端
- [ ] **2.1 左欄：空間導航與檔案總管**
  - [ ] 整合 Workspace 快速切換器。
  - [ ] 樹狀 Collection 檔案總管元件。
- [ ] **2.2 中欄：拖曳上傳與就地編輯器**
  - [ ] 拖曳上傳 (Drag & Drop) 互動 UX。
  - [ ] 嵌入 React Canvas 試算表 (Spreadsheet)。
  - [ ] 嵌入 Monaco Editor 實現實體資產就地編輯與保存。
- [ ] **2.3 右欄：技能面板與安全卡片**
  - [ ] Skill 快速啟動面板。
  - [ ] UI Gatekeeper 安全審查門戶卡片（顯示工具調用權限請求）。

---

## 🔒 Phase 3: Stdio MCP 整合、離線加密授權與 UI Gatekeeper
- [ ] **3.1 本地 Stdio MCP 安全沙盒**
  - [ ] 將 OpenDocuments 設定為標準 MCP Server，透過 Stdio/IPC 管道與桌面端傳輸 JSON-RPC。
- [ ] **3.2 UI Gatekeeper 攔截審查機制**
  - [ ] 攔截 LLM 發送之任何 `tools/call`（例如：寫入本機檔案、調用 Python 腳本）。
  - [ ] 強制前端跳出黃色警報卡片，必須經由人類【Approve】後才放行執行。
- [ ] **3.3 License 離線防盜版機制**
  - [ ] 採集 CPU/主機板 UUID，經過 SHA-256 產出「Hardware Fingerprint 本機指紋」。
  - [ ] 實作非對稱加密（ECC 私鑰簽名 -> 本地公鑰解密），支援公部門內網 100% 離線授權校驗。
  - [ ] 數據庫記錄防倒退時間戳，防止用戶手動倒退系統時鐘白嫖。

---

## 🛒 Phase 4: 開源市集與一鍵發布生態
- [ ] **4.1 GitHub API 串接之 Skill 市集**
  - [ ] 桌面端內建 Skill 商店 Grid 佈局。
  - [ ] 支援一鍵下載 YAML/JSON 格式 Skill。
- [ ] **4.2 Skill Shield 市集簽章**
  - [ ] 桌面端只執行經過官方私鑰加密認證簽章的 Skill，防範惡意指令。
- [ ] **4.3 一鍵靜態出版 (GitHub Pages Publisher)**
  - [ ] 內建輕量 SSG 編譯器，自動將 RAG 整理好之成果出版至個人/校園的 GitHub Pages。
- [ ] **4.4 預置行政特化型預設 Skills**
  - [ ] 「地獄突發排代機 (CSP)」
  - [ ] 「兼代課鐘點費期末大對帳 (UTF-8 BOM Excel-safe)」
  - [ ] 「多元選修成果展一鍵出版」
