# Workspaces Specification

## Purpose

Define the workspace model and resolution contract for the Rust backend,
aligned with the original Node.js implementation
(`packages/core/src/workspace/manager.ts` and `packages/server/src/http/workspace.ts`).
This spec records the verified Node contract, the Rust divergence, and the
pending workspace-layer decisions (`[待討論]` items).

## Requirements

### Requirement: workspace id 唯一值（Node 用 UUID）

Node `WorkspaceManager.create()` SHALL 用 `randomUUID()` 產生 workspace id
（`packages/core/src/workspace/manager.ts:39`），因此 Node 的 workspace id
是 UUID，與 name 不同。

Rust 現況：ensure-workspace `INSERT OR IGNORE INTO workspaces (id, name, created_at)`
以 `id = name` 建立（live DB 實證：`homelab|homelab`），與 Node 契約偏離。

- [待討論] **UUID 遷移（方案 B）**：恢復 Node 語意（id = UUID）。
  影響面：documents / conversations / collections 的 FK REFERENCES
  workspaces(id)、X-Workspace header 語意（Node 接受 id 或 name，
  `getById ?? getByName`，workspace.ts:39-63）、CLI `resolve_ws` 與 config
  `active_workspace` / `default_workspace`（存 **name**，語意不變）。
  遷移為一次性 DDL（改 id 連 FK 級聯）+ 解析層補 `getById`。
  技術選型已定：官方 uuid crate（v1.x，現鎖 1.24.0，v4 feature），
  與 document/conversation id、share token 同 crate。

#### Scenario: 目前 workspace 建立（偏離狀態）

- **WHEN** 全新安裝啟動 server（config `default_workspace = "homelab"`）
- **THEN** workspaces 表僅有 `id = "homelab"`、`name = "homelab"` 一行
- **THEN** 無任何 `"default"` row（與 Node `ensureDefault()` 硬編碼不同，刻意偏離）

#### Scenario: UUID 遷移完成後

- **WHEN** 執行 UUID 遷移（[待討論] 方案 B）
- **THEN** 新建 workspace 的 `id` 為 UUID v4，`name` 保持可讀名稱
- **THEN** 既有 workspace 的 id 已遷移為 UUID 且 FK 引用完整
- **THEN** X-Workspace header 接受 id 或 name（Node 雙軌語意）

### Requirement: workspace 解析機制

Rust 的 workspace 解析 SHALL 遵循：header `x-workspace`（非空）→ config
`default_workspace` 兜底（現為 `homelab`），與 Node 契約對齊但以 config
驅動取代 Node 的 `ensureDefault()` 硬編碼 "default"。

- Node 契約：header `x-workspace` **或** `x-workspace-id` **或**
  `x-workspace-name` 或 query `workspace` / `workspaceId` →
  `getById ?? getByName`（id/name 雙軌）→ 未知則自動 create →
  `config.workspace`（name）→ `ensureDefault()`。
- Rust 現況：`resolve_workspace` 只走 name（header 值原樣回傳），
  缺失/空 header → config `default_workspace`。無 `getById`。

- [待討論] **MCP tool workspace 參數預設**：`opendoc-mcp` 的 tool schema
  宣告 `"workspace": { "default": "default" }`（lib.rs:1835 / 2080），
  handler 再 `.unwrap_or("default")`（lib.rs:1885 / 2130）。缺參時：
  `opendocuments_search` 搜尋 "default"（不存在）→ 靜默空結果；
  `opendocuments_index_path` 上傳至 "default" → ensure-workspace
  憑空建立 "default" row（污染清單、打破「無 default row」不變量）。
  正確修法：schema 移除字面值，handler fallback 走 config 鏈
  （handler 內已載入 `app_cfg`，lib.rs:1893-1896）。與 UUID 遷移一起做。

#### Scenario: 無 header 解析

- **WHEN** 呼叫任一 workspace-scoped API 且無 `x-workspace` header
  （fresh install，config default 為 `homelab`）
- **THEN** 解析結果為 `homelab`（fresh DB 實證：conversation 落在 homelab）

#### Scenario: MCP tool 缺 workspace 參數（目前行為）

- **WHEN** 呼叫 `opendocuments_search` 或 `opendocuments_index_path` 未帶
  `workspace` 參數
- **THEN** 目前解析為字面值 `"default"`（偏離，[待討論]）
- **THEN** search 靜默空結果；index 於 server 建立 "default" workspace row

### Requirement: 預設 workspace 建立與保護

Rust SHALL 在啟動時以 config `default_workspace`（現為 `homelab`）
自動建立預設 workspace；預設 workspace SHALL 不可刪除。

- Node 對 `name === "default"` 回 400（`admin.ts:293-300`）；Rust 保護
  對象為 config 的 default（正確演進，2026-07-31 audit 確認）。

#### Scenario: 啟動自動建立

- **WHEN** server 以全新 DB 啟動
- **THEN** workspaces 表自動含有 config default workspace（`homelab`）一行

#### Scenario: 刪除預設 workspace

- **WHEN** 對 config default workspace 呼叫 `DELETE /api/v1/workspaces/{id}`
- **THEN** 回傳 400（不可刪除）

### Requirement: workspace list / delete API

`GET /api/v1/workspaces` SHALL 回 `{ workspaces: [...] }` 且公開（無 auth）；
`DELETE /api/v1/workspaces/:id` SHALL 對不存在回 404。

- workspace 物件形狀（Node `manager.ts:4-10`，camelCase）：
  `{ id, name, mode, settings, createdAt }`。Rust 現回傳相同欄位 + 額外
  `isDefault`（相容延伸，非偏離）。

#### Scenario: 列出 workspaces

- **WHEN** 未帶 auth 呼叫 `GET /api/v1/workspaces`
- **THEN** 回傳 200 `{ "workspaces": [...] }`，每項含
  `id` / `name` / `mode` / `settings` / `createdAt`

#### Scenario: 刪除不存在的 workspace

- **WHEN** 對不存在的 id 呼叫 `DELETE /api/v1/workspaces/{id}`
- **THEN** 回傳 404

### Requirement: 非偏離確認（2026-07-31 audit）

以下出現 `"default"` 字面值之處 SHALL 被視為非偏離（Node 原樣行為或
mock 資料），不列入遷移範圍：

- upload handler 的 `x-collection` header fallback（lib.rs:1607）——
  Node 同行為（LanceDB "default" collection 命名）。
- `opendoc-storage` 模擬候選集（lib.rs 306-344）—— mock 資料。
- 前端 `api.ts`：`localStorage 'active-workspace' || ''`，缺值由
  server 端 config 兜底，無 "default" 字面值。

#### Scenario: upload 缺 x-collection

- **WHEN** 上傳文件未帶 `x-collection` header
- **THEN** collection id 為 `"default"`（與 Node 相同，非偏離）

#### Scenario: 前端缺 active-workspace

- **WHEN** 前端 `localStorage` 無 `active-workspace` 值
- **THEN** 請求不帶 workspace header，server 以 config default 兜底
- **THEN** 解析為 `homelab`，無 "default" 字面值進入請求
