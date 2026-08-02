use std::sync::Arc;
use arrow_schema::{DataType, Field, Schema};
use lancedb::index::Index;
use lancedb::index::scalar::{FtsIndexBuilder, FullTextSearchQuery};
use lancedb::query::{ExecutableQuery, QueryBase};
// removed unused Value import

/// 獲取與先前 Node.js (Apache Arrow 綁定) 100% 鋼鐵對齊的相容 Schema 
///
/// 關鍵優化防線：
/// 1. 向量欄位使用 FixedSizeList 宣告，向量項為 Float32。
/// 2. 除了主鍵與向量，非核心欄位（如 metadata_json, collection_id）全面設為 true (Nullable)，
///    防止 Node.js 因寫入 undefined/null 在 Rust 讀取時拋出 Schema 可空性 mismatch Panic！
pub fn get_compat_schema(vector_dim: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        // 1. 識別碼
        Field::new("document_id", DataType::Utf8, false),
        Field::new("chunk_type", DataType::Utf8, false),
        // 2. 核心文本與向量 (必須精準對齊舊版維度，例如 Ollama 的 bge-m3 預設是 1024 維，Qwen 等是 1536/384)
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            ),
            false,
        ),
        Field::new("content", DataType::Utf8, false),
        // 3. 隔離標籤
        Field::new("workspace_id", DataType::Utf8, false),
        Field::new("collection_id", DataType::Utf8, true), // 舊版若有留空，需設為 true
        // 4. 動態元數據：如果舊版是存成字串化的 JSON，這裡就用 Utf8 (Nullable)
        Field::new("metadata_json", DataType::Utf8, true),
    ]))
}

/// 嘗試對 Table 執行 FTS 全文檢索，若 Tantivy 底層版本不一致或索引毀損，自動觸發「自癒重建」機制
/// 
/// 💡 亮點：這不會破壞表中的任何既有向量與文檔資料，僅重新整理索引檔案！
pub async fn verify_and_self_heal_fts(
    table: &lancedb::Table,
    index_columns: &[&str],
) -> Result<(), lancedb::Error> {
    // 試著進行一次微型的 test 文字搜尋
    let test_query = table
        .query()
        .full_text_search(FullTextSearchQuery::new("test".to_owned()));
    
    if test_query.execute().await.is_err() {
        eprintln!("⚠️ 檢測到舊 Table FTS 索引與當前 Rust SDK 版本衝突，正在自動重建全文檢索索引...");
        
        // 呼叫 create_index 自動重建，覆蓋毀損索引
        table
            .create_index(index_columns, Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await?;
        println!("🏆 全文檢索 FTS 索引自癒重建完畢 ！！！");
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_comp_fields() {
        let schema = get_compat_schema(1024);
        // 7 欄位：document_id, chunk_type, vector, content, workspace_id, collection_id, metadata_json
        assert_eq!(schema.fields().len(), 7);
        
        let vec_field = schema.field_with_name("vector").unwrap();
        match vec_field.data_type() {
            DataType::FixedSizeList(_, dim) => {
                assert_eq!(*dim, 1024);
            }
            _ => panic!("向量欄位型別不匹配！"),
        }
        
        // 確保動態欄位（metadata_json）的可空性
        let metadata_field = schema.field_with_name("metadata_json").unwrap();
        assert!(metadata_field.is_nullable());
    }
}
