# 📋 OpenDocuments Task Backlog & Execution Status

**English** | [繁體中文](../zh-TW/tasks.md)

This document tracks execution status across all engineering work packages.

---

## 🚨 Active Execution Items (Phase 1)

### 1.1 WebUI & RAG Streaming Improvements
- [x] **1.1.1 BYOK Management Interface**: Complete `SettingsPage.tsx` LLM provider management with connection diagnostic tests.
- [x] **1.1.2 Default Light Mode Theme**: Set crisp Light Mode default with clean DOM dark-mode initialization.
- [ ] **1.1.3 Markdown Code Highlighting & Copy Button**: Add syntax highlighting with a one-click copy button for code blocks.
- [ ] **1.1.4 Interactive Citations**: Transform `[1]`, `[2]` citation markers in LLM responses into interactive tags that highlight source document cards.
- [ ] **1.1.5 RAG Query Profiles**: Implement `Fast` (FTS5 top-5), `Balanced` (LanceDB + Reranker top-10), and `Precise` (Hybrid + Heavy Reranker top-15) search strategies.

### 1.2 CLI & TUI Enhancements
- [x] **1.2.1 Workspace Switching Persistence**: `opendoc workspace switch <name>` persists selection to `active_workspace` in `config.toml`.
- [x] **1.2.2 One-Line Cross-Platform Installer**: `install.sh` script supporting Linux and macOS x86_64/aarch64 binaries.
- [ ] **1.2.3 TUI On-the-Fly Workspace Switcher**: Implement `Ctrl+W` inline workspace switcher in `opendoc tui`.
- [ ] **1.2.4 TUI Chunk Inspector Modal**: Add RAG citation inspector modal to review retrieved chunk context directly inside TUI.
