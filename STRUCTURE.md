# 🗂️ LoomCowork™ & OpenDocuments 專案結構與架構地圖 (STRUCTURE.md)

本文件定義並記錄了開源 RAG 核心（OpenDocuments）與商業閉源桌面端（LoomCowork™）整合之系統架構、儲存庫目錄結構，以及長期的多階段開發任務藍圖。
本文件作為系統 RAG 測試與知識庫檢索之黃金基準來源。

---

## 🌐 1. 產品戰略定位：開源門面與商業控制艙

LoomCowork™ 採用 **「Open-Core (開源核心) + Product-Led Growth (產品驅動增長)」** 的雙軌推廣策略：
1. **OpenDocuments (開源大腦 + WebUI)**：主打 100% 本地、零信任 RAG 基礎設施。提供對齊 **ChatGPT/Gemini** 的左右對話流 WebUI。吸引極客與資安主管，建立極高的技術信任度。
2. **LoomCowork™ (商業桌面端)**：基於 Tauri 2.0 的單一輕量化安裝包（~60MB），採用 **三欄式「鋼鐵行政控制台」**。專為不具備 Prompt 技能、每天被破爛表格折磨的行政基層量身打造，主打 **「無腦拖曳、就地編輯、安全攔截、一鍵出版」**。

---

## 📦 2. 儲存庫與產品目錄結構 (Repository Map)

```plaintext
OpenDocuments/ (儲存庫根目錄)
├── open-documents-rust/             # 核心 Rust 單一程序工作區 (與 LoomCowork 桌面端共用 RAG 引擎)
│   ├── src/                         # 主控伺服器 entry (main.rs)
│   ├── crates/                      # 高度隔離之模組化「樂高積木」Crates
│   │   ├── opendoc-mcp/             # Axum 路由、SSE 串流、MCP 協議及 API 控制器核心 (lib.rs)
│   │   ├── opendoc-storage/         # SQLite 與本機儲存抽象、向量 Reranker 混合檢索 (lib.rs)
│   │   ├── opendoc-llm/             # OpenAI-compatible BYOK 客戶端與漸進式 SSE 串流解析
│   │   ├── opendoc-types/           # 跨模組共享之強型別數據模型 (DocumentChunk, Tag, etc.)
│   │   └── opendoc-parser-*/        # 獨立沙盒化之各式文件格式解析器 (PDF, DOCX, XLSX, HTML, Email)
│   └── docs/                        # [核心擴充] 產品及商業化規格書安全歸檔區
│       ├── LOOMCOWORK-PHASE-0-SPEC.md             # Phase 0 試用版與 OpenDocuments 整合規格
│       ├── LOOMCOWORK-PHASE-1-SPEC.md             # 閉源商業化 LoomCowork 雙欄「知識編織」規格
│       ├── LOOMCOWORK-MCP-DECOUPLING-SPEC.md      # 基於 MCP 協議之開閉源解耦架構規格
│       ├── LOOMCOWORK-MCP-RENDER-PIPELINE-SPEC.md # 基於 MCP 之檔案列表動態渲染與安全防護規格
│       ├── LOOMCOWORK-PLG-STRATEGY.md             # 開源信任與閉源變現雙軌並進商業推廣藍圖
│       ├── LOOMCOWORK-UI-UX-BLUEPRINT.md          # 開源 WebUI 與商業桌面端特化 UI/UX 佈局
│       ├── LOOMCOWORK-COMMERCIAL-LICENSE-SPEC.md  # 離線非對稱加密、機器碼鎖定與防時間倒流安全機制
│       └── LOOMCOWORK-HIGH-SCHOOL-ADMIN-SPEC.md   # 三大高中行政場景與對話式 Skill 產生器設計
│
├── packages/                        # 前端與客戶端模組
│   ├── web/                         # OpenDocuments 開源 WebUI (React 19 + Tailwind CSS)
│   │                                # ─ 對齊 ChatGPT/Gemini 聊天流，著重極致 Markdown 渲染
│   └── desktop/                     # [規劃中] LoomCowork™ Tauri 2.0 桌面客戶端
│       ├── src-tauri/               # Tauri 2.0 Rust 桌面外殼 (直接掛載 opendoc-storage/mcp Crate)
│       └── src/                     # 桌面端 React 專屬介面 (三欄式佈局：Loom Base, Canvas, Operator)
│
├── openspec/                        # 系統級行為契約規格定義 (OpenSpec 1.7)
└── STRUCTURE.md                     # 本文件 (系統架構與 RAG 測試之實體落地錨點)
```

---

## 🛠️ 3. 後端 Rust 核心設計原則

1. **單一二進位 (Single Binary)**：
   拒絕 CGO 或跨進程調用，所有的資料庫讀寫 (SQLite)、向量檢索、LLM 連接與 HTTP 服務均共享同一個原生進程。
2. **記憶體與依賴隔離**：
   嚴格實施 Cargo Workspace 模組化，避免巨型依賴 (如 PDF 渲染器、SSE 解析) 的記憶體與編譯時間污染主程式。
3. **金鑰安全隔離 (Secrets Isolation)**：
   拒絕在 OS 級別持久化環境變數，所有 BYOK 的金鑰 (API Keys) 均儲存於 SQLite 專屬加密/本地表中，運行時僅載入記憶體。

---

## 🗓️ 4. 多階段開發與任務對齊藍圖 (Roadmap & Milestones)

### 🚀 Phase 0：本地 MVP 驗證、API Parity 補齊與測試熔斷
* **進度 100% (已完成並通過端到端測試)**：
  * **Tags 系統**：實作 5 個標籤 CRUD 與貼標 Axum API，配合 SQLite 外鍵，完成隔離測試。
  * **複合篩選 (Complex Query)**：文件列表支援 status、sourceType 複合過濾，並支援 title、updated_at 多欄位動態 SQL 升降冪排序。
  * **BYOK LLM 層**：獨立 `opendoc-llm` crate，SQLite 加密儲存，支援 OpenAI 格式自備 API 金鑰，並實作 connection health check 診斷。
  * **結構化資產 (Extracted Assets)**：實作 `extracted_assets` 表，並完成資產 CRUD (CamelCase 格式與 SQLite 隔離綁定)。
* **防白嫖安全熔斷 (Time Bomb & Export Limiter)**：
  * 於後端 Rust 核心內建 45 天 **「Time Bomb 鋼鐵硬到期日」** 熔斷機制，到期自動提示並終止進程。
  * 限制導出額度：試用版允許無限建立 Workspace 與 Collection，但在實體導出 (CSV/ODS/Pages) 時，強行鎖死僅能輸出前 10 列，右欄 Monaco 畫布鎖定複製功能，建立 PLG 收割防線。

### 🎨 Phase 1：ChatGPT-Aligned WebUI & 漸進式 SSE 串流
* **目標**：讓開源 WebUI 的聊天與渲染體感完全對齊 ChatGPT/Gemini。
* **開發任務**：
  * 實作 WebUI 的 Markdown 與代碼區塊高亮，並提供 Copy 按鈕。
  * 將 Axum 的 `chat_stream_handler` 串流輸出標準化為 `StreamEvent` 封裝，吐出 Thought、Text、Status 事件。
  * 前端 React 實作打字機流流暢渲染、Thought 展開/摺疊摺疊。

### 🎛️ Phase 2：三欄式「行政工作艙」Tauri 2.0 桌面端
* **目標**：擺脫聊天流限制，打造資產與空間管理中心。
* **開發任務**：
  * **左欄 (The Loom Base)**：整合 Workspace 切換與樹狀 Collection 檔案總管。
  * **中欄 (The Cowork Canvas)**：實作拖曳上傳 (Drag & Drop) UX，並嵌入 React Canvas 試算表與 Monaco Editor 實現就地編輯實體資產。
  * **右欄 (The MCP Operator)**：整合 Skill 快速啟動面板與 UI Gatekeeper 安全審查門戶卡片。

### 🔒 Phase 3：Stdio MCP 整合、離線加密授權與 UI Gatekeeper
* **目標**：完成開閉源解耦，實現本地 Stdio MCP 安全沙盒。
* **開發任務**：
  * 將 OpenDocuments 設定為標準 MCP Server，透過 Stdio/IPC 管道向 LoomCowork 傳輸 JSON-RPC。
  * **UI Gatekeeper**：實作攔截機制，當 LLM 發送任何 `tools/call`（如寫入檔案或調用本地 Python）時，前端 Operator 欄強制跳出黃色警報卡片，必須經由人類【Approve】後才放行。
  * **License 防盜版矩陣**：
    * 採集 CPU/主機板 UUID 經 SHA-256 產出 **「Hardware Fingerprint 本機指紋」**。
    * 非對稱加密（ECC 私鑰簽名 $\to$ 本地公鑰解密），支援公部門內網 100% 離線授權校驗。
    * 數據庫記錄時間戳，防禦用戶手動倒退系統時鐘白嫖。

### 🛒 Phase 4：Loom Market 開源市集與一鍵發布生態
* **目標**：建立病毒式裂變，推廣至全台高中與公部門。
* **開發任務**：
  * **Loom Market**：串接 GitHub API 實現 Skill 市集 Grid 商店，支援一鍵下載 YAML/JSON 格式 Skill。
  * **Skill Shield (市集簽章)**：商業桌面端只執行經過官方私鑰加密認證簽章的 Skill，防止惡意指令或 trivial 複製。
  * **一鍵靜態出版 (GitHub Pages Publisher)**：內建靜態網站編譯器 (SSG)，自動把本地 RAG 整理好之成果出版至個人/校園的 GitHub Pages。
  * **高中場景特化**：預置「地獄突發排代機 (CSP)」、「兼代課鐘點費期末大對帳 (UTF-8 BOM Excel-safe)」、「多元選修成果展一鍵出版」三大真實場景 Skills。
