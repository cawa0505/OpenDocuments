# Architecture Overview

OpenDocuments is a modular, high-performance RAG platform written in Rust.

## Package Structure

```
apps/
  webui/      - React SPA (Vite + Tailwind CSS) frontend
crates/
  opendoc-cli - Main CLI and terminal interface (opendoc)
  opendoc-mcp - Axum API server, SSE streaming, and MCP protocol core
  opendoc-storage - SQLite relational storage and LanceDB Sidecar client
  opendoc-engine-lancedb - LanceDB vector engine Sidecar daemon
  opendoc-task - Task Execution Layer (TaskEnvelope, TaskResult, TaskExecutor)
  opendoc-ai - Native AI Engine abstraction (AiEngine, EngineConfig, HardwareBackend)
  opendoc-llm - OpenAI-compatible LLM client and progressively-parsed streaming
  opendoc-types - Shared strong types (DocumentChunk, Tag, etc.)
  opendoc-parser-* - Standalone sandboxed document format parsers (PDF, DOCX, XLSX, etc.)
```

## Data Flow

```
Document Source → Parser (chunks) → Chunker (semantic split)
  → Embedder (vectors) → Storage (SQLite + LanceDB)

User Query → Embedder (query vector) → Retriever (dense + sparse search)
  → Reranker → Context Window Fitting → Generator (LLM) → Response
```

## Key Design Decisions

### Single Binary Distribution
All layers (database, vector search, LLM clients, HTTP server, and MCP) compile into a single Rust binary. The React WebUI is compiled and fully embedded into the binary using `rust-embed`, allowing deployment with zero external Node.js dependencies.

### Hybrid Search (Dense + Sparse)
Combines dense vector similarity (LanceDB) with keyword matching (LanceDB full-text search) via Reciprocal Rank Fusion (RRF).

### Multi-Profile RAG
Three built-in profiles (fast/balanced/precise) trade off speed vs quality. Each profile configures retrieval depth and reranking.
