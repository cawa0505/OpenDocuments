# Document Ingest Specification

## Purpose

Define the document ingestion contract for the Rust backend, aligned with the
original Node.js implementation (packages/server/src/http/routes/documents.ts,
packages/core/src/ingest/pipeline.ts, packages/core/src/ingest/document-store.ts,
packages/cli/src/commands/index-cmd.ts). Corrects the current divergences:
real source_path semantics for CLI indexing, dedup by (workspace_id,
source_path), 50 MiB upload limit, and chunk id format verification.

## Requirements

### Requirement: Source Path 語意（CLI 保留真實路徑）
系統 SHALL 依來源區分 source_path 的儲存值：
- CLI 索引（`opendoc document index`）：上傳時附帶 `x-source-path` header，
  值為 `resolve(inputPath)` 後的絕對路徑；server 收到該 header 時
  source_path 直接使用該值（Node index-cmd.ts:43 行為）。
- HTTP 手動上傳：無 header 時 source_path = `{workspace_id}/{basename}`（現行格式）。

#### Scenario: CLI 索引帶真實路徑
- **WHEN** 執行 `opendoc document index ./docs`，其中 `docs/A.md` 的絕對路徑為 `/mnt/data/docs/A.md`
- **THEN** 上傳請求附帶 `x-source-path: /mnt/data/docs/A.md`
- **THEN** 文件列出的 `sourcePath` 為 `/mnt/data/docs/A.md`

#### Scenario: HTTP 手動上傳不帶路徑
- **WHEN** 透過 multipart 上傳 `B.md`（workspace `homelab`）且無 `x-source-path` header
- **THEN** `sourcePath` 為 `homelab/B.md`

#### Scenario: 不同目錄同名檔案不碰撞
- **WHEN** CLI 索引 `/dir1/report.md` 與 `/dir2/report.md`
- **THEN** 兩者 source_path 不同（絕對路徑不同），各自建立為獨立文件

### Requirement: 上傳去重（依 workspace + source_path）
系統 SHALL 在上傳時以 `(workspace_id, source_path)` 為唯一鍵：
- 存在且未刪除（deleted_at IS NULL）→ 更新既有文件（reindex 語意：
  更新 chunk_count、file_size_bytes、indexed_at、status='indexed'），不新增列。
- 不存在 → 新增文件。
- `ON CONFLICT(id) DO UPDATE`（現行 lib.rs:1476-1485）為死碼，SHALL 移除。

#### Scenario: 同 workspace 重複上傳同檔
- **WHEN** 對 workspace `homelab` 連續兩次上傳同名檔案 `dup.md`
- **THEN** documents 表中只有一列 `homelab/dup.md`
- **THEN** 第二次上傳回傳的 `documentId` 與第一次相同，`status` 為 `indexed`

#### Scenario: 不同 workspace 同名檔案各自獨立
- **WHEN** 上傳 `dup.md` 至 workspace `homelab` 與 `OpenDocuments`
- **THEN** 兩份文件並存（source_path 分別為 `homelab/dup.md`、`OpenDocuments/dup.md`）

### Requirement: 上傳大小上限 50MB
系統 SHALL 拒絕超過 50 MiB 的檔案上傳（Node documents.ts:55-58 行為），
回傳 HTTP 413，不寫入任何資料。

#### Scenario: 超過上限的檔案
- **WHEN** 上傳 51 MiB 的檔案
- **THEN** server 回傳 413 且 documents 表無新增列

### Requirement: DELETE 文件回傳 404（不存在時）
系統 SHALL 在 `DELETE /documents/:id` 時比對 Node 契約
（documents.ts:37-44）：文件不存在於該 workspace（或已軟刪除）→
回傳 404 `{ "error": "Document not found" }`；存在 → 軟刪除
（deleted_at = CURRENT_TIMESTAMP）並回傳 200 `{ "deleted": true }`。
（2026-07-31 修復：原先 0 rows 也回 200，且跨 workspace 的 id 會被
靜默忽略。）

#### Scenario: 刪除不存在的文件
- **WHEN** 對不存在的 id 呼叫 `DELETE /documents/{id}`（workspace `homelab`）
- **THEN** 回傳 404 `{ "error": "Document not found" }`

#### Scenario: 刪除跨 workspace 的文件
- **WHEN** 以 workspace `homelab` 刪除屬於 `OpenDocuments` 的文件
- **THEN** 回傳 404，且該文件保持未刪除

### Requirement: Chunk ID 格式與 Node 一致
系統 SHALL 以 `${documentId}_chunk_{i}` 格式（i 為 0 起始序號）寫入向量庫
（Node document-store.ts:147 行為）。

#### Scenario: 驗證 chunk id
- **WHEN** 上傳文件並完成切塊（N 塊）
- **THEN** 向量庫中該文件的 chunk id 依序為 `{documentId}_chunk_0` 至 `{documentId}_chunk_{N-1}`
- **THEN** documents 表的 `chunk_count` 等於 N

> 2026-07-31 調查結論：Rust 後端目前不持久化 chunk（parse 結果僅在記憶體、
> 僅寫 `chunk_count`）。本需求待向量庫寫入實作後驗證；若新增向量庫，
> chunk key 須使用穩定 document id（去重路徑已保證 id 穩定）。
