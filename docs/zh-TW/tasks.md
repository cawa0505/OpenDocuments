# 📋 OpenDocuments 待開發任務清單

[English](../en/tasks.md) | **繁體中文**

本文件依據架構地圖與當前研發狀態，追蹤所有工程任務的執行進度。

---

## 🚨 當前執行任務 (Phase 1)

### 1.1 WebUI 與 RAG 串流優化
- [x] **1.1.1 BYOK 設定介面**：完成 `SettingsPage.tsx` 供應商管理介面與連線測試功能。
- [x] **1.1.2 預設 Light Mode**：鎖定高質感明亮模式，並清潔 DOM 主題初始化邏輯。
- [ ] **1.1.3 Markdown 代碼高亮與 Copy 按鈕**：支援語法高亮與右上角一鍵複製按鈕。
- [ ] **1.1.4 互動式 Citation 連結**：將 LLM 回覆中的 `[1]`、`[2]` 標記轉化為可點擊聚焦來源卡片的互動標籤。
- [ ] **1.1.5 RAG 檢索偏好 (Query Profiles)**：實作 `Fast` (FTS5 Top-5)、`Balanced` (LanceDB Top-10) 與 `Precise` (混合+重度 Reranker Top-15) 檢索策略。

### 1.2 CLI 與 TUI 優化
- [x] **1.2.1 工作區切換持久化**：`opendoc workspace switch <name>` 正確將選擇寫入 `config.toml` 的 `active_workspace`。
- [x] **1.2.2 一鍵跨平台安裝腳本**：`install.sh` 腳本支援 Linux 與 macOS (x86_64/aarch64)。
- [ ] **1.2.3 TUI 即時工作區切換**：於 `opendoc tui` 中實作 `Ctrl+W` 原地彈出式工作區切換器。
- [ ] **1.2.4 TUI Chunk 檢視彈窗**：新增檢索片段 Inspector 彈窗，能在 TUI 內直接審查 RAG Chunk 內文。

---

## 🎨 後續工程計畫 (Phases 2-4)

- [ ] **Tauri 2.0 三欄式桌面控制艙**：檔案樹總管、Monaco 就地編輯器、試算表 Canvas。
- [ ] **Stdio MCP 整合**：將 OpenDocuments 作為標準 Stdio MCP Server 對外提供服務。
- [ ] **UI Gatekeeper 審查卡片**：LLM 執行工具調用前必須取得使用者手動 Approve 放行。
- [ ] **GitHub Skill 市集與 SSG 發布器**：Skill 下載網路與 GitHub Pages 一鍵出版工具。
