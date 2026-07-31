# Conversations Specification

## Purpose

Define the conversation endpoints contract for the Rust backend, aligned with
the original Node.js implementation (packages/server/src/http/routes/conversations.ts
and app.ts). The CRUD baseline (POST /conversations, DELETE /conversations/:id,
GET /conversations/:id/messages) is implemented and verified; this spec adds the
missing share capability.

## Requirements

### Requirement: 分享對話（share_token）

系統 SHALL 提供對話分享功能，行為對齊 Node conversations.ts:90-104：

- `POST /conversations/:id/share`：
  - 查詢：`SELECT * FROM conversations WHERE id = ? AND workspace_id = ? AND deleted_at IS NULL`
  - 不存在 → 404 `{ "error": "Conversation not found" }`
  - 產生 token：`randomBytes(16).toString('hex')`（**32 字元 hex**，非 UUID）
  - `UPDATE conversations SET shared = 1, share_token = ? WHERE id = ? AND workspace_id = ?`
  - 回傳 `{ "shareUrl": "/shared/{token}" }`

#### Scenario: 分享對話
- **WHEN** 對已存在的對話呼叫 `POST /conversations/{id}/share`（workspace `homelab`）
- **THEN** 回傳 200 `{ "shareUrl": "/shared/{token}" }`
- **THEN** 資料庫中該對話 `shared = 1` 且 `share_token` 為 32 字元 hex

#### Scenario: 分享不存在的對話
- **WHEN** 對不存在的 id 呼叫 `POST /conversations/{id}/share`
- **THEN** 回傳 404 `{ "error": "Conversation not found" }`

### Requirement: 公開對話視圖（免授權）

系統 SHALL 提供公開分享視圖，行為對齊 Node conversations.ts:7-14 與 app.ts:53-55：

- `GET /shared/:token` 與 `GET /api/v1/shared/:token` 皆註冊於
  authMiddleware 之前（**無 API key 驗證**；Rust 端僅需 `/api/v1/shared/:token`，
  裸路徑由前端 SPA 處理）。
- 查詢：`SELECT * FROM conversations WHERE share_token = ?`（**無 workspace
  範圍、無 deleted_at 過濾**）。
- token 無效 → 404 `{ "error": "Not found" }`（注意：body 為 "Not found"，
  與 share 的 "Conversation not found" 不同）。
- 有效 → 200 `{ "conversation": {raw row, snake_case 含 share_token}, "messages": [...] }`
  （messages 形狀與 `GET /conversations/:id/messages` 一致：
  id, conversationId, role, content, sources, profileUsed, confidenceScore,
  responseTimeMs, createdAt）。

#### Scenario: 透過分享連結讀取
- **WHEN** 以有效 token 呼叫 `GET /api/v1/shared/{token}`
- **THEN** 回傳 200 `{ "conversation": { ... }, "messages": [...] }`
- **THEN** conversation 為原始資料列（含 share_token）

#### Scenario: 無效 token
- **WHEN** token 不存在於任何 conversation
- **THEN** 回傳 404 `{ "error": "Not found" }`
