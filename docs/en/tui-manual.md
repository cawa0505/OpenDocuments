# 📟 OpenDocuments Terminal UI (TUI) User & Debugging Manual

**English** | [繁體中文](../zh-TW/tui-manual.md)

---

## 🌐 1. Overview & Purpose

In OpenDocuments' 100% local, zero-trust RAG architecture, alongside the modern web interface, a lightweight **zero-external-dependency Terminal User Interface (TUI)** is built directly into the main Rust binary (`opendoc tui`).

This TUI communicates directly with the local SQLite metadata database and LanceDB vector store without launching browser processes or Node.js runtimes:
1. **Parallel Hybrid Search**: Combines dense vectors and full-text keyword search from LanceDB with Reciprocal Rank Fusion and score filtering.
2. **On-the-Fly Workspace Switching**: Instantly switch workspace pointers with keyboard shortcuts, auto-persisted to `config.toml`.

---

## 🚀 2. Launch Instructions

Ensure your local database and binary are initialized, then run from any terminal:

```bash
# Launch TUI and load active_workspace (fallback to default_workspace)
opendoc tui
```

---

## 🕹️ 3. Keyboard Navigation & Controls

| Shortcut / Key | Active Context | Execution Behavior |
| :--- | :--- | :--- |
| **`Ctrl + W`** | Search View | **Open Workspace Picker**: Pause search and open workspace list drawer. |
| **`↑ / ↓`** | Workspace Picker | **Navigate Workspaces**: Move cursor through available workspaces. |
| **`Tab`** | Workspace Picker | **Autocomplete**: Fill input box with the selected workspace name. |
| **`Backspace / Key`** | Workspace Picker | **Fuzzy Filter**: Filter workspaces in real time. |
| **`Enter`** | Workspace Picker | **Switch Workspace**: Instantly switch database context and persist `active_workspace` to `config.toml`. |
| **`Enter`** | Search View | **Trigger RAG Search**: Execute hybrid search against active workspace. |
| **`Esc`** | Any View | **Exit / Back**: Return to search mode from workspace picker, or exit TUI cleanly. |

---

## 📐 4. Responsive Layout Breakpoints

The TUI features protective layout breakpoints to prevent terminal truncation or rendering crashes:

- **Ultra-Narrow Warning (Width < 50 || Height < 10)**:
  - Displays alert: `⚠️ Window too small, please expand terminal window...`
- **Medium Width Mode (50 <= Width < 85)**:
  - Re-apportioned view: `Filename (30%)` + `Excerpt (70%)` (hides Score column for narrow split terminals).
- **Wide Mode (Width >= 85)**:
  - Full 3-column view: `Filename (20%)` + `Score (10%)` + `Excerpt (70%)`.
  - **Score Highlight**: Scores > `0.75` highlight in bold green; lower scores highlight in yellow.

---

## 🛠️ 5. Debugging & Verification

1. **Verify Config File**:
   ```bash
   cat ~/.config/opendocuments/config.toml
   ```
   Check that `active_workspace` matches the workspace selected in TUI.

2. **Verify Workspace Table**:
   ```bash
   sqlite3 ~/.opendocuments/db.sqlite "SELECT id, name FROM workspaces;"
   ```
   Confirm the target workspace exists in local SQLite metadata.
