# WebUI Design System — 參考規範

> 目的：收錄 WebUI 反覆出錯的視覺/互動模式與既有設計語言，作為修改前的查閱基準。
> 適用範圍：`apps/webui/src/` 所有元件。
> 最後更新：2026-08-13（modal 定位、markdown 渲染、flexbox 要點）

---

## 1. 設計 token（既有語言，改動前先查這裡）

| Token | 值 | 說明 |
|-------|-----|------|
| 主色（action） | `blue-600` hover `blue-700` | 主按鈕、連結、active 狀態 |
| 強調色 | `blue-50` / `blue-100` | 淺藍背景、border 強調 |
| 危險色 | `red-600` hover `red-700` | 刪除、破壞性動作 |
| 中性色 | `slate-50/100/200/600/900` | 背景、border、次要文字 |
| 頁面背景 | `bg-slate-50` | 內容頁背景 |
| 卡片背景 | `bg-white` + `border-slate-200` + `shadow-sm` | 內容卡 |
| 文字級距 | `text-[13px]`（次要/UI）/ `text-[15px]`（正文） | 全站統一，不用 `text-sm` 混用 |
| 圓角 | `rounded-md`（按鈕、卡片）/ `rounded-lg`（大卡） | |
| 暗色模式 | `dark:` 前綴成對出現 | 每個元件必須同時有 light + dark |

---

## 2. Modal / Overlay 定位（反覆踩雷，務必照做）

**陷阱**：flex 容器（`flex`）預設 `align-items: stretch`，child 會被拉到容器全高。
若容器是 `fixed inset-0`（全視口），modal 會被撐到整頁高度 — 這是「modal 超級高」的元兇。

**正確做法 — block + 固定頂距**（現役 ConfirmDialog 模式）：

```tsx
<div className="fixed inset-0 z-50 pt-24" onClick={onCancel}>   {/* ① 不是 flex */}
  <div className="absolute inset-0 bg-black/50" />               {/* ② 遮罩 */}
  <div
    role="dialog"
    aria-modal="true"
    className="relative w-full max-w-sm mx-auto bg-white dark:bg-gray-900 rounded-xl shadow-2xl p-5"
    onClick={e => e.stopPropagation()}
  >
    {/* 內容：高度由內容決定，絕不被外部撐開 */}
  </div>
</div>
```

要點：
1. **外層不用 `flex`**。水平置中用 `mx-auto`（配 `w-full max-w-sm`），頂部距離用固定 `pt-24`（96px）。不要用 `pt-[15vh]` — 垂直螢幕上 modal 會跑到視口外。
2. 若一定要用 flex 對齊，child 必須有固定高度或 `items-start`/`self-start` 避免 stretch。
3. 遮罩是 `absolute inset-0`，modal 是 `relative`（疊在遮罩上）。
4. 遮罩點擊 = cancel（`onClick={onCancel}`），modal 本身 `stopPropagation`。
5. 固定頂距 vs 垂直置中：使用者明確偏好**固定頂距**（垂直螢幕不會掉到太下方）。需要改距離只改 `pt-*` 一處。

**互動契約**：
- `Escape` 關閉（`useEffect` 掛 `window keydown`）
- 開啟時 focus 到確認按鈕（`useRef` + `.focus()`）
- `aria-modal="true"` + `role="dialog"` + `aria-label`
- busy 期間兩個按鈕都 `disabled`，確認鈕顯示 busyLabel
- 危險動作：確認鈕 `bg-red-600`；一般動作：`bg-blue-600`

---

## 3. Markdown 渲染（2026-08-13 建立）

**共用元件**：`src/components/ui/Markdown.tsx`
- react-markdown 9 + rehype-highlight（程式碼高亮，樣式 `highlight.js/styles/github-dark.css` 已在 `main.tsx` 引入）
- `memo` 包裹：串流時僅 content 變化才重解析（防每 token 全量重掛載）
- 自訂 components：code/pre/table/th/td/blockquote/a/hr，統一樣式與設計 token 一致
- 預設不渲染 raw HTML（react-markdown 安全預設）

**使用規則**：
- 任何 AI 回答 / 文件內容的輸出都走 `<Markdown content={...} />`，禁止用 `<span>{content}</span>` 或裸字串輸出（否則 markdown 語法以純文字顯示）。
- **絕不可把 markdown 字串用 regex 切碎後分段渲染**（citation 除外，見下）。語法標記（`**`、`` ` ``、`###`）被切到不同片段會無法解析。

**Citation 與 markdown 共存**（ChatMessage 模式）：
- `processContent` 用 `/[(\d+)]/g` 找出 citation 標記，把內容切成「文字片段 + citation 按鈕」。
- 文字片段**各自獨立**進 `<Markdown>`，citation 按鈕保留互動（點擊開 source preview）。
- 限制：citation 出現在行內語法中間（如 `**粗體[1]**`）會被切壞 — 已知邊界，AI 回答的 citation 通常出現在句尾，可接受。

**Streaming 注意**：`currentStreamText` 每 chunk 更新，重建 message content；Markdown 的 memo 只會在 content 真正改變時重解析，避免閃爍。

---

## 4. 元件命名與結構

- 共用 UI 元件放 `src/components/ui/`（如 `ConfirmDialog.tsx`、`Markdown.tsx`）。
- 頁面元件放 `src/components/<domain>/`（documents / chat / collections / workspaces / plugins / dictionary / layout）。
- 使用者可見字串一律走 `src/lib/i18n.ts` 三語（en/ko/zhTW），禁止 inline raw 文案。
- i18n 鍵值三語數量與順序必須一致；缺鍵會 fallback 顯示原始 key（如 `documents.loading`），這是 bug 不是 feature。

---

## 5. 反模式速查（過去踩過的雷）

| 雷 | 正確做法 |
|----|---------|
| modal 用 `flex` + `items-center` 或 `pt-[15vh]` | block + `pt-24` + `mx-auto`（§2） |
| 主答案用 `<span>` 輸出 AI 內容 | `<Markdown content={...} />`（§3） |
| tailwind `prose` 沒效果 | 需 `@tailwindcss/typography` plugin（tailwind.config `plugins: []` 是空的） |
| 用 regex 切碎 markdown 再分段渲染 | 整段進 Markdown；只切 citation（§3） |
| 缺 i18n 鍵還上線 | 三語同步補鍵；fallback 顯示 key 本身就是壞的 |
| 只寫 light 樣式不寫 `dark:` | 每個元件成對補 `dark:` |
