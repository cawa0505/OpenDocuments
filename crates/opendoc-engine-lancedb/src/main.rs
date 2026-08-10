//! opendoc-engine-lancedb: private child-process owning all LanceDB operations.
//!
//! Communication: newline-delimited JSON over stdin/stdout. Logs go to stderr.
//! Core owns embedding + RRF fusion; engine owns LanceDB connection, index writes,
//! vector search, FTS search, and deletion.

mod engine;

use std::io::{self, BufRead, Write};

use opendoc_types::protocol::{EngineRequest, EngineResponse};

use engine::Engine;

/// CLI args `--uri` / `--table`, falling back to env `OPENDOC_LANCEDB_URI` /
/// `OPENDOC_LANCEDB_TABLE`, then defaults.
fn parse_args() -> (String, String) {
    let mut uri = std::env::var("OPENDOC_LANCEDB_URI").unwrap_or_default();
    let mut table = std::env::var("OPENDOC_LANCEDB_TABLE").unwrap_or_else(|_| "documents".into());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--uri" => uri = args.next().unwrap_or_default(),
            "--table" => table = args.next().unwrap_or_default(),
            _ => {}
        }
    }
    if uri.is_empty() {
        uri = std::env::temp_dir().join("opendoc-lancedb").display().to_string();
    }
    (uri, table)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let (uri, table) = parse_args();
    let mut engine = Engine::connect(&uri, &table).await;
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: EngineRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => {
                let resp = EngineResponse::err("parse".into(), format!("bad request: {e}"));
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap_or_default());
                let _ = out.flush();
                continue;
            }
        };
        let shutdown = matches!(&req, EngineRequest::Shutdown);
        let resp = engine.handle(req).await;
        let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap_or_default());
        let _ = out.flush();
        if shutdown {
            break;
        }
    }
}