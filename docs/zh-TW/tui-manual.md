# 📟 OpenDocuments Terminal UI (TUI) 深入使用與調試手冊

[English](../en/tui-manual.md) | **繁體中文**

---

## 🌐 1. 簡介與定位

在 OpenDocuments 的 100% 本地、零信任 RAG 架構中，除了現代化的左右對話流 WebUI 之外，專案特別針對**極客與本機運維除錯**，內建並特化了一款 **「0 外部依賴、極致輕量、動態響應」的終端機 RAG 檢索面板 (TUI)**。

這款 TUI 允許用戶直接與本地的 SQLite 元數據和 LanceDB 向量庫並行溝通，無須啟動任何外接 Node.js 進程或瀏覽器，即可原地在終端機內進行：
1. **多重混合檢索 (Hybrid Search)**：結合 LanceDB 稠密向量與全文關鍵字搜尋，並進行 RRF 重排與分數過濾。
2. **多 Role 空間無縫管理**：透過快捷鍵即時切換工作空間並進行持久化，即時改變 pointer。

---

## 🚀 2. 啟動方式

確保您的 Rust toolchain 與本地資料庫已就緒，於專案根目錄下執行：

```bash
# 啟動 TUI 並自動載入 active_workspace (若無則回退至 default_workspace)
opendoc tui
```

---

## 🕹️ 3. 鍵盤操縱與互動指南

TUI 全面遵從標準 CLI 與 Vim-like 的極簡操縱邏輯，各鍵位定義如下：

| 鍵位 / 快捷鍵 | 作用面板 | 實體執行行為與 UX 原理 |
| :--- | :--- | :--- |
| **`Ctrl + W`** | 搜尋面板 | **開闢 Workspace 切換控制艙**：原地暫停當前搜尋，於下方動態拉出 workspaces 的列表。 |
| **`↑ / ↓`** | 切換面板 | **輪巡工作空間**：在可用 Workspace 列表進行動態游標選擇，並自動在輸入框顯示該工作區名稱。 |
| **`Tab`** | 切換面板 | **自動補全 (Autocomplete)**：一鍵補全當前游標所選中的工作區名稱。 |
| **`Backspace / 字母`** | 切換面板 | **模糊過濾**：允許手動退格或輸入字元，底層會自動利用「不分大小寫的字串包含」實體過濾、並即時聚焦到最匹配的第一個 Workspace 上。 |
| **`Enter`** | 切換面板 | **原地熱切換與持久化**：一鍵確認切換。底層會非同步重新載入新的 Workspace SQLite 與 LanceDB，更新 TUI 視圖，並自動透過 `ConfigManager` 持久化至 `config.toml` 中。 |
| **`Enter`** | 搜尋面板 | **檢索觸發**：當處於搜尋狀態時，鍵入 Enter 即觸發歷史感知的 Hybrid Search 混合檢索。 |
| **`Esc`** | 任何面板 | **安全退出**：處於切換面板時退回搜尋模式（取消切換）；處於搜尋模式時優雅關閉 TUI 並歸還終端機控制權。 |

---

## 📐 4. 動態響應式設計 (TUI Media Queries)

為了防止不同終端機尺寸、分屏 (Tmux) 以及字型大小下的破版或 Panic，TUI 內建了**高防禦性的響應式斷點 (Media Queries)**：

- **嚴格過窄防禦 (Width < 50 || Height < 10)**：
  - 自動停用渲染並顯示紅色高亮警報：`⚠️ 視窗太小，請放大終端機以顯示 RAG 檢索面板...`，防止因為 Layout 零寬度或長度負數而觸發 TUI 框架 crash。
- **中等寬度響應 (50 <= Width < 85)**：
  - 自動丟棄 `Score` 欄位，實體重新分配畫面比例為：`檔案名稱 (30%)` + `內容摘要 (70%)`，確保在窄螢幕或 Tmux 垂直分屏下依然能清晰閱讀文本 Chunks。
- **寬螢幕模式 (Width >= 85)**：
  - 展示完整的三欄視圖：`檔案名稱 (20%)` + `Score (10%)` + `內容摘要 (70%)`。
  - **Score Filter 高亮**：大於 `0.75` 顯示綠色加粗，其餘顯示黃色，直觀反饋檢索置信度。

---

## 🛠️ 5. 本地調試與日誌排查

如果您在 TUI 中搜尋或切換空間時遭遇異常，可以透過以下物理路徑進行排查：

1. **檢查現役 CLI 配置**：
   ```bash
   cat ~/.config/opendocuments/config.toml
   ```
   確認 `active_workspace` 有正確與您在 TUI 中切換的名稱保持絕對一致。
2. **手動核對 Workspace 表資料**：
   ```bash
   sqlite3 ~/.opendocuments/db.sqlite "SELECT id, name FROM workspaces;"
   ```
   確認您輸入的 Workspace 確實存在於本地元數據庫中。
