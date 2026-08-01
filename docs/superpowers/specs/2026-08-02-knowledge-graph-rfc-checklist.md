# 🛠️ OpenDocuments 知識圖譜功能實作檢核文件 (v1.0)

本文件用於導引 rust-refactor 分支在 RAG 核心中加入輕量級 Markdown/OKF 知識圖譜的開發範疇。

## 🟥 第一階段：數據模型與解析器 (Data Model & Ingestion)
此階段目標是讓 Rust 後端在掃描文件時，除了提取文本算 Embedding 之外，還能抽離出「節點」與「邊」的結構。

- [ ] **1.1 強型別節點 (Graph Node) 結構設計**
  在 `crates/core` (或 `opendoc-rag`) 中定義 `GraphNode`，必須包含唯一 `doc_id`、`title`、`type` (行政/教學)、以及代表所屬區塊的 `workspace_id` 與 `collection_ids` 集合。
- [ ] **1.2 雙向邊 (Edges) 的邏輯實作**
  實作 `outbound_links: HashSet<String>`（此文件指向誰）與 `inbound_links: HashSet<String>`（誰指向此文件）。
- [ ] **1.3 OKF v0.1 Front Matter 解析器**
  整合 `serde_yaml`，專職解析 Markdown 頂部的 YAML 元數據（確保對齊教育部課綱指標或行政標籤）。
- [ ] **1.4 雙括號 [[WikiLink]] 正則提取器**
  撰寫高效的 Regex 模組，在文本 Chunking 前，先將內文中的 `[[target-doc-id]]` 提取出來，建立記憶體圖譜的邊。
- [ ] **1.5 物理與邏輯分離存放**
  確認：LanceDB 與圖譜 SQLite 數據均集中在 AppData 目錄（如 `~/.opendocuments/storage/`），使用者原始資料夾不產生任何系統碎屑檔案。

## 🟨 第二階段：記憶體圖譜管理器 (In-Memory Graph Manager)
此階段目標是在 OpenDocuments 運行時，提供一個微秒（$\mu s$）級別的圖形運算核心。

- [ ] **2.1 輕量圖拓撲引擎選型**
  評估使用原生 `HashMap<String, Vec<String>>` 實作鄰接表，或直接引入 `petgraph` 庫（若後續需要計算複雜的「最短教學路徑」或「法規相依權重」）。
- [ ] **2.2 Workspace 與 Collection 的「圖子集（Subgraph）」切片**
  設計過濾器，當前端透過 MCP 傳入指定的 `collection_id` 時，圖管理器能瞬間在記憶體中切出該集合的子圖（Subgraph），限縮檢索範圍。
- [ ] **2.3 增量更新與檔案監聽 (File Watcher) 聯動**
  當 Tauri 前端或本機 `Notify` 偵測到某個教案/公文被修改時，Rust 後端必須同時觸發：
  - 該檔案的向量重新計算（更新 LanceDB）。
  - 該檔案的連結重新解析（更新圖形鄰接表）。

## 🟩 第三階段：混合檢索演算法 (Hybrid Graph-Vector Retrieval)
這是 Graph RAG 的靈魂，決定了行政審查與教材重組時 AI 是否擁有「硬核邏輯線索」。

- [ ] **3.1 步驟一：語義粗篩 (Vector Search)**
  呼叫現有的 RAG 核心，從 LanceDB 撈出 Top-K（例如前 5 個）最接近輸入問題的文本 Chunks。
- [ ] **3.2 步驟二：拓撲擴展 (Graph Traversal / K-Step Hop)**
  從這 5 個向量節點出發，順著邊（Edges）向外延伸 1 到 2 步（1-Hop / 2-Hop），把與它們相連的「先修知識點」或「教育部法規條文」一併撈出來。
- [ ] **3.3 步驟三：重排與融合 (Reranking & Fusion)**
  撰寫融合演算法，將「語義相似度分數」與「圖譜關聯度權重」進行加權計算，組合出最終餵給 LLM 的最佳上下文（Context）。
- [ ] **3.4 預防孤立節點 (Isolated Node) 降級機制**
  若老師上傳的是全新、未建立任何連結的髒亂資料，系統必須自動降級為純向量檢索（Pure Vector RAG），且不引發系統崩潰。

## 🟦 第四階段：MCP 接口與安全門戶規格 (MCP & Gatekeeper)
此階段確保圖譜功能能完美服務於教師兼行政的多工現場。

- [ ] **4.1 擴充 MCP Tool 定義**
  在 MCP Server 中註冊以下新工具：
  - `get_concept_graph(collection_id)`：回傳前端畫布需要的圖結構 JSON。
  - `link_concepts(source_id, target_id)`：在兩個教材/公文間建立邏輯連結。
- [ ] **4.2 前端 Tauri React Flow / Canvas 數據對接**
  確保 Rust 後端回傳的 JSON 格式完美對齊前端渲染庫（如 React Flow 的 nodes 與 edges 陣列結構）。
- [ ] **4.3 跨 Workspace 知識穿透協議**
  設計一個特定的 MCP 接口，允許兼任行政的老師將「行政工作區」的審查標籤，跨區投射連結至「教學工作區」的個人教材中。
- [ ] **4.4 安全核准門戶卡片（Gatekeeper Card）攔截驗證**
  鋼鐵防線：當 AI 透過圖譜關聯分析，發出「自動修改本機 Excel 課表」或「重寫 Markdown 連結」的指令時，Tauri 後端必須強制攔截，並彈出黃色警告卡片等待人類 Approve。
