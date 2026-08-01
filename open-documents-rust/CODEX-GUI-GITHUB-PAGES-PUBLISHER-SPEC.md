# 🌐 Codex GUI 功能規格書：靜態公開課程網站一鍵發布 (GitHub Pages)

本規格書定義並記錄了 Codex GUI 閉源商業版下，針對「GitHub Pages 一鍵發布靜態公開課程網站」功能的完整產品、前端編織 (Jamstack) 與後端 Rust 原生 API 傳輸設計。

本功能充分利用桌面端 App 擁有本機最高系統權限的優勢，不依賴使用者本機的 Git 安裝與 SSH 複雜設定，而是直接透過後端 Rust 內建的 GitHub API 連接器進行輕量化直連，實現「零雲端維運成本、高隱私防禦、極致傻瓜化發布」的商業級出版閉環體驗。

---

## 🧭 1. 核心互動場景 (User Experience Flow)

1. **資產確認**：使用者在右欄 Artifacts 畫布區 (Monaco Editor / Canvas) 完成由 AI 編織並經由人工微調的結構化教材（Markdown 格式）。
2. **觸發發布**：點擊右欄右上角常駐的 **【🚀 一鍵上線課程站】** 按鈕。
3. **動態進度回饋**：左欄交談區 (Chat Stream) 暫停常規對話，啟動極具極客感的發布狀態追蹤面板，以打字機與狀態勾選特效平滑呈現進度。
4. **光速上線**：發布完成後，介面噴出帶有超連結的成功卡片，提示使用者網頁已在全球 CDN 上線。

---

## 🏗️ 2. 前端靜態網站生成規格 (SSG Pipeline)

為了將維運與流量成本降至絕對的 **$0 元**，系統採用純前端靜態編織技術（Jamstack），將 Markdown 轉化為生產環境就緒的網頁包。

### 2.1 預製課程模板 (App-Embedded Themes)
Tauri 前端資產內建一套高度最佳化的響應式課程模板（基於 Tailwind CSS 生態，VitePress 風格），包含：
* **Responsive Sidebar**：自動根據 Markdown 的 `##`、`###` 標題層級，動態生成左側章節導覽與目錄樹。
* **Content Canvas**：支援深色/淺色模式切換、程式碼塊語法高亮、以及客製化資訊區塊（Alerts/Notes）。
* **Download Hook**：網頁右上角內建按鈕，允許終端上課學生下載原始的 `.md` 或由瀏覽器直接列印為 A4 PDF。

### 2.2 記憶體編織引擎 (In-Memory Compiler)
點擊發布時，前端 JavaScript 模組在記憶體中啟動：
1. 將 Monaco Editor 內最終的 Markdown 文字提取出來。
2. 對文字進行結構化解析，將章節資料與元數據 (Metadata) 注入預設的 HTML 模板字串中。
3. 在前端記憶體中直接將這些文字與靜態編譯好的 CSS/JS 檔案包，組裝成一個標準的靜態網頁檔案矩陣：
   ```plaintext
   ├── index.html
   ├── 404.html
   └── assets/
       ├── main.js
       └── main.css
   ```

---

## 🔌 3. 後端 Rust GitHub 連接器通訊規格 (API Transport)

本功能拒絕調用本機作業系統的 Git CLI 進程，避免因使用者電腦未安裝 Git、未設定 SSH Key 或 GPG 簽章而導致的環境崩潰。全面採用純 Rust 原生 HTTP 連接器與 GitHub REST API 進行強型別直連。

### 3.1 安全防禦與憑證儲存
* 使用者初次使用時，於設定面板輸入 **GitHub Personal Access Token (PAT)** 與目標儲存庫名稱 (Repository Name)。
* 憑證由 Tauri 後端接收，透過 Rust 的強型別加密邏輯，直接持久化在本機的 SQLite 數據庫中（權限 600），數據絕不上傳至任何第三方雲端伺服器。

### 3.2 檔案矩陣推播協議 (REST API Base64 Upload)
後端獨立的 `crates/opendoc-connector-github` 模組，透過 `reqwest` 或 `octocrab` 直連 GitHub API，呼叫流程如下：

```rust
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConnectorError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("API error: {0}")]
    Api(String),
}

/// 後端 Rust 核心 GitHub Content API 呼叫實體
pub async fn upload_course_file(
    token: &str,
    owner: &str,
    repo: &str,
    path: &str,
    content: &[u8],
    sha: Option<String>, // 用於更新現有檔案
) -> Result<(), ConnectorError> {
    let client = reqwest::Client::new();
    let url = format!("https://api.github.com/repos/{}/{}/contents/{}", owner, repo, path);
    
    // 使用 base64 進行內容編碼
    let base64_content = base64::encode(content);
    
    let mut payload = serde_json::json!({
        "message": "AI 編織教材自動更新 - 由 Codex GUI 發布",
        "content": base64_content,
    });
    
    // 若檔案已存在，必須帶上原始 SHA 進行版本控制覆寫
    if let Some(s) = sha {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("sha".to_string(), serde_json::Value::String(s));
        }
    }
    
    let res = client.put(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", "Codex-GUI-App")
        .header("Accept", "application/vnd.github.v3+json")
        .json(&payload)
        .send()
        .await?;
        
    if !res.status().is_success() {
        let err_body = res.text().await.unwrap_or_default();
        return Err(ConnectorError::Api(err_body));
    }
    
    Ok(())
}
```

* **漸進式寫入**：Rust 連接器會自動比對 Repo 內已存在檔案的 SHA，僅對有變動的 HTML/Markdown 進行 Base64 覆寫，確保上傳頻寬與速度得到最極致的優化。
* **自動觸發 CI/CD**：寫入完成後，GitHub 官方的 Pages Actions 會被自動觸發，由 GitHub 免費提供全球 CDN 託管與防 DDoS 服務，完全免除維運費用。

---

## 🔄 4. 前後端 IPC 資料流協議 (Tauri Event Protocol)

前後端通訊完全拋棄字串通靈，採用狀態明確的強型別事件流進行即時滾動回饋：

```typescript
type GitHubPublishEvent =
  | { type: "compile_start" }                         // 開始在記憶體生成靜態網頁
  | { type: "api_connect"; repo: string }             // 正在透過 Rust 直連 GitHub API
  | { type: "uploading_file"; filename: string }       // 正在上傳特定網頁檔案 (如 index.html)
  | { type: "github_pages_trigger" }                  // 檔案上傳成功，正在等待 GitHub Pages 建置
  | { type: "publish_success"; url: string }           // 發布成功，回傳最終公開網址
  | { type: "publish_error"; error_message: string }   // 發生錯誤 (如 Token 失效或網路中斷)
```

### 📺 前端 UI 狀態審查面板渲染 (UI Gatekeeper)
當事件流進入 `github_pages_trigger` 時，左欄對話框渲染如下結構：

```plaintext
+-------------------------------------------------------------+
| 🔄 正在發布您的公開課程站...                                  |
| ├─ [✓] 網頁靜態模板編織完成                                   |
| ├─ [✓] 透過 Rust 連接器直連 GitHub API                        |
| ├─ [✓] 檔案傳輸完成 (index.html, assets/main.css)           |
| └─ ⏳ 正在啟動 GitHub 全球 CDN 部署...                        |
|                                                             |
| 🎉 上線成功！                                                |
| 🔗 您的公開課程網站已在全球啟用：                              |
| https://your-username.github.io/my-ai-course               |
+-------------------------------------------------------------+
```

---

## 💎 5. 進階商業變現與產品護城河 (Future Extensions)

1. **零成本流量池**：
   由於託管在 GitHub Pages，開發者不需為使用者的網站支付任何頻寬、伺服器或空間費用，工具本身的毛利率為 **100%**。軟體可以作為高溢價的專業商業工具直接販售 (SaaS/License 模式)。
2. **網頁內建 AI 助教掛件 (Embedded Chat Widget)**：
   * **規格**：在前端編織 HTML 模板時，自動在網頁右下角注入一段輕量的 JavaScript 聊天掛件。
   * **機制**：來到該網站上課的學生，可以直接點開掛件對著這份教材進行 RAG 提問。
   * **商業路由**：軟體使用者（作者）可在發布時選擇是否將自己的 BYOK (自備 Key) 限制額度綁定進網頁，或者要求前來瀏覽的學生「自備 Key 登入」，完美將 Token 成本轉嫁，同時創造出驚人的產品高級感。
