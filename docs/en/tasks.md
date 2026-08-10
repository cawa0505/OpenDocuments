# 📋 OpenDocuments Task Backlog & Execution Status

**English** | [繁體中文](../zh-TW/tasks.md)

This document tracks execution status and concrete verification criteria across all active Phase 1 engineering work packages.

---

## 🚨 Active Execution Items (Phase 1)

### 1.1 WebUI & RAG Streaming Improvements

- [x] **1.1.1 BYOK Management Interface**: Complete `SettingsPage.tsx` LLM provider management with connection diagnostic tests.
- [x] **1.1.2 Default Light Mode Theme**: Set crisp Light Mode default with clean DOM dark-mode initialization.
- [ ] **1.1.3 Markdown Code Highlighting & Copy Button**:
  - [ ] Support syntax highlighting in code blocks via lightweight markdown parser integration.
  - [ ] Add a floating "Copy" button to the top-right of every code block.
  - *Verification*: Confirm syntax color rendering for `rust`, `json`, and `javascript`; verify clipboard content matches the raw code exactly upon clicking Copy.
- [ ] **1.1.4 Interactive Citations**:
  - [ ] Parse `[1]`, `[2]` citation syntax dynamically from streaming markdown content.
  - [ ] Transform static tags into clickable anchor buttons that scroll the target document card into viewport with active styling.
  - *Verification*: Query with dense chunks; click `[1]`; ensure the citation card is highlighted and smooth-scrolled into view.
- [ ] **1.1.5 RAG Query Profiles**:
  - [ ] Add query profile selectors (`Fast`, `Balanced`, `Precise`) to the chat interface.
  - [ ] Implement backend profile routing: `Fast` (FTS5 top-5), `Balanced` (LanceDB + Reranker top-10), and `Precise` (Hybrid + Heavy Reranker top-15).
  - *Verification*: Verify REST request carrying profile headers matches expected chunk count and search strategy in backend logs.

### 1.2 CLI Enhancements

- [x] **1.2.1 Workspace Switching Persistence**: `opendoc workspace switch <name>` persists selection to `active_workspace` in `config.toml`.
- [x] **1.2.2 One-Line Cross-Platform Installer**: `install.sh` script supporting Linux and macOS x86_64/aarch64 binaries.

---

## 📋 Planned Execution Items (Phase 2 — Task Execution Layer & Native AI Engines)

Spec: [`openspec/specs/task-execution-ai-engines/spec.md`](../../openspec/specs/task-execution-ai-engines/spec.md)
Reference: [`docs/ref/en/task-execution-ai-engines-verification.md`](../ref/en/task-execution-ai-engines-verification.md)

### 2.0 Phase 0 — Baseline Hardening (no behavior change)

- [ ] **2.0.1 Audit `search_and_rerank` call sites**: enumerate all callers of the sync `SearchBackend` trait (mcp `lib.rs:187`/`:441`, CLI `SearchWrapper` main.rs:30, storage stub `lib.rs:394`) before the async signature change.
- [ ] **2.0.2 Async `SearchBackend` failing tests**: write failing unit tests for the trait method changing from `fn search_and_rerank(...) -> Vec<DocumentChunk>` to `async fn ... -> Vec<DocumentChunk>` (all call sites move to `.await`).
- [ ] **2.0.3 `[ai]`/`[task]` config parsing**: add sections to `AppConfig` with `#[serde(default)]` so existing `config.toml` files load unchanged (backward compat).
  - *Verification*: `cargo check` zero warnings; existing config loads; new config with `[ai]`/`[task]` parses.

### 2.1 Phase 1 — Task & AI Abstractions (pure Rust, CPU)

- [ ] **2.1.1 `opendoc-task` crate**: `TaskEnvelope`/`TaskResult`/`TaskType`, `TaskExecutor` trait, `InProcessExecutor`.
- [ ] **2.1.2 `opendoc-ai` crate**: `AiEngine` trait, `EngineConfig`, `HardwareBackend` probe (Vulkan→HIP→CPU).
- [ ] **2.1.3 `opendoc-ai-fastembed` crate**: bge-m3 embed + reranker on ONNX CPU (dim 1024).
- [ ] **2.1.4 Upload pipeline**: parse → embed (fastembed CPU) → LanceDB write (compat schema).
- [ ] **2.1.5 Real `LanceDbRetriever`**: vector + FTS5 + RRF + threshold replaces stub `search_and_rerank`.
  - *Verification*: real documents round-trip — index then query returns actual chunks; empty `Vec::new()` on no match.
