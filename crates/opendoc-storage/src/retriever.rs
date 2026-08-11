//! Core-side search retriever backed by the LanceDB engine sidecar.
//!
//! Core owns: embedding (BYOK / local models), RRF fusion, threshold/top-k,
//! and SearchHit formatting. Engine owns: LanceDB connection, compat schema,
//! index writes, vector/FTS search, and deletion — reached over stdio.
//!
//! Sync trait (`SearchBackend`) bridges to async embedding via the same
//! `block_in_place` + `Handle::block_on` convention used elsewhere.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use opendoc_types::{ChunkType, DocumentChunk, EmbeddingProvider};
use serde_json::Value;

use crate::sidecar_client::SidecarClient;

/// 檢索 hit。定義於 opendoc_types::protocol，由 graphify-plugin-opendoc Layer 2 消費。
pub use opendoc_types::protocol::SearchHit;

/// Sidecar-backed 檢索器。替換原 in-process `LanceDbRetriever`。
///
/// ponytail: `SidecarClient` 為 request/response 序列化通訊，單 `Mutex` 就夠；
/// 若未來需平行查詢，升級為連線池 / 背景 reader thread。
pub struct SidecarRetriever {
    client: Mutex<SidecarClient>,
    dim: usize,
    embed: Arc<dyn EmbeddingProvider>,
    /// chat 既有 sync `search_and_rerank`（無 workspace 參數）使用。
    default_workspace: String,
}

impl SidecarRetriever {
    /// Spawn 引擎子程序並完成 handshake。`engine_path` 為執行檔路徑（config 或 PATH）。
    pub async fn connect(
        engine_path: &str,
        lance_uri: &str,
        table_name: &str,
        dim: usize,
        default_workspace: &str,
        embed: Arc<dyn EmbeddingProvider>,
    ) -> Result<Self, String> {
        let embed_dim = embed.dim();
        if embed_dim != 0 && embed_dim != dim {
            return Err(format!(
                "embedding provider dim {} != 請求 dim {}",
                embed_dim, dim
            ));
        }
        let mut client = SidecarClient::spawn(engine_path, lance_uri, table_name)?;
        client.handshake(dim)?;
        Ok(Self {
            client: Mutex::new(client),
            dim,
            embed,
            default_workspace: default_workspace.to_string(),
        })
    }

    /// chat 既有 sync 路徑（`search_and_rerank`）委派至此：default_workspace、threshold、top_k=10。
    pub async fn search_default(
        &self,
        query: &str,
        threshold: f32,
    ) -> Vec<opendoc_types::DocumentChunk> {
        self.search_workspace(query, threshold, &self.default_workspace)
            .await
    }

    /// Chat retrieval scoped to the workspace resolved from the request.
    pub async fn search_workspace(
        &self,
        query: &str,
        threshold: f32,
        workspace_id: &str,
    ) -> Vec<opendoc_types::DocumentChunk> {
        let hits = self
            .search(query, 10, threshold, workspace_id)
            .await
            .unwrap_or_default();
        hits.into_iter()
            .map(|h| opendoc_types::DocumentChunk {
                chunk_type: ChunkType::Semantic,
                content: format!("{}\n\n{}", h.doc_path, h.snippet),
                workspace_id: workspace_id.to_string(),
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

    /// Lock the engine client and run `f`. If the engine crashed mid-call,
    /// respawn it once and retry (bounded restart, spec §6).
    fn with_client<T>(
        &self,
        mut f: impl FnMut(&mut SidecarClient) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut client = self
            .client
            .lock()
            .map_err(|_| String::from("engine lock poisoned"))?;
        match f(&mut client) {
            Ok(v) => Ok(v),
            Err(e) if !client.is_alive() => {
                // ponytail: fixed 500ms delay, single retry. Backoff policy is [待討論] per spec §6;
                // exponential backoff if crash-looping becomes a real problem.
                std::thread::sleep(std::time::Duration::from_millis(500));
                client
                    .respawn()
                    .map_err(|re| format!("engine_respawn_failed: {re}; original: {e}"))?;
                drop(client);
                let mut client = self
                    .client
                    .lock()
                    .map_err(|_| String::from("engine lock poisoned"))?;
                f(&mut client)
            }
            Err(e) => Err(e),
        }
    }

    /// Index：embed chunks → 送引擎寫入 LanceDB（引擎內先刪該 document 舊 chunks，reindex 冪等）。
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
                return Err(format!("向量維度 {} != table dim {}", v.len(), self.dim));
            }
        }
        self.with_client(|client| {
            client.index_chunks(
                workspace_id,
                document_id,
                collection_id,
                source_path,
                chunks,
                &vectors,
            )
        })
    }

    /// 主檢索入口：embed query → 引擎向量+FTS 候選 → core RRF 融合 → threshold/top-k → SearchHit。
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
        let qvec = qvec.into_iter().next().ok_or("embed 回傳空向量")?;
        if qvec.len() != self.dim {
            return Err(format!("query 向量維度 {} != {}", qvec.len(), self.dim));
        }

        let result = self.with_client(|client| client.search(workspace_id, &qvec, query, top_k))?;

        // RRF：以 (document_id, chunk_idx) 為 key 融合 vector + FTS rank。
        let mut fused: HashMap<String, RankedRow> = HashMap::new();
        for (k, rows) in [(60u32, result.vector_rows), (60u32, result.fts_rows)] {
            for r in rows {
                let (doc_path, headers, chunk_idx) = parse_metadata(&r.metadata_json, &r.document_id);
                let key = format!("{}#{}", r.document_id, chunk_idx);
                let rrf = 1.0 / (k as f32 + 1.0 + r.rank as f32);
                let entry = fused.entry(key.clone()).or_insert_with(|| RankedRow {
                    chunk_idx,
                    content: r.content.clone(),
                    doc_path,
                    headers,
                    cosine_distance: r.cosine_distance,
                    rrf_score: 0.0,
                });
                entry.rrf_score += rrf;
            }
        }

        let mut hits = fused.values_mut().collect::<Vec<_>>();
        hits.sort_by(|a, b| b.rrf_score.partial_cmp(&a.rrf_score).unwrap_or(std::cmp::Ordering::Equal));
        let mut out = Vec::new();
        for r in hits {
            let cos = (1.0 - r.cosine_distance / 2.0).max(0.0).min(1.0);
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

    /// 軟刪除文件時移除其引擎內 chunks（避免已刪文件仍被搜尋命中）。
    pub async fn delete_document(
        &self,
        workspace_id: &str,
        document_id: &str,
    ) -> Result<(), String> {
        self.with_client(|client| client.delete_document(workspace_id, document_id))
    }

    /// Engine availability probe (spec §5.3: search without the engine → 503 engine_unavailable).
    pub fn engine_available(&self) -> bool {
        match self.client.lock() {
            Ok(mut c) => c.is_alive(),
            Err(_) => false,
        }
    }
}

/// 一筆檢索中間列（core 端 RRF 用）。
struct RankedRow {
    chunk_idx: usize,
    content: String,
    doc_path: String,
    headers: Vec<String>,
    cosine_distance: f32,
    rrf_score: f32,
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
