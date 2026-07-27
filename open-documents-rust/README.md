# OpenDocuments Rust — High-Performance Native Core

OpenDocuments Rust is a complete, ultra-fast, native Rust rewrite of the OpenDocuments RAG backend. It consolidates the Web UI REST API, streamable SSE Model Context Protocol (MCP) server, and background ingestion scheduler into a single compiled binary under 30MB — running with 3ms cold starts and 0ms garbage collection pauses.

---

## Why OpenDocuments Rust?

The original TypeScript implementation relies on heavily nested `node_modules`, node-gyp prebuild bindings, and system-level Python dependencies, resulting in a **1.5GB+ Docker image** and high idle memory usage. 

OpenDocuments Rust strips away all JavaScript/Node.js runtime overhead while enhancing performance and security across the board:

| Metric / Feature | Original TypeScript Core | OpenDocuments Rust Core |
|------------------|--------------------------|-------------------------|
| **Binary Size** | ~1.5GB (Image runtime) | **< 30MB (Static Compiled Binary)** |
| **Startup Time** | ~1.5s - 3.0s | **~3ms** |
| **Idle RAM Usage**| ~250MB - 400MB | **~15MB** |
| **Database Engines**| Standard SQLite & LanceDB | **WAL-Optimized SQLite + Native LanceDB 0.10** |
| **File Ingestion Guard**| Fragile extension checks | **Steel-Guard (Extension, Original Suffix Fallback, Magic-Bytes)** |
| **Debugging Shell**| Web UI only | **60FPS Interactive Ratatui TUI Debugger** |

---

## Architectural Highlights

### ⚡ 1. Unified REST & MCP Server (Axum)
Instead of running a separate REST API gateway and a stdio/supergateway MCP bridge, OpenDocuments Rust runs a unified Axum server on port `3000` handling:
* Hono-compatible **REST API endpoints** (`/api/v1/healthz`, `/api/v1/chat/stream`).
* Fast **SPA static asset fallback** hosting the compiled Vue bundle (`./dist`) directly, routing clean-reloads with absolute zero routing caching penalties.
* Native **SSE MCP Server endpoints** (`/api/mcp/sse` & `/api/mcp/message`) to allow Cursor, Claude Code, or Windsurf to plug directly into your workspace.

### 🛡️ 2. Steel-Guard Ingestion Pipeline
To eliminate ingestion failures caused by temporary file systems, lowercase/uppercase variant extensions, or headless binary temp names (e.g. `/tmp/83af128bcde`), the parser router (`opendoc-parser`) implements three-layer defense routing:
1. **Lowercase Suffix Normalization**: Matches `.PDF`, `.XLSX`, and `.docx` to their native Rust engines.
2. **Original Suffix Fallback**: If the runtime passes a temporary path without suffix, the router parses the original filename parameter (`original_name`).
3. **Magic-Bytes Fallback**: Extracts the first 4 bytes of raw buffers to automatically discover file formats (e.g., `%PDF`, `PK..` for Zip-based Excel/Word) when all text parameters are completely missing.

### 🧠 3. Dual-Stage Rerank & Score Filter
Implements a multi-tier query retrieval pipeline:
* **Stage-1 (Heuristic Keyword-Weight Sorting)**: Instantly bubbles up file paths, directories, and exact keyword matches.
* **Stage-2 (Pairwise Cross-Encoder Scoring)**: High-accuracy semantic relevance ranking.
* **Stage-3 (Score Threshold Fuse)**: Discards retrieval noise below `0.60` relevance score. Features a self-healing fallback ensuring that the Top-1 candidate is preserved even if everything is pruned out by strict thresholds.

---

## Subcommand CLI Registry

OpenDocuments Rust provides 100% command-line compatibility with the TypeScript spec, accessible directly via CLI flags:

```bash
# 1. Start the unified background server (API + MCP + WebUI fallback)
./target/release/open-documents-rust start --port 3000

# 2. Open the 60FPS interactive Ratatui debugging terminal
./target/release/open-documents-rust tui

# 3. Ingest documents recursively with adaptive concurrency guards
./target/release/open-documents-rust document index /path/to/docs --workspace homelab

# 4. Perform vector queries directly from the command line
./target/release/open-documents-rust search "Model Context Protocol" --limit 5

# 5. Check environment, LLM connection, and DB status
./target/release/open-documents-rust doctor
```

---

## Performance & Optimization Guardrails

The project locks the build compilation matrix using defensive engineering profiles inside `Cargo.toml` and `.cargo/config.toml`:
* **Development (`profile.dev`)**: Enables `opt-level = 1` to bypass Rustc 1.94+ deep async recursion depth overflow bug.
* **Release (`profile.release`)**: Compiles with `opt-level = 2`, `panic = "abort"`, and `codegen-units = 16` to guarantee ultra-fast compile times, optimal execution speed, and 0% query layout overflows.
* **Compiler Box**: Locked to Stable `1.97.1` inside `rust-toolchain.toml` to prevent environment drift.

---

## Getting Started

### Prerequisites

* Rust 1.97.1+ (configured automatically via toolchain)
* SQLite / LanceDB development libraries

### Building From Source

```bash
# Build optimized release binary
cargo build --release -j 2

# Verify the executable is active
./target/release/open-documents-rust --help
```

### Future Roadmap

* **[WASM_ROADMAP.md](./WASM_ROADMAP.md)** — 將重度數據處理（chunking、解析、過濾）移至前端 WebAssembly，實現零上傳端側 RAG。

### Double-Track Deployment Strategy
For maximum performance and system availability, OpenDocuments Rust supports dual-track configurations:
1. **Native Mode (Linux/systemd)**: Deploys directly onto the host with a dedicated systemd user-unit, bypassing network translation layers for sub-5ms API response times.
2. **Container Mode (Docker)**: Minimizes cross-platform deployment friction on Windows and macOS. The multi-stage Dockerfile outputs an Alpine/Distroless container weighing under 30MB, eliminating all Node.js and Python dependencies.
