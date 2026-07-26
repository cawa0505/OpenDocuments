# OpenDocuments Rust 重構開源贊助與雙軌制戰略指南

## 🎯 核心願景
透過將 OpenDocuments 的 Parser 切片模組進行極致的 Rust-native 斷捨離，打造一個在開源 RAG 生態系中具備「降維打擊」實力的微型基礎設施積木（`semantic-office-parser` / `fast-doc-parser`），並以此為核心，敲開頂級科技巨頭與開源基金會的贊助大門。

---

## 🚀 雙軌制發佈戰略 (Matrix Strategy)

為了兼顧「技術極客的硬派肌肉展示」與「普通用戶的一鍵爽快安裝」，我們採取雙軌制發佈：

### 1. 軌道一：Linux 原生裸奔模式 (Native Mode)
* **文案**：「追求極致效能與 5 毫秒冷啟動？扔掉 Docker 與 node_modules，直接下載 25MB 的 Rust 單一二進位檔，搭配 systemd 實現無感常駐。」
* **受眾**：Homelab 用戶、效能偏執狂、Hacker News 技術社群。
* **技術亮點**：展現純 Rust 在底層整合（Systemd --user、WAL-optimized SQLite、LanceDB 0.10 本地極速檢索）上的極致硬實力。

### 2. 軌道二：跨平台一鍵 Docker 模式 (Container Mode)
* **文案**：「Windows / macOS 用戶？不想折騰環境、對齊編譯特徵與動態連結庫？一鍵 `docker compose up` 直接燙平環境。」
* **受眾**：一般開發者、小白用戶、跨平台調試場景。
* **技術亮點**：
  * 二階段編譯（Multi-stage build）。
  * 零 Node.js、零 Python 垃圾 runtime、靜態編譯連結。
  * 鏡像體積從 TypeScript 時代的 **1.5GB 暴扣至 30MB ！！！**

---

## 💰 科技巨頭開源贊助敲門磚

### 1. Google Open Source Peer Bonus Program (開源同儕獎金)
* **核心邏輯**：Google 瘋狂熱愛安全、純 Rust 且零外部 C 依賴的底層積木。
* **申請策略**：將 `opendoc-parser` 單獨抽離為獨立 Repo。README 只放三行：效能 Benchmark（Rust vs Python openpyxl 的記憶體與速度對比）。一旦吸引到 Google 工程師的目光，即可在 Google 內部獲得直接提名，獲取美金現金獎勵與官方背書。

### 2. Google Cloud for Open Source (GCP 算力贊助)
* **核心邏輯**：向開源專案提供免費的 GCP 額度用於 CI/CD 與 Benchmark 效能測試。
* **申請策略**：撰寫簡短 Proposal，強調專案在擺脫龐大 Docker、提升本地 AI RAG 隱私與端側運算效率上的貢獻。

### 3. Anthropic Claude & OpenAI 開源 Token 額度
* **核心邏輯**：有了 Google 或是社群的 Star 背書後，將整套極簡 TUI 知識庫展示給 LLM 供應商。
* **申請策略**：強調本專案如何幫助開發者在終端機（TUI）內實現無縫的 RAG 調試、顯著降低了對其 API 的無效調用，並提升了高質量 context 的重複利用率，以此申請大額的免費 API token 額度。

---

## 🛠️ 近期開發與重構節奏
1. **維持本地開發**：利用 Cargo Workspace 優勢，在同一個倉庫下進行高內聚開發，確保 Rust 重構 Phase 2/3、TUI 響應式、WAL-SQLite 與 Axum 大一統安全落地。
2. **準備拆分時機**：當 Phase 3 驗證完畢後，一鍵將 `crates/opendoc-parser-*` 獨立打包發佈，在 GitHub 開闢全新開源版圖。
