# 🗺️ OpenDocuments 開發藍圖

[English](../en/roadmap.md) | **繁體中文**

本藍圖記錄了 OpenDocuments 開源專案的工程目標與戰略里程碑。

---

## 🎯 階段總覽

| 階段 | 名稱 | 核心焦點 | 狀態 | 目標時程 |
| :--- | :--- | :--- | :--- | :---: |
| **Phase 0** | **核心 MVP 與 BYOK 網關** | Axum 服務、BYOK 金鑰管理、現行 LanceDB 檢索、標籤系統、CLI | ⏳ 進行中 | 2026 Q3 |
| **Phase 1** | **對齊 ChatGPT 流暢 WebUI** | React 19 WebUI、打字機 SSE 串流、互動 Citation 出處、代碼高亮 | ⏳ 進行中 | 2026 Q3 |
| **Phase 2** | **任務執行層與原生 AI 引擎** | 以 `TaskExecutor` 解耦 parse/embed/rerank/infer；llama.cpp (Vulkan/HIP) + fastembed (CPU)；可選 Spur 批次運算 | 📋 規劃中 | 2026 Q4 |

---

## 🚀 Phase 2：任務執行層與原生 AI 引擎 (規劃中)

規格：[`openspec/specs/task-execution-ai-engines`](../../openspec/specs/task-execution-ai-engines/spec.md)
參考：[`docs/ref/zh-TW/task-execution-ai-engines-verification.md`](../ref/zh-TW/task-execution-ai-engines-verification.md)

- [ ] **Phase 0 — 基線強化**：async `SearchBackend` 簽名、`[ai]`/`[task]` 設定解析、call-site 稽核。
- [ ] **Phase 1 — Task 與 AI 抽象層（CPU）**：`opendoc-task`/`opendoc-ai`/`opendoc-ai-fastembed`；upload→embed→LanceDB；真實 `LanceDbRetriever`。
- [ ] **Phase 2 — llama.cpp GPU backend**：`opendoc-ai-llamacpp`（feature-gated，Vulkan/HIP）；embed/rerank/infer；執行期備援。
- [ ] **Phase 3 — Spur 整合（選用）**：`SpurDaemonExecutor`（Mode 1）、`opendoc-worker daemon` + scale-to-zero（Mode 3）、批次 ETL（Mode 2）。
- [ ] **Phase 4 — 生成切換**：設定 `[ai.models.inference]` 時以 llama.cpp 原生 SLM；否則 BYOK 不變。

---

## 🚀 Phase 0：核心 MVP 與 BYOK 網關（進行中）

- [x] **單一二進位 Axum 架構**：整合 `rust-embed` 內嵌前端 WebUI 靜態資產。
- [x] **BYOK LLM 層**：SQLite 加密儲存自備 API 金鑰，支援 OpenAI 格式並具備連線健康診斷。
- [x] **現行檢索引擎**：LanceDB 稠密向量搜尋 + LanceDB FTS，搭配 RRF 重排。
- [ ] **目標混合檢索**：新增由核心管理的 SQLite FTS5 稀疏文字路徑；不可將 LanceDB FTS 誤寫成 SQLite FTS5。
- [ ] **LanceDB Engine 邊界**：`lancedb-engine-sidecar` 規格批准後，將 LanceDB／Arrow／DataFusion 移至由核心管理的私有 sidecar。
- [x] **標籤與複合條件過濾**：標籤 CRUD、文件狀態/類型過濾，以及動態升降冪排序。
- [x] **跨平台發布**：一鍵安裝腳本 (`install.sh`) 與 GitHub Release 自動化建置。

---

## 🎨 Phase 1：對齊 ChatGPT 流暢 WebUI (進行中)

- [x] **BYOK 設定 UI**：完成 `SettingsPage.tsx` 供應商管理介面。
- [x] **預設 Light Mode 視覺優化**：鎖定高質感明亮模式。
- [ ] **SSE 串流事件規範化**：統一 `StreamEvent` 封裝 (`Thought`, `Text`, `Status`)。
- [ ] **互動式 Citation 連結**：將 Markdown 的 `[1]` / `[2]` 出處轉化為可點擊聚焦文獻卡片的標籤。
- [ ] **RAG 檢索偏好設定**：提供 `Fast`、`Balanced`、`Precise` 三種檢索模式。
