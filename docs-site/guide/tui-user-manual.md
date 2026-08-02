# 📟 Terminal UI (TUI) User Manual

This guide describes OpenDocuments' built-in Terminal UI (`opendoc tui`), a lightweight, zero-dependency console RAG testing and exploration tool optimized for offline debugging and immediate hybrid search validation.

*For the complete detailed manual in Traditional Chinese, please refer to [docs/OPENDOC-TUI-MANUAL.md](https://github.com/cawa0505/OpenDocuments/blob/main/docs/OPENDOC-TUI-MANUAL.md).*

---

## 🚀 Getting Started

Launch the TUI from the root directory of your workspace:

```bash
opendoc tui
```

This will automatically resolve the active workspace using the CLI hierarchy:
`active_workspace` (stored in your config) $\to$ fallback to `default_workspace`.

---

## 🕹️ Keyboard Controls

| Shortcut / Key | Active Mode | Real-world Behavior & Action |
| :--- | :--- | :--- |
| **`Ctrl + W`** | Search Mode | **Activate Workspace Selector**: Pulls up the workspace selection overlay. |
| **`↑ / ↓` (Up / Down)** | Selection Mode | **Cycle Workspaces**: Moves the highlight cursor through the list and automatically populates the text input. |
| **`Tab`** | Selection Mode | **Autocomplete**: Completes the input field with the currently highlighted workspace name. |
| **`Backspace / Chars`** | Selection Mode | **Fuzzy Case-insensitive Filtering**: Input characters to filter and auto-focus on the first matching workspace. |
| **`Enter`** | Selection Mode | **Hot Swap & Persist**: Confirms the switch, reloads metadata & LanceDB vectors in the background, and persists the choice to `config.toml` (`active_workspace`). |
| **`Enter`** | Search Mode | **Trigger Search**: Runs hybrid dense/sparse retrieval against the active workspace. |
| **`Esc`** | Any Mode | **Back / Exit**: Closes the selector overlay or exits the TUI to the main shell. |

---

## 📐 Responsive断點 & Safety Design

The TUI features a robust defensive layouts system:
1. **Critical Width Guard (`Width < 50` or `Height < 10`)**: Stops rendering and triggers a bold red warning to prevent TUI framework crashes inside narrow terminal splits (e.g., small Tmux panes).
2. **Compact Screen Mode (`Width < 85`)**: Drops the `Score` column entirely to prioritize readability of the file names and snippets, re-allocating columns to `File Name (30%)` + `Snippet (70%)`.
3. **Widescreen Mode (`Width >= 85`)**: Displays the full 3-column dashboard: `File Name (20%)` + `Score (10% with green/yellow threshold highlights)` + `Snippet (70%)`.
