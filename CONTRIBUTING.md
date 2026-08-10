# Contributing to OpenDocuments

Thank you for your interest in contributing to OpenDocuments! OpenDocuments is a unified, high-performance RAG platform written in Rust.

---

## 🛠️ Development Setup

### Prerequisites

- **Rust** (1.75 or later)
- **Node.js** (20 or later) and **npm** (for building the WebUI frontend)
- **Git**

### Getting Started

```bash
# 1. Clone the repository
git clone https://github.com/cawa0505/OpenDocuments.git
cd OpenDocuments

# 2. Build and install (compiles frontend and installs the `opendoc` binary to ~/.cargo/bin/)
make install

# 3. Run unit tests across all Rust crates
cargo test --all

# 4. Start the server locally in debug/watch mode
make run
```

---

## 📁 Repository Structure

```
apps/
  webui/           - React SPA (Vite + Tailwind CSS) frontend
crates/
  opendoc-cli      - Main CLI and terminal interface (opendoc)
  opendoc-mcp      - Axum API server, SSE streaming, and MCP protocol core
  opendoc-storage  - SQLite metadata and LanceDB vector mixed retrieval store
  opendoc-llm      - OpenAI-compatible LLM client and progressively-parsed streaming
  opendoc-types    - Shared strong types (DocumentChunk, Tag, etc.)
  opendoc-parser-* - Standalone sandboxed document format parsers (PDF, DOCX, XLSX, etc.)
```

### Single Binary Ingestion
We use `rust-embed` to compile and bundle the React `dist` assets into the `opendoc` binary. There is **no need** to manually copy or reference `./dist` folders at runtime.

---

## 🛡️ Coding Conventions

- **The Small Files Principle**: Keep module files under 150 lines by leveraging Rust's module system. Use `lib.rs` / `main.rs` purely as navigation directories.
- **Strongly-Typed Custom Errors**: Eliminate raw `.unwrap()` calls. Use the centralized `OpenDocError` enum in each crate.
- **Data-Behavior Separation**: Ensure clear mental scoping by defining structure fields (data) at the top of the file, and implementing execution logic (behavior) in separate `impl` blocks further down.
- **No Unrequested Abstractions**: We adhere strictly to YAGNI (You Aren't Gonna Need It). Write the simplest possible correct code.

---

## 📝 Commit Messages

We prefer clear, concise commit messages that match standard semantic patterns:
- `feat: add knowledge-graph extraction rfc`
- `fix: resolve workspace header routing fallback`
- `docs: update deployment and auto-start guide`
