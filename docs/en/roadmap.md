# 🗺️ OpenDocuments Development Roadmap

**English** | [繁體中文](../zh-TW/roadmap.md)

This roadmap tracks the development progress of the OpenDocuments open-source project.

---

## 🎯 Phase Overview

| Phase | Title | Focus Area | Status | Target |
| :--- | :--- | :--- | :---: | :---: |
| **Phase 0** | **Single-Binary MVP & BYOK Gateway** | Single-binary Axum server, BYOK LLM, Hybrid RAG (LanceDB + FTS5), Tags, CLI/TUI | ✅ Completed | Q3 2026 |
| **Phase 1** | **ChatGPT-Aligned WebUI & Streaming** | React 19 WebUI, Typewriter SSE, Citation linking, Markdown code highlighting | ⏳ In Progress | Q3 2026 |
| **Phase 2** | **Task Execution Layer & Native AI Engines** | Decouple parse/embed/rerank/infer via `TaskExecutor`; llama.cpp (Vulkan/HIP) + fastembed (CPU); optional Spur batch compute | 📋 Planned | Q4 2026 |

---

## 🚀 Phase 2: Task Execution Layer & Native AI Engines (Planned)

Spec: [`openspec/specs/task-execution-ai-engines`](../../openspec/specs/task-execution-ai-engines/spec.md)
Reference: [`docs/ref/en/task-execution-ai-engines-verification.md`](../ref/en/task-execution-ai-engines-verification.md)

- [ ] **Phase 0 — Baseline hardening**: async `SearchBackend` signature, `[ai]`/`[task]` config parsing, call-site audit.
- [ ] **Phase 1 — Task & AI abstractions (CPU)**: `opendoc-task`/`opendoc-ai`/`opendoc-ai-fastembed`; upload→embed→LanceDB; real `LanceDbRetriever`.
- [ ] **Phase 2 — llama.cpp GPU backend**: `opendoc-ai-llamacpp` (feature-gated, Vulkan/HIP); embed/rerank/infer; runtime fallback.
- [ ] **Phase 3 — Spur integration (optional)**: `SpurDaemonExecutor` (Mode 1), `opendoc-worker daemon` + scale-to-zero (Mode 3), batch ETL (Mode 2).
- [ ] **Phase 4 — Generation switch**: native SLM via llama.cpp when `[ai.models.inference]` present; BYOK unchanged otherwise.

---

## 🚀 Phase 0: Single-Binary MVP & BYOK Gateway (Completed)

- [x] **Single-Binary Axum Architecture**: Unified server embedding React WebUI assets via `rust-embed`.
- [x] **BYOK LLM Layer**: Encrypted SQLite storage for OpenAI-compatible API keys with health-check diagnostics.
- [x] **Hybrid RAG Retrieval Engine**: Dense vector similarity (LanceDB) + Sparse full-text search (SQLite FTS5) with Reciprocal Rank Fusion (RRF).
- [x] **Tags & Complex Metadata Filtering**: Tag CRUD, document status/type filters, and multi-field sorting.
- [x] **Cross-Platform Distribution**: Installer script (`install.sh`) and GitHub Release automation.

---

## 🎨 Phase 1: ChatGPT-Aligned WebUI & Streaming (Active)

- [x] **BYOK Provider Configuration UI**: Full provider management UI in `SettingsPage.tsx`.
- [x] **Light Mode Visual Optimization**: Standardized clean Light Mode UI.
- [ ] **SSE Stream Event Normalization**: Standardized `StreamEvent` handling (`Thought`, `Text`, `Status`).
- [ ] **Interactive Citation Linking**: Clickable `[1]` / `[2]` citation tags in Markdown response linking directly to source cards.
- [ ] **Query Profiles**: User-selectable search profiles (`Fast`, `Balanced`, `Precise`).
