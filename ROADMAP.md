# 🗺️ OpenDocuments 研發任務與實施藍圖 (ROADMAP.md)

本文件定義並追蹤 OpenDocuments (開源大腦 + WebUI) 與夥伴桌面端整合的核心研發任務、待辦事項、以及具體實作細節。

---

## 📌 當前研發總覽 & 優先順序

| 優先級 | 模組 / 任務領域 | 核心目標 | 狀態 | 預估工時 |
|---|---|---|---|---|
| **P0** | **WebUI BYOK LLM 配置 UI 補齊** | 在 SettingsPage 增加 provider/key 設定，徹底解決 WebUI 預設 Echo 模式與讀取 `STRUCTURE.md` 降級問題 | ⏳ 待開發 | 2 hrs |
| **P0** | **WebUI / API PARITY 完整度測試與收尾** | 對齊與驗證前端所有呼叫路由（特別是 Chat、Upload、Workspaces 等），確保 100%parit-y | ⏳ 待開發 | 3 hrs |
| **P1** | **Beyond-Text 知識圖譜實體落地 (Phase 1)** | 實作 WikiLink、YAML FrontMatter 解析，SQLite Graph 儲存，建構初始 Graph 檢索 | ⏳ 待開發 | 8 hrs |
| **P2** | **Tauri 桌面端整合與 Stdio MCP 沙盒** | Stdio 管道 JSON-RPC 通訊，以及人類審查 [Approve] 安全攔截卡片 (UI Gatekeeper) | ⏳ 待開發 | 12 hrs |

---

## 📋 各階段實施細節與任務清單 (Milestones)

### 🎨 Phase 1: ChatGPT-Aligned WebUI & 漸進式 SSE 串流 (當前重點)

#### 1.1 WebUI BYOK LLM 設定頁面實作 (`SettingsPage.tsx`)
- [ ] **API 綁定補齊**：
  - 於 `apps/webui/src/lib/api.ts` 新增 `llm_providers` 的 CRUD 請求綁定：
    - `GET /api/v1/admin/llm/providers` (列出)
    - `POST /api/v1/admin/llm/providers` (建立/更新)
    - `DELETE /api/v1/admin/llm/providers/:id` (刪除)
    - `POST /api/v1/admin/llm/providers/test-connection` (測試連線)
- [ ] **前端 UI 介面**：
  - 在設定頁面中新增一個專屬的 **「自備金鑰 (BYOK) 設定卡片」**。
  - 提供以下輸入欄位：
    - 服務名稱 (Name，例如 `LiteLLM`, `OpenAI`)
    - 介面類型 (Provider，例如 `openai`, `ollama`)
    - API 終端路徑 (Base URL，例如 `https://litellm.int.fotolove.top`)
    - 預設模型 (Model，例如 `gpt-4o`, `9router/moonshotai/kimi-k3`)
    - API 金鑰 (API Key，密碼輸入框遮蔽，寫入 SQLite `llm_providers` 表後不向前端回顯)
  - 實作 **「測試連線」** 按鈕與狀態反饋，連線成功顯示綠色 Checked 標籤。
  - 實作 **「啟用/設為預設」** 切換器。

#### 1.2 SSE 串流輸出標準化與打字機體感
- [ ] **Thought 展開與摺疊**：
  - 前端 React 聊天介面實作對齊主流 LLM 的 Thought 思考過程 UI 區塊，支援一鍵展開與摺疊。
  - 後端 `chat_stream_handler` 串流 SSE 事件輸出對齊，確保 `Thought` 區塊與 `Text` 區塊流暢分離。

#### 1.3 核心 APIParity 完整度驗證與除錯
- [ ] 執行完整的 `E2E` 自動化 API Parity 驗證，檢測所有 Axum 路由與 React WebUI 的相容性。

---

### 🌐 Phase 2: Beyond-Text 知識圖譜實體落地 (RAG 升級)

#### 2.1 數據結構定義 (`opendoc-types`)
- [ ] 實作強型別 `GraphNode` 與 `GraphEdge` 結構：
  ```rust
  pub enum EdgeType {
      Explicit,       // 顯示連結 (e.g. WikiLink)
      Semantic,       // 語意向量相似連結 (e.g. KNN vector edge)
      EntityShared,   // 共享實體/事件 (e.g. NER entity hub)
  }
  ```
- [ ] 實作 `Chunk` 元數據擴充。

#### 2.2 解析器升級 (`opendoc-parser`)
- [ ] 在 `opendoc-parser` 中內建 Markdown 的 `[[WikiLink]]` 雙向連結正則萃取器。
- [ ] 實作 YAML Front Matter 屬性解析，自動將 tags / aliases / categories 關聯至 GraphNode 元數據。

#### 2.3 SQLite 知識圖譜儲存與混合檢索 (`opendoc-storage`)
- [ ] 於 SQLite 中建立 `graph_nodes` 與 `graph_edges` 儲存表。
- [ ] 實作 RAG 混合檢索器：將 Text FTS5 檢索、向量 Reranker、與 Graph 拓撲查詢（點-邊-點延伸）加權混合。

---

### 🔒 Phase 3: Stdio MCP 整合與 UI Gatekeeper (桌面端專屬)

#### 3.1 本地 Stdio MCP 沙盒
- [ ] 確保 `opendoc start --mcp-only` 命令在 stdio 管道中 100% 穩定，不因 any logger 或 stdout 污染 JSON-RPC 串流。

#### 3.2 UI Gatekeeper 安全審查門戶
- [ ] 實作雙向認證攔截器：當 LLM 傳回 any `tools/call`（例如請求寫入本地檔案、或執行 Python 腳本）時，後端強制掛起該 Tool Call。
- [ ] 前端 Operator 欄彈出黃色警告卡片，展示預期執行的命令與影響。
- [ ] 必須等 Operator 人類點擊 `[Approve]` 釋放信號後，後端才放行執行。

---

### 🛒 Phase 4: 開源市集與一鍵出版生態

#### 4.1 官方 Skill 商店 Grid
- [ ] 串接 GitHub API 讀取官方 Skills 倉庫，並在 WebUI/桌面端展示一鍵下載/更新 Grid 介面。

#### 4.2 靜態 SSG 一鍵出版
- [ ] 內建一鍵靜態 SSG 出版引擎：提取整理好之 RAG 成果、Collection 文件結構，自動編譯為 VitePress/Static HTML，並一鍵發佈至 GitHub Pages。

---

## 📝 [待討論] 決策盲點

1. **`llm_providers` API key 儲存細節**：
   - 應確保加密演算法為 AES-256-GCM 且金鑰自適應（不持久化至環境變數，而是以硬編碼鹽與本機指紋動態生成），避免 SQLite 直接被拖庫導致 Key 洩漏。
2. **多平台 (Windows/macOS) 的二進位相容性**：
   - 使用 `rust-embed` 單一二進位化後，Windows 下 FTS5 的 DLL 載入與編譯是否需要靜態綁定，需在 CI 流程中加入 Windows build 驗證。

---
*最後更新時間: 2026-08-02*  
*更新者: 懶惰但高效的資深 AI 開發助理 (Ponytail Mode: lite)*
