# 🗺️ OpenDocuments 研發任務與實施藍圖 (ROADMAP.zh-TW.md)

本文件定義並追蹤 OpenDocuments (開源大腦 + WebUI) 與夥伴桌面端整合的核心研發任務、優化路徑以及具體實作細節。

*語言*：[English](ROADMAP.md) | **繁體中文（同步版）**

---

## 📌 當前研發總覽與優先順序

| 優先級 | 模組 / 任務領域 | 核心目標 | 狀態 | 預估工時 |
|:---:|:---:|:---|:---:|:---:|
| **P0** | **WebUI BYOK LLM 配置 UI 補齊** | 在 SettingsPage 增加 provider/key 設定，解決預設 Echo 模式問題 | ⏳ 開發中 | 2 hrs |
| **P0** | **WebUI / API Parity 完整度測試** | 對齊與驗證前端所有呼叫路由，確保 100% 契約相容 | ⏳ 開發中 | 3 hrs |
| **P1** | **互動式 TUI 終端介面全新升級** | 實作多 Tab 分頁、Chunk 詳情 Modal 彈窗、事件抽屜與按鍵閃爍反饋 | ⏳ 規劃中 | 8 hrs |
| **P1** | **Beyond-Text 知識圖譜實體落地 (Phase 1)** | 實作 WikiLink、YAML FrontMatter 解析與 SQLite Graph 混合檢索 | ⏳ 規劃中 | 8 hrs |
| **P2** | **Tauri 2.0 桌面端與 Stdio MCP 沙盒** | Stdio 管道 JSON-RPC 通訊，以及人類審查 [Approve] 安全攔截卡片 | ⏳ 規劃中 | 12 hrs |

---

## 📋 各階段實施細節與任務清單 (Milestones)

### 🎨 Phase 1: ChatGPT-Aligned WebUI & 漸進式 SSE 串流

#### 1.1 WebUI BYOK LLM 設定頁面實作 (`SettingsPage.tsx`)
- [ ] **API 綁定補齊**：
  - 於 `apps/webui/src/lib/api.ts` 新增 `llm_providers` 的 CRUD 請求綁定：
    - `GET /api/v1/admin/llm/providers`
    - `POST /api/v1/admin/llm/providers`
    - `DELETE /api/v1/admin/llm/providers/:id`
    - `POST /api/v1/admin/llm/providers/test-connection`
- [ ] **前端 UI 介面**：
  - 提供金鑰遮蔽與 SQLite `llm_providers` 表加密儲存。
  - 實作「測試連線」按鈕與即時健康狀態反饋。

#### 1.2 互動式 TUI 終端介面全新升級 (`crates/opendoc-tui`)
- [ ] **Chrome 佈局與主題系統**：
  - 切分 Header/Tabs、Main Workspace、Event Log Drawer 與 Flash Footer。
  - 抽離 `theme.rs` 模組實現統一色彩板。
- [ ] **檢索結果互動選取與 Chunk 詳情 Modal 彈窗**：
  - 支援鍵盤 (`Up`/`Down`/`j`/`k`) 與滑鼠點擊選擇結果列。
  - 按 `Enter` 彈出 Modal 展示 Chunk 完整內文、相似度分數拆解 (Vector vs. BM25) 與檔案路徑。
- [ ] **多 Tab 視圖架構**：
  - `Tab 1`：🔍 Search & Inspector (檢索與深查)
  - `Tab 2`：📊 Workspace Document Matrix (工作區文件陣列與資料庫健康度)
- [ ] **事件抽屜與 Footer 閃爍反饋**：
  - 可折疊底部抽屜 (`L` 鍵) 記錄背景查詢日誌與耗時 (ms)。
  - 觸發快捷鍵時 Footer 標籤瞬時閃爍高亮。

---

### 🌐 Phase 2: Beyond-Text 知識圖譜實體落地 (RAG 升級)

#### 2.1 數據結構定義與解析 (`opendoc-types` & `opendoc-parser`)
- [ ] 實作強型別 `GraphNode` 與 `GraphEdge` 結構。
- [ ] 內建 Markdown `[[WikiLink]]` 雙向連結萃取與 YAML FrontMatter 解析器。

#### 2.2 SQLite 知識圖譜儲存與混合檢索 (`opendoc-storage`)
- [ ] 於 SQLite 中建立 `graph_nodes` 與 `graph_edges` 儲存表。
- [ ] 實作混合 RAG 檢索器：融合 FTS5 文字、向量 Reranker 與 Graph 拓撲延伸。

---

### 🔒 Phase 3: Stdio MCP 整合與 UI Gatekeeper (桌面端專屬)

#### 3.1 本地 Stdio MCP 沙盒
- [ ] 確保 `opendoc start --mcp-only` 命令在 stdio 管道中 100% 穩定通訊。

#### 3.2 UI Gatekeeper 安全審查門戶
- [ ] 攔截 LLM `tools/call` 請求（寫入檔案或執行腳本），跳出黃色警報卡片待 Operator 人類點擊 `[Approve]` 後放行。

---

### 🛒 Phase 4: 開源市集與一鍵出版生態

#### 4.1 官方 Skill 商店 Grid
- [ ] 串接 GitHub API 展示與一鍵下載 Skill。

#### 4.2 靜態 SSG 一鍵出版
- [ ] 內建一鍵 SSG 出版引擎，自動將 RAG 成果編譯發布至 GitHub Pages。
