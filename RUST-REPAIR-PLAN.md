# OpenDocuments 修復计划 (Unit Test First)

## 目標
修復 Core + API Server 關鍵缺陷，並为每条修复写入对应的 unit test。

---

## Phase 1: 核心基礎 (必須先通過)

| # | 修復項 | 測試類別 | 目標 |
|---|--------|----------|------|
| 1 | `ConfigManager` 無效配置時 return default | Unit | `ConfigManager::load()` |
| 2 | `resolve_ws()` fallback chain: opt_ws → active_ws → default_ws | Unit | 3 層 fallback |
| 3 | `WorkspaceStore` create/get/upsert/delete | Unit | 完整 CRUD |
| 4 | `DocumentStore` upsert doc, upsert chunk, soft delete | Unit | 完整 CRUD |

---

## Phase 2: API Server 修复

| # | 修復項 | 測試類別 | 目標 |
|---|--------|----------|------|
| 5 | `/health`, `/healthz` | Integration | return 200 + JSON |
| 6 | `/workspaces` GET/POST/DELETE | Integration | 返回 JSON |
| 7 | `/documents` GET | Integration | 返回 JSON |
| 8 | `/documents/upload` POST | Integration | 接受 multipart |
| 9 | `/documents/:id` DELETE | Integration | 返回 204 |

---

## Phase 3: WebSocket Chat (最高優先，阻塞 deployment)

| # | 修復項 | 測試類別 | 目標 |
|---|--------|----------|------|
| 10 | `/chat/stream` 路由存在且可連接 WebSocket | Integration | SSE events 流 |
| 11 | SSE events format: `sources`、`confidence`、`chunk`、`done`、`error` | Integration | 與 frontend 協議匹配 |
| 12 | Chat message pipeline: extract chunks → query DB → return | Integration | 返回 search results |

---

## Phase 4: 補充 (可延後)

| # | 修復項 | 目標 |
|---|--------|------|
| 13 | `/mcp/sse` 路由 | 可選 |
| 14 | `/mcp/message` 路由 | 可選 |
| 15 | Config Manager: `update_active_workspace()` | CI 稳定 |

---

## 驗證順序（每個階段）

1. `cargo test` → 所有 unit test 通過
2. `cargo build --release` → zero errors
3. `cargo install --path . --force` → 成功
4. 手動 curl 驗證每个 endpoint
5. 記錄到 deployment checklist

---

## 執行規則

- 每個修復都必須有 tests 先写，then 写代码
- 每个 stage 必須全部通过 before moving to next
- Don't claim completion without curl verification