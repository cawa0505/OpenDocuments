# 🕸️ OpenDocuments 知識圖譜檢索特化規格 (2026-08-02-knowledge-graph-retrieval.md)

本規格定義如何基於現有的 Rust RAG 核心，擴充「知識圖譜（Knowledge Graph）」檢索，作為向量檢索之外的第二檢索維度，用以達成教材重組、行政公文指標比對等關聯式檢索場景。

---

## 🧭 1. 核心設計原則

1. **非侵入式雙軌檢索 (Dual-Retrieval Pipeline)**:
   向量檢索負責「語意相似度」，圖譜檢索負責「實體/指標硬關聯」。兩者並行，最終透過 Reranker 進行混合評分融合。
2. **零外部依賴 (Zero Heavy Graph-DB)**:
   不引入額外的巨型圖資料庫（如 Neo4j），直接在 Rust 記憶體中維護輕量鄰接表（Adjacency List），並使用 SQLite 進行邊（Edges）與節點（Nodes）的持久化。
3. **Markdown 語意連結 (OKF 規範)**:
   完全遵循 Open Knowledge Format (OKF)，在文件解析時利用 Regex 自動提取 `[[doc-id]]` 與 YAML Front Matter 屬性建構邊界。

---

## 🛠️ 2. Rust 資料結構設計 (`crates/opendoc-rag/src/graph.rs`)

```rust
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocMetadata {
    pub id: String,
    pub r#type: String,      // 例如 "syllabus_indicator" (課綱指標), "lesson_plan" (教案), "document" (行政公文)
    pub title: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub metadata: DocMetadata,
    pub file_path: PathBuf,
    pub content: String,
    pub outbound_links: HashSet<String>, // 此節點指向的 target_id
}

/// 記憶體中輕量圖管理器
pub struct KnowledgeGraph {
    pub nodes: HashMap<String, GraphNode>,
    pub adjacency_list: HashMap<String, Vec<String>>, // 鄰接表，用於圖深度/廣度優先遍歷 (Traversal)
}
```

---

## 🔄 3. Markdown 連結解析器與圖譜吞吐 (`Graph Ingestion`)

在文件進行 RAG 分割（Chunking）的同時，多跑一軌圖譜吞吐函數：

```rust
use regex::Regex;
use std::fs;

impl KnowledgeGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            adjacency_list: HashMap::new(),
        }
    }

    /// 吞吐 Markdown 檔案，解析 [[target-id]] 建立實體關聯
    pub fn ingest_markdown_file(&mut self, path: PathBuf) -> Result<(), String> {
        let raw_content = fs::read_to_string(&path)
            .map_err(|e| format!("無法讀取檔案 {:?}: {}", path, e))?;
        
        // 1. 解析 Front Matter 與 Body
        let (metadata, markdown_body) = parse_front_matter(&raw_content)?;
        let doc_id = metadata.id.clone();

        // 2. 正則抓取 [[target-id]] 語法
        let re = Regex::new(r"\[\[([a-zA-Z0-9_-]+)\]\]")
            .map_err(|e| format!("正則表達式錯誤: {}", e))?;
        
        let mut outbound_links = HashSet::new();
        for cap in re.captures_iter(&markdown_body) {
            if let Some(target_id) = cap.get(1) {
                outbound_links.insert(target_id.as_str().to_string());
            }
        }

        let node = GraphNode {
            metadata: metadata.clone(),
            file_path: path,
            content: markdown_body,
            outbound_links: outbound_links.clone(),
        };

        // 3. 寫入記憶體與鄰接表
        self.nodes.insert(doc_id.clone(), node);
        self.adjacency_list.insert(doc_id, outbound_links.into_iter().collect());

        Ok(())
    }
}

/// 輔助函數：解析 Markdown Front Matter (--- 夾住的 YAML 結構)
fn parse_front_matter(content: &str) -> Result<(DocMetadata, String), String> {
    if !content.starts_with("---") {
        return Err("找不到 YAML Front Matter 開始標記".to_string());
    }

    let parts: Vec<&str> = content.splitn(3, "---").collect();
    if parts.len() < 3 {
        return Err("YAML Front Matter 格式不完整".to_string());
    }

    let yaml_str = parts[1];
    let body_str = parts[2].to_string();

    let metadata: DocMetadata = serde_yaml::from_str(yaml_str)
        .map_err(|e| format!("Front Matter 反序列化失敗: {}", e))?;

    Ok((metadata, body_str))
}
```

---

## 💾 4. SQLite 關係持久化 Schema

為了支援離線重啟，不流失圖關係，在 `init_db_pool` 的 DDL 中加入以下兩張輕量表：

```sql
-- 知識圖譜節點表
CREATE TABLE IF NOT EXISTS graph_nodes (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    node_type TEXT NOT NULL, -- syllabus_indicator, lesson_plan, etc.
    file_path TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- 知識圖譜關係邊表
CREATE TABLE IF NOT EXISTS graph_edges (
    source_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    edge_type TEXT DEFAULT 'links_to',
    PRIMARY KEY (source_id, target_id),
    FOREIGN KEY (source_id) REFERENCES graph_nodes(id) ON DELETE CASCADE
);
```

---

## 🏁 5. 第二維度檢索流程與混合評分融合 (Hybrid Retrieval Map)

當前端發起 `POST /chat/stream` 查詢時：

```plaintext
                    [ 🔍 User Query ]
                           │
             ┌─────────────┴─────────────┐
             ▼                           ▼
    [ 🧠 Vector Search ]        [ 🕸️ Graph Traversal ]
      基於語意相似度檢索           從關鍵實體節點出發
      (LanceDB Embedding)        深度/廣度盲戳鄰接關聯
             │                           │
             └─────────────┬─────────────┘
                           ▼
                  [ 🤝 Score Fusion ]
                    (Reciprocal Rank)
                           ▼
                 [ 🧬 Final Rerank ]
                 (Coherent Context)
```
