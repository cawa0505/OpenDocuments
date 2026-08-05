# 🗺️ OpenDocuments R&D Roadmap (ROADMAP.md)

This document tracks the strategic milestones, optimization paths, and implementation details for OpenDocuments (Open-Core RAG Engine + WebUI) and its companion Desktop Client ecosystem.

*Language*: **English (Primary)** | [繁體中文](ROADMAP.zh-TW.md)

---

## 📌 Development Overview & Priorities

| Priority | Module / Area | Core Milestone | Status | Est. Effort |
|:---:|:---:|:---|:---:|:---:|
| **P0** | **WebUI BYOK LLM Configuration** | Complete SettingsPage UI for LLM provider/key management to resolve echo fallback | ⏳ In Progress | 2 hrs |
| **P0** | **WebUI / API Parity Verification** | End-to-end verification of all Axum endpoints against React WebUI contracts | ⏳ In Progress | 3 hrs |
| **P1** | **Interactive TUI Engine Overhaul** | Implement multi-tab navigation, Chunk Inspector modal, Event Drawer, and Flash Footer | ⏳ Planned | 8 hrs |
| **P1** | **Beyond-Text Knowledge Graph (Phase 1)** | Implement WikiLink, YAML FrontMatter parser, and SQLite Graph hybrid retrieval | ⏳ Planned | 8 hrs |
| **P2** | **Tauri 2.0 Desktop & Stdio MCP Sandbox** | Stdio IPC JSON-RPC integration and human-in-the-loop UI Gatekeeper approval | ⏳ Planned | 12 hrs |

---

## 📋 Milestone Details & Execution Roadmap

### 🎨 Phase 1: ChatGPT-Aligned WebUI & SSE Streaming

#### 1.1 WebUI BYOK LLM Settings Page (`SettingsPage.tsx`)
- [ ] **API Bindings**:
  - Implement `llm_providers` CRUD endpoints in `apps/webui/src/lib/api.ts`:
    - `GET /api/v1/admin/llm/providers`
    - `POST /api/v1/admin/llm/providers`
    - `DELETE /api/v1/admin/llm/providers/:id`
    - `POST /api/v1/admin/llm/providers/test-connection`
- [ ] **Frontend UI Card**:
  - Masked API key inputs stored securely in SQLite `llm_providers`.
  - Real-time connection health checks with status indicators.

#### 1.2 Interactive TUI Engine Overhaul (`crates/opendoc-tui`)
- [ ] **Chrome Layout & Theme System**:
  - Split viewport into Header/Tabs, Main Content, Event Log Drawer, and Flash Footer.
  - Implement centralized `theme.rs` for unified color palette.
- [ ] **Interactive Result Selection & Chunk Inspector Modal**:
  - Keyboard (`Up`/`Down`/`j`/`k`) and mouse row selection.
  - Overlay `Modal` displaying full chunk text, similarity breakdown (Vector vs. BM25), and file path.
- [ ] **Multi-Tab Architecture**:
  - `Tab 1`: 🔍 Search & Inspector.
  - `Tab 2`: 📊 Workspace Document Matrix & DB Health.
- [ ] **Event Drawer & Footer Flash Feedback**:
  - Toggleable bottom drawer (`L` key) for background logs (latency, status).
  - Momentary button flashing on keyboard shortcut execution.

---

### 🌐 Phase 2: Beyond-Text Knowledge Graph (RAG Enhancement)

#### 2.1 Data Models & Parsing (`opendoc-types` & `opendoc-parser`)
- [ ] Implement `GraphNode` and `GraphEdge` strongly-typed structures.
- [ ] Add `[[WikiLink]]` bidirectional link extraction and YAML FrontMatter parser.

#### 2.2 SQLite Graph Storage & Hybrid Query (`opendoc-storage`)
- [ ] Build `graph_nodes` and `graph_edges` schema in SQLite.
- [ ] Implement hybrid RAG retriever combining FTS5 text, Vector reranking, and Graph topology traversal.

---

### 🔒 Phase 3: Stdio MCP Integration & UI Gatekeeper (Desktop Client)

#### 3.1 Local Stdio MCP Sandbox
- [ ] Ensure `opendoc start --mcp-only` operates flawlessly over stdio without stdout pollution.

#### 3.2 UI Gatekeeper Approval Portal
- [ ] Intercept LLM `tools/call` requests (file writes, code execution) and prompt human operator approval before execution.

---

### 🛒 Phase 4: Open-Source Marketplace & One-Click Publishing

#### 4.1 Skill Marketplace Grid
- [ ] Connect to GitHub API to display and install verified Skills.

#### 4.2 Static Site Generation (SSG) Publishing
- [ ] Built-in SSG engine to publish RAG collections to GitHub Pages.
