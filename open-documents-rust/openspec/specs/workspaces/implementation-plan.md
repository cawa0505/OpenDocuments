# Workspace UUID Implementation Plan

## 目標

將 workspace 主鍵恢復為 Node.js 版本的 UUID 語意，同時保留舊資料庫、CLI、
WebUI 與 MCP client 以 workspace name 操作的相容性。所有變更必須先在資料庫
副本驗證，禁止以正式資料庫作為 migration script 的測試環境。

## 已確認基線

- Node.js `WorkspaceManager.create()` 使用 `randomUUID()` 建立 workspace id。
- Node.js workspace resolver 接受 id 或 name；未知名稱會自動建立 workspace。
- Rust live DB 目前有兩個 legacy workspace，皆為 `id = name`：
  `homelab` 與 `OpenDocuments`。
- Live DB 目前至少包含 documents 429 筆、conversations 2 筆、dictionary 80 筆；
  migration 必須視為正式 user data migration。
- 9 張表以 `workspace_id` 參照 `workspaces(id)`：`workspace_members`、
  `connectors`、`documents`、`tags`、`conversations`、`query_logs`、
  `collections`、`api_keys`、`dictionary`。
- 這些 FK 只有 `ON DELETE CASCADE`，沒有 `ON UPDATE CASCADE`，不可直接更新
  parent id。
- 目前未提交的 Rust diff 無法編譯：
  `crates/opendoc-mcp/src/lib.rs` 的 resolver 函式宣告不完整；不得把此 diff
  視為已完成實作。
- 先前 `/tmp/migration-workspace-uuids.sh` 使用 SQLite 不支援的 procedure
  語法；該檔案作廢，禁止再執行。

## 相容性規範

### Workspace identity

- 新 workspace 的 `id` MUST 使用官方 `uuid` crate 的 UUID v4。
- `name` MUST 保持使用者可讀且繼續作為 config、CLI 與 WebUI 的穩定識別值。
- API response MUST 同時提供 `id` 與 `name`；既有 Node 欄位不可刪除或改名。
- Legacy `id = name` rows MUST 在 migration 前仍可正常解析，確保舊 Node DB
  可直接由新 binary 讀取。

### Resolver input

中央 resolver MUST 支援 Node.js 已接受的輸入：

1. `x-workspace`
2. `x-workspace-id`
3. `x-workspace-name`
4. query `workspace`
5. query `workspaceId`
6. 無 explicit value 時，config `active_workspace`（適用 MCP/CLI caller）
7. 最後回退 config `default_workspace`

解析值 MUST 以 `id` 優先、`name` 次之查找，最後統一回傳 canonical
`workspaces.id`，供所有 FK query 使用。

### Unknown workspace

- 為維持 Node.js 相容性，explicit unknown workspace name MUST 自動建立，且新 id
  MUST 為 UUID v4。
- 建立動作 MUST 由單一共用 helper 完成；禁止 upload、MCP、REST 各自維護
  不同 INSERT 邏輯。
- 建立動作 MUST 在 transaction 中重新查找後再 INSERT，避免並行請求重複建立。
- UUID 格式的 unknown `x-workspace-id` MUST 回 400，不得把 UUID 字串當作新
  workspace name；這是避免 typo／stale id 產生幽靈 workspace 的安全優化。
- 缺省 default workspace 若不存在，server startup MUST 先用 UUID 建立；若 runtime
  仍找不到，回 500 並記錄 invariant error，不可靜默建立名為 `default` 的 row。

### Default workspace

- default workspace 的名稱 MUST 完全來自 `config.toml`，禁止硬編碼 `default`。
- 刪除保護 MUST 以 config default name 解析出的 canonical id 判斷。
- MCP tool 未提供 workspace 時 MUST 使用 `active_workspace -> default_workspace`。
- WebUI MUST 使用 API 的 `isDefault` 判斷刪除保護；刪除 active workspace 後 MUST
  切回 `isDefault` workspace，缺值時送空 header 讓 server fallback。
- upload 的 `x-collection = default` 是 collection 名稱，不屬於本次 workspace 修正。

## 資料遷移規範

### 腳本要求

- 建立 repo 內可審查的 migration script；不得沿用 `/tmp` 失敗腳本。
- 預設 MUST 為 dry-run；只有明確 `--execute` 才能修改指定 DB。
- 執行前 MUST 使用 SQLite `.backup` 建立時間戳備份，並輸出備份路徑。
- 腳本 MUST 拒絕不存在、無法讀取、沒有 `workspaces` 表或正在被錯誤路徑指定的 DB。
- migration map MUST 先固定產生 `old_id -> new_uuid`，同一 transaction 內使用。
- 因 FK 無 `ON UPDATE CASCADE`，腳本可在單一 connection／transaction 中暫停 FK
  enforcement，更新全部 9 張子表後再更新 parent；commit 前 MUST 執行一致性檢查。
- 已是 UUID 的 workspace MUST 保持不變，使腳本可安全重跑。
- 不存在的 optional legacy table MUST 明確記錄為 skip；不得因版本差異留下半套
  transaction。

### 副本驗證

正式 DB 執行前，MUST：

1. 以 SQLite `.backup` 建立測試副本。
2. 記錄每個 workspace 與每張子表的遷移前 COUNT。
3. 對副本執行 dry-run，確認計畫只包含 legacy `id = name` rows。
4. 對副本執行 `--execute`。
5. 驗證所有 workspace id 為合法 UUID v4，name 不變。
6. 驗證每張子表總筆數及各 workspace 筆數前後一致。
7. 驗證 `PRAGMA integrity_check = ok` 且 `PRAGMA foreign_key_check` 零列。
8. 使用新 binary 對副本完成 name/UUID 雙軌 API contract tests。

任一步失敗即停止；禁止對正式 DB 執行。正式 DB migration 必須在新 binary 已完成
全部測試、安裝完成但服務停止的維護窗口內進行。

## 實作 Lanes

### Lane A — MCP／REST resolver（序列主線）

Owner: `crates/opendoc-mcp/src/lib.rs`

1. 先恢復目前 diff 的可編譯狀態，不擴增行為。
2. 補 failing tests，證明 name、UUID、Node header/query aliases、missing fallback、
   unknown name auto-create、unknown UUID 400、default delete protection。
3. 實作單一 resolver／get-or-create helper。
4. 逐一轉換所有 workspace-scoped handler，禁止保留原樣字串作 FK id。
5. 移除 MCP schema 與 handler 的 workspace `default` 字面值。

此 lane 完成前，不得執行 migration 或部署。

### Lane B — Storage seed（可與 Lane A 平行，檔案不重疊）

Owner: `crates/opendoc-storage/src/lib.rs`、
`crates/opendoc-storage/Cargo.toml`、`Cargo.lock`

1. 先以 name 查找 default workspace，兼容 legacy `id = name` 與 UUID rows。
2. 缺失時用 UUID v4 建立。
3. 新增 fresh DB test：default name 正確、id 為 UUID、重啟不重複建立。

### Lane C — WebUI（獨立檔案）

Owner: `packages/web/src/components/workspaces/WorkspacesPage.tsx`

1. 使用 `isDefault` 阻止刪除。
2. 刪除 active workspace 後切換到 API 回傳的 default name。
3. workspace subtitle 顯示 id；UUID 使用 monospace 並允許安全換行。
4. 執行 frontend typecheck/build；必要時做瀏覽器驗證。

### Lane D — Migration（依賴 Lane A + B tests green）

Owner: 新 migration script 與本 spec 文件。

1. 寫 dry-run-first script 與最小 shell self-check。
2. 只在 DB 副本演練。
3. 產出 before/after evidence；未通過不得要求正式執行許可。

各 writer lane 不得修改其他 lane 的 ownership。整合者負責處理跨 lane compile/test 衝突。

## 驗證閘門

### Gate 0 — Diff integrity

- `git diff --check` 通過。
- `cargo check` 零錯誤。
- 確認沒有意外覆寫使用者既有變更。

### Gate 1 — Red/green contract tests

至少覆蓋：

- `x-workspace` name -> canonical UUID
- `x-workspace` UUID -> same UUID
- Node header aliases與 query aliases
- missing／empty -> configured default
- unknown name -> one UUID workspace（重複請求仍只有一列）
- unknown UUID id -> 400
- new workspace API -> UUID id
- default workspace by UUID -> delete 400
- non-default workspace -> delete success
- MCP missing workspace -> active/default config chain，不建立 `default` row
- upload unknown name -> 共用 helper 建 UUID workspace

### Gate 2 — Project checks

- `cargo test -p opendoc-storage`
- `cargo test -p opendoc-mcp`
- root `cargo test`
- `cargo check`
- frontend `tsc -b && vite build`

### Gate 3 — DB copy migration

- before/after counts 完全一致。
- `integrity_check = ok`。
- `foreign_key_check` 零列。
- name 與 UUID header 對同一資料回相同結果。

### Gate 4 — Install and service

- 確認 source mtime 早於新 installed binary mtime。
- `cargo install --path . --force` 成功。
- restart `opendoc-server`。
- 使用正確 health endpoint、workspace list、documents、conversation、MCP search/index
  完成 HTTP contract 驗證。

### Gate 5 — WebUI

- default workspace 無 delete affordance／操作被阻止。
- UUID subtitle 不再與 name 重複，CJK／窄 viewport 不溢出。
- workspace switch、刪除 non-default、回退 default 後請求正常。

只有 Gate 0–5 全部通過，任務才可報告完成並 commit。正式 DB migration 的執行
需另行取得使用者明確同意。

## 回滾條件

- migration、FK、COUNT 任一不一致：停止服務，保留故障 DB，從執行前 `.backup`
  還原到新檔案後再原子替換；不得就地修補未知狀態。
- 新 binary API contract 失敗：不執行正式 DB migration，修 code 後重跑 Gate 0 起。
- WebUI 失敗但 backend contracts 正常：只回修 Lane C，不改 DB。

## 明確不做

- 不新增 UUID 套件；沿用官方 `uuid` crate。
- 不改 config 的 `active_workspace`／`default_workspace` 儲存語意（仍存 name）。
- 不把 collection 的 `default` 名稱改成 workspace 規則。
- 不加入 speculative cache；workspace 數量小，先用正確的 indexed lookup。
- 不在 startup 自動執行 legacy PK migration；資料型 migration 保持明確、可備份、
  可 dry-run 的維護操作。
