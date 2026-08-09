# Task Execution Layer & Native AI Engines — Technology Verification

**Status**: Reference — verified 2026-08-09
**English** | [繁體中文](../zh-TW/task-execution-ai-engines-verification.md)
**Linked spec**: [`openspec/specs/task-execution-ai-engines/spec.md`](../../../openspec/specs/task-execution-ai-engines/spec.md)

This document records the verified external-technology constraints and the resulting
architectural decisions that drive the Task Execution Layer and Native AI Engines spec.
It exists so implementation work (Phase 0+) can cite facts with sources instead of
re-deriving them.

---

## 1. Verified Constraints (T1–T7)

| # | Constraint | Source |
|---|-----------|--------|
| T1 | AMD Spur is a **Slurm-compatible batch scheduler**, not a task-queue framework. Components: controller `spurctld` (gRPC :6817), agent `spurd` (gRPC :6818), REST gateway `spurrestd`. Pre-1.0 (v0.9.0). Tasks are shell-script batch jobs; there is no pull-based worker queue and no structured task/result protocol. | github.com/ROCm/spur |
| T2 | ONNX Runtime has **no Vulkan execution provider** (feature requests #10603, #21917 remain open). ROCm EP was **removed in ORT ≥ 1.23**; its replacement MIGraphX EP is **Instinct (CDNA) only** — consumer Radeon is not supported. The `ort` Rust crate ships no ROCm/MIGraphX prebuilt binaries. | onnxruntime.ai docs; ONNX Runtime GitHub |
| T3 | Candle (huggingface) and mistral.rs have **no AMD GPU backend** — CPU/CUDA/Metal only. Pure-Rust GPU inference on AMD via these crates does not exist. | huggingface/candle README |
| T4 | llama.cpp supports **Vulkan and HIP** backends (both Radeon and Instinct), embedding mode (`--embedding`), and a **`/rerank` endpoint** (merged, #8555 closed). bge-m3 embedding and bge-reranker-family reranking run on GGUF. | ggml-org/llama.cpp; issue #8555 |
| T5 | fastembed-rs (Anush008) covers bge-m3 (dense + sparse + ColBERT) and BGE/Jina rerankers on **ONNX CPU**. The default quantized `BGEM3Q` model is CPU-only by design. | Anush008/fastembed-rs |
| T6 | bge-m3 dense vector = 1024 × f32 ≈ 4 KB binary ≈ 10–12 KB JSON. On 1 GbE LAN, inference latency dominates; serialization is not the bottleneck. JSON-first wire format is viable for v1. | Calculated (vector size); project LAN assumption |
| T7 | Spur's internal control plane is gRPC/Protobuf; its external gateway is JSON REST. Text Embeddings Inference (TEI) exposes identical APIs over HTTP JSON and gRPC. A JSON envelope designed to map 1:1 onto a future `.proto` matches this proven pattern. | github.com/ROCm/spur; huggingface/text-embeddings-inference |

## 2. Architectural Decisions (decided with user)

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | Spur = **Batch Compute Engine** (worker lifecycle + batch ETL), **not** the LAN task-routing layer. | T1: it is a scheduler, not a task queue. |
| D2 | **Realtime RAG never goes through `spur run` per request.** Realtime uses Mode 1 (resident daemon, RPC) or InProcess. | Cold-start of `spur run` per query is unacceptable. |
| D3 | Native AI engine primary: **llama.cpp** (GGUF, Vulkan/HIP) via `llama-cpp-rs` / C FFI — embedding, re-ranking, and small-LM generation. | T4: the only native AMD GPU path that covers all three workloads. |
| D4 | Native AI engine CPU fallback: **fastembed-rs** (ONNX CPU) — zero C++ FFI, clean single-binary default. | T2/T5: ORT on AMD = CPU anyway; fastembed is the maintained CPU path. |
| D5 | LAN task payload: **JSON-first with versioned envelope** (`TaskEnvelope`), protobuf deferred until a bulk/streaming hot path justifies it. | T6/T7: JSON cost is acceptable at v1 scale; envelope is designed protobuf-mappable. |
| D6 | Generation stays pluggable: native SLM via llama.cpp when configured, otherwise existing BYOK OpenAI-compatible providers (opendoc-llm unchanged). | Backward compatibility with the BYOK contract. |
| D7 | **Config file remains `config.toml`** (XDG: `~/.config/opendocuments/config.toml`); no rename to `OpenDocuments.toml`. New `[ai]`/`[task]` sections extend it. | XDG compliance; backward compatibility; user decision 2026-08-09. |

## 3. Dead Ends Explicitly Excluded

| Path | Why excluded |
|------|--------------|
| ONNX Runtime + Vulkan EP | Does not exist (T2). |
| ONNX Runtime + ROCm on Radeon | ROCm EP removed ≥ 1.23; MIGraphX is Instinct-only (T2). |
| Candle / mistral.rs on AMD GPU | No AMD backend (T3). |
| TEI as in-process engine | Sidecar process — breaks single-binary runtime constraint (#1445/#1994). |
| Protobuf for v1 LAN payloads | Premature; JSON envelope suffices until bulk/streaming path exists (T6, D5). |

## 4. Open Items (tracked in spec §10)

- R1: llama.cpp `/rerank` output quality vs existing ONNX `bge-reranker-base.onnx` — benchmark at Phase 2 gate.
- R2: `llama-cpp-rs` C++ build cost (cmake toolchain, compile time) vs sidecar — default is feature-gated FFI.
- R3: Spur pre-1.0 HA semantics — SpurDaemonExecutor must fall back to InProcess on RPC failure.
- R4: bge-m3 sparse/ColBERT deferred; dense (dim 1024) only in v1.
- R5: ONNX reranker (fastembed) and GGUF reranker (llama.cpp) both supported behind `AiEngine` in v1.
