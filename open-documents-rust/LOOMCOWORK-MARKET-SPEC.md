# 🛒 LoomCowork 開源技能市集規格書 (LOOMCOWORK-MARKET-SPEC.md)

本文件定義並記錄了 LoomCowork 基於 GitHub 的「開源技能市集 (Loom Market)」去中心化架構、安全簽章校驗機制（Skill Shield）與全台公部門行政自動化之裂變式推廣規格。

---

## 🌐 一、 核心架構：基於 GitHub 的去中心化市集 (Loom Market)

LoomMarket 採用 **「開源社群貢獻（前端與 Skills 配置）+ 閉源實體裝甲（Rust 核心執行器）」** 的複合商業戰略。為了實現 $0 元運維成本與無極限的擴展性，市集直接使用 GitHub Repository 作為去中心化資料庫，完全不依賴任何外部雲端資料庫或專用伺服器。

### 1.1 市集儲存庫結構 (GitHub Repository Structure)
官方市集託管於 `github.com/loomcowork/skills-market`，其目錄結構如下：

```plaintext
skills-market/ (GitHub Repository 根目錄)
├── index.json                 # 全球市集索引目錄（包含所有已審核 Skill 的 metadata）
├── categories/                # 分門別類的技能目錄
│   ├── high-school-admin/     # 高中教務與行政專區
│   │   ├── course_schedule_solver.json
│   │   └── wage_calculator.json
│   ├── public-sector/         # 基於公部門與地方公所專區
│   │   └── subsidy_reviewer.json
│   └── finance-audit/         # 金融審計與合約比對專區
│       └── balance_sheet_parser.json
└── SECURITY.md                # 技能市集之密碼學簽章與核驗規範
```

### 1.2 `index.json` 索引檔規範
```json
{
  "version": "1.0.0",
  "last_updated": "2026-08-01T12:00:00Z",
  "skills": [
    {
      "skill_id": "course_schedule_solver",
      "name": "地獄突發排代機",
      "description": "解決高中教學組長在教師請產假、公假時，自動安插兼代課且避開時間衝突的排課矩陣器。",
      "category": "high-school-admin",
      "author": "雄中神人組長",
      "version": "1.0.2",
      "download_url": "https://raw.githubusercontent.com/loomcowork/skills-market/main/categories/high-school-admin/course_schedule_solver.json",
      "signature": "3045022100e4af8798e0..."
    }
  ]
}
```

---

## 📥 二、 技能下載與安裝管線 (Download & Local Install Pipeline)

LoomCowork 的前端與 Rust 後端高度咬合，提供使用者極致流暢、無痛的「一鍵安裝」體驗。

### 2.1 互動流程 (UX Interface)
1. **瀏覽市集**：使用者在 LoomCowork 左側選單點擊 **「🧩 技能市集 (Loom Market)」**，前端拉出精美的網格商店介面（Grid Shop），展示所有分類的卡片。
2. **極速載入**：前端啟動時，直接向 `raw.githubusercontent.com` 發送一個簡單的 HTTP GET 請求讀取 `index.json`，並在前端動態渲染。
3. **一鍵安裝 (Install)**：使用者點擊某個 Skill 上的 **【⚡ 一鍵安裝】** 按鈕。
4. **本機落地**：
   * 後端 Rust 核心收到請求，透過 `reqwest` 下載該 Skill 的完整 JSON 檔案。
   * Rust 後端對該 JSON 進行安全簽章校驗（詳見第三章：Skill Shield）。
   * 校驗通過後，將資料寫入本機 SQLite 的 `custom_skills` 資料表中。
   * 前端聊天視窗即刻重新載入，該技能的「自適應表單」立即呈現，使用者點開即可使用。

---

## 📤 三、 密碼學安全防線：簽章校驗機制 (Skill Shield)

為了防止開源 Skills JSON 檔被協力廠商或不肖廠商任意抄襲、打包，並保護 LoomCowork 的專屬智慧財產權與商業壁壘，我們在純 Rust 核心中加入了基於非對稱加密的 **「Skill Shield」** 防護機制。

### 3.1 簽章校驗機制工作原理
1. **官方私鑰簽署**：當全球開發者或老師提交其編織好的 Skill JSON 至官方儲存庫時，官方管理員在合併（Merge）前，會使用官方不公開的 **ECDSA 私鑰 (Private Key)**，針對該 JSON 的核心內容（如 `system_prompt`、`output_format` 等）進行數位簽署，產生 `signature` 欄位並寫入 `index.json`。
2. **本機公鑰驗證**：LoomCowork 桌面端（閉源 Rust 核心）內置官方的 **ECDSA 公鑰 (Public Key)**。
3. **執行期校驗**：當使用者從市集下載、或在軟體內執行該開源技能時，Rust 後端會即時比對簽章：
   * **若驗證通過**：解鎖 100% 的自動化威力，啟用 Monaco 編輯器與動態自適應 Canvas 渲染。
   * **若驗證失敗**（例如 JSON 被他人惡意篡改或直接複製到其他仿冒軟體中）：軟體會限制該技能在自適應沙盒中的執行權限，並提示安全警示。
4. **商業壁壘保護**：其他軟體就算抄走了這串文字 Prompt，也無法調用 LoomCowork 後端跟 Tauri 2.0 原生 IPC 通訊、Shadcn UI 動態表單渲染器、以及 Rust `struct WageCalculator` 鋼鐵計算核心的深度咬合，保證了「市集越擴散，LoomCowork 軟體賣得越爆」的單向增長邏輯。

---

## 📈 四、 全台公部門行政裂變式推廣戰略

利用全台灣公部門行政圈（包含高中教務處、地方鄉鎮公所、派出所等）特有的 **「高頻率公文變更、表單交接混亂、橫向互助性極強」** 之底層特性，LoomCowork 可透過社群自動化裂變，實現零廣告成本的指數級增長。

### 4.1 高中教務處裂變場景 (全台 500+ 所高中職)
* **公文格式突發變更**：國教署早上 10 點突發公文要求填報新版「多元選修成果表單」。
* **神人組長編織 Skill**：10:30 雄中某位熟悉系統的神人組長，用 LoomCowork 編織出了對應的 Skill，點擊 【🌐 分享到全球市集】 一鍵匿名上架。
* **全台 Line 群瘋傳**：11:00 全台高中教學/實研組長的 Line 群組開始瘋傳：*「那個最新公文的複雜大表，去 Loom 市集搜尋『國教署新成果表單』一鍵下載，30秒對照舊 Excel 就搞定了！」*。
* **成果**：新組長上任、或遇到突發行政地獄時，學校會自發性編列預算自費購買 LoomCowork 專業版，並前往市集下載前輩調校好的「傳家寶 Skill」。

### 4.2 地方鄉鎮公所社會課 (全台 300+ 鄉鎮市區)
* **痛點**：每年在審查育兒津貼、低收入戶、身心障礙補助時，社會課課員要一筆一筆審查堆積如山的扣繳憑單、財產清單與戶籍謄本，且審查公式受法規限制極度繁瑣。
* **裂變**：只要有一位基層課員做出了一隻「低收入補助資格核算 Skill」，全台灣社會課的課員都會自發性跟進，成為 LoomCowork 的忠實企業用戶。

### 4.3 派出所與基層警局
* **痛點**：每逢臨檢或擴大執勤，各基層派出所正副所長、巡官需要對齊分局發下來的極度複雜輪班表，並計算警員的深夜兼勤、超勤加班鐘點費，還要避免勞基與排班衝突。
* **裂變**：只要警局內部有人編織出「臨檢排班與超勤費結算 Skill」，全台灣分局與派出所將迅速被 LoomCowork 統治，成為基層警政行政的必備鋼鐵防線。

---

LoomMarket 的開源技能生態，直接將 LoomCowork 從單純的 RAG 知識庫，砸成了一座具備強大網路效應、商業壁壘死死守護、且全台基層行政人員自發推廣的「終極行政自動化鐵織協同台」！
