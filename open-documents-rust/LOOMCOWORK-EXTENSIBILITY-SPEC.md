# 🔌 LoomCowork (Phase 1) 自訂 Skills、MCP 串接與 WASM 插件生態規格書 (LOOMCOWORK-EXTENSIBILITY-SPEC.md)

本文件定義並記錄了 LoomCowork 閉源商業化（Proprietary/Commercial）路線下，針對「自訂 Skills」、「MCP (Model Context Protocol) 核心」與「WASM 獨立沙盒插件生態」的完整產品與技術規格架構。

---

## 🛠️ 1. 自訂 Skills (Custom Skills) 實作架構

在 LoomCowork 的工作台定義中，一個 Skill（技能） 本質上就是：
**一個特定的系統提示詞 (System Prompt) + 綁定一組特定的輸入 Schema + 預設的右欄渲染格式。**

我們直接在前端提供一個「技能編輯 IDE 面板」（基於 Monaco Editor），免去使用者動態載入二進位的危險性。

### 1.1 Skill 配置結構 (儲存於 SQLite `custom_skills` 表)
當使用者在 UI 點擊「新增 Skill」時，會透過 JSON 編輯器定義其 DNA：

```json
{
  "skill_id": "extract_course_syllabus",
  "name": "課程大綱編織器",
  "description": "從原始文獻中提取特定主題，並在右欄編織成 Markdown 教材",
  "system_prompt": "你是一個資深教授。請檢索使用者提供的文獻，並嚴格按照指定的 JSON 格式輸出課程大綱...",
  "output_format": "markdown_canvas", 
  "user_inputs": [
    { "key": "target_audience", "label": "目標讀者", "type": "string", "default": "初學者" },
    { "key": "total_hours", "label": "預計總時數", "type": "number", "default": 4 }
  ]
}
```

### 1.2 前端動態表單與變數拼接 (UI 互動流)
* **動態表單生成**：當使用者在左欄聊天區切換到某個 Skill 時，聊天輸入框上方會自動根據 `user_inputs` 動態渲染對應的控制元件（例如：下拉選單、數字滑桿、輸入框）。
* **變數合成管線**：在使用者發送訊息時，前端會讀取這些自訂變數，將它們與 `system_prompt` 拼接，透過 Tauri IPC 發送給 Rust 後端，由 Rust 調用使用者自備的進階模型 (BYOK)，最終將精準的結構化資料噴回右欄。

---

## 🔌 2. 本機 MCP (Model Context Protocol) 整合架構

MCP 是目前 Anthropic 主導、地表最強大的 AI 工具對接標準。它讓 LLM 可以主動向客戶端申請「讀寫檔案」、「查詢資料庫」、「執行終端機命令」。

在 Tauri 的純 Rust 後端中，我們將其實作為一個 **「MCP Client 核心」**，向下控管各種 MCP Server，向上對接 LLM 的 Tool Call，並在中間加上安全審查閘門（Gatekeeping Gate）。

### 2.1 後端 Rust 的 MCP 管理器 (MCP Client)
在 Rust Workspace 中，建立 `mcp-client` 模組。使用者在設定面板中填入想引入的本機或遠端 MCP 伺服器配置（例如官方的 filesystem 或 postgres 伺服器）：

```json
{
  "mcpServers": {
    "local_file_system": {
      "command": "node",
      "args": ["/path/to/mcp-server-filesystem.js", "/users/username/projects"]
    }
  }
}
```
* **工作原理**：當 LoomCowork 啟動時，Rust 的 `mcp-client` 會讀取此設定，利用 `std::process::Command` 在背景拉起這些 MCP Server 進程，並透過標準輸入輸出（Stdio）與其保持 JSON-RPC 通訊，動態獲取這些伺服器提供的 Tools 清單並與 LLM 共享。

### 2.2 核心安全閘門：安全核准門戶 (UI Gatekeeper)
當高級推理模型（如 Claude 3.5 Sonnet 或 o3）在思考過程中，判定需要調用某個 MCP 工具時（例如：將右欄整理好的 CSV 寫入使用者的實體硬碟），後端 Rust 絕不默默執行。
1. **攔截請求**：Rust 的 MCP Client 攔截到 LLM 的 Tool Call 請求。
2. **推播至前端**：Rust 透過 Tauri Event 將工具名稱與詳細參數發送給前端。
3. **前端終端機審查卡片**：左欄的對話流中，打字機特效暫停，立刻跳出一個極具極客感的安全審查組件：
   ```plaintext
   [⚠️ MCP 安全授權請求]
   工具名稱: local_file_system/write_file
   寫入路徑: /users/username/projects/syllabus.csv
   寫入內容: "..."
   [拒絕]  [ 執行核准 ✓ ]
   ```
4. **命定落地**：使用者點擊 Approve，前端發送允許訊號回 Rust，Rust 命令底層的 MCP Server 執行實體硬碟寫入，並在右欄或左欄彈出成功提示 `[✓] 檔案已成功寫入本機目錄`。

---

## 📦 3. WASM 獨立沙盒插件生態 (WASM Sandboxed Plugins)

將 RAG 與問卷系統獨立為 WASM 插件（沙盒擴充包），在閉源商業化（Tauri）的思維下，是一個極度性感的「付費解鎖機制」或「企業訂製生態」。
這能讓 LoomCowork 核心保持極致輕量（~60MB），而將特定行業、特定髒亂數據的「高價值外掛引擎」交由 WASM 插件動態加載，且完全限制在 `wasmtime` 沙盒內安全執行，主程式免疫崩潰。

以下為 4 大商業領域、共 12 個極具變現價值的 WASM 插件擴充包提案藍圖：

### 📈 3.1 金融與財報審計擴充包 (Financial & Auditing)
金融業的資料對隱私極度敏感，且檔案格式極其髒亂、充滿嵌套表格。
1. **「上市財報 PDF 表格神抽手」插件**
   * **痛點**：財報 PDF 表格經常跨頁、合併儲存格，一般 RAG 讀進去直接變成文字垃圾。
   * **WASM 職責**：利用 Rust 專門解析 PDF 向量路徑的邏輯，精準把線條轉成網格，將跨頁表格還原成標準 JSON，讓 AI 能在右欄直接生出「歷年毛利比較表.csv」。
2. **「銀行對帳單與流水帳自動稽核」插件**
   * **痛點**：不同銀行匯出的 CSV、PDF 對帳單，欄位名稱、日期格式（明國紀年、西元紀年）完全不一樣。
   * **WASM 職責**：動態清洗並標準化所有銀行的流水帳，統一輸出為 Transaction 強型別，供 AI 影子代理直接比對是否有異常交易或矛盾。
3. **「併購盡職調查 (Due Diligence) 跨合約核對」插件**
   * **痛點**：企業併購時，需要交叉核對上百份採購、租賃合約與主條款之間是否存在法規與金額衝突。
   * **WASM 職責**：專門抽取法律條文之實體、定義段落並在 WASM 沙盒內建立高度壓縮的暫時性對比樹，大幅節約 Token 消耗。

### 🩺 3.2 醫療病歷與學術文獻擴充包 (Healthcare & Life Sciences)
1. **「DICOM 影像報告與病歷文本對齊」插件**
   * **痛點**：醫療影像（DICOM）的 Meta 數據與醫生的純文字病歷報告是分離的，RAG 很難跨模態檢索。
   * **WASM 職責**：在沙盒內提取 DICOM 標籤資訊，與臨床病歷（EMR）的純文字進行關聯性 Chunking，讓醫生能用左欄提問：「幫我找出所有病灶大於 2cm 且患者有糖尿病的病歷，整理成研究表格。」
2. **「PubMed 學術論文生醫實體識別 (NER)」插件**
   * **痛點**：生物醫學論文裡有一堆複雜的基因代號、藥物英文長字串，普通 LLM 經常張冠李戴。
   * **WASM 職責**：內建輕量級的本地正則或字典矩陣，在將論文送給 LLM 之前，先把所有的藥物、基因標註出來（Pre-labeling），逼 LLM 執行強型別結構化輸出時不得出錯。
3. **「臨床藥物相互作用 (DDI) 安全警示」插件**
   * **痛點**：醫生開藥時，需要確認多種處方藥之間是否存在致命的化學衝突。
   * **WASM 職責**：利用本地打包好的 DDI 交互作用資料庫（完全不聯網），即時審查病歷中的藥物清單，在對話發送前置入安全邊界警告。

### 🏢 3.3 企業人資與極端格式問卷擴充包 (Enterprise & HR Survey)
1. **「私有問卷／心理測驗極端橫向掃描」插件**
   * **痛點**：企業內部收集上來的員工滿意度、多選題問卷，有些是 Word 檔畫的表格，有些是 txt，格式混亂。
   * **WASM 職責**：這個插件專門生啃複雜的 Word XML 結構，把隱藏在合併儲存格裡的「員工開放式建議」精準撈出來，轉化為 JSON 陣列，直接觸發右欄的資產畫布，生成「員工痛點分析報告.md」。
2. **「獵頭專用：百大格式履歷結構化」插件**
   * **痛點**：求職者投遞的履歷五花八門（104 格式、CakeResume、自製 PDF/Word），HR 根本無法統一篩選。
   * **WASM 職責**：抹平所有履歷格式的排版差異，將「學歷、年資、核心技能、期望薪資」抽取成萬能的動態欄位（Map 結構），在右欄拉出一張「動態求職者比對大表格」，直接提供 CSV 下載。
3. **「企業員工考核 (OKR/KPI) 對齊與盲點診斷」插件**
   * **痛點**：跨部門 OKR 常流於形式，缺乏核心指標與業務流的實質對齊。
   * **WASM 職責**：解析大批量的部門 OKR 樹，在 WASM 沙盒內進行拓撲排序（Topological Sort），揪出互相矛盾或斷鏈的目標。

### 🛠️ 3.4 開發者與 IT 運維專用擴充包 (Developer & IT DevOps)
1. **「髒亂 Log 串流與死機線索編織」插件**
   * **痛點**：伺服器崩潰時，產生的 Nginx Log、Linux System Log、資料庫 Log 動輒幾百 MB，文字又臭又長，LLM 的 Token 根本塞不下。
   * **WASM 職責**：利用 Rust 的高速字串處理能力，在本地預先將 90% 的正常 Log（200 OK）過濾掉，只留下發生 Error 前後 5 秒的異常上下文片段（Context Context），進行精簡化 Chunking 再送給高級推理模型。這時 AI 就能在右欄直接幫工程師寫出「死機原因排查與修復腳本.sh」。
2. **「Swagger / OpenAPI 接口文檔自動教材生成」插件**
   * **痛點**：後端工程師丟出一隻幾萬行的 JSON API 文檔，前端工程師常常看不懂怎麼接。
   * **WASM 職責**：解析龐大的 OpenAPI JSON，將其拆解為模組，讓使用者下指令：「幫我把這套 API 整理成給『前端實習生』看的對接基本教材」，右欄直接長出帶有 Monaco Editor 代碼高亮的精美 Markdown 教材，支援一鍵匯出。
3. **「舊專案結構地圖 (Legacy Codemap) 自動導航與死代碼標記」插件**
   * **痛點**：接手大型舊專案（遺留代碼、無奈重構）時，難以釐清真實的調用鏈路與廢棄模組。
   * **WASM 職責**：利用 AST（抽象語法樹）在沙盒內高速生成拓撲關聯圖，提取未被引用的函式與無用欄位，讓 RAG 僅精確索引「活著的程式碼」。

---

## 💡 4. 閉源商業化變現哲學

在 LoomCowork 的商業藍圖裡，這套設計的精妙之處在於：
1. **主專案輕裝上陣**：核心底盤保持在 60MB 以內，無懈可擊的啟動速度，不含任何垃圾依賴。
2. **安全隔離的插件體系**：WASM 沙盒隔離機制提供 100% 安全感，就算插件崩潰或存在惡意代碼，也絕對無法波及主程式與客戶的 OS。
3. **高價值的行業護城河**：金融、醫療、開發者插件切中「剛性降本增效」痛點，能單獨作為付費擴充包銷售，毛利率極高，沒有雲端維運與 Token 邊際成本。
