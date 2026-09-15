//! Core-owned SQLite FTS5 稀疏文字檢索（hybrid-rag-retrieval spec §2.2）。
//!
//! 單一來源：FTS5 的 DDL／寫入／查詢／刪除全部集中在此模組，僅由
//! `SidecarRetriever` 呼叫；Axum handler 與 LanceDB engine 邊界皆不觸及。
//! DDL 在 `SidecarRetriever::connect` 執行（production 與 CLI 共用路徑）；
//! 測試狀態不建 SidecarRetriever，故無需鏡像 test DDL。
//! 無匹配／查詢過短回空 `Vec`（零 mock， zero-mock guarantee §2.3）。

use opendoc_types::DocumentChunk;
use sqlx::{Row, SqlitePool};

/// FTS5 虛擬表 DDL。`trigram` tokenizer 支援 CJK 子字串比對
/// （sqlx bundled SQLite ≥ 3.34；FTS5 由 libsqlite3-sys `-DSQLITE_ENABLE_FTS5` 編入）。
/// ponytail: trigram 至少需 3 字元，<3 字元查詢直接回空；
/// 短詞查詢成為痛點時的升級路徑：額外 unicode61 欄位或自訂 tokenizer。
pub async fn init(pool: &SqlitePool) -> Result<(), String> {
    // Legacy migration：v1.0.0 GA 前被移除的舊 FTS5 實作（unicode61 + chunk_id 欄位）
    // 可能在既有資料庫殘留同名 `chunks_fts` 表，與新 schema 撞名。偵測到舊 schema
    // 時 rename 為 `chunks_fts_legacy` 保留資料（非破壞），讓下方 IF NOT EXISTS 建新表。
    let legacy: Option<i64> = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('chunks_fts') WHERE name = 'chunk_id'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    if legacy == Some(1) {
        sqlx::query("ALTER TABLE chunks_fts RENAME TO chunks_fts_legacy")
            .execute(pool)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        eprintln!("🗂️  migration: 遺留 chunks_fts（舊 FTS5 實作）已 rename 為 chunks_fts_legacy");
    }
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
            content,
            workspace_id UNINDEXED,
            document_id UNINDEXED,
            chunk_idx UNINDEXED,
            doc_path UNINDEXED,
            headers_json UNINDEXED,
            tokenize = 'trigram'
        )",
    )
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// 一筆 FTS5 檢索列（core RRF 融合所需的最小欄位集）。
pub struct FtsRow {
    pub document_id: String,
    pub content: String,
    pub doc_path: String,
    pub headers_json: String,
    pub chunk_idx: usize,
}

/// 全文索引一份文件的所有 chunks。先刪後寫，與 engine `index_chunks` 同樣冪等。
/// `chunk_idx` 以傳入順序列舉，與 engine 端 metadata 的 chunk_idx 對齊。
pub async fn index_document(
    pool: &SqlitePool,
    workspace_id: &str,
    document_id: &str,
    doc_path: &str,
    chunks: &[DocumentChunk],
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM chunks_fts WHERE document_id = ?")
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    for (i, c) in chunks.iter().enumerate() {
        let headers_json = c
            .metadata
            .get("headers")
            .map(|h| h.to_string())
            .unwrap_or_else(|| "[]".to_string());
        sqlx::query(
            "INSERT INTO chunks_fts (content, workspace_id, document_id, chunk_idx, doc_path, headers_json) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&c.content)
        .bind(workspace_id)
        .bind(document_id)
        .bind(i as i64)
        .bind(doc_path)
        .bind(&headers_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

/// 文件刪除／重索引前移除其 FTS5 列。`document_id` 為 documents 表 PK，跨工作區唯一。
pub async fn delete_document(pool: &SqlitePool, document_id: &str) -> Result<(), String> {
    sqlx::query("DELETE FROM chunks_fts WHERE document_id = ?")
        .bind(document_id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// MATCH 查詢（workspace 隔離）。整句視為單一 phrase（精確子字串比對）；
/// 雙引號跳脫避免使用者輸入被解析成 FTS5 運算子。
/// ponytail: phrase 語意偏精確；召回不足時再放寬為 token OR。
pub async fn search(
    pool: &SqlitePool,
    workspace_id: &str,
    query: &str,
    top_k: usize,
) -> Result<Vec<FtsRow>, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    // trigram tokenizer 至少需要 3 字元，過短直接回空。
    if trimmed.chars().count() < 3 {
        return Ok(Vec::new());
    }
    let match_expr = format!("\"{}\"", trimmed.replace('"', "\"\""));
    let rows = sqlx::query(
        "SELECT document_id, content, chunk_idx, doc_path, headers_json FROM chunks_fts \
         WHERE chunks_fts MATCH ? AND workspace_id = ? ORDER BY rank LIMIT ?",
    )
    .bind(&match_expr)
    .bind(workspace_id)
    .bind((top_k * 3) as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| FtsRow {
            document_id: Row::get::<String, _>(&r, "document_id"),
            content: Row::get::<String, _>(&r, "content"),
            doc_path: Row::get::<String, _>(&r, "doc_path"),
            headers_json: Row::get::<String, _>(&r, "headers_json"),
            chunk_idx: Row::get::<i64, _>(&r, "chunk_idx") as usize,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendoc_types::ChunkType;

    fn chunk(content: &str) -> DocumentChunk {
        DocumentChunk {
            chunk_type: ChunkType::Semantic,
            content: content.to_string(),
            workspace_id: String::new(),
            collection_id: String::new(),
            file_path: String::new(),
            relevance_score: None,
            metadata: serde_json::json!({ "headers": ["Section A"] }),
        }
    }

    #[tokio::test]
    async fn fts5_init_renames_legacy_table() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        // 模擬遺留表：同名的舊 schema（unicode61 + chunk_id）
        sqlx::query(
            "CREATE VIRTUAL TABLE chunks_fts USING fts5(chunk_id, content, tokenize='unicode61')",
        )
        .execute(&pool)
        .await
        .unwrap();
        init(&pool).await.unwrap();

        // 遺留表已改名保留，新表以新 schema 建立
        let legacy_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM chunks_fts_legacy").fetch_one(&pool).await.unwrap();
        assert_eq!(legacy_count, 0);
        index_document(&pool, "ws1", "doc1", "/tmp/a.md", &[chunk("hello world marker")])
            .await
            .unwrap();
        assert_eq!(
            search(&pool, "ws1", "hello world marker", 10).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn fts5_index_search_delete_roundtrip() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        init(&pool).await.unwrap();

        let chunks = vec![
            chunk("The deployment guide covers the release workflow"),
            chunk("reaaaa unrelated body text"),
        ];
        index_document(&pool, "ws1", "doc1", "/tmp/a.md", &chunks)
            .await
            .unwrap();

        // 命中
        let hits = search(&pool, "ws1", "deployment guide", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].document_id, "doc1");
        assert_eq!(hits[0].chunk_idx, 0);
        assert_eq!(hits[0].doc_path, "/tmp/a.md");

        // workspace 隔離
        assert!(search(&pool, "ws2", "deployment guide", 10)
            .await
            .unwrap()
            .is_empty());

        // 過短查詢（<3 字元）回空
        assert!(search(&pool, "ws1", "ab", 10).await.unwrap().is_empty());

        // 無匹配回空（zero-mock）
        assert!(search(&pool, "ws1", "quantum entanglement flux", 10)
            .await
            .unwrap()
            .is_empty());

        // 重索引冪等：同一文件重新寫入不會產生重複列
        index_document(&pool, "ws1", "doc1", "/tmp/a.md", &chunks)
            .await
            .unwrap();
        assert_eq!(
            search(&pool, "ws1", "deployment guide", 10)
                .await
                .unwrap()
                .len(),
            1
        );

        // 刪除後不再命中
        delete_document(&pool, "doc1").await.unwrap();
        assert!(search(&pool, "ws1", "deployment guide", 10)
            .await
            .unwrap()
            .is_empty());
    }
}
