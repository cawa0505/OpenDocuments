# 📝 OpenDocuments 二進位解析器（Parser）技術演進與優化任務藍圖 (2026-08-02)

本文件定義了針對 Word/DOCX、PDF 與 PPTX 等二進位行政備課核心資產的 Parser 技術升級路徑，作為 RAG 核心第二階段（Phase 2）的實體研發檢核清單與功能規格書。

---

## 🟥 任務一：Word/DOCX 檔案的「實體表格（Table）解析器」擴充 (完備度 70% → 95%)

### 1. 現存技術缺口
現行的 `opendoc-parser-docx` 僅遍歷並抽取了 `ParagraphChild::Run -> RunChild::Text`，會直接略過所有內嵌於講義與鐘點費表格中的 Word Tables（`DocumentChild::Table`），導致結構化報表在 RAG 中完全遺失。

### 2. 優化實作技術路徑 (Rust)
我們需要在 `opendoc-parser-docx/src/lib.rs` 遍歷 `document.children` 時，加上對 `DocumentChild::Table` 的匹配分支：

```rust
// 偽代碼：在遍歷 document.children 時，加入 Table 的強型別抽取
match child {
    DocumentChild::Paragraph(p) => { /* 現行段落邏輯 */ }
    DocumentChild::Table(table) => {
        let mut table_markdown = String::new();
        for row in table.rows {
            let mut row_cells = Vec::new();
            for cell in row.cells {
                let mut cell_text = String::new();
                for p in cell.paragraphs {
                    // 遞迴抽取單格內的所有 Run 文字
                    for child in p.children {
                        if let ParagraphChild::Run(run) = child {
                            for r_child in &run.children {
                                if let RunChild::Text(t) = r_child {
                                    cell_text.push_str(&t.text);
                                }
                            }
                        }
                    }
                }
                row_cells.push(cell_text.trim().to_string());
            }
            // 轉換為 Markdown Pipe 格式：Column A | Column B | Column C
            table_markdown.push_str(&row_cells.join(" | "));
            table_markdown.push('\n');
        }
        
        // 壓入 Table 專屬 Chunk 區塊
        chunks.push(DocumentChunk {
            chunk_type: ChunkType::Table,
            content: table_markdown,
            workspace_id: workspace_id.to_string(),
            collection_id: collection_id.to_string(),
            file_path: path_str.clone(),
            relevance_score: None,
            metadata: serde_json::json!({
                "table_structure": "markdown_pipe"
            }),
        });
    }
}
```

### 3. 驗收基準
- [ ] 能解析包含 5 欄 10 列以上的 Word 鐘點費對帳表。
- [ ] 輸出為標準 `ChunkType::Table` Markdown Pipe 格式，並進入 LanceDB/SQLite 關係綁定。

---

## 🟨 任務二：PDF 檔案的「自適應多欄排版與 OCR 整合」 (完備度 50% → 85%)

### 1. 現存技術缺口
- **無 OCR 能力**：若遇到家長同意書或公文掃描檔（純圖片 PDF），`lopdf` 提取出空字串，RAG 檢索徹底失靈。
- **雙欄排版錯亂**：雙欄或多欄行政手冊文字流會被橫向切碎讀取，導致段落與語義混亂。

### 2. 優化實作技術路徑 (Rust)
我們將為 `opendoc-parser-pdf` 引入雙軌自適應抽取防線：

```plaintext
PDF 檔案上傳
    │
    ├──► 軌道 A：使用 lopdf 提取電子文字 ──► 成功提取且非空？ ──► [是] ──► 進行「幾何 Y 座標物理分行重組」
    │                                                                   │
    └──► 軌道 B：[否] ──► 啟動「Tesseract / OCR-engine 離線本地管道」 ──┴──► 降級產出 UTF-8 文字
```

#### A. 雙欄幾何排版重組（Y-Coordinate Sorting）
利用 `lopdf` 的 `get_dict` 與位置矩陣（TM/Td），在 Rust 中依據 X 與 Y 的幾何座標（Bounding Box）進行物理排序，先將左欄排完再排右欄，而非橫向直接一行讀到底。

#### B. 本地離線輕量 OCR
在 `opendoc-parser-pdf` 中可選整合 `tesseract-rs` 或調用系統本機的原生 PDF-Kit OCR（macOS 用 Vision.framework，Windows 10+ 用 Windows.Media.Ocr），實現公文/教材掃描檔的完全零信任本機識別，絕不將圖片外流。

### 3. 驗收基準
- [ ] 掃描件 PDF（純圖片）上傳後，能成功識別出 90% 以上的行政文字。
- [ ] 雙欄學術/課綱 PDF 文件在切分 Chunk 後，其內容上下文維持原本段落的語義連貫，不被左右夾雜切碎。

---

## 🟩 任務三：PowerPoint 簡報「備忘稿（Speaker Notes）與局部視覺」 (完備度 45% → 85%)

### 1. 現存技術缺口
教師的簡報大綱（Slide）字數極少，真正蘊含教學細節的「備忘稿」資訊全部遺失在 `ppt/notesSlides/notesSlide{}.xml` 中。

### 2. 優化實作技術路徑 (Rust)
在 `opendoc-parser-pptx` 中，除了遍歷 `ppt/slides/slide{}.xml` 外，同時在背景關聯讀取對應的 `ppt/notesSlides/notesSlide{}.xml`，將備忘稿與 Slide 本體的文字融合成同一個 Semantic Chunk：

```rust
// 偽代碼：聯調 notesSlide 內容
let notes_file_path = format!("ppt/notesSlides/notesSlide{}.xml", slide_index);
if let Ok(mut notes_file) = archive.by_name(&notes_file_path) {
    let mut notes_xml = String::new();
    notes_file.read_to_string(&mut notes_xml).unwrap_or(0);
    let extracted_notes = extract_xml_text(&notes_xml); // 提取備忘文字
    
    // 合流
    slide_text.push_str("\n[備忘提示/備課筆記]:\n");
    slide_text.push_str(&extracted_notes);
}
```

### 3. 驗收基準
- [ ] 簡報解析 Chunk 內同時包含投影畫面與下方「備忘稿」內文，RAG 對話時可針對簡報備忘錄進行精準回答。
