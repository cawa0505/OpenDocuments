//! End-to-end stdio round-trip against the real engine binary.
//!
//! Spawns the compiled `opendoc-engine-lancedb`, drives handshake → index →
//! search → delete → shutdown over newline-delimited JSON, and asserts the
//! child exits (no orphan) after `Shutdown`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

use opendoc_types::protocol::{
    EngineRequest, EngineResponse, HandshakeResult, RawSearchRow, SearchResult,
};
use opendoc_types::{ChunkType, DocumentChunk};

fn spawn_engine(dir: &std::path::Path) -> (Child, ChildStdin, BufReader<std::process::ChildStdout>) {
    let uri = dir.join("lancedb").display().to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_opendoc-engine-lancedb"))
        .args(["--uri", &uri, "--table", "documents"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn engine");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = BufReader::new(child.stdout.take().expect("stdout"));
    (child, stdin, stdout)
}

fn request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<std::process::ChildStdout>,
    req: &EngineRequest,
) -> EngineResponse {
    let line = serde_json::to_string(req).expect("serialize request");
    writeln!(stdin, "{line}").expect("write request");
    stdin.flush().expect("flush");
    let mut resp = String::new();
    stdout.read_line(&mut resp).expect("read response");
    serde_json::from_str(&resp).expect("parse response")
}

fn chunk(content: &str, headers: &[&str]) -> DocumentChunk {
    DocumentChunk {
        chunk_type: ChunkType::Semantic,
        content: content.to_string(),
        workspace_id: "ws-1".to_string(),
        collection_id: String::new(),
        file_path: "doc-1.md".to_string(),
        relevance_score: None,
        metadata: serde_json::json!({ "headers": headers }),
    }
}

#[test]
fn roundtrip_index_search_delete_shutdown() {
    let dir = std::env::temp_dir().join(format!("od-engine-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");

    let (mut child, mut stdin, mut stdout) = spawn_engine(&dir);

    // handshake — engine reports 4-dim from the request.
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::Handshake {
            protocol_version: "1".to_string(),
            vector_dim: 4,
        },
    );
    assert!(resp.ok, "handshake failed: {:?}", resp.error);
    let hs: HandshakeResult =
        serde_json::from_value(resp.result.unwrap()).expect("handshake result");
    assert_eq!(hs.vector_dimension, 4);

    // index two chunks with known 4-dim vectors.
    let vectors: Vec<Vec<f32>> = vec![vec![1.0, 0.0, 0.0, 0.0], vec![0.9, 0.1, 0.0, 0.0]];
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::IndexChunks {
            workspace_id: "ws-1".to_string(),
            document_id: "doc-1".to_string(),
            operation_id: "op-1".to_string(),
            collection_id: None,
            source_path: "docs/doc-1.md".to_string(),
            chunks: vec![
                chunk("bge-m3 embedding topic", &["嵌入模型"]),
                chunk("retrieval pipeline design", &["檢索流程"]),
            ],
            vectors,
        },
    );
    assert!(resp.ok, "index failed: {:?}", resp.error);

    // search — query vector matches chunk 0.
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::Search {
            workspace_id: "ws-1".to_string(),
            query_vector: vec![1.0, 0.0, 0.0, 0.0],
            query_text: "embedding".to_string(),
            top_k: 5,
        },
    );
    assert!(resp.ok, "search failed: {:?}", resp.error);
    let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
    assert!(!sr.vector_rows.is_empty(), "expected vector hits, got empty");
    let row: &RawSearchRow = &sr.vector_rows[0];
    assert_eq!(row.document_id, "doc-1");
    assert!(row.cosine_distance < 1.0, "cosine distance {}", row.cosine_distance);

    // workspace isolation — another workspace sees nothing.
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::Search {
            workspace_id: "ws-other".to_string(),
            query_vector: vec![1.0, 0.0, 0.0, 0.0],
            query_text: "embedding".to_string(),
            top_k: 5,
        },
    );
    let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
    assert!(sr.vector_rows.is_empty(), "cross-workspace leak: {:?}", sr.vector_rows);

    // delete — chunks gone.
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::DeleteDocument {
            workspace_id: "ws-1".to_string(),
            document_id: "doc-1".to_string(),
            operation_id: "op-2".to_string(),
        },
    );
    assert!(resp.ok, "delete failed: {:?}", resp.error);
    let resp = request(
        &mut stdin,
        &mut stdout,
        &EngineRequest::Search {
            workspace_id: "ws-1".to_string(),
            query_vector: vec![1.0, 0.0, 0.0, 0.0],
            query_text: "embedding".to_string(),
            top_k: 5,
        },
    );
    let sr: SearchResult = serde_json::from_value(resp.result.unwrap()).expect("search result");
    assert!(sr.vector_rows.is_empty(), "delete left rows: {:?}", sr.vector_rows);

    // shutdown — child must exit promptly (no orphan).
    let resp = request(&mut stdin, &mut stdout, &EngineRequest::Shutdown);
    assert!(resp.ok, "shutdown failed: {:?}", resp.error);
    drop(stdin);
    let status = child.wait().expect("wait engine exit");
    assert!(status.success(), "engine exit status {status:?}");

    let _ = std::fs::remove_dir_all(&dir);
}
