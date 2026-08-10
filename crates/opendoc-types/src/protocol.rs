//! Sidecar IPC protocol types.
//!
//! Newline-delimited JSON over child-process stdin/stdout.
//! Core writes requests, engine writes responses. See openspec/specs/lancedb-engine-sidecar.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Core → Engine request. `op` tag dispatches the operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum EngineRequest {
    Handshake {
        protocol_version: String,
        /// Embedding dimension from core (BYOK model / config). Engine needs it
        /// before creating the table schema.
        vector_dim: usize,
    },
    Health,
    IndexChunks {
        workspace_id: String,
        document_id: String,
        operation_id: String,
        collection_id: Option<String>,
        source_path: String,
        chunks: Vec<crate::DocumentChunk>,
        /// Pre-computed embedding vectors, one per chunk, aligned by index.
        /// Core owns embedding; engine receives vectors ready for LanceDB.
        vectors: Vec<Vec<f32>>,
    },
    Search {
        workspace_id: String,
        /// Pre-computed query embedding vector (core embeds).
        query_vector: Vec<f32>,
        /// Original query text for FTS (engine may use Lance FTS).
        query_text: String,
        top_k: usize,
    },
    DeleteDocument {
        workspace_id: String,
        document_id: String,
        operation_id: String,
    },
    Optimize,
    Shutdown,
}

/// Engine → Core response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl EngineResponse {
    pub fn ok(id: String, result: Value) -> Self {
        Self { id, ok: true, result: Some(result), error: None }
    }

    pub fn err(id: String, error: String) -> Self {
        Self { id, ok: false, result: None, error: Some(error) }
    }
}

/// Ranked search hit returned by the engine and passed through core to the public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub doc_path: String,
    pub spec_id: String,
    pub heading: String,
    pub score: f32,
    pub snippet: String,
}

/// Handshake result payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResult {
    pub protocol_version: String,
    pub engine_version: String,
    pub schema_version: String,
    pub capabilities: Vec<String>,
    pub vector_dimension: usize,
}

/// Health result payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResult {
    pub status: String,
}

/// One raw row from engine vector or FTS search, before core RRF fusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawSearchRow {
    pub document_id: String,
    pub content: String,
    pub metadata_json: String,
    pub cosine_distance: f32,
    pub rank: usize,
}

/// Search result: separate vector and FTS candidate lists for core to fuse via RRF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub vector_rows: Vec<RawSearchRow>,
    pub fts_rows: Vec<RawSearchRow>,
}