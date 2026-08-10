# OpenSpec Requirement: Task Execution Layer & Native AI Engines

**Spec ID**: `task-execution-ai-engines`
**Status**: Draft — Pending Approval
**Priority**: P1
**Primary Language**: English
**Last Updated**: 2026-08-09

---

## 1. Overview & Core Objective

Decouple hardware-bound operations (Parsing, Embedding, Re-ranking, Inference) from the
OpenDocuments API server into a **Task Execution Layer** with two execution modes:

- **InProcess** — engine runs inside the single Axum binary (default homelab deployment).
- **Spur** — AMD Spur (Slurm-compatible batch scheduler) manages long-lived worker
  processes on LAN GPU nodes; the API server talks to them over RPC.

Move away from external LLM providers for Embedding and Re-ranking toward **native Rust AI
engines**: llama.cpp (GGUF, Vulkan/HIP) as the GPU-capable primary, fastembed-rs
(ONNX CPU) as the no-FFI CPU fallback. Generation remains pluggable: native SLM via
llama.cpp or existing BYOK OpenAI-compatible providers.

**Guiding constraints (project-wide, must not be violated):**

- Single Rust Axum binary process as the unified backend; no Node.js runtime processes
  (#1445, #1994, #2217 — WebUI is npm-built then embedded, runtime is one binary).
- No hardcoded hostnames / private IPs / mock data in RAG flows (#2236, #2237); all
  retrieval dynamic against SQLite + vector store, empty `Vec::new()` on no match.
- Zero-warning compilation; `cargo check` must stay 100% clean.
- Config lives at `~/.config/opendocuments/config.toml` (XDG); this spec extends it.

---

## 2. Verified Technology Constraints (research date: 2026-08-09)

These are hard facts that shape the design. Sources: onnxruntime.ai docs, github.com/ROCm/spur,
rocm.blogs.amd.com, huggingface/candle README, Anush008/fastembed-rs, ggml-org/llama.cpp.

| # | Constraint | Implication |
|---|-----------|-------------|
| T1 | AMD Spur is a **Slurm-compatible batch scheduler** (controller `spurctld` gRPC :6817, agent `spurd` :6818, REST gateway `spurrestd`), pre-1.0 (v0.9.0). Tasks are shell-script batch jobs; no pull-based worker queue, no structured task/result protocol. | Spur = **Batch Compute Engine** for worker lifecycle + batch ETL. Realtime RAG must NOT go through `spur run` per request. Use RPC to resident workers (Mode 1) or in-process (InProcess mode). |
| T2 | ONNX Runtime has **no Vulkan execution provider** (never shipped; feature requests #10603, #21917 open). ROCm EP **removed in ORT ≥ 1.23**; replacement MIGraphX EP is **Instinct (CDNA) only** — Radeon not supported. `ort` crate ships no ROCm/MIGraphX prebuilt binaries. | ort on AMD = CPU out of the box. Vulkan/ROCm for ONNX is a dead end on consumer Radeon. |
| T3 | Candle and mistral.rs have **no AMD GPU backend** (CPU/CUDA/Metal only). | Pure-Rust GPU inference on AMD via candle/mistral.rs does not exist. |
| T4 | llama.cpp supports **Vulkan and HIP** backends (both Radeon and Instinct), embedding mode, and a `/rerank` endpoint (merged). bge-m3 embedding runs via `--embedding`; bge-reranker family runs via rerank API. | llama.cpp is the realistic native AMD GPU path for embed / rerank / small LM, integrated via `llama-cpp-rs` (or C FFI). |
| T5 | fastembed-rs covers bge-m3 (dense+sparse+ColBERT) and rerankers (BGE/Jina) on **ONNX CPU**; default quantized `BGEM3Q` is CPU-only by design (GPU EP fails). | Clean CPU fallback with zero C++ FFI; dimension must match LanceDB schema (1024). |
| T6 | LAN task payload: bge-m3 dense vector = 1024 × f32 ≈ 4 KB binary ≈ 10–12 KB JSON. On 1 GbE the bottleneck is inference, not serialization. | **JSON-first** with versioned envelope; protobuf/gRPC deferred until bulk/streaming path justifies it. |
| T7 | Spur's internal control plane is gRPC/Protobuf; external gateway is JSON REST. TEI exposes identical API over both HTTP JSON and gRPC. | Dual-wire pattern is proven; our JSON envelope should be designed to map 1:1 onto a future `.proto`. |

---

## 3. System Architecture

### 3.1 High-Level Diagram

```
                    ┌─────────────────────────────────────────────┐
                    │        OpenDocuments API Server (Axum)       │
                    │   opendoc-mcp  — single binary, RustEmbed     │
                    │   SQLite (metadata)  +  LanceDB (vectors)     │
                    └───────────────┬─────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────────────────────┐
                    │            Task Execution Layer                │
                    │         TaskExecutor trait (v2 dispatch)       │
                    └───────┬─────────────────────────┬─────────────┘
                            │                         │
              ┌─────────────┴──────────┐   ┌──────────┴─────────────┐
              │  Mode A: InProcess     │   │  Mode B/C: Spur Batch   │
              │  (default, single-box) │   │  Compute Engine          │
              │                        │   │                          │
              │  opendoc-worker  lib   │   │  spurctld (controller)   │
              │  embedded via          │   │     │ gRPC/REST          │
              │  AiEngine trait        │   │     ▼                   │
              │                        │   │  spurd on node N         │
              │  llama.cpp (GGUF,      │   │  ├─ Mode B: long-lived   │
              │   Vulkan/HIP/CPU)      │   │  │  opendoc-worker-daemon│
              │  fastembed-rs (CPU)    │   │  │  (VRAM-resident, RPC) │
              │                        │   │  └─ Mode C: batch jobs   │
              │                        │   │     opendoc-worker batch │
              └────────────────────────┘   └─────────────────────────┘
```

### 3.2 Spur Integration Modes (as decided)

| Mode | Use case | Mechanism | Notes |
|------|----------|-----------|-------|
| **1. Long-lived Worker (Daemon)** | Realtime RAG query / embedding / rerank | `spur run --gpus 1 -- opendoc-worker daemon --port 50051 --model bge-m3` at API startup; API talks RPC (JSON over HTTP/Unix socket) to resident worker | Zero cold-start; models loaded once in VRAM; Spur supervises lifecycle/HA (restarts worker on failure on any free-GPU node) |
| **2. Offline Batch ETL** | Bulk ingestion (thousands of files) | `spur run -- opendoc-worker batch --batch-id 001 --manifest ...` — one job per batch, isolated OS process, exits after write to vector DB | Crashes contained per job; no API-server memory/threads consumed |
| **3. Scale-to-Zero (On-demand)** | HomeLab power saving | No resident worker; on ingestion/search trigger, `spur run` starts worker with idle-timeout (e.g. 300 s) that exits and returns VRAM when idle | GPU VRAM = 0 when no tasks; cold-start accepted for batch ingest, avoided for realtime (realtime uses Mode 1) |

Rule: **Realtime path (search/rerank/infer) never goes through `spur run` per request.**
`spur run` is used only to (a) start/manage resident daemons, (b) submit batch ETL jobs.

---

## 4. Trait Contracts

### 4.1 Task Envelope (versioned JSON — protobuf-mappable)

```rust
// crate: opendoc-task
/// Wire protocol v1. Designed so the envelope maps 1:1 onto a future .proto:
///   message TaskEnvelope { uint32 version = 1; string task_id = 2; string task_type = 3; ... }
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEnvelope {
    pub version: u32,                 // wire version, currently 1
    pub task_id: String,              // UUID
    pub task_type: TaskType,          // Embed | Rerank | Infer | Parse
    pub workspace_id: String,
    pub model_ref: String,            // key into config [ai.models.*]
    pub payload: serde_json::Value,   // typed per TaskType (see §7.3)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskType { Embed, Rerank, Infer, Parse }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,           // completed | failed | cancelled
    pub output: serde_json::Value,    // per TaskType; never mock data — dynamic real results only
    pub error: Option<String>,
    pub node_id: String,              // which worker produced it
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}
```

### 4.2 TaskExecutor

```rust
// crate: opendoc-task
use async_trait::async_trait;

/// Dispatches tasks either in-process or to a Spur-managed worker.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Submit one task and await its result (realtime path).
    async fn execute(&self, task: TaskEnvelope) -> Result<TaskResult, TaskError>;

    /// Submit a batch for offline processing (fire-and-forget; Mode 2/3).
    async fn submit_batch(&self, tasks: Vec<TaskEnvelope>) -> Result<Vec<String>, TaskError>; // returns task_ids

    fn mode(&self) -> ExecutorMode;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorMode { InProcess, SpurDaemon, SpurBatch }

// Implementations:
// - InProcessExecutor      — routes tasks straight to the local AiEngine (single binary).
// - SpurDaemonExecutor     — RPC client to a resident opendoc-worker daemon (Mode 1).
// - SpurBatchExecutor      — submits `opendoc-worker batch` jobs via spurctld/spurrestd (Mode 2/3).
```

### 4.3 AiEngine (native inference backends)

```rust
// crate: opendoc-ai
use async_trait::async_trait;

/// Hardware execution provider selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HardwareBackend { Cpu, Vulkan, Hip }

/// One model instance: file + runtime backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineConfig {
    pub model_path: String,           // GGUF or ONNX file path
    pub backend: HardwareBackend,     // runtime-selected: Vulkan → HIP → CPU fallback
    pub device_id: Option<u32>,       // GPU index (Vulkan/HIP)
    pub threads: Option<u32>,         // CPU threads
    pub embedding_dim: usize,         // must match LanceDB compat schema (1024 for bge-m3)
}

/// Unified native AI backend: llama.cpp (GGUF, Vulkan/HIP/CPU) and fastembed-rs (ONNX CPU).
#[async_trait]
pub trait AiEngine: Send + Sync {
    fn engine_kind(&self) -> EngineKind;              // LlamaCpp | FastEmbed
    fn backend(&self) -> HardwareBackend;             // active provider

    /// Embed a batch of texts → dense vectors.
    async fn embed(&self, texts: Vec<String>, config: &EngineConfig) -> Result<Vec<Vec<f32>>, EngineError>;

    /// Re-rank query against candidate chunks → (index, score) pairs, top-k.
    async fn rerank(&self, query: &str, candidates: &[opendoc_types::DocumentChunk], config: &EngineConfig, top_k: usize)
        -> Result<Vec<(usize, f32)>, EngineError>;

    /// Native SLM generation (llama.cpp only; fastembed returns Unsupported).
    async fn infer(&self, messages: Vec<opendoc_llm::ChatMessage>, config: &EngineConfig, opts: &opendoc_llm::CompletionOptions)
        -> Result<String, EngineError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind { LlamaCpp, FastEmbed }
```

### 4.4 SearchBackend evolution

Current trait (sync, stub-returning) must become async and go through the TaskExecutor:

```rust
// BEFORE (crates/opendoc-mcp/src/lib.rs:64)
pub trait SearchBackend: Send + Sync {
    fn search_and_rerank(&self, query: &str, threshold: f32) -> Vec<opendoc_types::DocumentChunk>;
}

// AFTER
#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn search_and_rerank(&self, query: &str, threshold: f32) -> Vec<opendoc_types::DocumentChunk>;
}
```

- `McpState.search` stays `Arc<dyn SearchBackend>` — only the trait method becomes async;
  all call sites (`lib.rs:187`, `:441`, CLI `SearchWrapper`) move to `.await`.
- The real implementation (`opendoc_storage::ConfigManager::search_and_rerank`, currently
  returning `Vec::new()`) is replaced by `LanceDbRetriever` (§5.3) which: vector search in
  LanceDB → optional rerank via AiEngine → SQLite FTS5 merge → RRF.
- Empty result stays `Vec::new()` (#2237); **no static fallback chunks ever**.

---

## 5. Worker CLI: `opendoc-worker`

New binary crate `opendoc-worker`. Subcommands map 1:1 to `TaskType`:

```text
opendoc-worker embed   --model bge-m3     --input chunks.jsonl --output vectors.jsonl
opendoc-worker rerank  --model reranker   --query "..." --candidates candidates.jsonl --top-k 20
opendoc-worker infer   --model qwen2.5-3b --prompt "..." [--stream]
opendoc-worker parse   --file x.pdf --workspace <id> [--collection <id>]

# Mode 1 — resident daemon (VRAM-loaded model, JSON-RPC over HTTP/Unix socket)
opendoc-worker daemon  --listen 127.0.0.1:50051 --model bge-m3 [--rerank-model reranker] [--idle-timeout 300]
                       # idle-timeout > 0 ⇒ Mode 3 scale-to-zero; 0 ⇒ resident forever

# Mode 2/3 — batch job entrypoint (invoked by spur run)
opendoc-worker batch   --manifest batch-001.json
```

- CLI and daemon share one `App` struct over the same `AiEngine` instances (one model load).
- Daemon speaks the §4.1 JSON envelope over HTTP POST `/task` (or Unix socket) — no gRPC in v1.
- On startup, daemon performs hardware capability probe: Vulkan → HIP → CPU, per configured model.

---

## 6. Configuration Schema (extends `~/.config/opendocuments/config.toml`)

Add `[ai]` and `[task]` sections. Existing `[model]` keys stay untouched for backward compat.

```toml
# --- existing, unchanged ---
[server]
url = "http://127.0.0.1:3000"
api_key = ""

[database]
path = "~/.opendocuments"

[model]
default_workspace = "default"
active_workspace = "OpenDocuments"
score_threshold = 0.60
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx"   # legacy, superseded by [ai.models.reranker]

# --- new ---
[task]
executor = "inprocess"        # inprocess | spur_daemon | spur_batch
# transport = "http"          # reserved: realtime RPC transport (http | unix)

[task.spur]                   # only consulted when executor != "inprocess"
controller_url = "http://127.0.0.1:6817"     # spurctld gRPC (or spurrestd REST base URL)
partition = "default"
daemon_idle_timeout_seconds = 300            # Mode 3 scale-to-zero
daemon_port = 50051                          # resident worker RPC port

[ai]
preferred_backend = "vulkan"   # vulkan | hip | cpu — runtime fallback vulkan→hip→cpu
# prefer_ffi = true           # llama.cpp via llama-cpp-rs (Vulkan/HIP); false → CPU fastembed only

[ai.models.embedding]
name = "bge-m3"
engine = "llamacpp"            # llamacpp | fastembed
model_path = "~/.opendocuments/models/bge-m3-ggml-model-f16.gguf"
backend = "vulkan"             # per-model override; "auto" → [ai] preferred_backend
dimensions = 1024              # MUST match LanceDB compat schema

[ai.models.reranker]
name = "bge-reranker-v2-m3"
engine = "llamacpp"
model_path = "~/.opendocuments/models/bge-reranker-v2-m3-q4_k_m.gguf"
backend = "vulkan"

[ai.models.inference]         # optional; absent ⇒ generation stays BYOK (opendoc-llm providers)
name = "qwen2.5-3b-instruct"
engine = "llamacpp"
model_path = "~/.opendocuments/models/qwen2.5-3b-instruct-q4_k_m.gguf"
backend = "vulkan"
context_size = 8192

# Legacy BYOK providers table (SQLite llm_providers) remains the generation fallback
# when [ai.models.inference] is absent or engine fails. (BYOK keys stay in SQLite, 600 perms — #1882)
```

### 6.1 Config evolution rules

1. `[model].local_reranker_path` is read for backward compat; new installs write `[ai.models.reranker]`.
2. Missing `[ai]` section ⇒ InProcess executor with CPU fastembed fallback — zero new config required.
3. Model list is data, not code: adding a model = one `[ai.models.*]` block. No hardcoded model registry.

---

## 7. Pipeline Workflow (as decided)

### 7.1 Embedding (ingest path)

```
Upload/CLI index → opendoc_parser::parse_file → chunks
  → TaskEnvelope{embed} → TaskExecutor
     → InProcess: AiEngine.embed (llama.cpp Vulkan, or fastembed CPU)
       / SpurDaemon: RPC to resident worker
       / SpurBatch: opendoc-worker batch (Mode 2, isolated process)
  → vectors → LanceDB write (compat schema, dim 1024) + SQLite documents row (existing upload handler)
```

### 7.2 Retrieval + Re-ranking (query path — realtime, never per-request `spur run`)

```
Search query (API/MCP)
  → LanceDB dense vector search (query embedded via AiEngine.embed)
  → SQLite FTS5 sparse search (target; not currently implemented)
  → merge candidates
  → TaskEnvelope{rerank} → AiEngine.rerank (llama.cpp /rerank or fastembed reranker)
  → score threshold filter ([model].score_threshold) + top-k
  → opendoc_types::DocumentChunk results (empty Vec::new() on no match)
```

### 7.3 Generation (optional native SLM)

```
[ai.models.inference] present?
  → yes: TaskEnvelope{infer} → llama.cpp (native SLM short answers)
  → no : existing opendoc-llm BYOK providers (DeepSeek/Moonshot/OpenRouter/Ollama) — unchanged
```

---

## 8. Proposed Crate Layout

New crates (extracted from the monorepo pattern already in use):

```text
crates/
  opendoc-task/        # NEW — TaskEnvelope/TaskResult, TaskExecutor trait, ExecutorMode, TaskError
  opendoc-ai/          # NEW — AiEngine trait, EngineConfig, HardwareBackend, engine dispatch (kind→impl)
  opendoc-ai-llamacpp/ # NEW — llama.cpp integration via llama-cpp-rs (Vulkan/HIP/CPU); embed/rerank/infer
  opendoc-ai-fastembed/# NEW — fastembed-rs CPU wrapper (embed + rerank); zero C++ FFI
  opendoc-worker/      # NEW — binary: embed/rerank/infer/parse/daemon/batch subcommands
  opendoc-mcp/         # MOD — SearchBackend async; McpState gains Option<Arc<dyn TaskExecutor>>
  opendoc-storage/     # MOD — LanceDbRetriever (§5.3); ConfigManager gains [ai]/[task] parsing
  opendoc-llm/         # UNCHANGED — BYOK OpenAI-compatible client stays as generation fallback
```

Dependency direction: `opendoc-ai` ← `opendoc-ai-llamacpp`, `opendoc-ai-fastembed`;
`opendoc-worker` → all engine crates + `opendoc-task`; `opendoc-mcp` → `opendoc-task` + `opendoc-ai`
(optional, behind the executor abstraction). **No circular deps** (#2102).

`llama-cpp-rs` is an optional cargo feature of `opendoc-ai-llamacpp` (C++ build, cmake);
default build stays pure-Rust CPU via fastembed.

---

## 9. Refactoring Roadmap

Phased; each phase ends with the project verification cycle:
`cargo check` (zero warnings) → `cargo build` → `cargo install --path crates/opendoc-cli --force`
→ restart `opendoc-server` → HTTP contract verify → WebUI flow verify (#1556, #1577).

### Phase 0 — Baseline hardening (no behavior change)
- [ ] Audit current `search_and_rerank` call sites (mcp lib.rs:187/441, CLI SearchWrapper).
- [ ] Write failing unit tests for async `SearchBackend` signature change.
- [ ] Add `[ai]`/`[task]` parsing to `AppConfig` with `#[serde(default)]` (backward compat).

### Phase 1 — Task & AI abstractions (pure Rust, CPU)
- [ ] `opendoc-task`: TaskEnvelope/TaskResult/TaskExecutor + `InProcessExecutor`.
- [ ] `opendoc-ai`: AiEngine trait + `EngineConfig` + hardware probe (Vulkan→HIP→CPU).
- [ ] `opendoc-ai-fastembed`: bge-m3 embed + reranker on ONNX CPU (dim 1024).
- [ ] Wire upload handler: parse → embed (fastembed CPU) → LanceDB write (compat schema).
- [x] Replace stub `search_and_rerank` with `LanceDbRetriever` (LanceDB vector + LanceDB FTS + RRF + threshold).
- [ ] Add core-owned SQLite FTS5 as the target sparse lexical path.
- Gate: real documents round-trip — index → query returns actual chunks from DB, empty on no match.

### Phase 2 — llama.cpp GPU backend
- [ ] `opendoc-ai-llamacpp` (feature-gated, C++ build): embed + rerank + infer via Vulkan/HIP.
- [ ] Hardware probe + runtime fallback; per-model `backend` override honored.
- [ ] Model conversion doc: bge-m3/bge-reranker GGUF sourcing; dim 1024 verified against LanceDB.
- Gate: same round-trip on Vulkan backend; CPU fallback still functional when GPU absent.

### Phase 3 — Spur integration (optional, multi-node)
- [ ] `SpurDaemonExecutor` (Mode 1): RPC client to resident `opendoc-worker daemon`.
- [ ] `opendoc-worker daemon` + idle-timeout (Mode 3); spawn via `spur run` documented in `docs/`.
- [ ] `SpurBatchExecutor` (Mode 2): batch manifest + `opendoc-worker batch`; `spur run` submission.
- [ ] Config: `[task.spur]` honored; executor hot-switchable via config without recompile.
- Gate: daemon survives worker crash (Spur restarts it); batch ETL of 1000+ files without API-server memory growth.

### Phase 4 — Generation switch
- [ ] `TaskEnvelope{infer}` → llama.cpp SLM when `[ai.models.inference]` present; else BYOK unchanged.
- [ ] Chat handlers (mcp handlers/chat.rs) route through executor abstraction.

---

## 10. Risks & Open Questions

| # | Item | Status |
|---|------|--------|
| R1 | llama.cpp `/rerank` endpoint maturity for bge-reranker GGUF — verify quality vs existing ONNX `bge-reranker-base.onnx` (config `local_reranker_path`) | [待討論] benchmark at Phase 2 gate |
| R2 | `llama-cpp-rs` C++ build adds compile time & toolchain req (cmake) — acceptable? Alternative: run llama.cpp as OpenAI-compatible sidecar (breaks single-binary runtime) | [待討論] default = feature-gated FFI; sidecar rejected unless FFI proves unmaintainable |
| R3 | Spur pre-1.0 (v0.9.0) — HA/restart semantics still stabilizing | Mitigate: SpurDaemonExecutor treats worker as optional; falls back to InProcess on RPC failure (degraded but alive) |
| R4 | bge-m3 sparse/ColBERT representations — v1 embeds dense only (dim 1024); sparse/ColBERT deferred | [待討論] hybrid FTS5 already covers lexical; revisit if recall gap appears |
| R5 | `[model].local_reranker_path` (ONNX) vs `[ai.models.reranker]` (GGUF) — keep both engines supported behind AiEngine? | [待討論] yes in v1 (fastembed CPU can load existing ONNX), llama.cpp for GPU |
| R6 | Batch ETL vector-write concurrency (LanceDB write contention with realtime reads) | Design: batch jobs write via same LanceDB API in-process; document locking/retry at Phase 3 |
| R7 | Config filename: spec says `config.toml` (XDG, existing); original request said `OpenDocuments.toml` | ✅ **DECIDED 2026-08-09**: keep `config.toml`, no rename — XDG compliance wins |

---

## 11. Compliance Checklist

- [ ] Single Rust Axum binary backend preserved (InProcess mode default) — #1445/#1994.
- [ ] RAG dynamic only; empty `Vec::new()` on no match; no mock/static chunks — #2236/#2237.
- [ ] BYOK keys stay in SQLite `llm_providers` (600 perms), never exposed — #1882.
- [ ] Zero-warning `cargo check` after every phase — AGENTS.md §1.
- [ ] Every phase verified: install → restart → HTTP contract → WebUI — #1556/#1577.
- [ ] No hardcoded hostnames/IPs in any example config — all use `127.0.0.1` (T1, #2236).
