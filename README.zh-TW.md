<p align="center">
  <h1 align="center">OpenDocuments</h1>
  <p align="center"><strong>全 Rust 重寫、100% 本地優先、零信任的開源 AI 文件檢索 (RAG) 基礎設施</strong></p>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-TW.md">繁體中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-orange.svg" alt="Rust"></a>
</p>

---

## 🚀 為什麼選擇現代 Rust 重構？

OpenDocuments 最初受 Hono 與 Turborepo 啟發，以 TypeScript / Node.js 單一儲存庫 (Monorepo) 作為概念驗證。雖然該架構成功驗證了產品可行性，但為了徹底解決技術債、達到**零信任安全防線**與**極致執行效率**，我們以現代 Rust 進行了全面的底層重構：

1. **真正單一二進位檔 (Single Binary Distribution)**：舊版 Node.js 需要複雜的進程管理、動態 Node 依賴解析與 Port 綁定協調。新版 Rust 透過 `rust-embed` 將完整 Axum API 路由與 React WebUI 靜態資產直接編譯進二進位檔記憶體，完全不需任何外部 Node.js 執行環境。
2. **微秒級確定性記憶體佔用**：不再為了 PDF 解析、文字切片、SQLite 索引與向量運算而啟動多個動輒佔用 150MB+ 的 JS 執行階段。Rust 核心將所有子系統封裝於單一 OS 執行緒池中，進行微秒級的高效排程。
3. **Rust 原生內嵌式儲存**：SQLite (FTS5) 屬性索引與 LanceDB 向量相似度檢索直接內嵌於二進位進程中，完全消除跨進程 IPC 通訊與 C-binding 效能瓶頸。
4. **巨幅效能提升**：文字提取、語意切片與混合檢索 (RRF) 查詢規劃在 Rust 原生執行圖下提升了 **5 倍至 15 倍的速度**，即便在資源受限的基層硬體或 Homelab 設備上也能流暢運作。

---

## ⚡ 效能實測數據 (Performance Benchmark)

OpenDocuments 專為資源受限環境（如學校、公部門或老舊辦公電腦）優化，完全清理了傳統 Node.js 時代的架構負擔。

以下是使用 `hyperfine` 對 10,000 列複雜行政 Excel 表格進行索引解析時，舊版 TypeScript/Node.js 與新版 Rust 核心的實測對比：

| 指標 | 舊版 (TypeScript / Node.js) | 新版 (Rust Core) | 實質提升 |
| :--- | :--- | :--- | :--- |
| **冷啟動 / 靜置記憶體** | ~180 MB | **~18 MB** | **節省 90% RAM** |
| **解析與切片延遲** | ~14.25 秒 | **0.83 秒** | **快 17 倍** |
| **二進位檔大小 / 依賴** | 龐大 `node_modules` 依賴 | **單一二進位檔 (內嵌 WebUI)** | **零外部依賴** |

<details>
<summary>🔍 點擊查看 hyperfine 基準測試命令與原始 Log</summary>

```bash
# 測試環境: AMD Ryzen 5 5600GT, 64GB RAM, Linux (CachyOS)
# 測試工具: hyperfine --warmup 3

Benchmark 1: opendoc document index admin_heavy.xlsx
  Time (mean ± σ):     827.0 ms ±   6.2 ms    [User: 22.2 ms, System: 12.2 ms]
  Range (min … max):   819.6 ms … 835.5 ms    10 runs
```
</details>

---

## 什麽是 OpenDocuments？

**OpenDocuments 是一個開源、100% 本地部署的 RAG (檢索增強生成) 平台，能將散落的文件轉化為可供 AI 檢索的知識庫。** 它能解析多種複雜格式，透過向量與關鍵字雙路混合檢索，並精準附帶引文出處來回答自然語言問題。

適合使用 OpenDocuments 的場景：

- 需要**替代昂貴企業級 AI 檢索與專有知識庫工具**的私有化方案。
- **具備精準引文出處** 的 PDF、DOCX、XLSX、在地檔案與網頁 AI 文件搜尋。
- **完全在地化 RAG 架構**：可搭配 Ollama 離線運作，確保敏感公文與機密文件絕不出境。
- **AI 程式碼助理知識庫**：透過標準 MCP (Model Context Protocol) 協議，無縫對接 Claude Code、Cursor、Windsurf 等開發工具。
- **單一二進位檔高效運作**：Axum 後端與前端 React WebUI 完全打包於記憶體中，隨點隨用。

單一指令完成安裝並啟動：

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
opendoc start --port 3000
```

瀏覽器開啟 `http://localhost:3000`，立即開始建置專屬知識庫！

---

## OpenDocuments 如何運作？

OpenDocuments 會**連接您的檔案來源**、**解析並進行語意切片**、**將屬性儲存於 SQLite，向量儲存於 LanceDB**，接著透過**混合檢索、重排與引文生成**提供精準解答。每一份回答均附帶引用來源、信心分數與原文連結。

簡而言之：**OpenDocuments 是專為機構與團隊打造的私有、零信任 AI 文件搜尋引擎。**

---

## 核心功能特色

| 特性 | 實質價值 |
|------|----------|
| **私有化 RAG** | 在您自己的安全基礎設施上運行完整的文件檢索堆疊。 |
| **附帶引文解答** | 提問自然語言，並可精準追溯回答所依據的檔案章節與頁碼。 |
| **混合雙路檢索** | 結合 LanceDB 稠密向量搜尋、SQLite FTS5 關鍵字搜尋與互惠排名融合 (RRF)。 |
| **單一二進位套件** | 透過 `rust-embed` 將 Axum 後端與 React 前端完美打包，絕不發生 Port 衝突或資產遺失。 |
| **豐富檔案支援** | 原生支援 Markdown、PDF、DOCX、XLSX、CSV、HTML 與原始碼檔案。 |
| **在地或雲端模型** | 支援在地 Ollama，亦可對接 OpenAI、Anthropic、Google 與 xAI 等 API。 |
| **MCP Server 支援** | 讓 Claude Code、Cursor、Windsurf 等 AI 編輯器直接搜尋您的內部知識庫。 |
| **工作區邏輯隔離** | 支援基於 Workspace 與 Collection 的數據邏輯隔離，確保資料邊界清晰。 |

---

## 技術架構 (Rust Cargo Workspace)

OpenDocuments 採用模組化的 Cargo Workspace 設計：

```
apps/
  webui/           - React SPA (Vite + Tailwind CSS) 前端介面
crates/
  opendoc-cli      - 主控 CLI 與終端機進入點 (opendoc)
  opendoc-mcp      - Axum API 伺服器、SSE 串流與 MCP 協議核心
  opendoc-tui      - 基於 Ratatui 的輕量級終端機 RAG 介面
  opendoc-storage  - SQLite 屬性與 LanceDB 向量混合檢索儲存庫
  opendoc-llm      - OpenAI 兼容客戶端與漸進式串流解析器
  opendoc-types    - 跨模組強型別數據模型 (DocumentChunk, Tag 等)
  opendoc-parser-* - 獨立沙盒化之各式文件格式解析器 (PDF, DOCX, XLSX 等)
```

---

## 系統設定

OpenDocuments 採用標準 TOML 設定檔，儲存於 `~/.config/opendocuments/config.toml`。

首次執行 `opendoc` 時將自動生成預設設定：

```toml
[server]
url = "http://127.0.0.1:3000"

[database]
path = "~/.opendocuments"      # 資料庫檔案存放目錄

[model]
default_workspace = "default"  # 系統啟動時的預設工作區
active_workspace = "MyWorkspace"    # 當前作用中的工作區
score_threshold = 0.60             # RAG 檢索相似度門檻
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx"
```

---

## 快速上手

使用 OpenDocuments CLI 啟動在地 AI 文件搜尋引擎的最快方式：

### 1. 安裝 OpenDocuments

**方法 A：一鍵指令安裝（推薦）**

下載並安裝預編譯好的單一二進位檔 (Linux / macOS)：

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
```

**方法 B：從原始碼編譯**

```bash
# 複製儲存庫並將二進位檔安裝至 ~/.cargo/bin/opendoc
make install
```

### 2. 啟動伺服器

```bash
opendoc start --port 3000
```

開啟 `http://localhost:3000` 即可進入 Web 介面並開始建置索引！

### 3. 終端機 CLI 使用

您也可以直接使用 CLI 進行本地文件索引與查詢：

```bash
# 切換至指定工作區
opendoc workspace switch "MyWorkspace"

# 建立本地檔案或資料夾索引
opendoc document index /path/to/docs

# 直接在終端機提問
opendoc ask "我們的驗證系統是如何運作的？"
```

---

## ❤️ 支持與贊助 (Support & Sponsorship)

OpenDocuments 堅持 100% 開源、廠商中立與社群導向。如果 OpenDocuments 為您節省了硬體成本、保護了文件隱私，或是提升了日常行政效率，歡迎考慮支持本專案的持續維護：

- **GitHub Sponsors**：[贊助 OpenDocuments 專案](https://github.com/sponsors/cawa0505)

### 贊助資金運用方向

- **核心基礎設施維護**：持續優化 Linux, macOS 與 Windows 上的零依賴、極速單一二進位檔建置管線。
- **在地模型效能優化**：深化 ONNX / WASM 在地重排與向量量化，讓舊設備也能獲得高精度 RAG 體驗。
- **開源核心承諾**：確保核心 RAG 與 MCP 服務永久免費、開源且無商業鎖定。

---

## 授權條款 (License)

本專案採用 MIT 授權條款，詳情請參閱 [LICENSE](LICENSE) 檔案。
