# 🖥️ TUI Terminal UI

OpenDocuments includes a native, lightweight Terminal UI for RAG querying, state debugging, and instant workspace swapping.

To launch the TUI:
```bash
opendoc tui
```

---

## 📂 1. Dynamic Workspace Swapping

- **Obeys Active Context**: Starts up using your configured `active_workspace`. Falls back to `"default"` if not set.
- **`Ctrl+W` Swap**: Press **`Ctrl+W`** inside the TUI to pop a dynamic input prompt. Type the target workspace name and press **`Enter`** to instantly swap contexts, reload configs, and re-query on the spot without restarting. Press **`Esc`** to cancel.

---

## 🔍 2. Responsive Ingestion Debugging

- **Async Fetch**: Pressing **`Enter`** triggers a background mixed retrieval (LanceDB Vector + LanceDB Full-Text) with zero UI thread blocking.
- **Media Query Adaptation**: If your terminal window is narrower than `85` characters, the `Score` column is automatically hidden to reserve text readability. On wider terminals, scores above threshold are dynamically color-coded (**Green** for highly relevant, **Yellow** for lower relevance).
