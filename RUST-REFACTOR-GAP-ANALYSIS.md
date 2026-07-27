# Rust Refactor Gap Analysis

> 盤點 Node.js 原始 endpoints vs Rust 實作，列出缺失/損壞項目。
> 2026-07-27

## 一、Frontend API 合約（api.ts）對照

### ✅ 已實作且路由存在

| 前端呼叫 | Rust route | 備註 |
|----------|-----------|------|
| `GET /documents` | `list_documents_handler` | **欄位不足** — 缺 source_path, file_type, indexed_at 等 |
| `DELETE /documents/:id` | `delete_document_handler` | 正常 |
| `POST /documents/upload` | `upload_handler` | **source_path 只存檔名，不同目錄同名會衝突** |
| `GET /workspaces` | `list_workspaces_handler` | 正常 |
| `DELETE /workspaces/:id` | `delete_workspace_handler` | 正常 |
| `GET /conversations` | `list_conversations_handler` | **缺 limit/offset 分頁，缺 updated_at 欄位** |
| `GET /admin/stats` | `get_admin_stats_handler` | 正常（plugins 回 0） |
| `GET /admin/search-quality` | `get_admin_search_quality_handler` | intentDistribution/routeDistribution 回空 object |
| `GET /admin/query-logs` | `get_admin_query_logs_handler` | **缺 total/limit/offset 回傳** |
| `GET /admin/benchmark` | `get_admin_benchmark_handler` | 正常 |
| `GET /collections` | `list_collections_handler` | **缺 workspaceId, createdAt 欄位** |
| `GET /admin/connectors` | `get_admin_connectors_handler` | 正常 |
| `GET /health` | `health_handler` | 正常 |
| `GET /dictionary` | `get_dictionary_handler` | 正常 |
| `POST /dictionary` | `add_dictionary_handler` | 正常 |
| `DELETE /dictionary/:id` | `delete_dictionary_handler` | 正常 |
| `POST /dictionary/import-seed` | `import_seed_handler` | 正常 |
| `POST /query/feedback` | `query_feedback_handler` | 前端呼叫 `/chat/feedback`，**路由名不匹配** |

### ❌ 完全缺失（前端直接呼叫但 Rust 無 route）

| 前端呼叫 | 優先級 | 說明 |
|----------|--------|------|
| `GET /documents/:id` | 🔴 高 | 單一文件詳情，點擊文件需要 |
| `GET /documents/trash` | 🟡 中 | 回收桶列表 |
| `POST /documents/:id/restore` | 🟡 中 | 從回收桶還原 |
| `POST /conversations` | 🔴 高 | 建立新對話 |
| `DELETE /conversations/:id` | 🔴 高 | 刪除對話 |
| `PATCH /conversations/:id` | 🟡 中 | 更新對話標題 |
| `GET /conversations/:id/messages` | 🔴 高 | 取得對話訊息 |
| `POST /conversations/:id/share` | 🟢 低 | 分享對話 |
| `POST /collections` | 🟡 中 | 建立集合 |
| `DELETE /collections/:id` | 🟡 中 | 刪除集合 |
| `GET /collections/:id/documents` | 🟡 中 | 集合內文件列表 |
| `POST /collections/:id/documents/:docId` | 🟢 低 | 加文件進集合 |
| `DELETE /collections/:id/documents/:docId` | 🟢 低 | 從集合移除文件 |
| `GET /stats` | 🔴 高 | 前端 dashboard 呼叫（非 /admin/stats） |
| `POST /chat` | 🔴 高 | 非串流聊天（前端 api.ts 有此呼叫） |
| `POST /chat/feedback` | 🟡 中 | 前端呼叫 /chat/feedback，Rust 是 /query/feedback |
| `GET /readyz` | 🟢 低 | Kubernetes 就緒探針 |
| `GET /admin/audit-logs` | 🟢 低 | 審計日誌 |
| `GET /admin/plugins` | 🟢 低 | 插件健康狀態 |
| `GET /plugins` | 🟢 低 | 插件列表 |
| `POST /plugins/install` | 🟢 低 | 安裝插件 |
| `DELETE /plugins/:name` | 🟢 低 | 移除插件 |
| `GET /plugins/search` | 🟢 低 | 搜尋插件 |

## 二、資料模型不匹配

### DocumentItem 缺少欄位

前端 `Document` 型別需要：
```
id, title, source_type, source_path, file_type, file_size_bytes,
connector_id, chunk_count, status, content_hash, error_message,
created_at, updated_at, indexed_at, workspace_id
```

Rust `DocumentItem` 只回傳：
```
id, title, source_type, status, chunk_count, workspace_id, created_at
```
**缺少：source_path, file_type, file_size_bytes, indexed_at, updated_at, content_hash, error_message**

### Collections 缺少欄位

前端需要 `workspaceId`, `createdAt`，Rust 只回 `id, name, description`

### Conversations 缺少欄位

前端需要 `created_at`, `updated_at`, `workspace_id`，Rust 只回 `id, title, shared, createdAt`

### Admin query-logs 回傳格式

前端期望 `{ logs, total, limit, offset }`，Rust 只回 `{ logs }`

## 三、已知 Bug

1. **Document list 空白** — DocumentItem 欄位不足，前端渲染失敗
2. **Upload 同檔名衝突** — source_path 只存 sanitizedName，不同目錄同名會覆蓋
3. **Chat feedback 路由不匹配** — 前端呼叫 `/chat/feedback`，Rust 只有 `/query/feedback`
4. **Chat SSE 白屏** — chat_stream_handler SSE 格式可能不符前端 SSE parser

## 四、修復優先順序

### Round 1: 核心資料正確性（先修）
1. DocumentItem 加入缺少欄位（source_path, file_type, indexed_at 等）
2. Upload source_path 加入目錄前綴避免同名衝突
3. Conversations/Collections 回傳完整欄位

### Round 2: 缺失 endpoints（補齊前端必須的）
4. GET /documents/:id
5. POST /conversations, DELETE /conversations/:id, PATCH /conversations/:id, GET /conversations/:id/messages
6. GET /stats（非 admin）
7. POST /chat（非串流）+ POST /chat/feedback 路由

### Round 3: 次要 endpoints（可延後）
8. Documents trash/restore
9. Collections CRUD
10. Plugins, connectors, admin endpoints
11. Auth routes（OAuth）
