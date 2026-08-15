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
- [x] **1.1.5 RAG Query Profiles**:
  - [x] Add query profile selectors (`Fast`, `Balanced`, `Precise`) to the chat interface.
  - [x] Implement backend profile routing: `Fast` (FTS5 top-5), `Balanced` (LanceDB + Reranker top-10), and `Precise` (Hybrid + Heavy Reranker top-15).
  - *Verification*: Verify REST request carrying profile headers matches expected chunk count and search strategy in backend logs.

### 1.2 CLI Enhancements

- [x] **1.2.1 Workspace Switching Persistence**: `opendoc workspace switch <name>` persists selection to `active_workspace` in `config.toml`.
- [x] **1.2.2 One-Line Cross-Platform Installer**: `install.sh` script supporting Linux and macOS x86_64/aarch64 binaries.

### 1.3 GA Contract Audit

- [x] **1.3.1 Workspace card source audit**: `/workbench`, `/admin/stats`, and `/admin/connectors` query through `X-Workspace`; no direct `default_workspace` use was found in the card flow.
- [x] **1.3.2 CLI index sync code audit**: SHA-256 deduplication, changed-file re-upload, directory deletion pruning, and `X-Workspace` propagation are present.
- [ ] **1.3.3 CLI index sync integration tests**: verify empty directories, nested paths, and cross-workspace deletion isolation.
- [ ] **1.3.4 GitHub connector contract**: WebUI calls `/admin/connectors/github` and `/admin/connectors/github/sync`, but the Rust router does not provide these routes; workspace isolation must not be marked complete before implementation.
- [x] **1.3.5 Activity-log workspace read audit**: stats, workbench, and query-log reads include `workspace_id` filtering.
- [x] **1.3.6 Activity-log completeness**: fix total/pagination/DTO alignment, add workspace scoping to feedback updates, and implement delete API/UI.

### 1.4 Pre-GA WebUI Cleanup

- [ ] **1.4.1 Tailwind CSS v4 migration**: `tailwindcss ^4` + `@tailwindcss/vite` (in vite.config.ts), remove `postcss.config.js` / `autoprefixer`; switch to CSS-first config (`@import "tailwindcss"`, `@custom-variant dark`, `@theme`, `@plugin "@tailwindcss/typography"`). *Verification*: `npm run typecheck` zero errors, `make install`, page renders (not stub), dark mode works, no visual drift in modal/markdown.
- [ ] **1.4.2 Fix default workspace hardcoding**: `DictionaryPage.tsx:20` and `Sidebar.tsx:192` currently use `localStorage.getItem('active-workspace') || 'default'`, which treats the literal `default` as a workspace name; the fix is for the WebUI to call `getWorkbench()` on startup to fetch the backend-resolved workspace name (active → default_workspace fallback), store it in appStore, and have Sidebar/DictionaryPage read from the store instead of guessing from localStorage. *Verification*: open a fresh browser with cleared localStorage; Sidebar shows the configured `default_workspace` name (not the literal `default`).
- [x] **1.4.3 Fix generic LLM answers**: `chat.rs` system prompt does not inject workspace context, so when retrieval is a fuzzy hit the LLM does not know its own workspace or document scope and replies with generic content ("since you did not provide a specific project name…"); the prompt must inject the workspace name and indexed document scope, and explicitly state when no relevant documents exist. *Verification*: ask a deployment-flow question with the fast profile; the answer must cite workspace documents directly or explicitly state "no relevant documents in this workspace" — never a generic boilerplate answer.
- [x] **1.4.4 Chat layout restructure (bottom input + processing state)**: ChatPage currently places the input above and lets messages grow downward, which diverges from mainstream chat UI (Open WebUI style: scrollable message area on top, input pinned at bottom); also there is no backend-processing feedback between send and the first SSE chunk. Restructure to a flex column layout (message area `flex-1 overflow-y-auto`, input pinned to bottom) and show a thinking indicator when `isStreaming && !currentStreamText` (lazy-load state). *Verification*: after sending a question in the browser, the thinking indicator appears immediately; the message area scrolls independently with the input fixed at the bottom; new messages auto-scroll to the bottom.
- [x] **1.4.5 Workspace context and controlled document tools**: verify whether retrieved document content, sources, and workspace scope are fully passed to the LLM; if not, design document-list/content tools that can read only the current workspace through the existing backend data layer, then assess whether tool calling is needed. The LLM must not read arbitrary filesystem paths or bypass workspace isolation. *Verification*: ask about real documents and inspect the outbound LLM context and cited answer; when no relevant content exists, the answer must explicitly decline rather than use general knowledge. *Conclusion*: after the 1.4.3 fix, retrieved content fully reaches the LLM context (system prompt injects workspace document count and scope); GA keeps the Chat-centric RAG flow, controlled document tools are deferred post-GA.
- [x] **1.4.6 Rename conversation title**: add WebUI rename interaction on the existing conversation update API. Title updates must retain workspace scoping, input validation, and error feedback, without introducing a second conversation state implementation. *Verification*: rename in the browser, reload, and confirm the sidebar and conversation title agree; a different workspace must not update the conversation.
- [x] **1.4.7 Fix document Detail information-box width overflow**: the "Data loss prevention and integrity verification" box in document detail is widened by long text, breaking its intended column width and potentially introducing horizontal scrolling. Apply the flex/grid shrinking and text-wrapping contract from `AGENTS.md` §9.4; do not conceal the problem by reducing the overall font size. *Verification*: test the affected label, a long English word, a URL, and a narrow viewport; the box retains its intended column width, text wraps normally, and the page has no unintended horizontal scrollbar.
- [x] **1.4.8 Reduce WebUI `font-semibold` weight**: Changed the shared Tailwind font-weight token from 600 to 500 instead of scattering per-page overrides. This affects every paragraph, heading, button, and label using `font-semibold` while preserving the information hierarchy. *Verification*: `npm run typecheck` passes; inspect Traditional Chinese paragraphs, `h3`–`h5`, buttons, and status labels, then compare English and Korean.
- [x] **1.4.9 Support Escape-to-close for modals**: audit every WebUI modal/dialog/overlay and make closable modals respond to `Escape` while preserving `role="dialog"`, `aria-modal="true"`, focus, and busy-state contracts. Escape must not interrupt an in-progress destructive action. *Verification*: open each modal and confirm `Escape` closes it and restores focus to its trigger; Escape does not close while busy; background-page shortcuts do not fire.
- [x] **1.4.10 Use an explicit edit icon for the BYOK edit button**: the BYOK provider list "edit" button previously used a `+` icon, confusing it with "add provider"; switch to a pencil icon (`Pencil`) to express the edit semantics while keeping the existing button size and i18n labels. *Verification*: the "Edit provider" button in the Settings BYOK section shows a pencil icon (not `+`), and typecheck passes.
- [ ] **1.4.11 Consistent Markdown styling inside the Chat source-preview modal**: Markdown/code blocks inside the Chat document source-preview modal currently render inconsistently, with a white border around some dark content blocks but not others. Unify the background, border, and padding rules for equivalent content levels; do not change the modal width, external document cards, or other unspecified layout styles. *Verification*: open real source-preview modals containing normal Markdown, code blocks, and long text; equivalent blocks use consistent dark-background and white-border treatment, with no unintended horizontal scrolling across all three languages and narrow viewports.

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
