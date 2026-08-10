//! LanceDB engine: owns the connection, compat schema, index writes, vector/FTS
//! search, and deletion. Core owns embedding + RRF fusion.
//!
//! CLI args: `--uri <lancedb-uri> --table <table-name>`. Handshake carries the
//! vector dimension from core so the table can be created with the right schema.

use std::sync::Arc;

use arrow_array::{
    builder::{FixedSizeListBuilder, Float32Builder},
    Array, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::table::Table;
use opendoc_types::protocol::{
    EngineRequest, EngineResponse, HandshakeResult, HealthResult, RawSearchRow, SearchResult,
};
use opendoc_types::DocumentChunk;
use serde_json::json;

/// Escape a value for use inside a single-quoted SQL-like predicate literal.
/// LanceDB `only_if` predicates accept SQL filter syntax; doubling `'` is the
/// standard escape (a lone quote cannot terminate the literal).
fn sql_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

use lancedb::index::Index;
use lancedb::index::scalar::FtsIndexBuilder;

const PROTOCOL_VERSION: &str = "1";
const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const SCHEMA_VERSION: &str = "1";

/// Compat schema aligned with the legacy Node.js (Apache Arrow) layout.
fn compat_schema(vector_dim: i32) -> Arc<arrow_schema::Schema> {
    use arrow_schema::{DataType, Field};
    Arc::new(arrow_schema::Schema::new(vec![
        Field::new("document_id", DataType::Utf8, false),
        Field::new("chunk_type", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            ),
            false,
        ),
        Field::new("content", DataType::Utf8, false),
        Field::new("workspace_id", DataType::Utf8, false),
        Field::new("collection_id", DataType::Utf8, true),
        Field::new("metadata_json", DataType::Utf8, true),
    ]))
}

pub struct Engine {
    conn: lancedb::Connection,
    table_name: String,
    dim: usize,
}

impl Engine {
    /// Connect to LanceDB (async — call from a tokio context).
    pub async fn connect(uri: &str, table: &str) -> Self {
        let conn = lancedb::connect(uri)
            .execute()
            .await
            .unwrap_or_else(|e| panic!("lancedb connect {uri}: {e}"));
        Self { conn, table_name: table.to_string(), dim: 0 }
    }

    pub async fn handle(&mut self, req: EngineRequest) -> EngineResponse {
        let id = match &req {
            EngineRequest::Handshake { .. } => "handshake",
            EngineRequest::Health => "health",
            EngineRequest::IndexChunks { .. } => "index",
            EngineRequest::Search { .. } => "search",
            EngineRequest::DeleteDocument { .. } => "delete",
            EngineRequest::Optimize => "optimize",
            EngineRequest::Shutdown => "shutdown",
        }
        .to_string();
        match req {
            EngineRequest::Handshake { protocol_version, vector_dim } => {
                if protocol_version != PROTOCOL_VERSION {
                    return EngineResponse::err(
                        id,
                        format!("protocol mismatch: engine={PROTOCOL_VERSION}, core={protocol_version}"),
                    );
                }
                self.dim = vector_dim;
                let result = HandshakeResult {
                    protocol_version: PROTOCOL_VERSION.into(),
                    engine_version: ENGINE_VERSION.into(),
                    schema_version: SCHEMA_VERSION.into(),
                    capabilities: vec!["vector".into(), "fts".into()],
                    vector_dimension: self.dim,
                };
                EngineResponse::ok(id, serde_json::to_value(result).unwrap_or_default())
            }
            EngineRequest::Health => {
                let result = HealthResult { status: "ok".into() };
                EngineResponse::ok(id, serde_json::to_value(result).unwrap_or_default())
            }
            EngineRequest::IndexChunks {
                workspace_id,
                document_id,
                operation_id,
                collection_id,
                source_path,
                chunks,
                vectors,
            } => match self
                .index_chunks(
                    &document_id,
                    &workspace_id,
                    collection_id.as_deref(),
                    &source_path,
                    &chunks,
                    &vectors,
                )
                .await
            {
                Ok(_) => EngineResponse::ok(
                    id,
                    json!({ "operation_id": operation_id, "indexed": chunks.len() }),
                ),
                Err(e) => EngineResponse::err(id, e),
            },
            EngineRequest::Search {
                workspace_id,
                query_vector,
                query_text,
                top_k,
            } => match self.search(&workspace_id, &query_vector, &query_text, top_k).await {
                Ok(result) => EngineResponse::ok(id, serde_json::to_value(result).unwrap_or_default()),
                Err(e) => EngineResponse::err(id, e),
            },
            EngineRequest::DeleteDocument {
                workspace_id,
                document_id,
                operation_id,
            } => match self.delete_document(&workspace_id, &document_id).await {
                Ok(_) => EngineResponse::ok(id, json!({ "operation_id": operation_id })),
                Err(e) => EngineResponse::err(id, e),
            },
            EngineRequest::Optimize => match self.optimize().await {
                Ok(_) => EngineResponse::ok(id, json!({ "optimized": true })),
                Err(e) => EngineResponse::err(id, e),
            },
            EngineRequest::Shutdown => EngineResponse::ok(id, json!({ "shutdown": true })),
        }
    }

    async fn open_or_create_table(&self) -> Result<Table, String> {
        let names = self
            .conn
            .table_names()
            .execute()
            .await
            .map_err(|e| e.to_string())?;
        if names.iter().any(|n| n == &self.table_name) {
            self.conn
                .open_table(&self.table_name)
                .execute()
                .await
                .map_err(|e| e.to_string())
        } else {
            let schema = compat_schema(self.dim as i32);
            self.conn
                .create_empty_table(&self.table_name, schema)
                .execute()
                .await
                .map_err(|e| e.to_string())
        }
    }

    /// Reindex: delete old chunks for the document, then insert new ones.
    /// Vectors are pre-computed by core (BYOK / local model live in core).
    pub async fn index_chunks(
        &self,
        document_id: &str,
        workspace_id: &str,
        collection_id: Option<&str>,
        source_path: &str,
        chunks: &[DocumentChunk],
        vectors: &[Vec<f32>],
    ) -> Result<(), String> {
        if chunks.is_empty() {
            return Ok(());
        }
        if vectors.len() != chunks.len() {
            return Err(format!("vectors {} != chunks {}", vectors.len(), chunks.len()));
        }
        if self.dim == 0 {
            return Err("vector_dimension unset: handshake must precede index".into());
        }
        for v in vectors {
            if v.len() != self.dim {
                return Err(format!("vector dim {} != table dim {}", v.len(), self.dim));
            }
        }

        let table = self.open_or_create_table().await?;

        // Reindex idempotent: drop old chunks for this document.
        let predicate = format!("document_id = {}", sql_literal(document_id));
        let _ = table.delete(&predicate).await;

        let n = chunks.len();
        let document_id_col = StringArray::from(vec![document_id; n]);
        let chunk_types: Vec<String> = chunks
            .iter()
            .map(|c| format!("{:?}", c.chunk_type))
            .collect();
        let chunk_type_col = StringArray::from(
            chunk_types.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );
        let content_col = StringArray::from(
            chunks.iter().map(|c| c.content.as_str()).collect::<Vec<_>>(),
        );
        let workspace_id_col = StringArray::from(vec![workspace_id; n]);
        let collection_id_col = StringArray::from(
            chunks
                .iter()
                .map(|c| {
                    let cid = c.collection_id.as_str();
                    if cid.is_empty() {
                        collection_id.unwrap_or("")
                    } else {
                        cid
                    }
                })
                .collect::<Vec<_>>(),
        );
        let metadatas: Vec<String> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let headers = c
                    .metadata
                    .get("headers")
                    .and_then(|h| h.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                serde_json::to_string(&json!({
                    "doc_path": source_path,
                    "headers": headers,
                    "chunk_idx": i,
                }))
                .unwrap_or_default()
            })
            .collect();
        let metadata_json_col = StringArray::from(
            metadatas.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        );

        // vector column: FixedSizeList<f32, dim>
        let mut fsl = FixedSizeListBuilder::new(
            Float32Builder::with_capacity(self.dim * n),
            self.dim as i32,
        );
        for v in vectors {
            fsl.values().append_slice(v);
            fsl.append(true);
        }
        let vector_array = fsl.finish();

        let schema = compat_schema(self.dim as i32);
        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(document_id_col),
                Arc::new(chunk_type_col),
                Arc::new(vector_array),
                Arc::new(content_col),
                Arc::new(workspace_id_col),
                Arc::new(collection_id_col),
                Arc::new(metadata_json_col),
            ],
        )
        .map_err(|e| e.to_string())?;

        let reader = RecordBatchIterator::new(
            vec![Ok(batch)].into_iter(),
            compat_schema(self.dim as i32),
        );
        table.add(reader).execute().await.map_err(|e| e.to_string())?;

        // best-effort FTS index; failure degrades to vector-only search
        if let Err(e) = table
            .create_index(&["content"], Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await
        {
            eprintln!("[opendoc-engine] fts index build failed: {e}");
        }

        Ok(())
    }

    /// Search: vector + FTS candidates returned raw for core RRF fusion.
    pub async fn search(
        &self,
        workspace_id: &str,
        query_vector: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> Result<SearchResult, String> {
        if self.dim == 0 {
            return Err("vector_dimension unset: handshake must precede search".into());
        }
        if query_vector.len() != self.dim {
            return Err(format!("query vector dim {} != {}", query_vector.len(), self.dim));
        }
        let table = self.open_or_create_table().await?;
        let vec_rows = self.vector_search(&table, query_vector, workspace_id, top_k).await?;
        let fts_rows = self.fts_search(&table, query_text, workspace_id, top_k).await;
        Ok(SearchResult { vector_rows: vec_rows, fts_rows })
    }

    async fn vector_search(
        &self,
        table: &Table,
        qvec: &[f32],
        workspace_id: &str,
        top_k: usize,
    ) -> Result<Vec<RawSearchRow>, String> {
        let mut q = table
            .query()
            .only_if(format!("workspace_id = {}", sql_literal(workspace_id)))
            .select(Select::columns(&[
                "document_id",
                "content",
                "metadata_json",
                "_distance",
            ]))
            .nearest_to(qvec.to_vec())
            .map_err(|e| e.to_string())?;
        q = q.limit(top_k * 3);
        let mut stream = q.execute().await.map_err(|e| e.to_string())?;
        let mut rows = Vec::new();
        let mut rank = 0usize;
        while let Some(batch_res) = stream.next().await {
            let batch = batch_res.map_err(|e| e.to_string())?;
            let distances = batch
                .column_by_name("_distance")
                .ok_or("missing _distance")?;
            let distances = distances
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or("_distance not f32")?;
            for i in 0..batch.num_rows() {
                let dist = distances.value(i);
                rows.push(row_from_batch(&batch, i, dist, rank)?);
                rank += 1;
            }
        }
        Ok(rows)
    }

    /// Best-effort FTS; no index or no results → empty (degrades to vector-only).
    async fn fts_search(
        &self,
        table: &Table,
        query: &str,
        workspace_id: &str,
        top_k: usize,
    ) -> Vec<RawSearchRow> {
        use lancedb::index::scalar::FullTextSearchQuery;
        if query.trim().is_empty() {
            return Vec::new();
        }
        let q = table
            .query()
            .only_if(format!("workspace_id = {}", sql_literal(workspace_id)))
            .select(Select::columns(&[
                "document_id",
                "content",
                "metadata_json",
                "_score",
            ]))
            .full_text_search(FullTextSearchQuery::new(query.to_owned()))
            .limit(top_k * 3);
        let mut stream = match q.execute().await {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut rows = Vec::new();
        let mut rank = 0usize;
        while let Some(batch_res) = stream.next().await {
            let batch = match batch_res {
                Ok(b) => b,
                Err(_) => continue,
            };
            // _score may be f32 or f64; best-effort f32, else skip batch.
            if !batch
                .column_by_name("_score")
                .map(|s| s.as_any().is::<Float32Array>())
                .unwrap_or(false)
            {
                continue;
            }
            for i in 0..batch.num_rows() {
                if let Ok(row) = row_from_batch(&batch, i, 0.0, rank) {
                    rows.push(row);
                    rank += 1;
                }
            }
        }
        rows
    }

    pub async fn delete_document(&self, workspace_id: &str, document_id: &str) -> Result<(), String> {
        let table = self.open_or_create_table().await?;
        let predicate = format!(
            "workspace_id = '{}' AND document_id = '{}'",
            workspace_id.replace('\'', ""),
            document_id.replace('\'', "")
        );
        table
            .delete(&predicate)
            .await
            .map(|_| ())
            .map_err(|e| format!("lancedb delete 失敗: {e}"))
    }

    pub async fn optimize(&self) -> Result<(), String> {
        // ponytail: no-op. LanceDB compaction via optimize() is optional for
        // correctness. Trigger: run when a workspace's LanceDB directory grows
        // past ~1 GiB or search latency regresses; add `table.optimize()` then.
        Ok(())
    }
}

/// Parse one row out of a result batch. `cosine_distance` is 0.0 for FTS rows.
fn row_from_batch(
    batch: &RecordBatch,
    i: usize,
    cosine_distance: f32,
    rank: usize,
) -> Result<RawSearchRow, String> {
    let get_str = |name: &str| -> Result<String, String> {
        let arr = batch
            .column_by_name(name)
            .ok_or_else(|| format!("missing column {}", name))?;
        let arr = arr
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| format!("column {} not utf8", name))?;
        Ok(arr.value(i).to_string())
    };
    Ok(RawSearchRow {
        document_id: get_str("document_id")?,
        content: get_str("content")?,
        metadata_json: get_str("metadata_json").unwrap_or_default(),
        cosine_distance,
        rank,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opendoc_types::{ChunkType, DocumentChunk};
    use serde_json::Value;

    fn chunk(text: &str, header: &str, i: usize) -> DocumentChunk {
        DocumentChunk {
            chunk_type: ChunkType::Semantic,
            content: text.to_string(),
            workspace_id: "ws1".to_string(),
            collection_id: String::new(),
            file_path: "docs/spec.md".to_string(),
            relevance_score: None,
            metadata: serde_json::json!({
                "headers": [header],
                "chunk_idx": i,
            }),
        }
    }

    #[tokio::test]
    async fn round_trip_index_search_delete() {
        let dir = std::env::temp_dir().join(format!("opendoc-engine-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut e = Engine::connect(dir.to_str().unwrap(), "documents").await;

        // handshake: dim 4
        let resp = e
            .handle(EngineRequest::Handshake {
                protocol_version: "1".to_string(),
                vector_dim: 4,
            })
            .await;
        assert!(resp.ok, "handshake failed: {:?}", resp.error);
        let hs: HandshakeResult =
            serde_json::from_value(resp.result.unwrap()).expect("handshake result");
        assert_eq!(hs.vector_dimension, 4);

        // index two chunks with orthogonal vectors
        let chunks = vec![
            chunk("markdown chunking strategy", "分塊策略", 0),
            chunk("workspace isolation", "工作空間隔離", 1),
        ];
        let vectors: Vec<Vec<f32>> = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
        ];
        let resp = e
            .handle(EngineRequest::IndexChunks {
                workspace_id: "ws1".to_string(),
                document_id: "doc-1".to_string(),
                operation_id: "op-1".to_string(),
                collection_id: None,
                source_path: "docs/spec.md".to_string(),
                chunks,
                vectors,
            })
            .await;
        assert!(resp.ok, "index failed: {:?}", resp.error);

        // search near first vector → doc-1 found, rank 0
        let resp = e
            .handle(EngineRequest::Search {
                workspace_id: "ws1".to_string(),
                query_vector: vec![1.0, 0.0, 0.0, 0.0],
                query_text: "markdown chunking".to_string(),
                top_k: 5,
            })
            .await;
        assert!(resp.ok, "search failed: {:?}", resp.error);
        let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
        assert_eq!(sr.vector_rows.len(), 2, "vector search should find both rows");
        assert_eq!(sr.vector_rows[0].document_id, "doc-1");
        assert_eq!(sr.vector_rows[0].rank, 0);
        let meta: Value =
            serde_json::from_str(&sr.vector_rows[0].metadata_json).expect("metadata json");
        assert_eq!(meta["doc_path"], "docs/spec.md");

        // workspace isolation: other workspace → no rows
        let resp = e
            .handle(EngineRequest::Search {
                workspace_id: "ws-other".to_string(),
                query_vector: vec![1.0, 0.0, 0.0, 0.0],
                query_text: "anything".to_string(),
                top_k: 5,
            })
            .await;
        let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
        assert!(sr.vector_rows.is_empty(), "other workspace must be isolated");

        // delete → search empty
        let resp = e
            .handle(EngineRequest::DeleteDocument {
                workspace_id: "ws1".to_string(),
                document_id: "doc-1".to_string(),
                operation_id: "op-2".to_string(),
            })
            .await;
        assert!(resp.ok, "delete failed: {:?}", resp.error);
        let resp = e
            .handle(EngineRequest::Search {
                workspace_id: "ws1".to_string(),
                query_vector: vec![1.0, 0.0, 0.0, 0.0],
                query_text: "markdown chunking".to_string(),
                top_k: 5,
            })
            .await;
        let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
        assert!(sr.vector_rows.is_empty(), "deleted doc must not be searchable");

        let _ = std::fs::remove_dir_all(&dir);
    }
}