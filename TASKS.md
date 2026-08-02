# 📋 OpenDocuments 待開發任務清單 (TASKS.md)

本文件依據 `STRUCTURE.md` 與當前開發狀態，整理出後續所有待開發任務，作為後續執行的追蹤基準。

---

## 🚨 核心優先任務 (High Priority)
- [x] **0.1 WebUI LLM Provider 設定介面與 BYOK 整合** (對齊 `llm_providers` 表)
  - [x] 於 WebUI `SettingsPage.tsx` 補上完整的 LLM Provider 設定介面（支援自訂名稱、Provider 種類、Base URL、Model Name、API Key 遮罩輸入）。
  - [x] 串接後端 `GET /api/v1/admin/llm/providers` / `POST /api/v1/admin/llm/providers` / `DELETE` 與 `POST /api/v1/admin/llm/providers/test`，提供「一鍵連線測試」與「啟用開關」。
  - [x] 確保使用者設定並啟用 BYOK LLM 後，Chat 與 Chat Stream 徹底告別 Echo / 拼接 `STRUCTURE.md` 的 Fallback 模式，並與 SQLite 完美同步。

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
