# Collections Specification

## Purpose

Define the collections endpoints contract for the Rust backend, aligned with
the original Node.js implementation (packages/server/src/http/routes/collections.ts,
packages/core/src/document/collection-manager.ts, migration 002). The CRUD
baseline (POST /collections, DELETE /collections/:id) is implemented and
verified; this spec adds the document membership capability and the
fresh-install join table DDL.

## Requirements

### Requirement: collection_documents 關聯表（fresh install DDL）

系統 SHALL 在 `init_db_pool`（opendoc-storage）中建立 `collection_documents`
關聯表，結構與 Node migration
（packages/core/src/storage/migrations/002_add_versioning_collections.sql）一致：

```sql
CREATE TABLE IF NOT EXISTS collection_documents (
    collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    document_id TEXT REFERENCES documents(id) ON DELETE CASCADE,
    PRIMARY KEY (collection_id, document_id)
);
```

- 級聯刪除由 FK `ON DELETE CASCADE` 處理（collection 或 document 刪除時
  自動移除關聯列；Node migration 002 行為）。
- 既有部署（Node 時代建立的 DB）已含此表，CREATE IF NOT EXISTS 不影響。

#### Scenario: fresh install 建立關聯表
- **WHEN** 全新資料庫經 `init_db_pool` 初始化
- **THEN** `collection_documents` 表存在且結構與上列一致

### Requirement: 文件-集合成員操作

系統 SHALL 提供集合成員 API，行為對齊 Node collections.ts：

- `POST /collections/:id/documents/:docId`：加入文件，回傳 `{ "added": true }`。
- `DELETE /collections/:id/documents/:docId`：移除文件，回傳 `{ "removed": true }`。
- `GET /collections/:id/documents`：列出集合內文件，回傳 `{ collection, documents }`；
  collection 不存在 → 404 `{ "error": "Collection not found" }`。

語意細節（Node collection-manager.ts:38-70）：

- add/remove 使用 workspace 限定 SQL：collection 與 document 皆須屬於
  目前 workspace。
- **document 不存在時不報錯**（INSERT OR IGNORE ... SELECT / DELETE ...
  EXISTS 語意，silent no-op），僅回 `{ added: true }` / `{ removed: true }`。
- add 為 idempotent（INSERT OR IGNORE，重複加入無效果）。

#### Scenario: 加入文件至集合
- **WHEN** 對已存在集合呼叫 `POST /collections/{id}/documents/{docId}`
- **THEN** 回傳 200 `{ "added": true }`
- **THEN** `collection_documents` 表新增 `(collection_id, docId)` 列
- **THEN** 重複呼叫相同操作不回傳錯誤（idempotent）

#### Scenario: 移除文件
- **WHEN** 對已含 `docId` 的集合呼叫 `DELETE /collections/{id}/documents/{docId}`
- **THEN** 回傳 200 `{ "removed": true }`
- **THEN** `collection_documents` 表中該關聯列已刪除

#### Scenario: 列出集合文件
- **WHEN** 對已存在集合呼叫 `GET /collections/{id}/documents`
- **THEN** 回傳 200 `{ "collection": { ... }, "documents": [ ... ] }`，
  documents 為該集合內文件的完整物件（經 documents 表 join）

#### Scenario: 操作不存在的集合（GET）
- **WHEN** 對不存在的 collection id 呼叫 `GET /collections/{id}/documents`
- **THEN** 回傳 404 `{ "error": "Collection not found" }`

#### Scenario: 不存在的 document 加入/移除
- **WHEN** 對已存在集合呼叫 add/remove 但 `docId` 不存在於 workspace
- **THEN** 回傳 200 `{ "added": true }` / `{ "removed": true }`（silent no-op）
