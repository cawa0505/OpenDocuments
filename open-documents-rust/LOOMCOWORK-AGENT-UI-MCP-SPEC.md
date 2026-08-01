# 🤖 LoomCowork (Phase 1) Agent UI, Tool Call Interception & MCP Client Specification (LOOMCOWORK-AGENT-UI-MCP-SPEC.md)

本文件定義並記錄了 LoomCowork 閉源商業版在 Tauri 2.0 (Rust) + React 19 架構下，針對**「AI 代理人工作台 (Agent UI)」、「Tool Call 安全攔截器 (UI Gatekeeper)」與「本機 MCP Client 協定與連接器」**的完整技術實作規格。

---

## 🎨 1. Agent UI & Tool Call 互動設計 (UX / Interactive Pipeline)

傳統的 AI 聊天介面只能進行「單向的文字一問一答」，而高級的 AI 代理人（如 Claude 3.5 Sonnet / o3-mini）具備自主呼叫外部工具 (Tool Call) 的能力。

為了讓使用者能完全掌控 AI 代理人的實體行為（不論是寫入本機檔案、讀取資料庫、還是執行編譯指令），LoomCowork 實作了 **「UI Gatekeeper」安全審查機制**：

```plaintext
[LLM 推理中] ── (偵測到 Tool Call) ──► [Rust 後端攔截] 
                                            │
                                    (SSE 推播安全警報)
                                            │
                                            ▼
                                  [左欄對話流暫停，跳出審查卡片]
                                  ├─ 🔴 敏感操作：寫入實體檔案
                                  ├─ 📂 路徑: /home/user/project/index.html
                                  └─ ⚡ 參數: "..."
                                            │
                             ┌──────────────┴──────────────┐
                             ▼                             ▼
                        [❌ 拒絕 (Reject)]            [✓] 批准 (Approve)
                             │                             │
                     (回傳 Error 予 LLM)          (呼叫本機/遠端 MCP 伺服器)
                             │                             │
                      [LLM 繼續思考自癒]            [取得真實資料/結果]
                             │                             │
                             └──────────────◄──────────────┘
                                            │
                                            ▼
                                   [打字機繼續流暢噴字]
```

### 1.1 互動元件與視覺回饋 (UI Components)
* **左欄對話卡片**：當 LLM 發送 `tool_use` 請求時，左欄會渲染一個帶有霓虹黃色外框的 `[⚠️ Tool Call Pending]` 動態卡片。
* **按鈕特效**：提供 `[ 拒絕 (Reject) ]`（紅色、Hover 變亮、點擊帶震動感）與 `[ 批准 (Approve) ]`（綠色、帶有雷達波紋呼吸燈效果）。
* **狀態變更**：當批准後，卡片會轉為 `[✓ Executing...]` ➔ `[✓ Completed]`（深綠色勾），並展開顯示執行結果（StdOut / JSON）。

---

## 🔌 2. MCP Client 本機進程管理器 (Process Manager)

後端 Rust 將其作為一個獨立的 `mcp-client` 模組，負責管理本機拉起的 Stdio 進程或連向 TCP/SSE 的 MCP Servers。

### 2.1 資料表結構：SQLite `mcp_servers` 表
為了保存使用者自行擴充的 MCP 伺服器配置，於 SQLite 資料庫建立配置表：

```sql
CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    command TEXT NOT NULL,          -- 例如 "node" 或 "python"
    args TEXT NOT NULL,             -- JSON Array of String, 例如 ["/path/to/mcp.js"]
    env TEXT,                       -- JSON Object, 儲存環境變數密鑰
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    UNIQUE(workspace_id, name)
);
```

### 2.2 本機進程生命週期 (Stdio Spawn)
當使用者在 UI 啟用某個本機進程型（Stdio）的 MCP 伺服器時，Rust 後端將調用 `tokio::process::Command` 拉起進程：

* **Stdio 雙向管線**：
  * **主程式 (LoomCowork)** 的 `ChildStdin` 綁定至 **MCP Server** 的 `Stdout`。
  * **主程式** 的 `ChildStdout` 綁定至 **MCP Server** 的 `Stdin`。
  * 雙方透過 **JSON-RPC 2.0** 協定進行資料傳輸。
* **自動守護 (Auto-Restart)**：若本機 MCP 伺服器進程異常退出（如記憶體溢出），Rust 後端會自動觸發 3 次指數退避重試拉起，並在 UI 發送狀態事件 `[⚠️ local-mcp-server crashed, restarting...]`。

---

## ⚡ 3. 攔截器協定與 SSE 資料流擴充 (SSE Extension Protocol)

為了在 SSE 串流傳輸過程中完美交織「文字 Delta」、「工具呼叫」、與「安全阻斷」三大狀態，我們擴充了 `StreamEvent` SSE 事件協定：

### 3.1 事件型別定義 (Extended `StreamEvent`)

```typescript
type StreamEvent =
  | { type: "text"; delta: string }                      // 左欄打字機
  | { type: "thought"; delta: string }                   // AI 思考鏈推理
  | { type: "status"; message: string }                  // 當前動作狀態推播
  | { 
      type: "tool_call_pending"; 
      call_id: string;                                   // 唯一的安全攔截 ID
      server_name: string;                               // 請求調用的 MCP 伺服器
      tool_name: string;                                 // 工具名稱 (例如 local_file_system/write_file)
      arguments: any;                                    // LLM 傳入的工具參數
    }
  | { type: "tool_call_executing"; call_id: string }     // 批准後，顯示執行中
  | { type: "tool_call_result"; call_id: string; result: any } // 執行完畢，回傳 StStdout
  | { type: "tool_call_rejected"; call_id: string }      // 使用者拒絕執行
```

### 3.2 阻斷控制 (Stream Blocking Control)
當後端 `chat_stream_handler` 解析 LLM 傳回的流（例如 Claude 3.5 Sonnet 的 `tool_use` chunk）時：

1. **偵測並生成攔截鎖**：
   * Rust 後端解析出 Tool Call，**不直接調用工具**。
   * 在 SQLite 或記憶體中建立一筆 pending 狀態的攔截鎖，生成 `call_id`。
2. **向前端噴射 `tool_call_pending` SSE 事件**：
   * 向前端傳送此事件並暫停讀取 LLM 的後續輸出，保持與 LLM 的連線或暫存其對話 context。
3. **前端回應與 API 調用**：
   * 前端 UI 彈出審查面板。
   * 使用者點擊 `Approve` ➔ 前端發送 `POST /api/v1/mcp-calls/:id/approve`。
   * 使用者點擊 `Reject` ➔ 前端發送 `POST /api/v1/mcp-calls/:id/reject`。
4. **解鎖與復歸**：
   * 後端收到核准 ➔ 實際呼叫 Stdio 進程的 MCP Server，將 Stdout 結果轉換為 JSON-RPC 回應，以 `tool_call_result` 傳送給前端，並將該結果發回給 LLM ➔ 讓 LLM 繼續根據結果生成文字。
   * 後端收到拒絕 ➔ 傳送拒絕回應給 LLM ➔ 讓 LLM 了解此操作已被使用者阻斷，自動改採其他方案或向使用者致歉。

---

## 🛠️ 4. 後端 Rust HTTP 路由與控制器規格 (Axum Controllers)

為了支援這套安全攔截與進程管理，Rust 必須新增以下 Axum 路由：

### 4.1 MCP 伺服器配置 API (MCP Server Registry)
* `GET /api/v1/mcp-servers` (列出該 workspace 註冊的所有 MCP 伺服器)
* `POST /api/v1/mcp-servers` (新增或更新 MCP 伺服器，包含 command, args, env)
* `DELETE /api/v1/mcp-servers/:id` (註銷特定 MCP 伺服器)

### 4.2 工具調用與安全審查 API (Tool Call Gatekeeper)
* `POST /api/v1/mcp-calls/:id/approve` (核准編號為 id 的工具調用)
* `POST /api/v1/mcp-calls/:id/reject` (拒絕編號為 id 的工具調用)

---

## 🎯 5. 商業變現與企業防禦價值 (Commercial Moat)

1. **零溢出安全防禦 (Zero-Trust Local Execution)**：
   * 大企業最怕 AI 自動化腳本在背景惡意將機密代碼上傳至網路上，或是誤刪資料庫。
   * 我們的「本機 Stdio 攔截器 + UI 卡片審查」將生殺大權 100% 交還給使用者。安全感領先雲端 SaaS 三個世代。
2. **本地 IT 工具生態 (Enterprise Tooling Market)**：
   * 企業內部的運維人員可自行撰寫 MCP 伺服器。
   * LoomCowork 瞬間即可載入，變成大企業內部私有的 **「全功能 AI 運維工程師 (Local AI DevOps Agent)」**！
