# Codebase 衝突盤點進度（v1.0.0 GA 前）

> 本文為跨 session handoff 用參考文件，記錄 codebase 現況衝突盤點的進行中狀態。
> 目的：讓每次修改建立在查證後的資訊上，避免「文件不清楚、資訊參照模糊」造成的改 A 壞 B。
> 狀態標記：`[已確認]` = 已直接查證 codebase；`[待驗證]` = 尚未查證完畢。
> 本文件不新增任何未驗證的完成聲稱（AGENTS.md §0）。

## 1. 調查方法與分工

- 2026-08-13 啟動，三個並行 explorer lane + orchestrator 補充查證。
- exp-1（後端 API 契約）：完成 ✅（結果見 §2）
- exp-2（前端 DTO/i18n 衝突）：任務停止，orchestrator 接手自行查證
- exp-3（文件 vs 實作衝突）：任務停止，orchestrator 接手自行查證
- 已歸檔 6 條重複 ctx memory（workspace 解析 ×4、active_workspace ×1、Query Profile ×1、SSE ×1）

## 2. 後端 API 契約（exp-1 完成結果）

### 2.1 REST Routes（`crates/opendoc-mcp/src/lib.rs`）

| Method | Path | Handler |
|---|---|---|
| GET | `/api/v1/workbench` | `workbench_handler` |
| POST | `/api/v1/chat` | `chat_handler` |
| POST | `/api/v1/search` | `search_handler` |
| GET | `/api/v1/documents` | `list_documents_handler` |
| GET | `/api/v1/documents/trash` | `list_trash_handler` |
| POST | `/api/v1/documents/upload` | `upload_handler` |
| POST | `/api/v1/documents/:id/restore` | `restore_document_handler` |
| GET+DELETE | `/api/v1/documents/:id` | `get_document_handler` / `delete_document_handler` |
| GET+POST | `/api/v1/workspaces` | `list_workspaces_handler` / `create_workspace_handler` |
| DELETE | `/api/v1/workspaces/:id` | `delete_workspace_handler` |
| GET+POST | `/api/v1/collections` | `list_collections_handler` / `create_collection_handler` |
| DELETE | `/api/v1/collections/:id` | `delete_collection_handler` |
| GET | `/api/v1/collections/:id/documents` | `list_collection_documents_handler` |
| POST+DELETE | `/api/v1/collections/:id/documents/:docId` | `add_collection_document_handler` / `remove_collection_document_handler` |
| GET+POST | `/api/v1/conversations` | `list_conversations_handler` / `create_conversation_handler` |
| DELETE+PATCH | `/api/v1/conversations/:id` | `delete_conversation_handler` / `update_conversation_handler` |
| GET | `/api/v1/conversations/:id/messages` | `list_conversation_messages_handler` |
| POST | `/api/v1/conversations/:id/share` | `share_conversation_handler` |
| GET | `/api/v1/shared/:token` | `shared_conversation_handler` |
| GET+POST | `/api/v1/dictionary` | `get_dictionary_handler` / `add_dictionary_handler` |
| DELETE | `/api/v1/dictionary/:id` | `delete_dictionary_handler` |
| POST | `/api/v1/dictionary/import-seed` | `import_seed_handler` |
| GET | `/api/v1/tags` | `list_tags_handler` |
| POST | `/api/v1/tags` | `create_tag_handler` |
| DELETE | `/api/v1/tags/:id` | `delete_tag_handler` |
| POST | `/api/v1/documents/:docId/tags/:tagId` | `tag_document_handler` |
| DELETE | `/api/v1/documents/:docId/tags/:tagId` | `untag_document_handler` |
| GET+POST | `/api/v1/extracted-assets` | `list_assets_handler` / `extract_asset_handler` |
| GET+DELETE | `/api/v1/extracted-assets/:id` | `get_asset_handler` / `delete_asset_handler` |
| GET | `/api/v1/healthz` / `/api/v1/health` | `health_handler` |
| GET | `/api/v1/readyz` | `readyz_handler` |
| GET | `/api/v1/admin/stats` | `get_admin_stats_handler` |
| GET | `/api/v1/admin/version-check` | `version_check_handler` |
| GET | `/api/v1/admin/plugins` | `get_admin_plugins_handler` |
| GET | `/api/v1/admin/search-quality` | `get_admin_search_quality_handler` |
| GET | `/api/v1/admin/benchmark` | `get_admin_benchmark_handler` |
| GET | `/api/v1/admin/connectors` | `get_admin_connectors_handler` |
| GET | `/api/v1/admin/query-logs` | `get_admin_query_logs_handler` |
| DELETE | `/api/v1/admin/query-logs/:id` | `delete_query_log_handler` |
| GET+POST | `/api/v1/admin/llm/providers` | `list_llm_providers_handler` / `upsert_llm_provider_handler` |
| DELETE | `/api/v1/admin/llm/providers/:id` | `delete_llm_provider_handler` |
| POST | `/api/v1/admin/llm/test` | `test_llm_provider_handler` |
| POST | `/api/v1/query/feedback` | `query_feedback_handler` |
| POST | `/api/v1/chat/feedback` | `chat_feedback_handler` |
| GET | `/api/v1/stats` | `stats_handler` |

### 2.2 `resolve_workspace_id` 行為（`crates/opendoc-mcp/src/utils.rs`）

exp-1 報告：`X-Workspace` header 非空 → `active_workspace` → `default_workspace`，graceful fallback。
**⚠ 與 ctx memory #1497 衝突**：「未知 workspace 回 400，缺 fallback 回 500」— `[待驗證]` 誰對誰錯，需直接讀 `utils.rs` 原始碼。

### 2.3 已知 DTO 疑點

- **Workbench `workspace.name` 回 UUID 而非名稱** `[已確認]`：`workbench_handler` 的 `workspace.name` 填入 workspace_id（UUID），前端 `types.ts` 宣告為 name。前端拿不到真實 workspace 名稱。
- Chat request body 欄位 `query` `[已確認]`：前端 api.ts 與後端 chat_handler 一致（AGENTS.md 8.3 match）。
- SSE 事件名 `chunk/sources/confidence/done/error` `[已確認]`：sse.rs 一致。

## 3. 前端 vs 後端 route 比對（orchestrator 接手）

### 3.1 已驗證一致（前端 api.ts → 後端 lib.rs）

| 前端呼叫 | 後端 route | 狀態 |
|---|---|---|
| `/stats` | `/stats` (832) | ✅ |
| `/chat/feedback` | `/chat/feedback` (828) | ✅ |
| `/query/feedback` | `/query/feedback` (827) | ✅ |
| `/documents*` / `/workspaces*` / `/collections*` / `/conversations*` / `/dictionary*` / `/tags*` | 對應 route | ✅ |

### 3.2 衝突候選

| 前端呼叫（api.ts） | 後端 route | 狀態 |
|---|---|---|
| `/plugins` / `/plugins/search?q=` / `/plugins/install` / `/plugins/:name` DELETE | 只有 `/admin/plugins`，搜無 `route("/plugins` | `[待驗證]` — 需確認前端 plugins 頁面是否呼叫到不存在的 route |
| `/admin/connectors/github` + `/admin/connectors/github/sync` | 只有 `/admin/connectors` | `[已確認]` 衝突 — 08-12 audit §2.2 已記錄，GitHub connector 尚未實作 |
| `/admin/stats` / `/admin/version-check` / `/admin/search-quality` / `/admin/query-logs*` | 對應 route | ✅ |

## 4. 文件 vs 實作衝突（exp-3 範圍，orchestrator 接手）

- `[已確認]` **AGENTS.md §8.3「BYOK 金鑰不外流」**：`llm-providers` handler 只暴露 `hasApiKey`，實作一致。
- `[已確認]` **workbench 可用性**：`/workbench` route 一直存在（lib.rs:794），從未拿掉；「workbench 用不到可以拿掉」是錯誤記憶。
- `[待驗證]` **roadmap/tasks 完成狀態矛盾**：08-12 audit §4 記錄 roadmap 標若干 `[x]`（Citation、Query Profiles、SSE 規範化）但 tasks 仍 `[ ]`——需重新核對現況。

## 5. 其他進行中/已知事項

- **Tailwind v4 遷移半完成** `[已確認]`：source 已改（package.json `tailwindcss ^4` + `@tailwindcss/vite`、vite.config.ts、globals.css v4 寫法，刪 tailwind.config.ts/postcss.config.js），`npm run typecheck` + `npm run build` 通過；但 **binary 未重 build**（`make install` 未跑）。已排進 tasks.md 1.4。
- **`|| 'default'` hardcoding** `[已確認]`：`DictionaryPage.tsx:20` 與 `Sidebar.tsx:192` 用字面 `'default'` 當 workspace fallback；正解是啟動時 `getWorkbench()` 取後端解析名稱存 store。已排進 tasks.md 1.4.2。
- **LLM 公版回答** `[已確認]`：`chat.rs` system prompt 未注入工作區資訊，檢索模糊時 LLM 回公版內容。已排進 tasks.md 1.4.3。
- **AGENTS.md §0 正確性優先原則** `[已確認]`：已新增（單一來源、修根源、查證不猜）。

## 6. 下次續調研切入點

1. 讀 `utils.rs` `resolve_workspace_id` 原始碼，判定 #1497 memory 與 exp-1 報告誰對。
2. 確認前端 `PluginsPage.tsx` 實際呼叫的 route，判定 `/plugins/*` 是否 dead call。
3. 重核 roadmap.md / tasks.md 完成狀態對齊（Citation、Query Profiles、SSE）。
4. 完成後將衝突清單完整排入 `docs/zh-TW/tasks.md` 1.4 小節。
5. git commit 今日變更（AGENTS.md、design-system.md、tasks.md 1.4、memory 清理）。
6. 更新 relay handoff（graphify）。
