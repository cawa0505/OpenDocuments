# Design

## Context

WebUI 為 React 19 + Vite SPA，嵌入單一 Rust binary（`webui-architecture` 規格）。Dogfood 發現的缺陷集中在：SSE 會話狀態機缺少有界等待、i18n 詞庫有洞、清單渲染直接輸出後端原樣欄位（絕對路徑、UUID）。動機見 proposal.md，行為契約見 specs/。

## Goals / Non-Goals

**Goals:**

- 會話恢復與串流等待全部有界化，卡死必有出口（error + cancel/retry）。
- zh-TW 詞庫補齊＋用語修正，建立「raw key 不得直出」的檢查點。
- 顯示層隱私：路徑摘要化、識別碼預設收合。
- 檢索偏好單一狀態來源（對齊 AGENTS §0.1 單一來源原則）。
- 空狀態/說明/行銷腔文案清理。

**Non-Goals:**

- 不變更 REST/SSE API 契約、DTO、後端行為。
- 不做主題/視覺重設計（§8：視覺變更需另行授權）。
- 不引入新框架或狀態管理庫——用既有 store 慣例收斂。

## Decisions

- **有界等待放前端狀態機**：逾時、取消、重試皆為展示層狀態轉移，不動後端。備選（後端 heartbeat）影響 API 契約，違反本 change 邊界，不採。逾時長度與重試次數屬調校細節，實作時定（可 config 常數），spec 只鎖「有界＋出口存在」。
- **顯示摘要共用單一格式化 helper**：路徑→來源摘要、UUID→短碼收合，避免各頁自寫切片（否則又是多來源）。完整路徑/長碼經明確操作（展開/複製鈕）揭露。
- **偏好狀態收斂進既有全域 store**，兩處 UI 同讀同寫同一 slice；不採 localStorage 各自快取（§0.2 反例模式）。
- **i18n 修補＋例會掃描**：只修本次發現＋同族掃描；不強制加 lint 規則（可為後續任務）。PRC 用語表先放詞庫維護註記，不建獨立模組。

## Risks / Trade-offs

- [有界等待的前端逾時與後端實際慢查詢衝突（精準模式 reranker 本來就慢）] → 逾時只對「零事件窗口」計時，收到任何 chunk/SSE 事件即重置；臨界值實作時以真實慢查詢校準。
- [同名文件辨識需要後端提供可辨識欄位] → 先用既有回傳欄位做摘要；若欄位不足以區分，僅此處允許在 design 範圍內擴充顯示所欄位（非契約變更），不足再回報。
- [文案修正可能與 docs-site 文件不同步] → UI 文案優先；文件站另案。

## Migration Plan

純前端顯示/狀態修正，無資料料遷移。驗證走 `webui-architecture` 的端到端流程：`npm run build` → `cargo install --path crates/opendoc-cli --force` → 重啟 opendoc-server → 瀏覽器逐場景複驗（對照 specs 各 Scenario）。

## Open Questions

- 無阻礙實作的決策懸置；逾時常數與重試次數為調校細節，由實作端依風險條校準。
