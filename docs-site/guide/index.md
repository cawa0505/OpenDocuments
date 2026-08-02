# Quick Start Guide

Get OpenDocuments running in under 5 minutes.

## Prerequisites

- **Rust toolchain (1.80+)** — installed via [rustup.rs](https://rustup.rs)
- **Make** — standard build utility (usually pre-installed on Linux/macOS)
- **Ollama** (optional, for local LLM running via OpenAI-compatible BYOK setup) — [download](https://ollama.com)

## Install from Source

To compile and install the high-performance unified `opendoc` binary (which embeds the frontend WebUI directly):

```bash
# Clone the repository
git clone https://github.com/cawa0505/OpenDocuments.git
cd OpenDocuments

# Build the WebUI and install the Rust CLI automatically
make install
```

This compiles and places the `opendoc` binary in your user's cargo binary path (e.g., `~/.cargo/bin` or `%USERPROFILE%\.cargo\bin`), which is automatically available in your PATH.

---

## Start the Server

```bash
opendoc start --port 3000
```

Open [http://localhost:3000](http://localhost:3000) — you will see a beautiful chat UI, document manager, and admin stats dashboard. The frontend is served directly from the memory of the `opendoc` binary!

---

## Workspace & Document Indexing

OpenDocuments works on **Workspaces** (isolated folders with independent SQLite databases and LanceDB vector stores).

```bash
# Index a local directory into your active workspace
opendoc document index /path/to/docs
```

Or simply drag-and-drop your files directly onto the Web UI!

---

## Ask Questions in Terminal (TUI)

OpenDocuments features a high-performance terminal chat interface (TUI) for local operations:

```bash
opendoc tui
```

---

## Next Steps

- [Deployment & Auto-Start](/guide/deployment) — run persistently on Windows, macOS, and Linux
- [Architecture](/guide/architecture) — understand the single-binary Rust design
- [MCP Server](/guide/mcp-knowledge-base) — hook OpenDocuments up as a local knowledge base for Claude Code and Cursor
