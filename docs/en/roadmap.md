# 🗺️ OpenDocuments Development Roadmap

**English** | [繁體中文](../zh-TW/roadmap.md)

This roadmap tracks the development progress of the OpenDocuments open-source project.

---

## 🎯 Phase Overview

| Phase | Title | Focus Area | Status | Target |
| :--- | :--- | :--- | :---: | :---: |
| **Phase 0** | **Core MVP & BYOK Gateway** | Axum server, BYOK LLM, current LanceDB retrieval, Tags, CLI | ⏳ In Progress | Q3 2026 |
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

## 🚀 Phase 0: Core MVP & BYOK Gateway (In Progress)

- [x] **Single-Binary Axum Architecture**: Unified server embedding React WebUI assets via `rust-embed`.
- [x] **BYOK LLM Layer**: Encrypted SQLite storage for OpenAI-compatible API keys with health-check diagnostics.
- [x] **Current Retrieval Engine**: LanceDB dense-vector search + LanceDB FTS with Reciprocal Rank Fusion (RRF).
- [ ] **Target Hybrid Retrieval**: Add core-owned SQLite FTS5 as the sparse lexical path; do not confuse LanceDB FTS with SQLite FTS5.
- [x] **LanceDB Engine Boundary**: LanceDB/Arrow/DataFusion moved into a private core-managed sidecar (spec Approved / Production).
- [x] **Tags & Complex Metadata Filtering**: Tag CRUD, document status/type filters, and multi-field sorting.
- [x] **Cross-Platform Distribution**: Installer script (`install.sh`) and GitHub Release automation.

---

## 🎨 Phase 1: ChatGPT-Aligned WebUI & Streaming (Active)

- [x] **BYOK Provider Configuration UI**: Full provider management UI in `SettingsPage.tsx`.
- [x] **Light Mode Visual Optimization**: Standardized clean Light Mode UI.
- [x] **SSE Stream Event Normalization**: Standardized `StreamEvent` handling (`chunk`, `sources`, `confidence`, `done`, `error`).
- [x] **Interactive Citation Linking**: Clickable `[1]` / `[2]` citation tags in Markdown response linking directly to source cards.
- [x] **Query Profiles**: User-selectable search profiles (`Fast`, `Balanced`, `Precise`). `[待討論] 實際檢索流程差異` 另行討論中。

---

## 🎯 v1.0.0 Scope

**v1.0.0 = Phase 0 (Core MVP) + Phase 1 (WebUI) completion.** Single-machine deployment only.

- **Primary focus: WebUI + API.** Every chat feature must actually work — correct end-to-end search from the WebUI is the release gate, not compilation.
- **File identity: reference full paths.** Documents are located by absolute `source_path` (no relative-name or virtual-file indirection); future multi-machine storage extends this, it does not replace it.

The acceptance bar for v1.0.0: `cargo check` zero warnings → install → real upload → real search → chat round-trip verified in the WebUI, for every listed feature below.

- [ ] **Activity-log contract**: fix total/pagination/DTO alignment, workspace-scope feedback updates, and add delete API/UI.
- [ ] **GitHub connector contract**: implement the create and sync routes already called by the WebUI, with workspace isolation for every operation.
- [ ] **CLI index sync integration tests**: verify empty directories, nested paths, and cross-workspace deletion isolation; hash deduplication, changed-file re-upload, and deletion pruning have passed code audit.

---

## 🗄️ Beyond v1.0.0 (Backlog)

Features explicitly deferred past the v1.0.0 release, tracked here so they are not forgotten. Each item links to its governing spec where one exists.

| Feature | Spec | Notes |
| :--- | :--- | :--- |
| **Target Hybrid Retrieval — SQLite FTS5** | [`hybrid-rag-retrieval`](../../openspec/specs/hybrid-rag-retrieval/spec.md) | Core-owned sparse lexical path; enables pure-lexical fallback when the engine is unavailable. LanceDB FTS remains the active lexical path until then. |
| **Phase 2 — Task Execution Layer & Native AI Engines** | [`task-execution-ai-engines`](../../openspec/specs/task-execution-ai-engines/spec.md) | llama.cpp (Vulkan/HIP) embed/rerank/infer, `opendoc-ai-fastembed` process boundary, generation switch via `[ai.models.inference]`. |
| **Spur integration & server/worker mode** | deferred note #33 | `SpurDaemonExecutor` (Mode 1), `opendoc-worker daemon` + scale-to-zero (Mode 3), batch ETL (Mode 2); LAN workers on private networks. Transport-agnostic stdio JSON-RPC (future TCP/unix socket); engine config independent from core. NOTE: #2039 forbids Docker deployment — server/worker is the future direction, not containers. |
| **WebUI redesign** | — | WebUI redesign is not ruled out; evaluated only after v1.0.0 GA. Current WebUI is the v1.0.0 deliverable — every chat feature must actually work end-to-end (see Scope). |
| **Binary size reduction** | [`binary-size-architecture`](../../openspec/specs/binary-size-architecture/spec.md) | Engine is 354 MB (strip+LTO); zero-behavior-change slimming backlog. |
| **FastEmbed process boundary** | `task-execution-ai-engines` | Currently feature-gated inside `opendoc-storage` (`embedding-fastembed`); may move into the engine/worker boundary later. |
| **LanceDB S3 backend** | — | Use case: a school with an IT admin deploys LAN storage on older machines (S3-compatible services such as Garage / SeaweedFS); LanceDB table over S3 instead of local disk. |
| **S3 object store as NAS replacement** | — | When LanceDB scales multi-machine, the app can be configured to also store physical files to an in-LAN S3 service, building a "NAS + AI Search" foundation that replaces a conventional NAS. |
