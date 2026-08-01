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
default_workspace = "GraphifyOpt"  # Default workspace created on system startup
active_workspace = "MyWorkspace"    # Active workspace (managed by `opendoc workspace switch`)
score_threshold = 0.60             # RAG retrieval similarity cutoff threshold (0.0 to 1.0)
local_reranker_path = "~/.opendocuments/models/bge-reranker-base.onnx" # Path to ONNX local reranker model
```

---

## 📁 Workspace Configuration

Workspaces are completely self-contained. When you define a workspace or switch to one, OpenDocuments dynamically maps:
1. **Metadata Database**: An independent SQLite file at `{path}/{workspace_name}/opendocuments.db` storing conversations, tags, document properties, and logs.
2. **Vector Space**: A standalone LanceDB database under the `{path}/{workspace_name}/collections/` directory storing text chunk embeddings.

Because of this physical decoupling, switching workspaces in the CLI or WebUI instantly switches backend pointers with **microsecond latency** and zero risk of cross-workspace data leakage.

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

## 🎯 RAG 檢索偏好優化 (Query Profiles)

在 WebUI 的「系統設定」或 RAG 檢索中，OpenDocuments 提供三種預置的「檢索偏好設定（Query Profiles）」，用以平衡本機硬體資源消耗與檢索精準度。

### 1. 快速檢索 (Fast Profile)
* **檢索策略**：純關鍵字檢索 (SQLite FTS5) 與最輕量的語義粗篩。
* **適用場景**：本機硬體資源極度受限（例如無 GPU 的公務舊電腦）、或對單純事實性命名實體（如：找特定「公文字號」、「姓名」）進行檢索時。
* **技術特點**：
  * 跳過本機 ONNX 重排模型 (Reranker)。
  * 限制檢索上下文深度 (Top-K Chunks <= 3)。
  * 反應時間最快，通常在 50ms 內完成。

### 2. 平衡檢索 (Balanced Profile)
* **檢索策略**：雙路混合檢索 (Dense + Sparse Hybrid Search)。
* **適用場景**：大部分日常行政稽核與教材備課查詢，是系統的預設推薦設定。
* **技術特點**：
  * 同時調用 SQLite FTS5 進行全文索引與 LanceDB 進行向量相似度檢索。
  * 使用倒數排名融合 (RRF, Reciprocal Rank Fusion) 演算法，完美合併兩路分數。
  * Top-K Chunks 設為 5，並進行輕量級本機重排。
  * 平衡了語義理解與精準關鍵字匹配，本機 CPU 運算開銷適中。

### 3. 精準檢索 (Precise Profile)
* **檢索策略**：混合檢索 (Hybrid Search) ➔ 全面本機交叉重排 (ONNX Cross-Reranker) ➔ 知識關聯度拓撲。
* **適用場景**：複雜的教學計畫書合規性審查、多份法規交叉比對、或需要极高推理連貫性的生成場景。
* **技術特點**：
  * Top-K 擴大至 10 到 15。
  * 強制調用本機 ONNX Reranker 模型 (`bge-reranker-base`) 對所有候選文本進行深度交互注意力計算，重新評估語義關聯。
  * 上下文攜帶更為完整豐富，但會消耗較多本機記憶體與計算資源。
