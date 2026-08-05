<p align="center">
  <h1 align="center">OpenDocuments</h1>
  <p align="center"><strong>以 Rust 重構之高效能、零信任私有化 AI 文件檢索與 RAG 基礎設施 (支援 PDF, DOCX, XLSX, 本機檔案與網頁)</strong></p>
</p>

<p align="center">
  <a href="README.md">English</a> | 繁體中文
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="授權條款"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-orange.svg" alt="Rust 語言"></a>
</p>

---

## 🚀 為什麼選擇以現代 Rust 重構？

OpenDocuments 最初靈感來自 TypeScript / Node.js 單體儲存庫 (使用 Hono 與 Turborepo)。雖然該架構成功驗證了概念，但為了徹底解決技術債、滿足零信任隱私與極致資源效率需求，我們進行了**全數以現代 Rust 重頭重構**：

1. **真正單一二進位檔 (Single-Binary Distribution)**：透過 `rust-embed` 將 Axum API 路由與 React WebUI 靜態資產直接編譯進二進位檔記憶體中，只需單一執行檔即可運行，零外部依賴。
2. **確定性記憶體佔用 (Deterministic Memory Footprint)**：無需啟動多個動輒佔用 150MB+ 的 Node.js 執行階段，Rust 執行階段將所有子系統封裝在單一高效能 OS 執行緒池中，提供微秒級排程。
3. **Rust 原生內嵌儲存 (Embedded Storage)**：透過 SQLite (FTS5) 處理詮釋資料與全文檢索，並透過 LanceDB 處理向量相似度，完全內嵌於單一進程，消除 IPC 跨進程開銷與慢速 C 語言綁定橋接。
4. **顯著效能提升**：文字解析、語意切片與互惠排名融合 (RRF) 查詢規劃在 Rust 原生執行圖下可達到 **5x 至 15x 的速度提升**，即使在舊型或低規格 Homelab 設備上也能即時回應。

---

## ⚡ 實測效能基準 (Performance Benchmark)

OpenDocuments 已全數以 Rust 重構，以清空技術債並針對資源受限環境（如學校或公部門舊型電腦）進行深度優化。

以下為傳統 TypeScript/Node.js 實作與全新 Rust 核心的極速對比（使用 `hyperfine` 實測 10,000 列複雜行政 Excel 檔案）：

| 評測指標 | 舊版 (Node.js) | 現代 (Rust 核心) | 改善幅度 |
| :--- | :--- | :--- | :--- |
| **冷啟動 / 待機記憶體** | ~180 MB | **~18 MB** | **節省 90% RAM** |
| **檔案解析與切片延遲** | ~14.25 秒 | **0.83 秒** | **提速 17 倍** |
| **二進位體積與依賴** | 龐大的 `node_modules` | **單一執行檔 (內嵌 WebUI)** | **零外部環境依賴** |

<details>
<summary>🔍 點擊查看 hyperfine 基準測試命令與日誌</summary>

```bash
# 測試環境: AMD Ryzen 5 5600GT, 64GB RAM, Linux (CachyOS)
# 評測工具: hyperfine --warmup 3

Benchmark 1: opendoc document index admin_heavy.xlsx
  Time (mean ± σ):     827.0 ms ±   6.2 ms    [User: 22.2 ms, System: 12.2 ms]
  Range (min … max):   819.6 ms … 835.5 ms    10 runs
```
</details>

---

## 什麼是 OpenDocuments？

**OpenDocuments 是一個開源、可私有化部署的 RAG (檢索增強生成) 基礎設施，能將分散的檔案轉化為可供 AI 檢索的私有知識庫。** 它能解析排版複雜的文件，透過「向量 + 關鍵字」混合檢索建立索引，並生成附帶精準出處引用的自然語言解答。

適合使用 OpenDocuments 的場景：

- 需要**替代商業化企業 AI 搜尋**與昂貴 SaaS 知識庫工具。
- 需要支援 PDF, DOCX, XLSX, 本地檔案與網頁的 **附帶引用出處 AI 文件搜尋**。
- 需要搭配 Ollama **全程在地執行的私有化 RAG 架構**，確保敏感文件絕不上雲。
- 透過 MCP 協議為 **AI 編程助手 (Claude Code, Cursor, Windsurf 等)** 提供本地私有知識庫。
- 需要 **Rust 原生單一執行檔**，從記憶體同時提供 API 服務與嵌入式 WebUI。

單一指令完成安裝與啟動：

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
opendoc start --port 3000
```

開啟 `http://localhost:3000`，建立文件索引並開始提問與檢視引用出處。

---

## 🤝 AI Provider 與生態系合作夥伴

OpenDocuments 作為 **Token 高效型 RAG 網關 (Token-Efficient RAG Gateway)**，旨在將私有文件知識庫無縫連接至各大前沿 AI 模型與 LLM 供應商。

### 1. Token 成本深度優化 (降低 70%+ 上下文浪費)
結合 **LanceDB 稠密向量**、**SQLite FTS5 稀疏關鍵字索引** 與 **RRF (Reciprocal Rank Fusion) 互惠重排**，OpenDocuments 能在建構 Prompt 上下文前精準過濾無關片段，將 Token 開銷**降低 70%+**，大幅提升呼叫 **Claude 3.7 Sonnet**、**GPT-4o/o3-mini**、**Google Gemini 1.5 Pro**、**Grok 3** 與 **Ollama** 時的回答品質與回應速度。

### 2. 標準化 BYOK 與通訊協定相容性
- **BYOK (自備 API 金鑰)**：金鑰加密儲存於本地 SQLite 表 (`600` 權限)，零遠端追蹤，絕不洩漏至前端或第三方伺服器。
- **OpenAI & Anthropic 相容**：開箱即用支援漸進式 SSE 串流與統一模型路由。
- **Model Context Protocol (MCP)**：作為標準 Stdio/IPC MCP Server，讓 **Claude Code**、**Cursor** 與 **Windsurf** 等開發者工具能安全檢索本地私有文件。

### 3. 歡迎 AI 模型與雲端供應商合作 (Grants)
我們非常歡迎 AI 模型廠商、API 聚合服務與雲端基礎設施供應商 (如 Anthropic, OpenAI, Groq, Together AI, Google Cloud, AWS) 透過 **API Grants / 測試額度** 進行合作。贊助額度將用於：
- 自動化 CI/CD 評測管線，持續測試最新 LLM 模型的檢索與對齊能力。
- 測試多模態與長上下文模型在複雜文件上的召回精準度。
- 維護 100% 免費開源、廠商中立的核心組件，服務全球開發者社群。

---

## 核心功能特色

| 功能特性 | 說明 |
|---------|------|
| **私有化 RAG 部署** | 在您自己的安全基礎設施上運行完整的 AI 文件搜尋系統。 |
| **附帶出處引用** | 以自然語言提問，並能明確查看支撐解答的原始文件章節與頁碼。 |
| **混合檢索 (Hybrid)** | 結合稠密向量搜尋、SQLite FTS5 關鍵字搜尋、重排 (Reranking) 與父文件召回。 |
| **單一二進位封裝** | Axum 後端與 React WebUI 透過 `rust-embed` 打包為單一執行檔，零外部資源需求與 Port 衝突。 |
| **豐富檔案格式** | 原生支援 Markdown, PDF, DOCX, XLSX, CSV, HTML 與程式碼解析。 |
| **本地或雲端模型** | 可選擇完全在地運行 Ollama，或連接 OpenAI, Anthropic, Google, xAI 等雲端服務。 |
| **MCP 伺服器** | 讓 Claude Code, Cursor, Windsurf 等 MCP 客戶端直接檢索您的內部知識庫。 |
| **工作區邏輯隔離** | 支援基於 Workspace 與 Collection 的邏輯隔離，確保資料邊界安全。 |

---

## 技術架構 (Modern Rust Workspace)

OpenDocuments 採用模組化 Rust Cargo Workspace 設計：

```
apps/
  webui/           - React SPA (Vite + Tailwind CSS) 前端
crates/
  opendoc-cli      - 主控 CLI 與終端介面 (opendoc)
  opendoc-mcp      - Axum API 伺服器、SSE 串流與 MCP 協定核心
  opendoc-tui      - 基於 Ratatui 的輕量化終端 RAG 介面
  opendoc-storage  - SQLite 詮釋資料與 LanceDB 向量混合檢索儲存庫
  opendoc-llm      - OpenAI 相容 LLM 客戶端與漸進式串流解析器
  opendoc-types    - 跨模組共享之強型別資料模型 (DocumentChunk, Tag 等)
  opendoc-parser-* - 沙盒化之各式文件格式解析器 (PDF, DOCX, XLSX 等)
```

---

## 設定檔說明

OpenDocuments 使用位於 `~/.config/opendocuments/config.toml` 的標準 TOML 設定檔。

第一次執行 `opendoc` 時將會自動建立並初始化預設設定：

```toml
[server]
url = "http://127.0.0.1:3000"

[database]
path = "~/.opendocuments"      # 資料庫檔案存放基礎目錄

[model]
default_workspace = "default"  # 系統啟動時建立的預設工作區
active_workspace = "MyWorkspace"    # 當前作用中工作區
score_threshold = 0.60             # RAG 檢索相似度過濾門檻值
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx"
```

---

## 快速開始 (Quick Start)

這是使用 OpenDocuments CLI 啟動本地 AI 文件搜尋引擎最快的方式。

### 1. 安裝 OpenDocuments

**選項 A：一鍵安裝 (推薦)**

下載並安裝預先編譯好的單一執行檔 (Linux / macOS)：

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
```

**選項 B：從原始碼編譯**

```bash
# 複製儲存庫並安裝統一執行檔至 ~/.cargo/bin/opendoc
make install
```

### 2. 啟動伺服器

```bash
opendoc start --port 3000
```

開啟 `http://localhost:3000` 即可進入 Web UI 開始建立索引！

### 3. 命令行指令使用

您也可以直接使用 CLI 指令進行文件索引與提問：

```bash
# 切換至特定工作區
opendoc workspace switch "MyWorkspace"

# 建立本地檔案/資料夾索引
opendoc document index /path/to/docs

# CLI 快速提問
opendoc ask "我們的驗證系統是如何運作的？"
```

---

## ❤️ 贊助與專案支持 (Support & Sponsorship)

OpenDocuments 為 100% 開源、廠商中立且由社群驅動的專案。如果 OpenDocuments 幫您節省了硬體成本、保護了文件隱私，或提升了行政工作效率，歡迎支持本專案的持續維護與開發：

- **Solana (SOL)**：[`4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF`](https://solscan.io/account/4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF)
  ```text
  4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF
  ```

### 贊助資金用途

- **核心基礎設施**：維護跨 Linux, macOS 與 Windows 的零依賴、極速單一二進位檔編譯。
- **本地模型優化**：持續推進嵌入式 ONNX / WASM 本地重排模型與向量量化，服務低規格硬體。
- **開源核心承諾**：確保核心 RAG 與 MCP 伺服器功能永遠 100% 免費且完全開源。

---

## 授權條款 (License)

本專案採用 MIT 授權條款 - 詳見 [LICENSE](LICENSE) 檔案。
