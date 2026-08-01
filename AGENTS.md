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

> **更新時間**: 2026-08-02  
> **最後修訂者**: AI 助手（遵循用戶要求，將安裝路徑對齊 Mono-repo `crates/opendoc-cli` 規範，並補上 Makefile 建置指引）
