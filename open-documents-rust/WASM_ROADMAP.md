# OpenDocuments Next-Gen Roadmap: Frontend Wasm Revolution

## 核心願景 (The Mission)

徹底消滅瀏覽器端 JavaScript 在處理大檔案、大量 Chunking 與跨文本過濾時的記憶體暴漲與主執行緒卡頓。保留 Vue/TS 在 UI 渲染上的生態優勢，將「重度數據計算核心」以 Wasm 為媒介，用 Rust 物理降維打擊前端。

---

## 📅 階段一：Parser 積木化與 wasm-bindgen 橋接層 (Foundation)

- [ ] **封裝獨立 Crate `opendoc-parser-wasm`**
  - 在專案中解耦 `opendoc-parser`，確保其不依賴任何伺服器端（如 tokio 檔案系統）的原生 API，純粹以記憶體 Buffer（`&[u8]`）作為輸入。

- [ ] **定義強型別資料邊界 (TS/Rust Boundary)**
  - 利用 `serde-wasm-bindgen` 代替標準的 JSON 字串傳遞，最大化優化 JS 記憶體堆疊（Heap）與 Wasm 線性記憶體之間的零拷貝（Zero-copy）或極速序列化傳輸。

  型別宣告規格：
  ```rust
  #[wasm_bindgen]
  pub fn parse_document_to_chunks(file_buffer: &[u8], file_ext: &str) -> Result<JsValue, JsValue>
  ```

- [ ] **環境降級與流式處理 (Streaming Ingestion)**
  - 針對 `calamine` (Excel) 與 `docx-rs` (Word) 在 Wasm 環境下的相容性進行壓測，移除所有不相容的 C 動態庫依賴，確保 100% 純 Rust 實現。

---

## 📅 階段二：前端「零上傳」在地解析網絡 (Client-Side Edge RAG)

- [ ] **Vue 3 異步 Web Worker 整合**
  - 將編譯產出的 `.wasm` 與 `.js` 膠水代碼放入獨立的 Web Worker 中執行。
  - 鋼鐵防線：確保即使用戶拖入 100MB 的史詩級 Excel 報表，Rust 核心在背後算到冒煙，Vue 的主 UI 執行緒依然保持 60 FPS 絕對流暢，按鈕點擊零延遲。

- [ ] **實現本地端語義切片 (Local Semantic Chunking)**
  - 直接在瀏覽器端完成文本清洗、正則過濾、段落邊界判定與智慧 Chunk 切分。
  - 切分完成後，前端直接整理出 LanceDB 規格的 JSON 陣列，伺服器 API 轉變為「純儲存節點」，不再消耗伺服器任何 CPU 進行文件解析。

---

## 📅 階段三：互動式海量數據交叉過濾器 (Advanced Analytics)

- [ ] **動態文檔知識圖譜 (Knowledge Graph Compute)**
  - 當 WebUI 需要渲染包含數萬個節點的文檔關聯圖譜時，將圖論演算法（如最短路徑、社群發現演算法）交由 Wasm 端的 Rust 執行。

- [ ] **前端 BM25 / 精準分數過濾引擎**
  - 在後端傳回數千筆初步檢索結果後，由 Wasm 在前端進行二次即時布林過濾與動態權重重排（Rerank 預處理），滑動 UI 篩選條時畫面即時響應。

---

## 🛠️ 開發與編譯工具鏈備忘 (Tooling Stack)

編譯神器：使用 `wasm-pack` 進行出貨：

```bash
wasm-pack build --target web --release
```

優化保險絲：在 `Cargo.toml` 中為 wasm profile 開啟大小與速度優化：

```toml
[profile.release.package.opendoc-parser-wasm]
opt-level = "z"     # 物理優化二進位檔體積，確保網路載入極速
lto = true          # 開啟全域優化 (Link Time Optimization)
```
