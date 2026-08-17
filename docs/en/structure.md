# 🗂️ OpenDocuments System Architecture & Directory Map

**English** | [繁體中文](../zh-TW/structure.md)

This document defines the system architecture, repository directory layout, and data flow specifications for the OpenDocuments open-source RAG engine and desktop client ecosystem.

---

## 🌐 1. Strategic Positioning

OpenDocuments adopts an **Open-Core + Desktop Control Center** architecture:
1. **OpenDocuments Core (Server + WebUI)**: A 100% local, zero-trust RAG infrastructure offering a ChatGPT/Gemini-aligned conversational WebUI. Built for developers, privacy advocates, and security officers requiring strict data sovereignty.
2. **Desktop Client (Tauri 2.0)**: A lightweight administrative workspace featuring a three-column control cabin ("Drag & Drop, In-Place Editing, Security Interception, One-Click Publishing") tailored for operational efficiency.

---

## 📦 2. Repository Directory Map

```plaintext
OpenDocuments/ (Repository Root)
├── apps/                            # Frontend Applications
│   └── webui/                       # OpenDocuments WebUI (React 19 + Tailwind CSS)
│                                    # ─ ChatGPT/Gemini-aligned chat flow, Markdown rendering, Light Mode default
│
├── crates/                          # Modular Rust Workspace (Cargo Workspace)
│   ├── opendoc-cli/                 # Main CLI binary entry point (main.rs) with background daemon
│   ├── opendoc-mcp/                 # Axum routes, SSE streaming, MCP protocol, and embedded WebUI assets
│   ├── opendoc-storage/             # SQLite relational storage and LanceDB Sidecar client
│   ├── opendoc-engine-lancedb/      # LanceDB vector engine Sidecar daemon, isolating heavy Arrow/Lance dependencies
│   ├── opendoc-task/                # Task Execution Layer (TaskEnvelope, TaskResult, TaskExecutor, InProcessExecutor)
│   ├── opendoc-ai/                  # Native AI Engine abstraction (AiEngine, EngineConfig, HardwareBackend)
│   ├── opendoc-llm/                 # OpenAI-compatible BYOK client with SSE stream parsing
│   ├── opendoc-types/               # Shared strongly-typed data models (DocumentChunk, Tag, etc.)
│   └── opendoc-parser-*/            # Sandboxed file parsers (PDF, DOCX, XLSX, HTML, Email, Jupyter)
│
├── docs/                            # Internal Architecture, Roadmap, Tasks & Manuals
│   ├── en/                          # English Documentation
│   │   ├── structure.md             # System Architecture & Directory Map
│   │   ├── roadmap.md               # Multi-phase R&D Roadmap
│   │   ├── tasks.md                 # Task Backlog & Execution Tracking
│   └── zh-TW/                       # Traditional Chinese Synchronized Documentation
│       ├── structure.md
│       ├── roadmap.md
│       └── tasks.md
│
├── docs-site/                       # Documentation Site Source (VitePress)
├── openspec/                        # System Behavior Specifications (OpenSpec 1.7)
├── scripts/                         # Maintenance and Audit Tooling
├── install.sh                       # One-line Cross-Platform Installation Script
└── README.md                        # Master Project Index & Quick Start (English)
```

---

## 🛠️ 3. Backend Rust Core Principles

1. **Single Binary Architecture**:
   Eliminates external Node.js dependencies. SQLite database operations, LanceDB vector search, LLM connectivity, HTTP REST/SSE services, and embedded React WebUI assets are bundled into a single native Rust process.
2. **Module Isolation & Low Footprint**:
   Strict Cargo Workspace modularization ensuring small file sizes, low compilation overhead, and minimal runtime memory (~18MB baseline).
3. **Secrets Isolation (BYOK)**:
   API keys are encrypted in SQLite local tables with 600 filesystem permissions and loaded only into memory at request time. No OS-level environment variable persistence.

---

## 📐 4. Data Flow & Retrieval Architecture

### 4.1 Document Ingestion Flow
```plaintext
Local File ──> Parser (Text/Table/PDF/DOCX)
  ──> Semantic Splitter (Chunking)
  ──> Embedder (Vectorization)
  ──> Dual Storage (SQLite Attributes + LanceDB Vectors)
```

### 4.2 Hybrid Retrieval Query Flow
```plaintext
User Query ──> Query Embedder
  ──> Dual Parallel Retriever (LanceDB Vector Similarity + LanceDB Full-Text Search)
  ──> Reciprocal Rank Fusion (RRF Reranker)
  ──> Context Window Assembler
  ──> LLM Generator (BYOK Streaming) ──> SSE Response
```

### 4.3 Hybrid Search (Dense + Sparse)
Combines LanceDB dense vector embeddings with LanceDB full-text keyword matching, fused via Reciprocal Rank Fusion (RRF) for optimal relevance and terminology precision.
