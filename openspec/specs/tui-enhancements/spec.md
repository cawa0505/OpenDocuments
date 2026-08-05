# OpenSpec Requirement: Enhanced Terminal User Interface (TUI)

**Spec ID**: `tui-enhancements`  
**Status**: Draft / Proposed  
**Priority**: P1  
**Primary Language**: English  

---

## 1. Overview & Objectives

This specification defines the architectural and user-experience enhancements for the `opendoc-tui` terminal interface. The goal is to evolve the current single-view table into a feature-rich, keyboard/mouse-driven interactive terminal workspace inspired by high-performance Rust terminal utilities (e.g., `ratatui`, `graphify-cli`).

---

## 2. Core Functional Requirements

### 2.1 Chrome & Layout Architecture
- **Dedicated Chrome Sections**: The TUI layout MUST be partitioned into four clear regions:
  1. **Header / Tabs**: Workspace indicator and view switcher (`🔍 Search & Inspector`, `📊 Document Matrix`).
  2. **Main Workspace**: Adaptive content viewport taking 70-100% of vertical height.
  3. **Event Log Drawer**: Collapsible 30% bottom panel for background task logs and error diagnostics (toggled via `L` key).
  4. **Footer Action Bar**: Live keybinding guide with momentary visual flash feedback (`Flash`) upon user keypresses.

### 2.2 Interactive Result Selection & Chunk Inspector Modal
- **Keyboard & Mouse Navigation**: Users MUST be able to navigate search results using `Up` / `Down` arrow keys, `k` / `j`, or mouse clicks.
- **Chunk Inspector Modal**:
  - Pressing `Enter` on any search result MUST open an interactive modal dialog.
  - The modal MUST display:
    - Full chunk text with syntax/markdown formatting.
    - Exact similarity score (Vector vs. FTS5 BM25 breakdown).
    - Source document path, chunk index, and workspace metadata.
    - Quick actions (`[c] Copy snippet`, `[o] Open source file in default editor`).

### 2.3 Multi-Tab Architecture (`ActiveTab`)
- **Tab 1: 🔍 Search & Inspector**: Real-time hybrid retrieval (Vector + FTS5) with interactive result table and Chunk Inspector modal.
- **Tab 2: 📊 Workspace Document Matrix**: Overview of indexed documents, status counts (`indexed`, `pending`, `failed`), total chunks, and database health metrics.

### 2.4 Centralized Theme & Event Logging System
- **Theme Module (`theme.rs`)**: Decouple hardcoded colors into a unified palette (Cyan primary, Yellow warnings, Green success, Dark Gray borders).
- **Event Log Module (`log.rs`)**: Bounded in-memory event buffer (max 200 entries) capturing backend async status, search execution latency (ms), and workspace switch events.

---

## 3. Behavior Contract & Specifications

```spec
WHEN the user launches `opendoc tui`
THEN the TUI MUST initialize with active workspace state and render the Chrome layout.

WHEN the user presses `Up` / `Down` or clicks a search result row
THEN the selection cursor MUST update dynamically.

WHEN the user presses `Enter` on a selected row
THEN a Chunk Inspector modal MUST overlay the main viewport displaying full content and similarity metrics.

WHEN the user presses `L`
THEN the Event Log Drawer MUST toggle between visible (30% height) and hidden (0% height).

WHEN the user triggers any valid shortcut key
THEN the corresponding footer tag MUST briefly flash to provide immediate visual feedback.
```
