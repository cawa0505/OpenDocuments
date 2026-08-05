<p align="center">
  <h1 align="center">OpenDocuments</h1>
  <p align="center"><strong>Self-hosted RAG platform for AI document search across PDFs, DOCX, XLSX, local files, and web sources — written in Rust</strong></p>
</p>

<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-TW.md">繁體中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue.svg" alt="License"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Rust-1.80%2B-orange.svg" alt="Rust"></a>
</p>

---

## 🚀 Why Modern Rust Rewrite?

OpenDocuments was originally inspired by and built as a TypeScript / Node.js monorepo server using Hono and Turborepo. While that architecture served as an excellent proof of concept, we undertook a **comprehensive ground-up rewrite in modern Rust** to address critical technical debt and fulfill zero-trust, high-efficiency requirements:

1. **True Single-Binary Distribution**: The complete Axum API router and React WebUI static assets are compiled directly into the binary memory using `rust-embed`. Runs as a standalone file with zero external dependencies.
2. **Deterministic Memory Footprint**: Rather than spawning multiple heavy JS runtimes (each consuming 150MB+ overhead), the Rust runtime encapsulates all subsystems within a single, highly-optimized OS thread pool with microsecond-level scheduling.
3. **Rust-Native Embedded Storage**: Metadata indexing via SQLite (FTS5) and vector similarity via LanceDB are embedded natively into the binary process—eliminating any IPC crossing or slow C-binding bridges.
4. **Performance Gains**: Text extraction, semantic chunking, and Reciprocal Rank Fusion (RRF) query planning perform **5x to 15x faster** under the Rust-native execution graph, unlocking real-time responsiveness even on constrained homelab hardware.

---

## ⚡ Performance Benchmark

OpenDocuments has been completely rewritten in Rust to clear technical debt and optimize for resource-constrained environments (like legacy government/school PCs).

Here is a quick comparison between the legacy TypeScript/Node.js implementation and the new Rust core, measured using `hyperfine` on a 10,000-row messy administrative Excel sheet:

| Metric | Legacy (Node.js) | Modern (Rust Core) | Improvement |
| :--- | :--- | :--- | :--- |
| **Cold Start / Idle Memory** | ~180 MB | **~18 MB** | **90% RAM Saved** |
| **Parsing & Chunking Latency** | ~14.25 seconds | **0.83 seconds** | **17x Faster** |
| **Binary Size / Dependencies** | Thick `node_modules` | **Single Binary (with WebUI embedded)** | **Zero External Dependency** |

<details>
<summary>🔍 Click to view hyperfine benchmark command & output log</summary>

```bash
# Environment: AMD Ryzen 5 5600GT, 64GB RAM, Linux (CachyOS)
# Tool used: hyperfine --warmup 3

Benchmark 1: opendoc document index admin_heavy.xlsx
  Time (mean ± σ):     827.0 ms ±   6.2 ms    [User: 22.2 ms, System: 12.2 ms]
  Range (min … max):   819.6 ms … 835.5 ms    10 runs
```
</details>

---

## What is OpenDocuments?

**OpenDocuments is an open-source, self-hosted RAG (Retrieval-Augmented Generation) platform that turns scattered documents into an AI-searchable knowledge base.** It parses format-complex documents, indexes them with hybrid vector + keyword search, and answers natural-language questions with cited sources.

Use OpenDocuments when you want:

- A **self-hosted alternative to enterprise AI search** and proprietary knowledge-base search tools.
- **AI document search with citations** for PDFs, DOCX, XLSX, local files, and web sources.
- A **local-first RAG stack** that can run entirely with Ollama so sensitive documents stay on your own infrastructure.
- A **knowledge base for AI coding assistants** through MCP, including Claude Code, Cursor, Windsurf, and other MCP clients.
- A **high-performance Rust-native core** that compiles into a single binary, serving both the backend and embedded WebUI from memory.

Install with a single command and launch:

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
opendoc start --port 3000
```

Open `http://localhost:3000`, index your documents, and ask questions with source citations.

---

## How OpenDocuments Answers Questions

OpenDocuments **connects to your document sources**, **parses and chunks each document**, **stores metadata in SQLite and vectors in LanceDB**, then **retrieves, reranks, and generates grounded answers**. Every answer includes source citations, confidence scores, and links back to the underlying documents.

In short: **OpenDocuments is a private, zero-trust AI search engine for your organization's documents.**

---

## Key Features

| Feature | What it means |
|---------|---------------|
| **Self-hosted RAG** | Run the full document search stack on your own secure infrastructure. |
| **Cited AI answers** | Ask natural-language questions and see exactly which documents support the answer. |
| **Hybrid retrieval** | Combine dense vector search, SQLite FTS5 keyword search, reranking, and parent-document recall. |
| **Single-Binary Package** | Axum backend and React WebUI are packaged into a single binary via `rust-embed`. Zero external asset requirements or port collision. |
| **Broad file formats** | Native support for Markdown, PDF, DOCX, XLSX, CSV, HTML, and code. |
| **Local or cloud models** | Use Ollama locally or cloud providers such as OpenAI, Anthropic, Google, and xAI. |
| **MCP server** | Let Claude Code, Cursor, Windsurf, and other MCP clients search your internal knowledge base. |
| **Workspace isolation** | Role-based workspace and collection logical isolation for secure multi-context data boundaries. |

---

## Technical Architecture (Modern Rust Workspace)

OpenDocuments is designed as a modular Rust Cargo Workspace:

```
apps/
  webui/           - React SPA (Vite + Tailwind CSS) frontend
crates/
  opendoc-cli      - Main CLI and terminal interface (opendoc)
  opendoc-mcp      - Axum API server, SSE streaming, and MCP protocol core
  opendoc-tui      - Lightweight Ratatui-based terminal RAG UI
  opendoc-storage  - SQLite metadata and LanceDB vector mixed retrieval store
  opendoc-llm      - OpenAI-compatible LLM client and progressively-parsed streaming
  opendoc-types    - Shared strong types (DocumentChunk, Tag, etc.)
  opendoc-parser-* - Standalone sandboxed document format parsers (PDF, DOCX, XLSX, etc.)
```

---

## Configuration

OpenDocuments is configured via a standard TOML file located at `~/.config/opendocuments/config.toml`. 

The configuration is automatically initialized with default values the first time you run `opendoc`.

```toml
[server]
url = "http://127.0.0.1:3000"

[database]
path = "~/.opendocuments"      # Base directory for database files

[model]
default_workspace = "default"  # Default workspace created on system startup
active_workspace = "MyWorkspace"    # Active workspace
score_threshold = 0.60             # RAG retrieval similarity cutoff threshold
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx"
```

---

## Quick Start

This is the fastest way to run a local AI document search engine with the OpenDocuments CLI.

### 1. Install OpenDocuments

**Option A: One-line Install (Recommended)**

Download and install the pre-compiled single binary (Linux / macOS):

```bash
curl -fsSL https://raw.githubusercontent.com/cawa0505/OpenDocuments/main/install.sh | sh
```

**Option B: Build from Source**

```bash
# Clone the repository and install the unified binary to ~/.cargo/bin/opendoc
make install
```

### 2. Start the Server

```bash
opendoc start --port 3000
```

Open `http://localhost:3000` to access the Web UI and start indexing!

### 3. Command-Line Usage

You can also use the CLI to directly query and index local documents:

```bash
# Switch to a specific workspace
opendoc workspace switch "MyWorkspace"

# Index local files/folders
opendoc document index /path/to/docs

# Quick CLI query
opendoc ask "How does our auth system work?"
```

---

## ❤️ Support & Sponsorship

OpenDocuments is 100% open-source, vendor-neutral, and community-driven. If OpenDocuments saves you hardware costs, protects your document privacy, or streamlines your daily administrative workflow, consider supporting its ongoing development:

- **Solana (SOL)**: [`4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF`](https://solscan.io/account/4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF)
  ```text
  4pb8p2cTHdQb9WmU68n6AtQ3rrEHEzkQoESAXADzwKSF
  ```

### Where Sponsorship Funding Goes

- **Core Infrastructure**: Maintaining zero-dependency, ultra-fast single binary builds across Linux, macOS, and Windows.
- **Local Model Optimization**: Enhancing embedded ONNX / WASM local reranking and vector quantization for constrained devices.
- **Open-Core Guarantee**: Ensuring core RAG and MCP server capabilities remain 100% free and open-source forever.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
