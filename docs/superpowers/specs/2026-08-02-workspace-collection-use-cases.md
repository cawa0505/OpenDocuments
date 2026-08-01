# 🏫 教師兼行政的 Workspace ✕ Collection 實務落地場景與技術契合規格

本文針對台灣校園與公部門中「兼任行政職教師」的實務痛點，定義桌面客戶端與 OpenDocuments RAG 引擎在處理「工作區（Workspace）」與「文集（Collection）」時的物理隔離、邏輯對齊及跨域同步的系統實作機制。

---

## 🧭 一、 雙重身份的定義與物理隔離 (The Core Concept)

一個兼任教學組長或註冊組長的老師，在系統中擁有完全不同的兩個思維與資料視角。

### 1. Workspace (工作區)：代表老師的「身份/專案範疇」
工作區是實體資料夾與底層 RAG 獨立索引（SQLite 關係庫 + 向量庫）的硬性隔離邊界。

*   **「教務處-教學組」工作區**：
    *   **實體路徑**：`~/Documents/Workspaces/Academic_Affairs/`
    *   **內容屬性**：教育部 108 課綱官方文件、全校各科教學計畫書、自主學習審查規章、排代課備忘錄。
    *   **RAG 隔離**：使用專屬的 `academic_affairs.db`，向量資料庫指標與行政法規高度綁定。
*   **「個人教學-高一國文」工作區**：
    *   **實體路徑**：`~/Documents/Workspaces/Teacher_Chinese/`
    *   **內容屬性**：歷年授課 PPT、學習單 Word 檔、課外補充文章、學生平時成績 CSV。
    *   **RAG 隔離**：使用專屬的 `teacher_chinese.db`，其向量檢索空間不與行政法規交叉污染。

> 💡 **Rust 底層優勢**：當老師在桌面客戶端左側導覽列切換 Workspace 時，Rust 後端只需在毫秒內切換讀取不同的本地 SQLite 連線與路徑指標，記憶體佔用極低、切換完全無縫、零延遲。

---

## 🎨 二、 Collection (文集/集合)：代表老師的「任務/思維模組」

Collection 是在同一個工作區之內，透過向量索引與 Markdown 知識圖譜（Graph）所拉出來的「邏輯主題分類空間」。

### 1. 情境 A：行政審查任務 (在行政工作區中)
*   **Collection 名稱**：`2026_第一學期_國文科教學計畫書審查`
*   **檔案內容**：全校國文老師交過來的 15 份 Markdown / ODS / DOCX 計畫書。
*   **AI 影子代理**：本地 Rust RAG 讀取此文集，將其比對該行政工作區內建的「教育部課綱核心指標」，並在右欄 Monaco 畫布生成一鍵「合規性稽核大表（漏失指標預警）」。

### 2. 情境 B：教材解構與重組 (在個人教學工作區中)
*   **Collection 名稱**：`跨領域探究_地理與歷史的交會`
*   **檔案內容**：自己的大航海時代臺灣講義、歷史科與地理科老師分享的補充教材。
*   **知識圖譜編織**：利用雙括號 `[[knowledge-node]]` 語法，系統自動在 Collection 內部解析出知識點與前置學習節點的邊，繪製成網狀拓撲（Graph），輔助老師一鍵重組產出新的「跨領域教案大綱」。

---

## 🔄 三、 殺手級跨專案功能：教師兼行政的「雙向知識對齊」

由於桌面客戶端將 OpenDocuments 核心直接內建在本地，且兩者透過標準 MCP / API 緊密咬合，這使得系統能實作市面上所有雲端 RAG 工具（如 Dify、Coze）或單一聊天介面都無法實現的**跨工作區關聯感知（Cross-Workspace Relation Alignment）**。

### 🔗 雙向對齊運作管線 (Implementation Pipeline)

```plaintext
【教務處行政工作區】                                   【個人教學工作區】
 審查 Collection: 國文科計畫書                         教材文集: 高一國文講義
  └─ AI 發現：陳老師的計畫書                            └─ 講義: 臺灣開發史.md (UUID: doc_999)
     第三單元未對齊 A2 素養指標                            ▲
                                                           │ (100% 本地 SQLite 外鍵連結)
  [點擊: 同步至我的教學工作區] ────────────────────────────┘
  (系統在 sqlite 自動建立 cross_workspace_relation 紀錄)
```

1.  **行政端發現缺陷**：
    老師以「教學組長」身份在行政工作區中審查自己的國文計畫書，AI 代理提示：「此計畫書與教育部公布的素養指標 A2 缺乏對齊。」
2.  **一鍵跨區標記（Approve / Sync）**：
    老師無須複製貼上或切換視窗，直接在桌面客戶端的右欄 Gatekeeper 安全卡片點擊 **「同步至我的教學工作區」**。
3.  **底層關係織入（DB Relationship Link）**：
    Rust 後端的 SQLite 會在全局關聯表 `cross_workspace_relations` 中寫入一筆非對稱關聯：
    ```sql
    INSERT INTO cross_workspace_relations (
        from_workspace_id, from_doc_id, 
        to_workspace_id, to_doc_id, 
        relation_type, metadata
    ) VALUES (
        'academic-affairs-uuid', 'review-doc-123', 
        'teacher-chinese-uuid', 'lecture-doc-999', 
        'requires_alignment_fix', '{"message": "教學組審查：第三單元與 A2 素養指標未對齊"}'
    );
    ```
4.  **教學端主動喚醒**：
    當老師下課回到家，切換到「個人教學-高一國文」工作區，點開 `臺灣開發史.md` 準備備課時，桌面客戶端的 Canvas 畫布右側會亮起一盞溫暖的黃色警告燈，提示：
    > ⚠️ **行政審查回饋**：您在「教學組長」工作區對此檔案標註了 `requires_alignment_fix`。第三單元需要補強素養指標 A2 的教案描述。

這項功能徹底解決了教師兼行政人員在雙重工作切換時的「記憶斷層」，讓本地 RAG 從一個冷冰冰的檢索器，晉升為最體貼的 **「行政與教學合一控制台」**。
