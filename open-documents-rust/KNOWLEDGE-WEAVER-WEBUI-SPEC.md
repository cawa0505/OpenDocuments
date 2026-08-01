# 📖 Private Knowledge Weaver (私有化知識編織工作站)
## WebUI 核心實作與前後端事件流規格書 (Phase 1)

本文件紀錄了 **OpenDocuments / LoomCowork** 雙欄「知識編織工作台」的核心前端規格、通訊協定事件流、以及後端防禦與雙通道設計，便於未來隨時存取與討論。

---

## 🎨 一、 雙欄工作區 (Workspace Dual-Panel) 視覺與體驗規格

全視窗滿版設計（No Scrolling Canvas），兩欄獨立滾動。

### 1. 左欄：Chat 串流交談區（佔寬 40% ~ 45%）
* **打字機特效 (SSE Stream Rendering)**：
  - 後端傳回的 Markdown 必須即時解析（Incremental Markdown Parsing），禁止等整段話完才顯示，防止字體閃爍。
  - 使用高性能 Markdown 解析器（如 `react-markdown` 搭配 `remark-gfm`），並對代碼區塊進行動態語法高亮。
* **自動滾動鎖定 (Smart Scroll Anchor)**：
  - AI 在噴字時，若使用者沒有手動往上捲動，聊天視窗必須自動保持在最底部。
  - 若使用者往上捲動檢查歷史，必須立刻暫停自動滾動，並在右下角顯示輕量「↓ 有新訊息」懸浮按鈕，點擊滑動回底部。
* **輸入框動態高度 (Auto-resizing Input)**：
  - 支援 `Shift + Enter` 換行。
  - 輸入框根據文字行數動態長高（Max Height 設為 200px），超過後轉為內部滾動，文字永不溢出。

### 2. 右欄：Artifacts 資產畫布區（佔寬 55% ~ 60%）
* **獨立滾動面（Isolated Scroll）**：右欄的表格、教材 Markdown 擁有自己獨立的滾動條，與左欄的對話瀑布流完全隔離。
* **狀態切換器（Skeleton Loader）**：當左欄出現開啟資產（`artifact_start`）的訊號時，右欄立刻亮起並進入骨架屏（Skeleton）加載狀態，隨後數據噴入時平滑淡入呈現。
* **漸進式資產渲染（Incremental Artifact Rendering）**：當收到 `artifact_chunk` 時，右欄的表格或 Markdown 必須即時將資料 append 進去。使用者會看到表格橫列（Rows）一列列跳出來，或文章一節節長出來，而非最後啪一聲閃現。

---

## 🔄 二、 前後端資料流通訊規格 (Stream Event Framing)

為了達到極致流暢體感，前後端通訊採用結構化事件流。

### 1. 前端 TypeScript 事件型別定義

```typescript
type StreamEvent = 
  | { type: "text"; delta: string }                    // 打字機字串碎片 (左欄)
  | { type: "thought"; delta: string }                 // AI 的思考鏈推理過程 (左欄，可折疊)
  | { type: "status"; message: string }                // 當前動作狀態（如：正在檢索向量庫...）
  | { type: "artifact_start"; id: string; format: "table" | "markdown" } 
  | { type: "artifact_chunk"; id: string; chunk: any }   // 資產資料的增量更新 (平滑 append 進右欄)
  | { type: "artifact_end"; id: string }                // 結束資產生成
```

* **推理折疊（Thought Process Toggle）**：AI 的思考過程（`thought`）渲染在專門的灰色小區塊內，預設展開，生成完畢後自動折疊收起。

### 2. 後端 Rust 事件型別宣告

利用 `serde` 屬性完美對應前端的 Tagged Union 契約：

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 打字機文字碎片 (左欄)
    Text { delta: String },
    /// AI 思考鏈推理過程 (左欄，預設折疊)
    Thought { delta: String },
    /// 當前動作狀態 (左欄，例如 "正在檢索向量庫...")
    Status { message: String },
    /// 開啟右欄畫布並進入 Skeleton Loader
    ArtifactStart { id: String, format: String }, // format: "table" | "markdown"
    /// 資產資料的增量更新 (平滑 append 橫列)
    ArtifactChunk { id: String, chunk: serde_json::Value },
    /// 結束資產生成，啟用下載按鈕
    ArtifactEnd { id: String },
}
```

---

## 🛡️ 三、 防禦性雙通道資料設計 (Gold & Adaptive Channel)

確保後端不崩潰、前端不漏字、下載不亂碼。

```plaintext
[使用者提問]
   │
   ▼
1. 語意檢索 (Vector Search) ➔ 從 LanceDB 精準撈出相關 Chunk
   │
   ▼
2. 知識壓縮 (Synthesis) ➔ 將撈出的碎片，用強型別 JSON 提取成「知識特徵矩陣」
   │
   ▼
3. 結構化生成 ➔ 最終再交給 LLM，限定它「只能根據步驟 2 的強型別矩陣」編織教材，防堵幻覺
```

### 1. 🌟 黃金通道 (Standard Schema)
* **後端硬防禦**：定義 Rust 強型別結構體（如 `CurriculumOutline`）實作 `serde_json` 序列化。
* **LLM 導引**：在 System Prompt 使用 JSON Schema 強制約束（Structured Outputs），保證 LLM 100% 吐出匹配此結構的資料。
* **前端體驗**：識別 `asset_type == 'curriculum_syllabus'`，啟動專屬渲染器，以「章節卡片 + 引用來源標籤」高美感 100 分完美呈現。

### 2. 🛡️ 自適應通道 (Dynamic Schema)
* **後端海綿**：使用者給出未定義指令時，後端利用 `Vec<serde_json::Map<String, Value>>` 吞下未知 JSON。
* **前端通靈自適應**：前端動態讀取 JSON 物件的 Key 自適應生成 `<table>`，生成 80 分實用表格，資訊完全不漏：

```typescript
function RenderDynamicTable({ artifactData }) {
  if (!artifactData || artifactData.length === 0) return <Skeleton />;
  const headers = Object.keys(artifactData[0]);
  return (
    <div className="artifact-table-container">
      <table>
        <thead>
          <tr>{headers.map(key => <th key={key}>{key}</th>)}</tr>
        </thead>
        <tbody>
          {artifactData.map((row, idx) => (
            <tr key={idx}>
              {headers.map(key => <td key={key}>{String(row[key] ?? '')}</td>)}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

---

## 📥 四、 萬能下載與落地管線

下載按鈕位於右欄右上角，觸發純前端的 Blob 轉檔下載。

1. **BOM 頭 CSV 匯出（防 Windows Excel 亂碼）**：
   - 欄位字串自動包裹雙引號 `"`，並將內部的雙引號轉義為 `""`。
   - **核心安全細節**：字串開頭強制加上 **BOM 頭 (`\uFEFF`)**，確保繁體中文環境使用者直接雙擊打開 CSV 時 100% 正常解碼不亂碼。
2. **Markdown 匯出**：
   - 直接將右欄累積的純文字打包成 `type: "text/markdown;charset=utf-8"` 的 Blob 下載，副檔名為 `.md`。
3. **精美 HTML/A4 列印 (Paged Media)**：
   - 前端寫一組乾淨的 `@media print` CSS。
   - 設定好 `@page { size: A4; margin: 20mm; }`，隱藏左欄交談瀑布流，使用者點擊「列印 PDF」直接呼叫 `window.print()`，瀏覽器即可生成完美的 A4 講義。
