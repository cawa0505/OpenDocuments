# Node.js → Rust 功能對照盤點

> 盤點時間: 2026-07-27
> 來源: `packages/server/src/http/routes/*.ts` (13 files) vs `open-documents-rust/crates/opendoc-mcp/src/lib.rs`
> 前端 API 呼叫: `packages/web/src/lib/api.ts`

---

## 一、Rust 已實作路由 (25 條)

| # | Method | Route | Handler | 備註 |
|---|--------|-------|---------|------|
| 1 | GET | /api/v1/healthz | health_handler | ✅ |
| 2 | GET | /api/v1/health | health_handler | ✅ |
| 3 | GET | /api/v1/workbench | workbench_handler | ✅ |
| 4 | GET | /api/v1/documents | list_documents_handler | ⚠️ 回傳欄位不完整 |
| 5 | POST | /api/v1/documents/upload | upload_handler | ⚠️ source_path 衝突 |
| 6 | DELETE | /api/v1/documents/:id | delete_document_handler | ✅ |
| 7 | GET | /api/v1/workspaces | list_workspaces_handler | ✅ |
| 8 | POST | /api/v1/workspaces | create_workspace_handler | ✅ |
| 9 | DELETE | /api/v1/workspaces/:id | delete_workspace_handler | ✅ |
| 10 | GET | /api/v1/collections | list_collections_handler | ⚠️ 回傳欄位不完整 |
| 11 | GET | /api/v1/conversations | list_conversations_handler | ⚠️ 回傳欄位不完整 |
| 12 | GET | /api/v1/dictionary | get_dictionary_handler | ✅ |
| 13 | POST | /api/v1/dictionary | add_dictionary_handler | ✅ |
| 14 | DELETE | /api/v1/dictionary/:id | delete_dictionary_handler | ✅ |
| 15 | POST | /api/v1/dictionary/import-seed | import_seed_handler | ✅ |
| 16 | GET | /api/v1/admin/stats | get_admin_stats_handler | ✅ |
| 17 | GET | /api/v1/admin/search-quality | get_admin_search_quality_handler | ✅ |
| 18 | GET | /api/v1/admin/benchmark | get_admin_benchmark_handler | ✅ |
| 19 | GET | /api/v1/admin/connectors | get_admin_connectors_handler | ✅ (stub) |
| 20 | GET | /api/v1/admin/query-logs | get_admin_query_logs_handler | ✅ |
| 21 | POST | /api/v1/query/log | query_log_handler | ✅ |
| 22 | POST | /api/v1/query/feedback | query_feedback_handler | ❌ 路徑不對 (前端叫 /chat/feedback) |
| 23 | POST | /api/v1/chat/stream | chat_stream_handler | ⚠️ SSE event 格式待驗證 |
| 24 | GET | /mcp/sse | sse_handler | ✅ |
| 25 | POST | /mcp/message | message_handler | ✅ |

---

## 二、Rust 缺失路由 (30 條)

### A. Documents 模組 (4 條缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 1 | GET | /api/v1/documents/:id | documents.ts:30 | `getDocument(id)` | 🔴 高 |
| 2 | GET | /api/v1/documents/trash | documents.ts:15 | (未使用但功能存在) | 🟡 中 |
| 3 | POST | /api/v1/documents/:id/restore | documents.ts:22 | (未使用但功能存在) | 🟡 中 |
| 4 | POST | /api/v1/chat | chat.ts:56 | `chat(query, profile)` | 🔴 高 (非串流聊天) |

### B. Conversations 模組 (5 條缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 5 | POST | /api/v1/conversations | conversations.ts:44 | `createConversation(title)` | 🔴 高 |
| 6 | DELETE | /api/v1/conversations/:id | conversations.ts:56 | `deleteConversation(id)` | 🔴 高 |
| 7 | PATCH | /api/v1/conversations/:id | conversations.ts:69 | `updateConversation(id, {title})` | 🟡 中 |
| 8 | GET | /api/v1/conversations/:id/messages | conversations.ts:31 | `getConversationMessages(id)` | 🔴 高 |
| 9 | POST | /api/v1/conversations/:id/share | conversations.ts:90 | `shareConversation(id)` | 🟢 低 |

### C. Collections 模組 (5 條缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 10 | POST | /api/v1/collections | collections.ts:25 | `createCollection({name})` | 🔴 高 |
| 11 | DELETE | /api/v1/collections/:id | collections.ts:33 | `deleteCollection(id)` | 🔴 高 |
| 12 | GET | /api/v1/collections/:id/documents | collections.ts:14 | `getCollectionDocuments(id)` | 🟡 中 |
| 13 | POST | /api/v1/collections/:id/documents/:docId | collections.ts:41 | `addDocumentToCollection(...)` | 🟡 中 |
| 14 | DELETE | /api/v1/collections/:id/documents/:docId | collections.ts:50 | `removeDocumentFromCollection(...)` | 🟡 中 |

### D. Tags 模組 (5 條缺失，整體缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 15 | GET | /api/v1/tags | tags.ts:8 | (未在 api.ts) | 🟢 低 |
| 16 | POST | /api/v1/tags | tags.ts:13 | (未在 api.ts) | 🟢 低 |
| 17 | DELETE | /api/v1/tags/:id | tags.ts:20 | (未在 api.ts) | 🟢 低 |
| 18 | POST | /api/v1/documents/:docId/tags/:tagId | tags.ts:26 | (未在 api.ts) | 🟢 低 |
| 19 | DELETE | /api/v1/documents/:docId/tags/:tagId | tags.ts:32 | (未在 api.ts) | 🟢 低 |

### E. Plugins 模組 (4 條缺失，整體缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 20 | GET | /api/v1/plugins/search | plugins.ts:21 | `searchPlugins(q)` | 🟢 低 |
| 21 | GET | /api/v1/plugins | plugins.ts:36 | `getPlugins()` | 🟢 低 |
| 22 | POST | /api/v1/plugins/install | plugins.ts:53 | `installPlugin(name)` | 🟢 低 |
| 23 | DELETE | /api/v1/plugins/:name | plugins.ts:72 | `removePlugin(name)` | 🟢 低 |

### F. Chat Feedback 路徑不匹配 (1 條)

| # | Method | Route (Node.js) | Route (Rust) | 前端呼叫 | 優先級 |
|---|--------|----------------|-------------|---------|--------|
| 24 | POST | /api/v1/chat/feedback | /api/v1/query/feedback ❌ | `submitFeedback(queryId, feedback)` | 🔴 高 |

### G. Health / Admin 補充 (4 條缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 25 | GET | /api/v1/stats | health.ts:19 | `getStats()` | 🔴 高 |
| 26 | GET | /api/v1/readyz | health.ts:36 | (K8s/readiness) | 🟡 中 |
| 27 | GET | /api/v1/admin/audit-logs | admin.ts:65 | (未在 api.ts) | 🟢 低 |
| 28 | GET | /api/v1/admin/plugins | admin.ts:186 | `getPluginHealth()` | 🟡 中 |

### H. Auth 模組 (2 條缺失，整體缺失)

| # | Method | Route | Node.js 來源 | 前端呼叫 | 優先級 |
|---|--------|-------|-------------|---------|--------|
| 29 | GET | /auth/login/:provider | auth-routes.ts:13 | (OAuth flow) | 🟢 低 |
| 30 | GET | /auth/callback/:provider | auth-routes.ts:40 | (OAuth flow) | 🟢 低 |

---

## 三、已存在路由的回傳格式問題

### 3.1 DocumentItem 缺少欄位

前端 `Document` 類型期望以下欄位，但 Rust `DocumentItem` 缺少:

| 欄位 | 前端要求 | Rust 回傳 | 影響 |
|------|---------|----------|------|
| `source_path` | `string` | ❌ 缺少 | 文件列表無法顯示來源路徑 |
| `file_type` | `string \| null` | ❌ 缺少 | 無法顯示副檔名圖示 |
| `file_size_bytes` | `number \| null` | ❌ 缺少 | 無法顯示檔案大小 |
| `indexed_at` | `string \| null` | ❌ 缺少 | 無法顯示索引時間 |
| `updated_at` | `string` | ❌ 缺少 | 無法顯示更新時間 |
| `error_message` | `string \| null` | ❌ 缺少 | 無法顯示索引錯誤 |
| `content_hash` | `string \| null` | ❌ 缺少 | 無法比對重複檔案 |
| `connector_id` | `string \| null` | ❌ 缺少 | 無法顯示連接器來源 |

### 3.2 Upload handler source_path 衝突

Rust upload handler 將 `source_path` 設為檔名本身（如 `CHANGELOG.md`）。
Node.js 也如此做，但 DB 中不同目錄上傳同檔名的文件會覆蓋。

**修正**: 用 `{workspace_id}/{uuid}-{filename}` 作為 `source_path`。

### 3.3 Conversations 回傳欄位

Rust list_conversations 可能缺少:
- `updated_at` (前端靠此排序)
- `shared` / `share_token`

### 3.4 Collections 回傳欄位

Rust list_collections 可能缺少:
- `description`
- `autoRules`

---

## 四、執行計劃 (按優先級排序)

### Phase 1: 🔴 高優先 — 阻塞前端基本功能 (10 條)

| 步驟 | 修復項目 | 預估工時 |
|------|---------|---------|
| 1.1 | 補齊 DocumentItem struct 所有 DB 欄位 + SQL query | 30min |
| 1.2 | 修復 upload handler source_path 為 `workspace/uuid-filename` | 15min |
| 1.3 | 新增 `GET /documents/:id` handler | 20min |
| 1.4 | 新增 `POST /chat` 非串流聊天 handler | 45min |
| 1.5 | 新增 `POST /chat/feedback` handler (或重命名 /query/feedback) | 15min |
| 1.6 | 新增 Conversations CRUD: POST, DELETE, GET /:id/messages | 60min |
| 1.7 | 新增 Collections CRUD: POST, DELETE | 30min |
| 1.8 | 新增 `GET /stats` endpoint (或共用 admin/stats) | 10min |
| 1.9 | cargo check + cargo install + restart | 15min |
| 1.10 | curl 驗證每個新增 route | 30min |
| **小計** | | **~4.5h** |

### Phase 2: 🟡 中優先 — 完整功能 (8 條)

| 步驟 | 修復項目 | 預估工時 |
|------|---------|---------|
| 2.1 | Conversations: PATCH /:id, POST /:id/share | 30min |
| 2.2 | Collections: GET /:id/documents, POST/DELETE /:id/documents/:docId | 30min |
| 2.3 | Documents: GET /trash, POST /:id/restore | 30min |
| 2.4 | GET /readyz 深度健康檢查 | 20min |
| 2.5 | GET /admin/plugins 真實健康檢查 | 15min |
| 2.6 | SSE chat_stream handler 事件格式驗證與修正 | 45min |
| 2.7 | cargo check + install + restart | 15min |
| 2.8 | curl + 前端流程驗證 | 45min |
| **小計** | | **~3.75h** |

### Phase 3: 🟢 低優先 — 延後 (14 條)

| 步驟 | 修復項目 | 備註 |
|------|---------|------|
| 3.1 | Tags 模組 (5 routes) | 前端未使用 |
| 3.2 | Plugins 模組 (4 routes) | npm-dependent，不適合 Rust |
| 3.3 | Auth OAuth flow (2 routes) | 安全考量，需完整實作 |
| 3.4 | MCP SSE + Message | 已存在 |

---

## 五、Unit Test 計劃

每個 Phase 1 修復都需要對應的 integration test:

```rust
#[tokio::test]
async fn test_get_document_by_id() { ... }

#[tokio::test]
async fn test_upload_source_path_unique() { ... }

#[tokio::test]
async fn test_chat_non_streaming() { ... }

#[tokio::test]
async fn test_chat_feedback() { ... }

#[tokio::test]
async fn test_conversations_crud() { ... }

#[tokio::test]
async fn test_collections_crud() { ... }

#[tokio::test]
async fn test_stats_endpoint() { ... }
```

---

## 六、驗證 Checklist

每個步驟完成後必須執行:

- [ ] `cargo check` — zero errors
- [ ] `cargo test` — all tests pass
- [ ] `cargo install --path . --force` — install to ~/.cargo/bin
- [ ] `systemctl --user restart opendoc-server.service` — restart
- [ ] curl 驗證 endpoint 回傳正確 JSON 格式
- [ ] 前端對應頁面操作正常
