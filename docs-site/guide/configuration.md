# Configuration

OpenDocuments is configured via a standard TOML file located at `~/.config/opendocuments/config.toml`. 

The configuration is automatically initialized with default values the first time you run `opendoc`. Any manual edits to the TOML file will be loaded dynamically by the server, and command-line workspace changes (e.g. switching active workspaces) are persisted directly back to this file.

---

## 📝 Full Example (`config.toml`)

Below is a complete, annotated example of the `~/.config/opendocuments/config.toml` file:

```toml
# Unified server connection details
[server]
url = "http://127.0.0.1:3000"
api_key = ""                  # Optional security API key

# Local filesystem storage configuration
[database]
path = "~/.opendocuments"      # Base directory for database files (expands to user's home folder)

# Model and retrieval parameters
[model]
default_workspace = "default"  # Default workspace created on system startup
active_workspace = "MyWorkspace"    # Active workspace (managed by `opendoc workspace switch`)
score_threshold = 0.60             # RAG retrieval similarity cutoff threshold (0.0 to 1.0)
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx" # Path to ONNX local reranker model

# Task Execution Layer abstraction
[task]
executor = "inprocess"             # "inprocess" | "spur_daemon" | "spur_batch"

# Native AI Engine selection
[ai]
preferred_backend = "cpu"          # "cpu" | "vulkan" | "hip"
```

---

## 📁 Workspace Configuration

Workspaces are logically isolated within a unified database structure. When you switch to a workspace, OpenDocuments dynamically filters metadata, documents, and conversations under that workspace's unique ID. This logical decoupling ensures that switching workspaces in the CLI or WebUI instantly partitions your data with **microsecond latency** and zero risk of cross-workspace data leakage.

---

## 🛠️ Modifying Configuration

### Via Command-Line (CLI)

You can view or update configuration values directly via the CLI tool:

```bash
# View active and default workspaces and config path
opendoc config show

# Switch the current active workspace (persists to model.active_workspace in config.toml)
opendoc workspace switch "MyNewWorkspace"
```

### Direct TOML Editing

Since TOML is a highly readable format, you can safely open `~/.config/opendocuments/config.toml` in any editor (such as VS Code, Vim, or Notepad) and change paths or thresholds directly.

To apply changes, simply restart your running `opendoc start` service.

---

## 🎯 RAG Query Profiles

In WebUI system settings or RAG APIs, OpenDocuments provides three built-in **Query Profiles** to balance local hardware resource usage with retrieval precision.

### 1. Fast Profile
* **Retrieval Strategy**: Keywords-only search (LanceDB full-text) with the lightest semantic pre-filtering.
* **Best For**: Highly constrained local hardware (such as legacy office PCs without a GPU) or when searching for pure factual named entities (e.g., finding specific document serial numbers or names).
* **Technical Details**:
  * Skips the local ONNX Reranker model.
  * Limits retrieval context depth (Top-K Chunks <= 3).
  * Fastest response time, typically completing within 50ms.

### 2. Balanced Profile (Default)
* **Retrieval Strategy**: Two-way hybrid search (Dense + Sparse Hybrid Search).
* **Best For**: Most daily administrative audits and material search tasks. This is the default recommended profile.
* **Technical Details**:
  * Concurrently queries LanceDB full-text index and LanceDB for vector similarity.
  * Uses Reciprocal Rank Fusion (RRF) to merge scores from both pathways.
  * Sets Top-K Chunks to 5 and applies a lightweight local re-ranking.
  * Perfectly balances semantic understanding and exact keyword matching with moderate CPU overhead.

### 3. Precise Profile
* **Retrieval Strategy**: Hybrid Search ➔ Comprehensive local cross-re-ranking (ONNX Cross-Reranker) ➔ Knowledge association topology.
* **Best For**: Complex compliance audits of educational plans, cross-referencing multiple regulations, or generation scenarios requiring deep semantic coherence.
* **Technical Details**:
  * Expands Top-K to 10 or 15.
  * Forces the invocation of the local ONNX Reranker model (`bge-reranker-base`) to calculate deep cross-attention scores and re-evaluate semantic relevance.
  * Provides the most comprehensive context window, though it consumes more local memory and compute.
