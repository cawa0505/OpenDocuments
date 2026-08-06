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

### 1.2 CLI & TUI Enhancements

- [x] **1.2.1 Workspace Switching Persistence**: `opendoc workspace switch <name>` persists selection to `active_workspace` in `config.toml`.
- [x] **1.2.2 One-Line Cross-Platform Installer**: `install.sh` script supporting Linux and macOS x86_64/aarch64 binaries.
- [ ] **1.2.3 TUI On-the-Fly Workspace Switcher**:
  - [ ] Bind `Ctrl+W` in TUI to trigger an inline workspace selector modal.
  - [ ] Redraw and re-query active list immediately upon workspace selection without restarting TUI.
  - *Verification*: Open `opendoc tui`, press `Ctrl+W`, select a different workspace, verify footer/header updates and search queries scope to new workspace.
- [ ] **1.2.4 TUI Chunk Inspector Modal**:
  - [ ] Bind `Enter` (or a dedicated inspector key `i`) on selected search results in TUI.
  - [ ] Render a structured popup modal detailing raw text chunk content, metadata properties, and similarity score.
  - *Verification*: Select a document in TUI, press `Enter`, inspect modal text, ensure scroll keys (`Up`/`Down`) work inside the popup, and `Esc` closes it cleanly.
