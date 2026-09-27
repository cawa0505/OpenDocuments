# Proposal

## Why

2026-09-27 以真實瀏覽器 dogfood `http://localhost:3006` 全站走查（Chat、文件管理、工作空間、活動日誌、詞彙對齊、系統設定），發現 10 項以上損害信任與操作正確性的缺陷：會話恢復卡死無出口、i18n 缺漏直出 raw 英文、清單外洩主機絕對路徑、同名文件難以辨識易誤刪、檢索偏好兩處控制不同步、空狀態文案錯置等。多數屬 AGENTS.md §0.1「單一來源」與 §0.2「簡單能用」反例的變形。依 §8，UI 行為修正須有正式需求記錄，故立此 change。

## What Changes

- 即時答詢會話恢復不得永久卡在 disabled「思考中」：等待有界化，逾時轉為明確錯誤態並提供取消/重試。
- 清查並補齊 zh-TW i18n：文件狀態列舉值（`indexed`）、詞彙對齊空狀態標題（`Empty Terminology`）、側欄 `Workspace:` 字首等不得直出 raw 英文。
- zh-TW 用語修正：運維→營運、數據→資料、大盤→總覽，並掃描同族 PRC 用語。
- 文件清單：同名文件提供顯著辨識提示，刪除確認須呈現足以分辨的資訊；清單不顯示主機絕對路徑，完整路徑經明確操作才揭露。
- 檢索偏好（速度/平衡/精準）收斂為單一狀態來源，首頁選擇器與設定頁雙向同步。
- 活動日誌各面板空狀態文案與自身資料來源正確映射。
- 詞彙對齊頁：去除重複說明與行銷腔（「100% 精準對齊！」），改中性事實文案。
- 工作空間清單：機器識別碼（UUID）與 CLI 提示預設收合；disabled 主操作維持最低對比；破壞性按鈕與相鄰元素保有緩衝；遵循 §9 換行契約。

## Capabilities

### New Capabilities

- `webui-ux-hardening`: WebUI 狀態恢復、i18n 完整性、隱私顯示、同名文件辨識與偏好狀態單一來源的行為契約（dogfood 修復集）。

### Modified Capabilities

（無 — 既有 capability 的需求行為不變）

## Impact

- `apps/webui/src/`：Chat/SSE 會話狀態、Documents 清單與確認對話框、Workspaces 清單、活動日誌空狀態、Settings，以及 i18n 詞庫與顯示格式化。
- 不變更 REST/SSE API 契約與 DTO（`webui-architecture` 規格保護範圍）；若實作確需擴充，另開 change。
- 依 AGENTS §3.0，於 `docs/zh-TW/tasks.md` 登錄。
