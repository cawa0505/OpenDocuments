# WebUI Markdown 渲染元件 — 任務切細文件

> 狀態：進行中
> 更新：2026-08-13
> 依 §3708（Markdown 渲染須支援增量串流更新，避免文字閃爍或破塊）與 AGENTS.md §8 執行

## 1. 現況診斷（為何一直無法正確實作）

| # | 問題 | 證據 |
|---|------|------|
| P1 | 主答案路徑**沒有使用 ReactMarkdown** | `apps/webui/src/components/chat/ChatMessage.tsx:119-127` 的 `renderContent()` 僅對 `message.content` 做 citation regex 切割，string 片段直接包 `<span>` 輸出。標題、列表、粗體、程式碼區塊全部以純文字顯示 |
| P2 | `prose` 樣式無效 | `apps/webui/tailwind.config.*` 的 `plugins: []`，未安裝/註冊 `@tailwindcss/typography`；`ChatMessage.tsx:141` 的 `prose prose-sm` 不產生任何樣式 |
| P3 | Citation 與 Markdown 衝突 | `processContent()`（ChatMessage.tsx:40-117）在 markdown 渲染前用 regex 把 `[n]` 從字串中切出，Markdown parser 無法對剩餘字串做結構解析；citation 互動標籤與 markdown 元素無法共存 |
| P4 | 無共用 Markdown 元件 | 唯一 ReactMarkdown 使用點在 source preview modal（ChatMessage.tsx:236），無自訂 components、無 code 高亮、無暗色模式適配 |
| P5 | Streaming 無增量機制 | `isStreaming` 時整塊內容重渲染（ChatMessage.tsx:243-245 僅加游標），長回答會閃爍/破塊 |

## 2. 任務切細（依序執行，每步獨立可驗證）

### Step 1 — Prose 樣式基礎
- [x] 安裝 `@tailwindcss/typography` 並在 tailwind.config 註冊 `typography` plugin
- [x] 驗證：`npm run typecheck` 通過；`prose` class 開始產生樣式

### Step 2 — 共用 Markdown 元件 `Markdown.tsx`
- [x] 新增 `apps/webui/src/components/ui/Markdown.tsx`：
  - `ReactMarkdown` + `rehype-highlight`（code 高亮，已依賴 `rehype-highlight@7`）
  - 自訂 components：`code`/`pre`（行內 vs 區塊）、`table`/`thead`/`th`/`td`、`ul`/`ol`、`blockquote`、`a`（新視窗開啟）、`hr`
  - 樣式與現有設計語言一致（slate/blue、text-[13-15px]、rounded-md 邊框）
  - 暗色模式：使用 `dark:` 變體而非依賴全局 `!important` 覆蓋
- [x] 驗證：typecheck 通過；元件可被 ChatMessage 引用

### Step 3 — Citation 與 Markdown 共存（P3 正解）
- [x] 保留 `processContent()` 的 citation 切割邏輯（valid/out-of-bounds/invalid 三態互動標籤）
- [x] 文字片段改由 `Markdown` 元件渲染（而非純 `<span>`），使 citation 與 markdown 結構共存
- [x] 驗證：typecheck；citation 在 markdown 結構內仍可點擊

### Step 4 — ChatMessage 主答案整合
- [x] `ChatMessage.tsx` 主答案改用 `Markdown` 元件取代 `renderContent()` 的純 `<span>` 輸出
- [x] Citation callback 接到現有 `setSelectedSource` 與 source preview modal（保留原三態互動）
- [x] Source preview modal 改用 `Markdown` 元件
- [x] 驗證：typecheck；瀏覽器端到端（見 §5）

### Step 5 — Streaming 增量渲染（§3708）
- [x] `Markdown` 元件以 `React.memo` 包裹，串流時僅在 content 實際變化才重解析，避免每 token 全量重掛載
- [x] 保留 streaming 游標；`done` 後最終完整渲染
- [x] 驗證：typecheck；長回答串流無閃爍、無破塊

### Step 6 — 全站一致性
- [x] Source preview modal（ChatMessage.tsx:236）改用 `Markdown` 元件
- [x] 檢查其他未格式化文字輸出點：Sidebar 的 content 為對話標題（非 markdown），DictionaryPage 無 content 輸出，均不需套用
- [x] 驗證：typecheck + make install + 端到端

## 3. 不做的範圍（YAGNI）
- 不引入新的 markdown 生態依賴（已夠：react-markdown 9 + rehype-highlight 7 + typography plugin）
- 不做 WYSIWYG 編輯器、不做 LaTeX/數學公式、不做 mermaid 圖表
- 不處理 XSS 風險面以外的 HTML 白名單客製（react-markdown 預設不渲染 raw HTML，符合安全預設）

## 4. 驗證契約
1. `cd apps/webui && npm run typecheck` — 零錯誤零警告
2. `make install` — web + engine + binary 同輪建置
3. 瀏覽器 `http://localhost:3006`：聊天送出含 markdown（標題/列表/粗體/程式碼區塊/citation）的回答，驗證渲染與 citation 互動
4. Streaming：長回答滾動顯示無閃爍
