# OpenDocuments AGENTS.md — 開發者要求協議

> **注意**：每次操作前，請先確認以下事項：
> 1. [ ] 目標工作區配置已正確設定 (`config.toml` → `active_workspace`)
> 2. [ ] 編譯與安裝已驗證 (`cargo check` 零錯誤 → `cargo install --path crates/opendoc-cli --force` 或使用 `make install`)
> 3. [ ] 核心邏輯 (`resolve_ws`) 已在正確的匹配條件下調用

---

## 1. 核心註解
| 項目 | 說明 |
|------|------|
| 核心目標 | `resolve_ws` helper 需正確回退至 `active_workspace` → `default_workspace` |
| 配置位置 | `~/.config/opendocuments/config.toml` (`model.active_workspace`) |
| 執行優先級 | 工作區切換必須 persisted，否則後續命令將使用預設值 |
| 零警告編譯防線 | 所有對程式碼的修改、新增或修復，凡是編譯 `cargo check` 出現的任何 Warning（如 `unused_imports`, `unused_variables`），必須立刻清理與移除多餘引用，維持 100% 絕對乾淨編譯。 |
| 零 Mock 動態原則 | 嚴禁在 RAG 核心或 TUI 當中塞入任何硬編碼的靜態 Mock 文件（如私人 IP、Bumblebee 機器、或硬寫死 mock 檔案）。TUI 與 RAG 必須完全、且動態對接 SQLite 實體資料庫，實時查詢、過濾與反映當前工作空間中的實體檔案，做到真實呈現。 |

---

## 2. 工作區修正协议 (要求協議)
> 以下是 2026-07-xx 期間修正的核心問題，避免 repeat 錯誤

### 2.1 基本修正流程
```
1. resolve_ws 必須返回 String（非 Option<String>）
2. 所有 workspace 參數必須是 Option<String>
3. 每次修改動動後，立即 run cargo check
4. 只有 check 通過後，才執行 cargo install
5. 安裝後必測 2 個核心命令：
   - opendoc workspace switch <name>
   - opendoc document index .
```

### 2.2 關鍵函數模板
```rust
let resolve_ws = |opt_ws: Option<String>| -> String {
    opt_ws.filter(|s| !s.is_empty())
        .map(|s| s.trim().to_owned())
        .or_else(|| app_cfg.model.active_workspace.clone())
        .unwrap_or_else(|| app_cfg.model.default_workspace.clone())
};
```

### 2.3 常見錯誤與修正
| 錯誤 | 原因 | 修正 |
|------|------|------|
| `WorkspaceSubcommands::Show` not covered | 匹配沒有覆蓋所有分支 | 添加 `=>` 分支 |
| `use of moved value: workspace` | 同一項在 loop 中多次調用 `resolve_ws()` | 在 loop 外解析一次，存入 `String` 後重用 |
| `TryFrom<&Option<String>>` | `Option<String>` 不能直接傳給 HeaderValue | 改用 `.as_deref()` 或直接 `&resolved` |
| `update_active_workspace` not found | 方法未實現在 `ConfigManager` | 改用 `get_config() → update_config(cfg)` |

---

## 3. 驗證流程
```bash
# Step 1:  compilation
cargo check                          # 零錯誤
cargo build                          # 零錯誤

# Step 2: Installation
cargo install --path crates/opendoc-cli --force      # 安裝到 ~/.cargo/bin/opendoc
# 或者直接在根目錄執行自動化建置 (含 WebUI 打包)
make install

# Step 3: 功能驗證
opendoc workspace switch OpenDocuments
cat ~/.config/opendocuments/config.toml | grep active_workspace
# 必須輸出: active_workspace = "OpenDocuments"

# Step 4: 工作流程驗證
opendoc document index . | grep "目標工作空間: OpenDocuments"
```

---

## 4. 錯誤排查速查表
| 現象 | 可能原因 | 檢查命令 |
|------|---------|---------|
| 索引顯示 `default` | `active_workspace` 未設定 | `grep active_workspace *.toml` |
| 上傳返回 500 | 工作區 FK 違反 | `sqlite3 db.sqlite "SELECT * FROM workspaces;"` |
| CLI 指令不可見 | binary 未安裝 | `which opendoc && ls -l ~/.cargo/bin/opendoc` |
| 編譯報錯 | 類型不匹配 | `cargo check 2>&1 \| grep error` |

---

## 5. 注意事項
> - 禁止手動修改 `Cargo.lock` 或 `target/` 目錄
> - `resolve_ws` 必須使用閉包形式，以避免引用捕獲問題
> - 配置更新後，需要重新載入 `ConfigManager` 才能生效
> - 每改一次，必須立即測試，避免錯誤積累

---

## 6. 核心隱私與安全條款 (嚴格限制)
> **!重要!** 本專案為本地優先、零信任 RAG 基礎設施。任何 AI 開發助手在修改程式碼時，必須嚴格遵守以下隱私與安全規範：
> 1. **禁止硬編碼任何用戶的私有設備主機名、IP 位址或內部測試拓撲**：
>    - 嚴禁硬編碼任何如 `bumblebee`、`arhat`、`192.168.*.*` 等內部設備名稱與私有 IP。
>    - 所有的測試、範例、預設值、模擬或預置回傳數據，必須使用標準環回位址 `127.0.0.1`、`localhost` 或與用戶無關的標準 RFC 5737 測試位址 (例如 `192.0.2.0/24` 系列)。
> 2. **禁止寫入任何 Mock/寫死的回傳數據**：
>    - 禁止為了圖方便而在核心 RAG 檢索流程（如 `search_and_rerank`）中塞入寫死的假資料或模擬 chunk。
>    - 核心 RAG 必須是完全動態的，僅能回傳用戶實體資料庫與向量庫動態檢索到的真實資料。若無匹配，直接回傳空陣列 `Vec::new()`。
> 3. **金鑰安全物理隔離**：
>    - 嚴格落實 `opendoc-llm` 核心的 BYOK 金鑰安全防線，所有 API 密鑰僅儲存在本地 SQLite 表，記憶體僅在請求發送時載入，任何前端對外 API 路由嚴禁回傳 API 密鑰本體。
>
> 任何違反上述條款的 PR/變更都將直接拒絕並退回重寫。

---

> **更新時間**: 2026-08-03  
> **最後修訂者**: AI 助手（遵循用戶要求，追加嚴格的本地隱私與零信任 RAG 安全條款，並清空底層所有硬編碼 mock 測試內容）
