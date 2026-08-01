# 📦 LoomCowork (Phase 1) 產品與技術規格書

## 一、 產品核心定位與三大商業亮點
LoomCowork 是一款私有化部署的 AI 數據與知識編織工作台（Knowledge Weaver IDE）。它不只是聊天工具，而是本機數據的生產力中樞。

* **亮點一：資產編織畫布（Asset Weaver Canvas）**
  打破傳統 AI 聊天室「對話完知識即死」的痛點。UI 採用雙欄設計，AI 提取的資料會直接在右欄「無中生有」長出結構化的 Markdown 教材或 CSV 數據表，使用者可直接在畫布中雙擊修改、調整章節，一鍵直接在本機硬碟生成實體專案檔，實現「對話即資產」。

* **亮點二：本機作業系統影子代理（Actionable OS Agent）**
  利用桌面端最高系統權限，內建安全 MCP (Model Context Protocol) 執行緒。當 AI 判定需要執行本機任務（如：修改 Obsidian 筆記、匯入本地 SQLite、執行驗證腳本）時，前端跳出終端機風格的審查面板（Gatekeeping），使用者點擊 Approve 後直接在本機 Process 幹活。

* **亮點三：零信任、零耗能的純血 Rust RAG**
  徹底拒絕臃腫的 Docker 與幾 GB 的 Python 運行期。後端核心與向量庫完全 Serverless 化，打包僅約 60MB，運行記憶體僅數十 MB，啟動 0.1 秒。搭配 BYOK (Bring Your Own Key) 機制，研發與維運 Token 成本為零，隱私防線拉滿。

## 二、 頂級架構技術選型 (Technological Stack)
為了兼顧「ChatGPT 級別的流暢體感」與「作業系統級的硬核操控力」，專案採用 Tauri 2.0 (Rust + Web) 混合架構Workspace，徹底焊死膠水層，封死 AI 寫 Mock 程式碼的退路。

```plaintext
+-----------------------------------------------------------------------+
| 前端外殼 (UI/UX Canvas)                                               |
| TypeScript + React 19 + Tailwind CSS + Shadcn UI + Monaco Editor      |
+-----------------------------------------------------------------------+
                                   │
              Tauri 2.0 原生 IPC 通訊 (強型別巨集命令綁定)
                                   │
+-----------------------------------------------------------------------+
| 後端大腦 (Rust Workspace Core Component)                              |
| ├── rag-engine  : LanceDB (Rust Serverless 向量庫)                    |
| ├── mcp-client  : 負責本地 Process 與 OS 工具鏈綁定                     |
| └── wasm-runner : wasmtime (沙盒化動態文件解析引擎)                     |
+-----------------------------------------------------------------------+
```

### 1. 前端 UI 棧（極致白嫖 Web 生態）
* **外殼框架**：Tauri 2.0 前端視窗。調用系統原生 Webview，前端 HTML/JS 在編譯時會被直接加密包進二進位執行檔內，具備天然的程式碼保護。
* **核心組件**：TypeScript + React 19 + Tailwind CSS。利用強型別鎖死前端與 Rust 之間的事件定義。
* **視覺與基礎組件**：Shadcn UI (Radix UI)。直接繼承地表最成熟的輸入框高度自適應、智慧滾動區域（ScrollArea）與動態表格基礎型態。
* **代碼與資產編輯器**：內建微軟開源的 Monaco Editor 核心，讓右欄畫布區直接擁有 VS Code 等級的語法高亮、點擊編輯與代碼補全功能。

### 2. 後端核心棧（純血 Rust 鋼鐵防線）
* **主控核心**：純 Rust Workspace。拒絕與 Go (Wails) 混用產生的 CGO 污染，徹底實現 Single Binary。
* **向量資料庫**：LanceDB (Rust 原生版本)。Serverless 架構，無須常駐後端 Process，直接以單一檔案讀寫本機向量數據，輕量且速度極快。
* **沙盒解析引擎**：wasmtime。所有上傳的未知問卷、私有格式檔案，一律丟進 WASM 虛擬機沙盒內執行清洗與解析，主程式完全免疫惡意程式碼與崩潰傳導。
* **LLM 串接與路由**：利用 reqwest 與自研的強型別 JSON Schema Client，直連遠端高階模型（BYOK）或本地 Ollama，強制模型執行 Structured Outputs，從源頭扼殺幻覺。

## 三、 介面互動與視覺版面規格 (UI/UX Spec)
採用全視窗滿版、無防漏邊邊的 雙欄工作區 (Workspace Dual-Panel) 設計。

### 1. 左欄：Chat 串流交談區（佔寬 40%）
* **漸進式 Markdown 渲染**：後端噴回的數據流進行即時增量解析（Incremental Parsing），打字機效果流暢，程式碼塊（Code Block）同步高亮。
* **推理過程折疊（Thought Process Toggle）**：AI 的深度推理鏈（`type: "thought"`）渲染在灰色可折疊區塊內，動態展開，回答完畢自動收起。
* **自動滾動鎖定 (Smart Scroll Anchor)**：AI 噴字時視窗自動釘在最底部；若使用者手動往上捲動檢查歷史，自動暫停滾動並懸浮顯示「↓ 有新訊息」按鈕。

### 2. 右欄：Artifacts 資產畫布區（佔寬 60%）
* **獨立滾動面（Isolated Scroll）**：右欄擁有自己獨立的滾動條，與左欄的對話瀑布流完全隔離，避免閱讀疲勞。
* **動態自適應表格引擎（Dynamic Table Engine）**：若 LLM 吐出系統未定義的自訂欄位，前端 JavaScript 自動提取 JSON 的 `Object.keys()`，動態拉出對齊的 HTML 表格 Header 與 Rows，保證弱型別兜底，最低限度正確呈現，軟體絕不崩潰。

## 四、 資料流與本機落地通訊協議 (IPC / SSE Protocol)
前後端通訊完全拋棄通靈 Markdown 的做法，採用強型別結構化事件流：

```typescript
type StreamEvent = 
  | { type: "text"; delta: string }                  // 打字機純文字
  | { type: "thought"; delta: string }               // 推理鏈碎片
  | { type: "status"; message: string }              // 本地狀態列提示
  | { type: "artifact_start"; id: string; format: "table" | "markdown" }
  | { type: "artifact_chunk"; id: string; chunk: any } // 資產數據的增量 append
  | { type: "artifact_end"; id: string }
```

### 📥 萬能本機落地與下載管線 (Download Pipeline)
資產畫布右上角固定常駐下載與落地按鈕：
* **本機直接落地（Tauri 原生）**：使用者點擊「存入專案」，後端 Rust 繞過瀏覽器限制，直接在實體硬碟建立資料夾，將 Markdown 或 CSV 寫入指定目錄。
* **前端萬能 Blob 下載（緩衝安全通道）**：
  * **CSV 匯出**：前端自動包裹雙引號並轉義，開頭強制注入 BOM 頭 (`\uFEFF`)，確保客戶用 Windows Excel 雙擊打開時，中文絕對不亂碼。
  * **Markdown 匯出**：將右欄 Monaco 編輯器內的最終文字直接打包成 `.md` 檔案下載。

## 五、 閉源商業化變現路徑 (Phase 1 檢驗指標)
* **授權費模式（Pure Profit License）**：軟體本體不包 Token，客戶自備 API Key (BYOK)，開發者賺取純粹的軟體授權與工作台工具費，沒有邊際成本。
* **企業隱私通行證**：純 Rust + LanceDB + WASM 沙盒的極致安全組合，是攻入對數據極度敏感的企業、Homelab、醫療、金融客戶的絕佳武器。
* **付費 WASM 插件擴充包**：未來特定的私有格式解析器（如特定企業的問卷、特殊 Log 格式），可編譯成加密的 `.wasm` 檔案作為付費增值組件，放入軟體內動態載入，變現想像力極大。
