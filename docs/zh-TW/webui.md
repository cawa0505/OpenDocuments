# 🖥️ OpenDocuments WebUI — 建置、測試與介面規範

[English](../en/webui.md) | **繁體中文**

本文件為內嵌 React WebUI 如何建置、測試、綁定至單一二進位發布版本的權威依據。任何 AI 助手要改動 `apps/webui/` 前，必須先讀本文件，再讀 `AGENTS.md`，再讀以下引用的 OpenSpec 規格。

---

## 1. WebUI 在哪

| 項目 | 路徑 |
| --- | --- |
| 源碼根目錄 | `apps/webui/src/` |
| 打包輸出（Rust 消費） | `apps/webui/dist/` |
| npm 套件清單 | `apps/webui/package.json` |
| TS 組態 | `apps/webui/tsconfig.json` |
| Vite 組態 | `apps/webui/vite.config.ts` |
| Rust 內嵌點 | `crates/opendoc-mcp/src/lib.rs`（`RustEmbed`，`#[folder = "../../apps/webui/dist/"]`） |
| 規格書 | [`openspec/specs/webui-architecture/spec.md`](../../openspec/specs/webui-architecture/spec.md) |

WebUI **不是**獨立部署。它被編譯成靜態資源，透過 `rust-embed` 內嵌進 `opendoc` 二進位。Axum 伺服器直接從記憶體 serve。線上環境沒有獨立網頁伺服器。

---

## 2. 技術棧與依賴

- React 19 + TypeScript 5.5、Vite 6、Tailwind CSS 3.4、PostCSS、autoprefixer。
- 狀態：`zustand` 5。Markdown：`react-markdown` 9 + `rehype-highlight` 7。Icons：`lucide-react`。
- 伺服時零 Node.js 執行期。Node.js **僅為打包工具鏈**，用來產生 `dist/`。

依賴列於 `apps/webui/package.json`。沒有架構理由不要新增頂層依賴；優先用 stdlib 與已安裝套件（見全域 ponytail 階梯）。

---

## 3. 建置

### 3.1 前置條件
- Node.js 20+ 與 npm（僅打包階段）。
- 可運作的 Rust toolchain（內嵌階段）。

### 3.2 唯一合法建置路徑：`make`

**禁止直接跑 `npm run build` 或 `npm install`**。這樣會讓 `apps/webui/dist/` 比已安裝的 binary 新，下一次不安裝就 `cargo install` 會把舊的（往往是 stub）`dist/` 折進 binary。唯一正確路徑是走 `Makefile`，它把 `build-web` 排在 `install-cli` 之前:

```bash
# 由 repo 根目錄 — 唯一合法路徑:
make install              # build-web → 安裝 engine + 主二進位至 ~/.cargo/bin
make build                # build-web + cargo build --release（不安裝）
```

`make install` / `make build` 不可繞過。它們內部呼叫 `npm install` 與 `npm run build`，然後在同一輪立刻重 build 並安裝 Rust 二進位，讓 `rust-embed` 在同一次動作裡把新產生的 `dist/` 折進 binary。把兩半拆開手動跑，就是把 404 stub 送上 production 的方式。

若 `apps/webui/dist/` 在 `cargo install` 時不存在或過舊，`crates/opendoc-mcp/build.rs` 會靜默寫一份 "WebUI Assets Not Compiled" stub `index.html`，而 `rust-embed` 會把這份 stub 折進 binary。結果是頁面 200 卻空白。`Makefile` 的鏈結防止這件事；手動 npm + cargo 不防止。

### 3.3 Typecheck（關卡，可單獨跑）

```bash
cd apps/webui && npm run typecheck
```

這是唯一可以手動呼叫的 npm 指令。不碰 `dist/`。任何 `make` 跑之前必須乾淨。

### 3.4 Dev server（Vite HMR，不產 dist）

```bash
cd apps/webui && npm run dev
```

Vite 監聽 `http://localhost:5173`，`/api` proxy 到 `http://localhost:3006`（見 `vite.config.ts`）。另外在本機 `:3006` 跑 Rust server 才能完整 end-to-end 開發。此路徑不產生也不消費 `dist/`——純 dev HMR 對著跑中的 binary。

### 3.5 手動 cargo install（進階，不鼓勵）

如果你確實理解順序，`make install` 的手動等價是:

```bash
cd apps/webui && npm install && npm run build   # 產生 dist/
cd /mnt/data/btrfs-ssd/Projects/Jimmy/homelab-integration/repos/OpenDocuments
cargo install --path crates/opendoc-cli --force
cargo install --path crates/opendoc-engine-lancedb --force
```

但若 `dist/` 在 `cargo install` 時不存在，`build.rs` 會寫 stub，你就把 404 WebUI 送上線。除非有特定理由，用 `make install`。事後必須驗證服務的頁面是真的 SPA（`curl http://localhost:3006/` 必須回含 `/assets/index-*.js` 的 HTML）。

---

## 4. 驗證與測試

### 4.1 Typecheck（強制關卡）

```bash
cd apps/webui && npm run typecheck
```

必須 exit 0。任何型別錯誤擋 build 與 merge。改動任何 `.ts/.tsx` 後第一件事就是跑這個。

### 4.2 確認服務的 WebUI 是真的（發布關卡）

`make install` 並重啟伺服器後:

```bash
curl -s http://localhost:3006/ | grep -q "assets/index-" && echo OK || echo STUB
```

若印 `STUB`，binary 內嵌的是 `build.rs` 的 fallback `index.html`，不是真的 SPA。重 build（`make install`）、重啟伺服器、再查。不要帶著 stub 继续。

### 4.3 End-to-end WebUI 驗證（發布關卡）

光編譯過不夠。WebUI 發布關卡是實際瀏覽器 round-trip。最少可驗證路徑：

```bash
# 1. 重 build WebUI + 重裝二進位
make install

# 2. 確認安裝後二進位的 mtime 新於源碼編輯時間
ls -l ~/.cargo/bin/opendoc

# 3. 啟動大一統伺服器。兩種等價方式：
#    a) 前景（互動除錯用）：
opendoc start --port 3006
#    b) systemd user 服務（本機 canonical）：
systemctl --user restart opendoc-server.service
systemctl --user status  opendoc-server.service

# 4. 瀏覽器開 http://localhost:3006
#    - 登入頁或聊天頁要能 render，不是空白或 404
#    - 開 DevTools → Network → 確認 /api/v1/health 回 200
#    - 送一條聊天查詢 → SSE stream 必須產生 chunks + sources
#    - 從文件頁上傳一份文件 → /api/v1/documents/upload 回 200
```

頁面空白或 assets 404，最可能原因是 `dist/` 沒重 build 就 `cargo install`。重 build → 重 install → 再查 mtime。

### 4.4 HTTP 介面 smoke check

WebUI 依賴路徑的最少 curl：

```bash
curl -s http://localhost:3006/api/v1/health
curl -s http://localhost:3006/api/v1/workbench
curl -s -X POST http://localhost:3006/api/v1/chat \
  -H 'Content-Type: application/json' \
  -d '{"query":"hello"}'
```

全部必須 2xx + JSON body。500 / 422 是後端或介面錯，不是 WebUI 錯——先到 `crates/opendoc-mcp/src/handlers/` 追路由，再動前端。

---

## 5. 架構契約（不可破）

以下規則由專案規格與 `AGENTS.md` 強制。違反任一條是 blocker，不是風格問題。

1. **單一二進位。** WebUI 透過 `rust-embed` 內嵌於 `opendoc`。無獨立靜態伺服器、伺服時無 Node.js runtime、禁 Docker（`#2039`）。
2. **內嵌路徑是 `apps/webui/dist/`。** `crates/opendoc-mcp/src/lib.rs` 的 `#[folder]` 指向這裡。若打包輸出搬家，`vite.config.ts` 的 `outDir` 與 `#[folder]` 必須同步更新。
3. **SPA fallback。** 非 `/api` 的未知路由從記憶體回 `index.html` 以支援 client-side routing。不要移除 `static_handler` 的 fallback。
4. **API base 為 `/api/v1`。** 所有 WebUI 呼叫走 `src/lib/api.ts` 與 `src/lib/sse.ts`。base 路徑在該檔 hard-code；dev proxy 只在 dev 把 `/api` 指到 `:3000`。
5. **工作區解析。** WebUI 從 `localStorage['active-workspace']` 送 `X-Workspace`。空 header 由伺服器端 `resolve_workspace_id` 回退到設定的預設工作區（`#3294`）。不要在前端發明 fallback。
6. **BYOK / API 金鑰絕不外流。** WebUI 只在 `localStorage` 持自身後端 auth 的 API key；LLM 業者金鑰存 SQLite，永遠不回前端（`#3318`）。
7. **production 路徑零 mock。** 全專案禁止 stub response、寫死 chunk、假 hit（`#3317`、AGENTS.md §6/§7）。WebUI 必須忠實呈現後端真實回傳，包含空結果。
8. **i18n。** 所有使用者可見字串走 `src/lib/i18n.ts`（`translate`）。不要 inline raw 文案。header `<select>` 切 `locale` 並存 `localStorage['opendocuments-locale']`；值以 `X-Locale` 與 `Accept-Language` 送出。
9. **預設 Light Mode。** 預設主題為 Light；暗色模式經 header toggle 開啟並存在 app store。未經同意不要改預設（`fix(webui): lock default theme to clean light mode`）。
10. **零警告編譯** 同樣適用 TS build。`npm run typecheck` 在任何 `make` 之前必須乾淨。`make` 鏈結本身會跑 `npm run build`；絕不要手動呼叫。

---

## 6. 源碼地圖

```
apps/webui/src/
├── App.tsx                  # Auth gate → Layout → page router
├── main.tsx                 # Vite entry
├── lib/
│   ├── api.ts               # 所有 REST 呼叫（base /api/v1）
│   ├── sse.ts               # 聊天串流（POST /api/v1/chat/stream）
│   ├── auth.ts              # 後端 auth 的 stored API key
│   ├── i18n.ts              # translate(locale, key, values?)
│   └── types.ts             # 共享 DTO（必須與後端完全對齊）
├── stores/
│   ├── appStore.ts          # currentPage, theme, locale, profile
│   └── chatStore.ts         # messages, streaming state, conversations
└── components/
    ├── auth/                # LoginPage
    ├── layout/              # Layout, Sidebar, CommandPalette
    ├── dashboard/           # UnifiedDashboard
    ├── chat/                # ChatPage, ChatInput, ChatMessage, SourceCard
    ├── documents/           # DocumentsPage
    ├── collections/         # CollectionsPage
    ├── dictionary/          # DictionaryPage
    ├── settings/            # SettingsPage (BYOK providers)
    ├── health/              # HealthPage
    ├── connectors/          # ConnectorsPage
    ├── workspaces/          # WorkspacesPage
    ├── plugins/             # PluginsPage
    └── admin/               # Admin dashboards
```

`App.tsx` 的 `PAGES` 是可路由頁面的清單。新增頁面就註冊於此並在 sidebar 加入口。

---

## 7. 反模式（過去踩過）

- 把 `ChatPage.tsx` 砍到只剩 import、沒有 `export function ChatPage` — bundle 直接壞。重構時務必保留元件 body。
- Zustand store 的 interface（`ChatState`）加了欄位，但 `create(...)` 初始物件沒加 — TS 直接擋 build。interface 與初始狀態必須同步。
- 引入 SSE 協定實際未 emit 的 `onStatus` / `phase` 欄位 — 死分支。`src/lib/sse.ts` 的 event type 必須字對字對齊後端的 `event:` 行（`chunk`、`sources`、`confidence`、`done`、`error`）。
- 在前端發明工作區 fallback（例如 `X-Workspace` 空時前端自己挑一個）。永遠讓 header 空、由 `resolve_workspace_id` 決定。
- 寫死私有主機名、IP、測試拓撲。只能用 `127.0.0.1`、`localhost` 或 RFC 5737 位址（AGENTS.md §6）。
- 改後端路由介面卻沒同步更新 `src/lib/api.ts` 與 `src/lib/types.ts`。`types.ts` 的 DTO 必須鏡像 Rust response struct。

---

## 8. 壞掉時怎麼查

1. `cd apps/webui && npm run typecheck` — 看第一個錯。
2. `curl -s http://localhost:3006/ | grep -q "assets/index-"` — 若 grep 沒中，binary 內嵌的是 stub。重 build（`make install`）、重啟伺服器、再查。
3. typecheck 與 build 都乾淨但瀏覽器空白 → 安裝的二進位是舊的。重 build `dist/` → `make install` → 查 `~/.cargo/bin/opendoc` mtime。
4. 頁面載入正常但 API 404/500 → 到 `crates/opendoc-mcp/src/lib.rs` 與 `crates/opendoc-mcp/src/handlers/` 追路由。前端通常是對的，後端是嫌疑。
5. SSE 串流壞，比對後端送出的 `event:` 名稱與 `src/lib/sse.ts` 的 `switch (eventType)`。必須字對字相同。

---

## 9. 發布 checklist（WebUI 切片）

- [ ] `apps/webui/src/` 改動通過 `npm run typecheck`，零錯誤。
- [ ] `make install` 成功；`~/.cargo/bin/opendoc` mtime 新於源碼編輯時間。
- [ ] `curl -s http://localhost:3006/ | grep -q "assets/index-"` 回 OK — 非 stub。
- [ ] `opendoc start --port 3006` 無錯啟動（或 `systemctl --user restart opendoc-server.service` 後 unit 為 active）。
- [ ] 瀏覽器 `http://localhost:3006` render 聊天 UI 且 `/api/v1/health` 回 200。
- [ ] 至少一次聊天 round-trip 與一次文件上傳 end-to-end 成功。
- [ ] 沒有新增頂層 npm 依賴，除非有架構決策記錄。
- [ ] `dist/` **沒有**進 git（它是 gitignored 的打包輸出）。

---

*最後更新：2026-08-11。Owner：OpenDocuments core。適用規格：`single-binary-architecture`、`webui-architecture`（OpenSpec）。*