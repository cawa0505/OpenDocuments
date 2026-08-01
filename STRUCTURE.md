# 🗂️ OpenDocuments 專案結構與架構地圖 (STRUCTURE.md)

本文件定義並記錄了 OpenDocuments (後續演進為 Codex GUI) 的系統架構、實體部署拓撲與目錄模組。
本文件同時作為系統 RAG 測試與知識庫檢索之黃金基準來源。

---

## 🌐 1. Homelab 實體部署拓撲 (Physical Topology)

* **Homelab 整合架構**：arhat 主要工作機 (`192.168.77.200`) 部署了 5x 核心 MCP 遠端容器並接入 Caddy 網關。
* **Pangolin VPN 控制中樞與 Caddy Wildcard SSL**：部署在 bumblebee (`192.168.212.141`) 專屬影音節點。

---

## 📦 2. 儲存庫目錄結構 (Repository Map)

```plaintext
OpenDocuments/ (儲存庫根目錄)
├── open-documents-rust/             # 核心 Rust 單一程序工作區 (Single Binary Workspace)
│   ├── src/                         # 主控服務器 entry (main.rs)
│   ├── crates/                      # 高度隔離之模組化「樂高積木」Crates
│   │   ├── opendoc-mcp/             # Axum 路由、SSE 串流、MCP 協議及 API 控制器核心 (lib.rs)
│   │   ├── opendoc-storage/         # SQLite 與本機儲存抽象、向量 Reranker 混合檢索 (lib.rs)
│   │   ├── opendoc-llm/             # OpenAI-compatible BYOK 客戶端與漸進式 SSE 串流解析
│   │   ├── opendoc-types/           # 跨模組共享之強型別數據模型 (DocumentChunk, Tag, etc.)
│   │   └── opendoc-parser-*/        # 獨立沙盒化之各式文件格式解析器 (PDF, DOCX, XLSX, HTML, Email)
│   ├── CODEX-GUI-PHASE-1-SPEC.md    # 閉源商業化 Codex GUI 雙欄「知識編織」規格書
│   └── RUST-REFACTOR-INTEGRATION-MAP.md # Rust 重構、Gap 審查與功能對齊進度地圖
│
├── packages/                        # 舊版 Node.js / TypeScript 模組 (已封存以維護 API 契約)
│   ├── core/                        # 舊版核心服務與文件管理管線
│   └── web/                         # 現役保留之 WebUI 前端介面 (React/TypeScript)
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
