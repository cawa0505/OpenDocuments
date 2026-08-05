# 🗂️ OpenDocuments 系統架構與專案結構圖

語言：[English](../en/structure.md) | **繁體中文**

本文件定義並記錄了開源 RAG 核心（OpenDocuments）與桌面端（Desktop Client）整合之系統架構、儲存庫目錄結構與資料流規格。

---

## 🌐 1. 產品戰略定位

本專案採用 **「開源核心 (Open-Core) + 桌面控制艙」** 雙軌架構：
1. **OpenDocuments Core (Server + WebUI)**：主打 100% 本地、零信任 RAG 基礎設施。提供對齊 **ChatGPT/Gemini** 的左右對話流 WebUI。吸引極客與資安主管，建立極高的技術信任度。
2. **桌面客戶端 (Tauri 2.0)**：基於 Tauri 2.0 的單一輕量化安裝包，採用 **三欄式「行政控制台」**（「無腦拖曳、就地編輯、安全攔截、一鍵出版」）。

---

## 📦 2. 儲存庫與目錄結構

```plaintext
OpenDocuments/ (儲存庫根目錄)
├── apps/                            # 前端應用程式目錄
│   └── webui/                       # OpenDocuments WebUI (React 19 + Tailwind CSS)
│                                    # ─ 對齊 ChatGPT/Gemini 聊天流，Markdown 渲染與預設 Light Mode
│
├── crates/                          # 高度隔離之模組化 Rust Cargo Workspace
│   ├── opendoc-cli/                 # 主控伺服器 CLI 與進入點 (main.rs)，整合 TUI 與服務背景啟動
│   ├── opendoc-mcp/                 # Axum 路由、SSE 串流、MCP 協議及 API 控制器核心，內嵌前端 WebUI
│   ├── opendoc-storage/             # SQLite 與 LanceDB 向量儲存、FTS5 與 RAG 混合檢索
│   ├── opendoc-llm/                 # OpenAI-compatible BYOK 客戶端與漸進式 SSE 串流解析
│   ├── opendoc-types/               # 跨模組共享之強型別數據模型 (DocumentChunk, Tag, etc.)
│   └── opendoc-parser-*/            # 獨立沙盒化之各式文件格式解析器 (PDF, DOCX, XLSX, HTML, Email, Jupyter)
│
├── docs/                            # 內部架構、藍圖、任務與手冊歸檔區
│   ├── en/                          # 英文技術文檔
│   │   ├── structure.md             # 系統架構與目錄結構
│   │   ├── roadmap.md               # 多階段研發藍圖
│   │   ├── tasks.md                 # 任務清單與執行追蹤
│   │   └── tui-manual.md            # 終端機 TUI 使用手冊
│   └── zh-TW/                       # 繁體中文同步技術文檔
│       ├── structure.md
│       ├── roadmap.md
│       ├── tasks.md
│       └── tui-manual.md
│
├── docs-site/                       # 官方文檔網站 (VitePress)
├── openspec/                        # 系統級行為契約規格定義 (OpenSpec 1.7)
├── scripts/                         # 維護與自動化審查腳本
├── install.sh                       # 一鍵跨平台安裝腳本
└── README.md                        # 主索引與快速開始 (英文主版)
```

---

## 🛠️ 3. 後端 Rust 核心設計原則

1. **單一二進位 (Single Binary)**：
   拒絕外部 Node.js 執行進程，所有的資料庫讀寫 (SQLite)、向量檢索 (LanceDB)、LLM 連接、HTTP 服務以及 embedded React WebUI 均共享同一個原生 Rust 進程。
2. **記憶體與依賴隔離**：
   嚴格實施 Cargo Workspace 模組化與小檔案原則，避免巨型依賴的記憶體與編譯時間污染主程式。
3. **金鑰安全隔離 (Secrets Isolation)**：
   拒絕在 OS 級別持久化環境變數，所有 BYOK 的金鑰 (API Keys) 均加密儲存於 SQLite 專屬本地表中（權限 600），運行時僅載入記憶體。

---

## 📐 4. 資料流與檢索架構

### 4.1 資料寫入流程 (Ingestion Flow)
```plaintext
本地文件 ──> 解析器 Parser (文字/表格/PDF/DOCX)
  ──> 語意切片 Chunker (Semantic Split) 
  ──> 向量化 Embedder (ONNX Vectors) 
  ──> 雙路儲存 Storage (SQLite 屬性 + LanceDB 向量)
```

### 4.2 混合檢索流程 (Hybrid Query Flow)
```plaintext
使用者問題 ──> 問題向量化 Embedder
  ──> 雙路並行檢索 Retriever (LanceDB 向量相似度 + SQLite FTS5 全文檢索)
  ──> 重排 Reranker (RRF 排序與融合) ──> 上下文組裝 Context Window
  ──> 大型語言模型 Generator (BYOK) ──> SSE 漸進式串流
```
