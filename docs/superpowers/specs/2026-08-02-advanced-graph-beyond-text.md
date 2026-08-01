# 🕸️ 跨越文本的超文字圖譜規格書 (Beyond-Text Graph RAG Specification)

本文件定義了開源 RAG 核心（OpenDocuments）與閉源商業桌面端整合之「高階知識圖譜拓撲與多模態關聯檢索」之架構設計與 Ingestion 規格，專為行政 Binary 髒數據（PDF、PPTX、XLSX、影音）之潛在關係比對與教材重組量身打造。

---

## 🎨 1. 核心概念：超文字圖譜的三維拓撲矩陣

在高中教學與行政實務現場，有超過 80% 的核心資產均為 Binary 檔案。傳統僅能依賴 Markdown `[[WikiLink]]` 連結的圖譜無法處理這類髒數據。因此，我們在後端 Rust 核心引進**三維拓撲邊（Edge）**：

### 🟢 概念一：多模態錨點圖譜 (Multi-Modal Anchor Graph)
* **目的**：將二進位檔案（如 PPTX、PDF 掃描檔）中的「局部視覺插圖」與「相鄰文字」直接對齊成圖譜節點。
* **機制**：
  1. **視覺抽離**：在 Ingestion Pipeline 中解析 PPTX 投影片時，將裡面的插圖（如：17 世紀熱蘭遮城復原圖）與旁邊的說明文字提取為獨立的 `Image Node`。
  2. **多模態對齊**：利用輕量化的本機多模態模型（如 CLIP，經由 Rust `candle` 框架運行），將圖片與文字進行相似度計算。當使用者搜尋「校園古蹟活化補助公文」時，語義檢索撈到了公文，圖譜則因「視覺語意相似度」自動將「熱蘭遮城復原圖」跨檔案、跨格式連線拉過來。

### 🟡 概念二：潛在語義拓撲 (Latent Semantic Topology - LST)
* **目的**：拋棄人類手動建立連結的繁瑣，由 Rust 直接以「向量距離的數學幾何」隱形織網。
* **機制**：
  1. **KNN 自動織網 (K-Nearest Neighbors Graph)**：當 Excel 課表或 Word 講義被切成 100 個 Chunks 進 RAG 後，它們在多維空間呈現為一堆點群。
  2. **距離閾值阻斷**：後端會在本機執行一個密集的幾何計算，若 `Chunk A` (代課假單 Excel Row) 與 `Chunk B` (行政公文 PDF) 的向量餘弦相似度大於閾值 $\epsilon$（例如 0.85），則自動在記憶體圖譜中為其連上一條 **`Semantic Edge`**。

### 🔴 概念三：實體與事件共振網 (Entity-Event Resonance Network - EERN)
* **目的**：針對高中行政現場的「硬核實體/代數」（教師姓名、科目代碼、教育部法規文號、學期）自動交叉感染與聚集。
* **機制**：
  1. **NER 正則提取器**：Ingestion 階段透過 Rust 的 Regex 模組高速過濾提取特定實體：
     * **法規代碼**：`臺教授國字第\d+號`
     * **學期/科目**：`高[一二三](?:特選)?(?:國文|英文|地理|歷史)`
     * **人名與超鐘點**：`[張王李劉趙]\w{2}`、`超鐘點`
  2. **共振中心 (Resonance Hub) 綁定**：
     * 檔案 A (Excel 鐘點費清冊) 👉 包含 `張翠芬`、`超鐘點`
     * 檔案 B (Word 教學進度表) 👉 包含 `張翠芬`、`高一英文`
     * 檔案 C (PDF 教育局公文) 👉 包含 `超鐘點` 時數限制
     * 系統自動生成一個虛擬的實體節點 `張翠芬`，並將這三個二進位檔案的 Chunk 強行連線。當組長點開「張翠芬」節點，即可一眼看清她的薪資、進度表，以及頭頂高懸的限制法規。

---

## 🛠️ 2. Rust 資料結構擴充 (Crates/core/src/graph/beyond_text.rs)

我們在 Rust 圖譜管理器中，將一條邊（Edge）的型別抽象為多態列舉（Enum）：

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum EdgeType {
    /// 人類明確寫下的連結 (e.g. [[doc-id]] 或 Front Matter)
    Explicit,
    /// 向量空間中幾何鄰居 (餘弦相似度大於閾值 epsilon)
    Semantic { similarity: f32 },
    /// 共享特定硬核實體 (如 姓名、公文字號、科目代碼)
    EntityShared { entity: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: EdgeType,
    pub weight: f32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChunkMetadata {
    pub doc_id: String,
    pub page_or_row_index: u32,
    pub source_file_type: String, // ".xlsx", ".pptx", ".pdf", ".md"
    pub extracted_entities: HashSet<String>,
}
```

---

## 🎛️ 3. 雙欄 UI 控制與混合檢索 (Hybrid Retrieval)

在前端 Tauri / React Flow 畫布上，提供極致掌控感的**關係滑桿（Relationship Slider）**：

1. **圖譜過濾器（Graph Filter）**：
   * 允許使用者拉動過濾滑桿：「只想看人類建立的硬連結（Explicit）」👉 「開啟 AI 幫我找出來的隱形教材關聯（Semantic）」👉 「開啟行政名詞交叉網（EntityShared）」。
2. **混合檢索（Hybrid RAG）混合演算法**：
   * 步驟一：語義粗篩（Vector Search Top-K Chunks）。
   * 步驟二：從向量節點出發，依據使用者拉動的滑桿過濾條件，順著 Edges 向外延伸進行 K-Step 遍歷，擴充上下文。
   * 步驟三：提供 `Isolated Node` 的退級（Fallback）機制，若為全新未連線之二進位檔案，自動安全降級為純向量檢索，防範系統崩潰。
