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
- [x] **LanceDB Engine 邊界**：LanceDB／Arrow／DataFusion 已移至由核心管理的私有 sidecar（spec Approved / Production）。
- [x] **標籤與複合條件過濾**：標籤 CRUD、文件狀態/類型過濾，以及動態升降冪排序。
- [x] **跨平台發布**：一鍵安裝腳本 (`install.sh`) 與 GitHub Release 自動化建置。

---

## 🎨 Phase 1：對齊 ChatGPT 流暢 WebUI (進行中)

- [x] **BYOK 設定 UI**：完成 `SettingsPage.tsx` 供應商管理介面。
- [x] **預設 Light Mode 視覺優化**：鎖定高質感明亮模式。
- [ ] **SSE 串流事件規範化**：統一 `StreamEvent` 封裝 (`Thought`, `Text`, `Status`)。
- [ ] **互動式 Citation 連結**：將 Markdown 的 `[1]` / `[2]` 出處轉化為可點擊聚焦文獻卡片的標籤。
- [ ] **RAG 檢索偏好設定**：提供 `Fast`、`Balanced`、`Precise` 三種檢索模式。

---

## 🎯 v1.0.0 範圍

**v1.0.0 = Phase 0（Core MVP）+ Phase 1（WebUI）完成。僅支援單機部署。**

- **重點：WebUI 與 API。** 每個 chat 功能都必須真正可用——WebUI 端到端正確搜尋是發行門檻，不是「能編譯就好」。
- **檔案定位：reference full path。** 文件以絕對 `source_path` 定位（不做相對名稱或虛擬檔案間接層）；未來多機儲存是延伸此設計，不是取代它。

v1.0.0 驗收門檻：`cargo check` 零警告 → 安裝 → 真實上傳 → 真實搜尋 → WebUI 中 chat 來回驗證，以下每個 feature 都要過。

---

## 🗄️ v1.0.0 之後 (Backlog)

明確延後至 v1.0.0 發布之後的功能，集中在此追蹤以免遺忘。每一項都連結其管轄規格（若存在）。

| 功能 | 規格 | 備註 |
| :--- | :--- | :--- |
| **目標混合檢索 — SQLite FTS5** | [`hybrid-rag-retrieval`](../../openspec/specs/hybrid-rag-retrieval/spec.md) | 核心擁有的稀疏詞法路徑；engine 不可用時可提供純詞法 fallback。在此之前 LanceDB FTS 仍是現行詞法路徑。 |
| **Phase 2 — 任務執行層與原生 AI 引擎** | [`task-execution-ai-engines`](../../openspec/specs/task-execution-ai-engines/spec.md) | llama.cpp (Vulkan/HIP) embed/rerank/infer、`opendoc-ai-fastembed` 進程邊界、`[ai.models.inference]` 生成切換。 |
| **Spur 整合與 server/worker 模式** | deferred note #33 | `SpurDaemonExecutor` (Mode 1)、`opendoc-worker daemon` + scale-to-zero (Mode 3)、批次 ETL (Mode 2)；私有網路 LAN worker。stdio JSON-RPC 保持 transport 無關（未來 TCP/unix socket）；engine 設定獨立於 core。注意：#2039 禁止 Docker 部署 — server/worker 是未來方向，非容器。 |
| ~~TUI 增強~~ | [`tui-enhancements`](../../openspec/specs/tui-enhancements/spec.md) | **已取消（2026-08-10）**：架構上不適合本專案，暫時沒有能取代 WebUI 的更好做法。Spec 撤回；`opendoc-tui` crate 與 `tui` feature 列為移除候選。 |
| **二進位瘦身** | [`binary-size-architecture`](../../openspec/specs/binary-size-architecture/spec.md) | Engine 354 MB (strip+LTO)；zero-behavior-change slimming backlog。 |
| **Graphify Layer 2 完整整合** | — | Plugin 端消費 `heading`/`spec_id`、向量 fallback 端到端驗證（Layer 1 不受影響）。 |
| **FastEmbed 進程邊界** | `task-execution-ai-engines` | 目前 feature-gated 於 `opendoc-storage`（`embedding-fastembed`）；日後可能移入 engine/worker 邊界。 |
| **LanceDB S3 後端** | — | 情境：有網管的小學校可以用舊電腦部署區網內儲存（S3 相容服務如 Garage、SeaweedFS）；LanceDB 表建於 S3 而非本地磁碟。 |
| **S3 物件儲存取代 NAS** | — | LanceDB 擴充成多機時，應用可設定同時把實體檔案存入區網內 S3 服務，建立「NAS + AI Search」基礎建設，取代傳統 NAS。 |
