//! Core-side sidecar client: spawns the engine binary as a child process and
//! communicates over newline-delimited JSON stdio.
//!
//! Sync by design — stdio is blocking IO, and `SearchBackend` is sync.
//! Replaces the direct `lancedb::Connection` ownership in the old retriever.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use opendoc_types::protocol::{
    EngineRequest, EngineResponse, HandshakeResult, HealthResult, SearchResult,
};
use opendoc_types::DocumentChunk;

pub struct SidecarClient {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    child: Child,
    pub handshake: HandshakeResult,
    /// Spawn parameters retained so a crashed engine can be respawned (bounded restart, spec §6).
    engine_path: String,
    lance_uri: String,
    table_name: String,
    vector_dim: usize,
    closed: bool,
}

impl SidecarClient {
    /// Spawn the engine binary and perform the startup handshake.
    /// `engine_path` is the configured or bundled executable path.
    pub fn spawn(engine_path: &str, lance_uri: &str, table_name: &str) -> Result<Self, String> {
        let mut child = Command::new(engine_path)
            .env("OPENDOC_LANCEDB_URI", lance_uri)
            .env("OPENDOC_LANCEDB_TABLE", table_name)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn_engine_failed: {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "engine_stdin_unavailable".to_string())?;
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| "engine_stdout_unavailable".to_string())?,
        );

        let client = SidecarClient {
            stdin,
            stdout,
            child,
            handshake: HandshakeResult {
                protocol_version: String::new(),
                engine_version: String::new(),
                schema_version: String::new(),
                capabilities: Vec::new(),
                vector_dimension: 0,
            },
            engine_path: engine_path.to_string(),
            lance_uri: lance_uri.to_string(),
            table_name: table_name.to_string(),
            vector_dim: 0,
            closed: false,
        };
        Ok(client)
    }

    /// Send a request and read the matching response (newline-delimited JSON).
    fn request(&mut self, req: EngineRequest, id: &str) -> Result<EngineResponse, String> {
        let json = serde_json::to_string(&req).map_err(|e| format!("serialize_failed: {e}"))?;
        writeln!(self.stdin, "{json}").map_err(|e| format!("write_failed: {e}"))?;
        self.stdin.flush().map_err(|e| format!("flush_failed: {e}"))?;

        let mut line = String::new();
        self.stdout
            .read_line(&mut line)
            .map_err(|e| format!("read_failed: {e}"))?;
        line = line.trim().to_string();
        if line.is_empty() {
            return Err("engine_eof".to_string());
        }
        let resp: EngineResponse =
            serde_json::from_str(&line).map_err(|e| format!("deserialize_failed: {e}"))?;
        if resp.id != id {
            return Err(format!("id_mismatch: expected={id} got={}", resp.id));
        }
        Ok(resp)
    }

    /// Startup handshake: declare protocol version + vector dim so the engine
    /// can create the table with the correct schema. Verifies dim echoed back.
    pub fn handshake(&mut self, vector_dim: usize) -> Result<HandshakeResult, String> {
        let resp = self.request(
            EngineRequest::Handshake {
                protocol_version: "1".to_string(),
                vector_dim,
            },
            "handshake",
        )?;
        if !resp.ok {
            return Err(resp.error.unwrap_or_else(|| "handshake_failed".to_string()));
        }
        let result: HandshakeResult = serde_json::from_value(
            resp.result.unwrap_or_default(),
        )
        .map_err(|e| format!("handshake_parse_failed: {e}"))?;
        if result.vector_dimension != vector_dim {
            return Err(format!(
                "vector_dim_mismatch: expected={vector_dim} got={}",
                result.vector_dimension
            ));
        }
        self.vector_dim = vector_dim;
        Ok(result)
    }

    pub fn health(&mut self) -> Result<HealthResult, String> {
        let resp = self.request(EngineRequest::Health, "health")?;
        if !resp.ok {
            return Err(resp.error.unwrap_or_else(|| "health_failed".to_string()));
        }
        serde_json::from_value(resp.result.unwrap_or_default())
            .map_err(|e| format!("health_parse_failed: {e}"))
    }

    /// Index chunks with pre-computed vectors (core embeds, engine writes).
    pub fn index_chunks(
        &mut self,
        workspace_id: &str,
        document_id: &str,
        collection_id: Option<&str>,
        source_path: &str,
        chunks: &[DocumentChunk],
        vectors: &[Vec<f32>],
    ) -> Result<(), String> {
        let resp = self.request(
            EngineRequest::IndexChunks {
                workspace_id: workspace_id.to_string(),
                document_id: document_id.to_string(),
                operation_id: format!("index-{document_id}"),
                collection_id: collection_id.map(|s| s.to_string()),
                source_path: source_path.to_string(),
                chunks: chunks.to_vec(),
                vectors: vectors.to_vec(),
            },
            "index",
        )?;
        if resp.ok {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_else(|| "index_failed".to_string()))
        }
    }

    /// Search: engine returns raw vector + FTS rows; core does RRF fusion.
    pub fn search(
        &mut self,
        workspace_id: &str,
        query_vector: &[f32],
        query_text: &str,
        top_k: usize,
    ) -> Result<SearchResult, String> {
        let resp = self.request(
            EngineRequest::Search {
                workspace_id: workspace_id.to_string(),
                query_vector: query_vector.to_vec(),
                query_text: query_text.to_string(),
                top_k,
            },
            "search",
        )?;
        if !resp.ok {
            return Err(resp.error.unwrap_or_else(|| "search_failed".to_string()));
        }
        serde_json::from_value(resp.result.unwrap_or_default())
            .map_err(|e| format!("search_parse_failed: {e}"))
    }

    /// Delete all chunks for a document (soft-delete cleanup).
    pub fn delete_document(&mut self, workspace_id: &str, document_id: &str) -> Result<(), String> {
        let resp = self.request(
            EngineRequest::DeleteDocument {
                workspace_id: workspace_id.to_string(),
                document_id: document_id.to_string(),
                operation_id: format!("delete-{document_id}"),
            },
            "delete",
        )?;
        if resp.ok {
            Ok(())
        } else {
            Err(resp.error.unwrap_or_else(|| "delete_failed".to_string()))
        }
    }

    /// Graceful shutdown: send Shutdown, drain the response so the child's write
    /// cannot block on a full pipe, then wait for the child to exit.
    pub fn shutdown(&mut self) {
        let _ = writeln!(self.stdin, "{{\"op\":\"Shutdown\"}}");
        let _ = self.stdin.flush();
        let mut buf = String::new();
        let _ = self.stdout.read_line(&mut buf); // drain response, ignore content
        let _ = self.child.wait();
        self.closed = true;
    }

    pub fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
        }
    }

    /// Bounded restart: respawn the engine binary and re-run the handshake.
    /// ponytail: single immediate retry (no backoff); backoff policy is [待討論] per spec §6.
    pub fn respawn(&mut self) -> Result<(), String> {
        let dim = self.vector_dim;
        let mut fresh = Self::spawn(&self.engine_path, &self.lance_uri, &self.table_name)?;
        fresh.vector_dim = dim;
        fresh.handshake(dim)?;
        *self = fresh;
        Ok(())
    }
}

impl Drop for SidecarClient {
    fn drop(&mut self) {
        if self.closed {
            return; // graceful shutdown already completed
        }
        // ponytail: kill as last-resort cleanup; normal shutdown should call shutdown()
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
