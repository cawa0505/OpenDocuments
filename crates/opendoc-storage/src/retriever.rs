use std::sync::Arc;

use arrow_array::{
    builder::{FixedSizeListBuilder, Float32Builder},
    Array, Float32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use futures::StreamExt;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::table::Table;
use opendoc_types::{ChunkType, DocumentChunk, EmbeddingProvider};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::lancedb::get_compat_schema;

/// 檢索 hit。由 graphify-plugin-opendoc Layer 2 消費。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub doc_path: String,
    pub spec_id: String,
    /// R2：chunk 所屬 section 的原始 heading 文字（非 slug）。Plugin 拿
    /// `doc_path + heading` 自算 `sha1(...)[0..12]` 對映回內部 spec_id。
    pub heading: String,
    pub score: f32,
    pub snippet: String,
}

/// LanceDB 後檢索器。open 連線、維護相容 schema 表、index write（embed→LanceDB）與向量+FTS 混合檢索。
///
/// ponytail: index 與 search 均為 async；上層（opendoc-mcp）透過既有的
/// `tokio::task::block_in_place` + `Handle::block_on` 慣例從 sync trait 呼叫，
/// 單 search 請求會佔住一個 runtime worker 執行緒；高併發想升級為 per-request embed 池。
pub struct LanceDbRetriever {
    conn: lancedb::Connection,
    table_name: String,
    dim: usize,
    embed: Arc<dyn EmbeddingProvider>,
    /// chat 透過既有 sync `search_and_rerank`（無 workspace 參數）時使用。
    /// ponytail: 升級上限——chat 路徑應改帶入 per-request workspace，目前用伺服器預設值。
    default_workspace: String,
}

impl LanceDbRetriever {
    pub async fn connect(
        uri: impl Into<String>,
        table_name: impl Into<String>,
        dim: usize,
        default_workspace: impl Into<String>,
        embed: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self, String> {
        let uri = uri.into();
        let conn = lancedb::connect(&uri)
            .execute()
            .await
            .map_err(|e| e.to_string())?;
        let embed_dim = self_embed_dim_for_check(&embed);
        if embed_dim != 0 && embed_dim != dim {
            return Err(format!(
                "embedding provider dim {} != 請求 dim {}",
                embed_dim, dim
            ));
        }
        Ok(Self {
            conn,
            table_name: table_name.into(),
            dim,
            embed,
            default_workspace: default_workspace.into(),
        })
    }

    /// chat 既有 sync 路徑（`search_and_rerank`）委派至此：用 default_workspace、threshold、top_k=10。
    pub async fn search_default(
        &self,
        query: &str,
        threshold: f32,
    ) -> Vec<opendoc_types::DocumentChunk> {
        let hits = self
            .search(query, 10, threshold, &self.default_workspace)
            .await
            .unwrap_or_default();
        hits.into_iter()
            .map(|h| opendoc_types::DocumentChunk {
                chunk_type: ChunkType::Semantic,
                content: format!("{}\n\n{}", h.doc_path, h.snippet),
                workspace_id: self.default_workspace.clone(),
                collection_id: String::new(),
                file_path: h.doc_path,
                relevance_score: Some(h.score),
                metadata: serde_json::json!({
                    "spec_id": h.spec_id,
                    "heading": h.heading,
                    "score": h.score,
                }),
            })
            .collect()
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
            let schema = get_compat_schema(self.dim as i32);
            self.conn
                .create_empty_table(&self.table_name, schema)
                .execute()
                .await
                .map_err(|e| e.to_string())
        }
    }

    /// 移除指定文件的全部 chunks（軟刪除文件時呼叫，避免已刪文件仍被搜尋命中）。
    /// workspace_id/document_id 皆為 UUID，無需跳脫；若日後開放任意字串需 escape。
    pub async fn delete_document(
        &self,
        workspace_id: &str,
        document_id: &str,
    ) -> Result<(), String> {
        let table = self.open_or_create_table().await?;
        let predicate = format!(
            "workspace_id = '{}' AND document_id = '{}'",
            workspace_id, document_id
        );
        table
            .delete(&predicate)
            .await
            .map(|_| ())
            .map_err(|e| format!("lancedb delete 失敗: {e}"))
    }

    /// 確保 content 欄有 FTS 索引。首次 insert 後呼叫；best-effort（已存在或失敗皆忽略）。
    async fn ensure_fts_index(&self, table: &Table) {
        use lancedb::index::Index;
        use lancedb::index::scalar::FtsIndexBuilder;
        let _ = table
            .create_index(&["content"], Index::FTS(FtsIndexBuilder::default()))
            .execute()
            .await;
    }

    /// Index：embed chunks → delete 舊 chunks(document_id) → insert LanceDB。reindex 冪等。
    pub async fn index_chunks(
        &self,
        document_id: &str,
        workspace_id: &str,
        collection_id: Option<&str>,
        source_path: &str,
        chunks: &[DocumentChunk],
    ) -> Result<(), String> {
        if chunks.is_empty() {
            return Ok(());
        }
        let texts: Vec<String> = chunks.iter().map(|c| c.content.clone()).collect();
        let vectors = self.embed.embed(&texts).await?;
        if vectors.len() != chunks.len() {
            return Err(format!(
                "embed 回傳維度不符: {} != {}",
                vectors.len(),
                chunks.len()
            ));
        }
        for v in &vectors {
            if v.len() != self.dim {
                return Err(format!(
                    "向量維度 {} != table dim {}",
                    v.len(),
                    self.dim
                ));
            }
        }

        let table = self.open_or_create_table().await?;

        // delete 舊 chunks（reindex 冪等）。document_id 為 uuid，無單引號逸出需求。
        let predicate = format!("document_id = '{}'", document_id.replace('\'', ""));
        let _ = table.delete(&predicate).await;

        let n = chunks.len();
        let document_id_col = StringArray::from(vec![document_id; n]);
        let chunk_types: Vec<String> = chunks
            .iter()
            .map(|c| format!("{:?}", c.chunk_type))
            .collect();
        let chunk_type_col = StringArray::from(
            chunk_types
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );
        let content_col = StringArray::from(
            chunks
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>(),
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
            metadatas
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
        );

        // vector 欄：FixedSizeList<f32, dim>
        let mut fsl = FixedSizeListBuilder::new(
            Float32Builder::with_capacity(self.dim * n),
            self.dim as i32,
        );
        for v in &vectors {
            fsl.values().append_slice(v);
            fsl.append(true);
        }
        let vector_array = fsl.finish();

        let schema = get_compat_schema(self.dim as i32);
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
            get_compat_schema(self.dim as i32),
        );
        table
            .add(reader)
            .execute()
            .await
            .map_err(|e| e.to_string())?;

        // best-effort FTS 索引
        self.ensure_fts_index(&table).await;

        Ok(())
    }

    /// 向量檢索：embed query → cosine nearest → 過濾 workspace → filter threshold。
    async fn vector_search(
        &self,
        table: &Table,
        qvec: &[f32],
        workspace_id: &str,
        top_k: usize,
    ) -> Result<Vec<RankedRow>, String> {
        let mut q = table
            .query()
            .only_if(format!("workspace_id = '{}'", workspace_id.replace('\'', "")))
            .select(Select::columns(&[
                "document_id",
                "content",
                "metadata_json",
                "_distance",
            ]))
            .nearest_to(qvec.to_vec())
            .map_err(|e| e.to_string())?;
        q = q.limit(top_k * 3);
        let stream = q.execute().await.map_err(|e| e.to_string())?;
        let mut rows = Vec::new();
        let mut rank = 0usize;
        let mut stream = stream;
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
                rows.push(RankedRow::from_batch(&batch, i, "vector", dist, rank)?);
                rank += 1;
            }
        }
        Ok(rows)
    }

    /// 全文檢索：best-effort。FTS 索引未建或查無結果 → 回空 Vec（退化為純向量）。
    async fn fts_search(
        &self,
        table: &Table,
        query: &str,
        workspace_id: &str,
        top_k: usize,
    ) -> Vec<RankedRow> {
        use lancedb::index::scalar::FullTextSearchQuery;
        let fts = match FullTextSearchQuery::new(query.to_owned()) {
            fts => fts,
        };
        let q = table
            .query()
            .only_if(format!("workspace_id = '{}'", workspace_id.replace('\'', "")))
            .select(Select::columns(&[
                "document_id",
                "content",
                "metadata_json",
                "_score",
            ]))
            .full_text_search(fts)
            .limit(top_k * 3);
        let stream = match q.execute().await {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut rows = Vec::new();
        let mut rank = 0usize;
        let mut stream = stream;
        while let Some(Ok(batch)) = stream.next().await {
            let scores = match batch.column_by_name("_score") {
                Some(s) => s,
                None => continue,
            };
            // _score 可能 f32 / f64；best-effort 取 f32，否則跳過該 batch
            let scores = match scores.as_any().downcast_ref::<Float32Array>() {
                Some(s) => s,
                None => {
                    continue;
                }
            };
            for i in 0..batch.num_rows() {
                let _sc = scores.value(i);
                if let Ok(row) = RankedRow::from_batch(&batch, i, "fts", 0.0, rank) {
                    rows.push(row);
                    rank += 1;
                }
            }
        }
        rows
    }

    /// 主檢索入口：向量 + FTS RRF 融合，回傳 top_k 且 score >= threshold。
    /// score = cosine 相似度（1 - distance/2），∈ [0,1]；RRF 只影響排序，不改 score。
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        threshold: f32,
        workspace_id: &str,
    ) -> Result<Vec<SearchHit>, String> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let qvec = self.embed.embed(&[query.to_owned()]).await?;
        let qvec = qvec
            .into_iter()
            .next()
            .ok_or("embed 回傳空向量")?;
        if qvec.len() != self.dim {
            return Err(format!("query 向量維度 {} != {}", qvec.len(), self.dim));
        }

        let table = self.open_or_create_table().await?;
        let vec_rows = self.vector_search(&table, &qvec, workspace_id, top_k).await?;
        let fts_rows = self.fts_search(&table, query, workspace_id, top_k).await;

        // RRF：以 (document_id, chunk_idx) 為 key 融合 rank。
        let mut fused: HashMap<String, RankedRow> = HashMap::new();
        for (rank_weight, rows) in [((60, "vector"), vec_rows), ((60, "fts"), fts_rows)] {
            for r in rows {
                let key = format!("{}#{}", r.document_id, r.chunk_idx);
                let rrf = 1.0 / (rank_weight.0 as f32 + 1.0 + r.rank as f32);
                let entry = fused.entry(key.clone()).or_insert(r);
                entry.rrf_score += rrf;
            }
        }

        let mut hits = fused.values_mut().collect::<Vec<_>>();
        hits.sort_by(|a, b| b.rrf_score.partial_cmp(&a.rrf_score).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = Vec::new();
        for r in hits {
            // score = cosine 相似度
            let cos = {
                let d = r.cosine_distance;
                (1.0 - d / 2.0).max(0.0).min(1.0)
            };
            if cos < threshold {
                continue;
            }
            out.push(SearchHit {
                doc_path: r.doc_path.clone(),
                spec_id: build_spec_id(&r.doc_path, &r.headers, r.chunk_idx),
                heading: r.headers.last().cloned().unwrap_or_default(),
                score: cos,
                snippet: snippet(&r.content),
            });
            if out.len() >= top_k {
                break;
            }
        }
        Ok(out)
    }
}

/// 一筆檢索中間列。
struct RankedRow {
    document_id: String,
    chunk_idx: usize,
    content: String,
    doc_path: String,
    headers: Vec<String>,
    cosine_distance: f32,
    rank: usize,
    rrf_score: f32,
}

impl RankedRow {
    fn from_batch(
        batch: &RecordBatch,
        i: usize,
        _source: &str,
        cosine_distance: f32,
        rank: usize,
    ) -> Result<Self, String> {
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
        let document_id = get_str("document_id")?;
        let content = get_str("content")?;
        let metadata_json = get_str("metadata_json").unwrap_or_default();
        let (doc_path, headers, chunk_idx) = parse_metadata(&metadata_json, &document_id);
        Ok(RankedRow {
            document_id,
            chunk_idx,
            content,
            doc_path,
            headers,
            cosine_distance,
            rank,
            rrf_score: 0.0,
        })
    }
}

fn parse_metadata(metadata_json: &str, document_id: &str) -> (String, Vec<String>, usize) {
    let v: Value = serde_json::from_str(metadata_json).unwrap_or(Value::Null);
    let doc_path = v
        .get("doc_path")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| document_id.to_string());
    let headers = v
        .get("headers")
        .and_then(|h| h.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let chunk_idx = v
        .get("chunk_idx")
        .and_then(|x| x.as_u64())
        .map(|x| x as usize)
        .unwrap_or(0);
    (doc_path, headers, chunk_idx)
}

fn build_spec_id(doc_path: &str, headers: &[String], chunk_idx: usize) -> String {
    match headers.last() {
        Some(h) if !h.is_empty() => format!("{}#{}", doc_path, slugify(h)),
        _ => format!("{}#chunk-{}", doc_path, chunk_idx),
    }
}

fn slugify(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn snippet(content: &str) -> String {
    let trimmed = content.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 240 {
        return trimmed.to_string();
    }
    let s: String = chars.iter().take(240).collect();
    format!("{}…", s)
}

/// lancedb::Connection 的查詢輔助（dim 校驗在 connect 階段以 provider::dim() 為準）。
fn self_embed_dim_for_check(embed: &Arc<dyn EmbeddingProvider>) -> usize {
    embed.dim()
}
