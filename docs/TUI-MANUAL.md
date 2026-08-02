# 🖥️ OpenDocuments TUI 終端機開發與使用手冊

OpenDocuments 提供極致輕量、無須啟動 Web 服務與 Node 渲染的**本機 Terminal UI (TUI) 檢索 state 偵錯面板**。

## 🚀 1. 啟動 TUI

在終端機執行以下命令即可進入原生 TUI 檢索介面：
```bash
opendoc tui
```

---

## 📂 2. 工作區連動 (Workspace Context)

1. **自動遵從 Active Workspace**：
   - TUI 啟動時會自動讀取並優先加載本機持久化的 `active_workspace`（即您在 CLI 中使用 `opendoc workspace switch <name>` 所設定的工作區）。
   - 若未設定 `active_workspace`，則自動回退至配置中的 `default_workspace`（預設為 `"default"`）。
2. **`Ctrl+W` 原地無縫切換**：
   - 在 TUI 介面中，隨時按下 **`Ctrl+W`** 快捷鍵。
   - 介面中央會動態彈出亮黃色的 `切換 Workspace` 臨時輸入框。
   - 輸入您要切換的目標工作空間名稱後，按下 **`Enter`** 鍵，TUI 將會在背景**非同步持久化配置**並原地無痛切換，無須重新啟動 TUI！
   - 按下 **`Esc`** 鍵可隨時取消輸入並退回。

---

## 🔍 3. 檢索、過濾與熱鍵

- **動態混合檢索**：
  - 在上方 Cyan（青色）搜尋框直接輸入問題。
  - 按下 **`Enter`** 鍵，將會在背景透過多執行緒異步觸發 `FTS5 + Vector` 雙路混合檢索，杜絕介面卡頓。
- **響應式排版 Media Queries**：
  - TUI 內建響應式寬度偵測。當終端機寬度小於 `85` 字元時，會自動隱藏分數（Score）欄位，讓出最大寬度給檔案名稱與摘要。
  - 當視窗大於 `85` 字元時，顯示完整 3 欄（檔案、分數、摘要），高於阻斷閾值的相關分數將高亮顯示為**綠色**，其餘為**黃色**。
- **熱鍵速查**：
  - **`Ctrl+W`**：切換目前 Workspace。
  - **`Esc`**：退回上一步 / 關閉輸入框 / 關閉並退出 TUI。
