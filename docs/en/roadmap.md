# 🗺️ OpenDocuments R&D Roadmap

**English** | [繁體中文](../zh-TW/roadmap.md)

This roadmap outlines the multi-phase engineering objectives and strategic milestones for the OpenDocuments ecosystem.

---

## 🎯 Phase Overview

| Phase | Title | Focus Area | Status | Target |
| :--- | :--- | :--- | :---: | :---: |
| **Phase 0** | **Single-Binary MVP & BYOK Gateway** | Single-binary Axum server, BYOK LLM, Hybrid RAG (LanceDB + FTS5), Tags, CLI/TUI | ✅ Completed | Q3 2026 |
| **Phase 1** | **ChatGPT-Aligned WebUI & Streaming** | React 19 WebUI, Typewriter SSE, Citation linking, Markdown code highlighting | ⏳ In Progress | Q3 2026 |
| **Phase 2** | **3-Column Administrative Control Cabin** | Tauri 2.0 Desktop app, In-place spreadsheet & Markdown editing, Drag-and-drop | 📅 Planned | Q4 2026 |
| **Phase 3** | **Stdio MCP Integration & UI Gatekeeper** | Local Stdio MCP sandbox, Human-in-the-loop tool call approval, License security | 📅 Planned | Q4 2026 |
| **Phase 4** | **Open Ecosystem & Publisher** | GitHub-driven Skill Marketplace, Skill Shield signatures, Static Site Publisher | 📅 Planned | Q1 2027 |

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

---

## 🎛️ Phase 2: Tauri 2.0 Administrative Control Cabin (Planned)

- [ ] **Left Column**: Workspace switcher and tree-view Collection Explorer.
- [ ] **Center Column**: Drag-and-drop ingestion with embedded React Canvas spreadsheet and Monaco Editor.
- [ ] **Right Column**: Skill activation panel and UI Gatekeeper authorization cards.

---

## 🔒 Phase 3: Stdio MCP Integration & Security Gatekeeper (Planned)

- [ ] **Stdio MCP Server**: Run OpenDocuments as a standard MCP server communicating over IPC/Stdio.
- [ ] **UI Gatekeeper**: Intercept LLM `tools/call` invocations and require manual user approval.
- [ ] **Hardware-Bound Licensing**: Hardware fingerprinting (CPU/Board UUID) with asymmetric ECC signature verification.

---

## 🛒 Phase 4: Open Skill Marketplace & Publisher (Planned)

- [ ] **GitHub Skill Marketplace**: In-app grid store for downloading YAML/JSON skills.
- [ ] **Skill Shield**: Cryptographic verification for skills to prevent malicious execution.
- [ ] **GitHub Pages Publisher**: Built-in static site generator publishing RAG knowledge bases directly to GitHub Pages.
