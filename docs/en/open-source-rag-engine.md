# Open Source RAG Engine (Data Plane)

Language: **English** | [繁體中文](../zh-TW/open-source-rag-engine.md)

---

## 1. Executive Overview

The Open Source RAG Engine provides a pure, high-performance Data Plane for Retrieval-Augmented Generation (RAG). Built on top of LanceDB and Apache Arrow, it delivers sub-millisecond vector similarity search combined with client-driven SQL-like pre-filtering capabilities. Unlike the enterprise version—which integrates Control Plane mechanisms such as OAuth/SSO authentication, zero-trust RBAC enforcement, and centralized policy gateways—the open-source Data Plane focuses strictly on efficient vector storage, indexing, and deterministic filtering. Designed for independent developers, local AI agents, and open-source contributors, this engine serves as a lightweight, zero-dependency foundation for high-density document retrieval workflows and forms the core execution layer upon which enterprise Control Plane management features are layered.

---

## 2. Technical Architecture

### Core Components

- **`VectorSearchQuery`**: Data structure encapsulating the dense query embedding, result cardinality limit, and optional SQL-like pre-filter expression string.
- **`LanceDbEngine`**: Core Rust execution engine managing physical LanceDB table connections, Apache Arrow record batch transformations, and vector distance calculations.
- **`open_source_rag_search`**: Public Tauri IPC command handler exposing high-speed vector retrieval directly to frontend applications and desktop UI bridges.
- **`LanceDbEngineWrapper`**: Thread-safe state container wrapping `LanceDbEngine` within `std::sync::Mutex` or `tokio::sync::Mutex` for managed lifecycle across async tasks.

### Data Flow

```
┌───────────────────────────┐
│       Client Layer        │
│  (Query Vector + Filter)  │
└─────────────┬─────────────┘
              │ 1. IPC / API Request
              ▼
┌───────────────────────────┐
│   Tauri IPC / API Bridge  │
│ (open_source_rag_search)  │
└─────────────┬─────────────┘
              │ 2. Unwraps VectorSearchQuery
              ▼
┌───────────────────────────┐
│     LanceDbEngine         │
│ Vector Search + Pre-Filter│
└─────────────┬─────────────┘
              │ 3. Executes Arrow/LanceDB Scan
              ▼
┌───────────────────────────┐
│   Vec<RAGChunkResult>     │
│ (Structured Snippets +    │
│    Metadata Scoring)      │
└─────────────────────────┘
```

1. **Query Preparation**: Client generates query embedding vector (`Vec<f32>`) and specifies optional SQL pre-filter constraints (e.g., `department = 'HR' AND min_security_level <= 2`).
2. **Search Execution**: `LanceDbEngine` applies pre-filter predicates during vector index traversal over LanceDB Apache Arrow datasets.
3. **Result Structuring**: Matched record batches are converted into `Vec<RAGChunkResult>` with normalized similarity scores and source metadata snippets.

### Integration Points

- **Tauri IPC Bridge**: Exposed via `src-tauri/src/commands/lancedb.rs` as `#[tauri::command]`.
- **RAG Subsystem Compatibility**: Plug-and-play compatibility with existing embedding pipelines and document chunking parsers.

---

## 3. API Documentation

### Open Source Tauri Command Signature

```rust
#[tauri::command]
pub async fn open_source_rag_search(
    app: tauri::AppHandle,
    query_vector: Vec<f32>,
    top_k: usize,
    filter: Option<String>,
) -> Result<Vec<RAGChunkResult>, String>
```

#### Parameters
- `app`: `tauri::AppHandle` — Application context handle for accessing managed state (`LanceDbEngineWrapper`).
- `query_vector`: `Vec<f32>` — Dense vector representation of the input query.
- `top_k`: `usize` — Maximum number of nearest-neighbor candidates to return.
- `filter`: `Option<String>` — Optional SQL-like string filter executed prior to or during vector traversal.

#### Returns
- `Ok(Vec<RAGChunkResult>)`: Array of matched document chunks sorted by descending relevance.
- `Err(String)`: Error description if index lookup or SQL filter parsing fails.

### Supported Filter Syntax

The pre-filter parser evaluates standard SQL-like expressions against Arrow column metadata:

- **Simple Equality**: `column = 'value'`
- **Numeric Comparison**: `column <= value`, `column > value`, `column = value`
- **Logical Operators**: `AND`, `OR`
- **Supported Metadata Columns**:
  - `department` (String)
  - `min_security_level` (Integer / u8)
  - `score` (Float / f32)

#### Filter Query Examples

```sql
department = 'HR' AND min_security_level <= 2
department = 'Engineering' OR score > 0.85
min_security_level = 1 AND department = 'Finance'
```

---

## 4. Data Structures

### `VectorSearchQuery`

```rust
pub struct VectorSearchQuery {
    pub query_vector: Vec<f32>,
    pub limit: usize,
    pub filter: Option<String>,
}
```

### `RAGChunkResult`

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RAGChunkResult {
    pub file_name: String,
    pub file_path: String,
    pub content_snippet: String,
    pub score: f32,
    pub department: Option<String>,
    pub allowed_roles: Vec<String>,
    pub allowed_users: Vec<String>,
    pub min_security_level: Option<u8>,
}
```

---

## 5. Security Model

### Open Source (Data Plane Only)

- **Unauthenticated Execution**: The engine operates purely as a data processing pipeline without embedded identity verification or token inspection.
- **Client-Controlled Filtering**: Pre-filter conditions are constructed and supplied directly by the client application.
- **Stateless & Credential-Free**: No user credentials, session tokens, or policy tables are persisted or evaluated by the engine.

> **Key Design Distinction**: In the Open Source Data Plane, the client determines the filter criteria, and the engine executes them verbatim. The Enterprise Control Plane adds server-side policy enforcement, injecting mandatory security constraints before queries reach the Data Plane.

---

## 6. Performance Characteristics

- **Query Latency**: ~10ms execution latency for typical index scans (top-k = 5 to 50).
- **Scalability**: Sub-linear vector search scaling backed by LanceDB disk-native IVF-PQ / DiskANN indexing.
- **Memory Footprint**: Low static memory consumption; Arrow record batches are zero-copy memory mapped.
- **Filter Overhead**: Lightweight SQL pre-filter evaluation with minimal AST parsing overhead.

---

## 7. Deployment & Setup

### Cargo Dependencies

Add the following crate dependencies to `Cargo.toml`:

```toml
[dependencies]
reqwest = { version = "0.13", features = ["blocking", "json", "rustls"] }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```

### State Management Setup

To register `LanceDbEngineWrapper` within Tauri's managed state:

```rust
use std::sync::Mutex;
use tauri::State;

pub struct LanceDbEngineWrapper(pub Mutex<LanceDbEngine>);

fn main() {
    let engine = LanceDbEngine::new("./lancedb_data").expect("Failed to initialize LanceDB");

    tauri::Builder::default()
        .manage(LanceDbEngineWrapper(Mutex::new(engine)))
        .invoke_handler(tauri::generate_handler![open_source_rag_search])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

---

## 8. Usage Examples

### Basic Rust Search Implementation

```rust
use crate::commands::lancedb::{LanceDbEngine, VectorSearchQuery};

fn execute_search(engine: &LanceDbEngine, embedding_vector: Vec<f32>) -> Result<(), Box<dyn std::error::Error>> {
    let query = VectorSearchQuery {
        query_vector: embedding_vector,
        limit: 5,
        filter: Some("department = 'HR' AND min_security_level <= 2".to_string()),
    };

    let results = engine.search_sync(query)?;

    for chunk in results {
        println!("[{}] Score: {:.4} | File: {}", chunk.file_name, chunk.score, chunk.file_path);
        println!("Snippet: {}\n", chunk.content_snippet);
    }

    Ok(())
}
```

---

## 9. Limitations & Considerations

- **Data Plane Scope**: No built-in authentication, authorization, or user management.
- **Simplified SQL Parser**: Pre-filter syntax is limited to simple logical operators (`AND`, `OR`) and basic comparison predicates.
- **Column Constraints**: Filtering is supported only on indexed metadata attributes (`department`, `min_security_level`, `score`).
- **Client Security Boundary**: Applications utilizing the open-source engine must handle security filtering on the client or API gateway level if multi-tenancy is required.

---

## 10. Comparison Matrix

| Feature | Open Source Version (Data Plane) | Enterprise Version (Control Plane) |
| :--- | :--- | :--- |
| **Vector Search** | ✅ High-performance LanceDB / Arrow | ✅ High-performance LanceDB / Arrow |
| **Pre-filtering** | ✅ Client-controlled SQL pre-filter | ✅ Server-enforced Policy + SQL pre-filter |
| **Authentication** | ❌ None (Data Plane only) | ✅ OAuth2, OIDC, API Key Gateway |
| **RBAC / ABAC** | ❌ Client responsibility | ✅ Server-side Zero-Trust enforcement |
| **SSO Integration** | ❌ None | ✅ Okta, Azure AD, SAML 2.0 |
| **Pricing** | 🆓 Open Source / Free | 💼 Commercial Subscription |
