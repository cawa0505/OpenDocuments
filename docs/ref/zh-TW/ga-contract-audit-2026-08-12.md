# v1.0.0 GA 契約稽核現況（2026-08-12）

> 本文為跨 session handoff 用參考文件，記錄 v1.0.0 GA 前已確認的契約狀態與待辦。
> 來源：`docs/zh-TW/tasks.md`、`docs/zh-TW/roadmap.md`、clean git 基線（HEAD `2f831d1`）。
> 本文件不新增任何未驗證的完成聲稱。

## 1. Git 基線（安全點）

- 工作樹完全乾淨，無未提交／staged 變更。
- `main` 領先 `origin/main` 5 個 local commits；**尚未 push**。
- 最新安全基線 commit：`2f831d1`（docs: mark activity-log contract complete in roadmap/tasks）。
- 先前一段對話中錯誤新增的 GitHub Connector 草稿（`admin.rs` 額外 102 行、含重複 fn 與不存在的 `state.0`）已完整撤除，`cargo check -p opendoc-mcp` 通過。

## 2. v1.0.0 GA 門檻（待辦，依 tasks.md / roadmap.md）

### 2.1 CLI index 同步整合測試（roadmap 條目 `[ ]`，tasks 1.3.3 `[ ]`）
- 需補的三項：空目錄、巢狀路徑、跨 workspace 不互刪。
- 程式碼層稽核（tasks 1.3.2 `[x]`）已確認：SHA-256 去重、內容變更重傳、目錄內本機刪除同步刪除、`X-Workspace` 傳遞。
- 狀態：**尚未實作測試**。

### 2.2 GitHub connector 契約（roadmap `[ ]`，tasks 1.3.4 `[ ]`）
- WebUI 呼叫 `/admin/connectors/github` 與 `/admin/connectors/github/sync`，Rust router 尚未提供。
- 完成 connector 實作前不得標記 workspace 隔離完成。
- 狀態：**尚未實作**。先前錯誤草稿已移除，需依真實契約重做。

### 2.3 活動日誌契約（tasks 1.3.5 `[x]`、1.3.6 `[x]`；roadmap `[x]`）
- 已確認完成：統計／workbench／query-log 讀取有 `workspace_id` 條件；總數／分頁／DTO 對齊；feedback workspace 條件；刪除 API 與 UI。
- 已 commit。

### 2.4 RAG Query Profiles（tasks 1.1.5 `[ ]`，roadmap `[x]` 但標 `[待討論] 實際檢索流程差異`）
- roadmap 標 `[x]` 但附帶「另行討論中」；tasks 仍 `[ ]`。
- **狀態矛盾**：不得依現有看似完成但實際未定的文件直接實作。需先定案 Fast/Balanced/Precise 真實檢索差異後更新 OpenSpec。

## 3. Phase 1 WebUI 其餘待辦（tasks 1.1.x）

- `[ ]` 1.1.3 Markdown 代碼高亮與 Copy 按鈕。
- `[ ]` 1.1.4 互動式 Citation 連結（roadmap 標 `[x]`，tasks 仍 `[ ]` — 亦需稽核對齊）。
- `[x]` 1.1.1 BYOK 設定介面。
- `[x]` 1.1.2 預設 Light Mode。

## 4. 已確認文件矛盾（待稽核對齊）

- roadmap 標若干完成項（Citation、Query Profiles、SSE 事件規範化）為 `[x]`，但 tasks.md 對應項仍 `[ ]`。
- 進入實作前，需先統一兩份文件狀態，避免重工或誤判。

## 5. 下一步建議順序

1. CLI index 三項整合測試 → 驗證後 local commit。
2. GitHub connector 真實契約 + workspace 隔離 → 驗證後 local commit。
3. 稽核並對齊 tasks.md / roadmap.md 完成狀態。
4. 定案 Fast/Balanced/Precise 真實檢索差異 → 更新 OpenSpec。
5. v1.0.0 GA 全套端到端驗收 → release commit（暫不 push）。