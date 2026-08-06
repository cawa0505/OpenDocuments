# 🗺️ OpenDocuments 開發藍圖

🔗 [English](../en/roadmap.md) | **繁體中文**

本藍圖記錄了 OpenDocuments 開源專案的工程目標與戰略里程碑。

---

## 🎯 階段總覽

| 階段 | 名稱 | 核心焦點 | 狀態 | 目標時程 |
| :--- | :--- | :--- | :--- | :---: |
| **Phase 0** | **單一二進位 MVP 與 BYOK 網關** | 單一 Axum 進程、BYOK 金鑰管理、混合 RAG、標籤系統、CLI/TUI | ✅ 已完成 | 2026 Q3 |
| **Phase 1** | **對齊 ChatGPT 流暢 WebUI** | React 19 WebUI、打字機 SSE 串流、互動 Citation 出處、代碼高亮 | ⏳ 進行中 | 2026 Q3 |

---

## 🚀 Phase 0：單一二進位 MVP 與 BYOK 網關 (已完成)

- [x] **單一二進位 Axum 架構**：整合 `rust-embed` 內嵌前端 WebUI 靜態資產。
- [x] **BYOK LLM 層**：SQLite 加密儲存自備 API 金鑰，支援 OpenAI 格式並具備連線健康診斷。
- [x] **混合 RAG 檢索引擎**：LanceDB 稠密向量相似度 + SQLite FTS5 全文檢索，搭配 RRF 重排。
- [x] **標籤與複合條件過濾**：標籤 CRUD、文件狀態/類型過濾，以及動態升降冪排序。
- [x] **跨平台發布**：一鍵安裝腳本 (`install.sh`) 與 GitHub Release 自動化建置。

---

## 🎨 Phase 1：對齊 ChatGPT 流暢 WebUI (進行中)

- [x] **BYOK 設定 UI**：完成 `SettingsPage.tsx` 供應商管理介面。
- [x] **預設 Light Mode 視覺優化**：鎖定高質感明亮模式。
- [ ] **SSE 串流事件規範化**：統一 `StreamEvent` 封裝 (`Thought`, `Text`, `Status`)。
- [ ] **互動式 Citation 連結**：將 Markdown 的 `[1]` / `[2]` 出處轉化為可點擊聚焦文獻卡片的標籤。
- [ ] **RAG 檢索偏好設定**：提供 `Fast`、`Balanced`、`Precise` 三種檢索模式。
