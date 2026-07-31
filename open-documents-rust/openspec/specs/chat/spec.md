# Chat Specification

## Purpose

Define the chat endpoints contract for the Rust backend, aligned with the
original Node.js implementation (packages/server/src/http/routes/chat.ts) and
the frontend SSE event handling (packages/web/src/lib/sse.ts). The normal
stream event sequence is implemented and verified.

## Requirements

### Requirement: conversationId 驗證（404）

POST /api/v1/chat/stream 收到 `conversationId` 時，若該 conversation 不存在
於目前 workspace（或已軟刪除），SHALL 回傳 404 與 Node 契約 body
`{ "error": "Conversation not found" }`（Node: chat.ts:122-128；實作：
lib.rs chat_stream_handler，2026-07-31 驗證）。

- 未提供 `conversationId` 時不做驗證，直接執行查詢。

#### Scenario: 不存在的 conversationId
- **WHEN** 請求帶不存在的 `conversationId`
- **THEN** 回應 404，body 為 `{ "error": "Conversation not found" }`

#### Scenario: 省略 conversationId
- **WHEN** 請求未帶 `conversationId`
- **THEN** 正常執行查詢並回傳 SSE 串流

### Requirement: SSE 事件序列含 error 事件（延後）

系統 SHALL 在 chat stream（POST /api/v1/chat/stream）中，於 pipeline 失敗時
發出 `error` 事件。前端已實作 error 事件處理（sse.ts:76-78）。

- `error` 事件 data 格式：`{ "error": "<錯誤訊息>" }`（前端 onError 讀 `parsed.error`）。
- 發出 `error` 事件後 SHALL 結束串流（不發 `done`）。
- 正常路徑事件序列保持現狀（已驗證 lib.rs:2079-2082）：`sources` → `confidence` → `chunk` → `done`。

> **現況（2026-07-31 驗證）**：Rust 端 `search_and_rerank` 回傳
> `Vec<DocumentChunk>`（無 Result、無 LLM streaming），sync handler 無
> mid-stream 失敗路徑，error 事件目前不可觸發。此需求保留為契約目標，
> 待 LLM streaming 落地時實作（ponytail: 同 lib.rs stream handler 註解）。

#### Scenario: pipeline 失敗時發出 error 事件（待 streaming 落地）
- **WHEN** 查詢 pipeline（搜尋/rerank）拋出錯誤
- **THEN** 串流發出 `event: error`，data 為 `{ "error": "..." }`
- **THEN** 串流結束，不發 `done`

#### Scenario: 正常查詢維持既有事件序列
- **WHEN** pipeline 成功完成
- **THEN** 串流依序發出 `sources`、`confidence`、`chunk`、`done` 四事件
