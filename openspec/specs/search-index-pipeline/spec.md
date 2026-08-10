# OpenSpec Requirement: Search & Index Pipeline (Layer 2 Fallback)

**Spec ID**: `search-index-pipeline`  
**Status**: Approved / Production  
**Priority**: P0  
**Primary Language**: English  
**Last Updated**: 2026-08-10  
**Source**: `docs/OpenDocuments-Requirements.md` (graphify-plugin-opendoc Layer 2 requirements R1–R5)

---

## 1. Overview & Core Objective

This specification defines the minimal OpenDocuments upstream deliverables that enable
graphify-plugin-opendoc **Layer 2** (vector soft-search fallback): a workspace-scoped
search REST endpoint and a programmatically triggerable index write path
(`file → chunk → embedding → LanceDB → searchable`). Layer 1 (hard links) is already
shipped with zero OD dependency; Layer 2 is consulted only when a hard link is absent.

**Original verified state before implementation (2026-08-10)**:

| Item | State | Evidence |
|------|-------|----------|
| `opendoc_storage::ConfigManager::search_and_rerank` | Stub returning `Vec::new()` | `crates/opendoc-storage/src/lib.rs:394` |
| `SearchBackend` impl in `opendoc-mcp` | `MockSearch` only (empty) | `crates/opendoc-mcp/src/lib.rs:1013`, wired at `:842` |
| `POST /api/v1/search` / `/documents/search` | Route missing | Router `crates/opendoc-mcp/src/lib.rs:666-719` |
| Index write path (embed → LanceDB write) | None; `lancedb.rs` has compat schema + FTS self-heal only | `crates/opendoc-storage/src/lancedb.rs` |
| Embedding dependency | None (no fastembed/onnx in workspace) | Cargo.toml scan |

**Implementation clarification:** the current retriever performs LanceDB dense-vector
search plus **LanceDB FTS**, fused with RRF. SQLite FTS5 is a documented target sparse
path but is not implemented. SQLite Vector is not used. The proposed
`lancedb-engine-sidecar` boundary moves current Lance vector and Lance FTS operations
to the engine; core-owned SQLite FTS5 remains future work.

**Related specs**: `hybrid-rag-retrieval` defines the target LanceDB vector + SQLite
FTS5 + RRF architecture. `lancedb-engine-sidecar` defines the proposed process and
storage boundary. This spec owns only the public REST/index contracts they must satisfy.

---

## 2. System Contracts & Requirements

### 2.1 Search REST Endpoint (R1)

- `POST /api/v1/search` with `Content-Type: application/json` and `X-Workspace` header
  (or equivalent `POST /api/v1/workspaces/<workspace_id>/search`; one of the two MUST
  be implemented — both MUST NOT be required).
- Request body fields:

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `query` | string | (required) | Code symbol or natural-language fragment |
| `top_k` | int | 10 | Return top K hits |
| `threshold` | float | 0.0 | Minimum similarity; below → excluded |

- Response (HTTP 200):

```json
{
  "hits": [
    { "doc_path": "docs/auth.md", "spec_id": "docs/auth.md#token-spec", "score": 0.87, "snippet": "..." }
  ]
}
```

### 2.2 Hit Fields (R2)

- `doc_path`: workspace-root-relative path (required).
- `spec_id`: block ID in `<doc_path>#<slug>` form. If OD does not slice blocks, it MUST
  return `doc_path` plus chunk location info (heading text/level or byte offset) so the
  plugin can recompute the slug.
- `score`: sortable float, higher = more relevant. Plugin only uses it for top-k
  ordering and threshold filtering; no distribution assumption.
- `snippet`: optional preview text.

### 2.3 Index Write Path (R3)

- A programmatically triggerable pipeline MUST exist — one of:
  1. **REST**: `POST /api/v1/workspaces/<id>/index` (async, HTTP 202 + `job_id`); or
  2. **MCP tool**: a callable index tool in `opendoc-mcp` (not `MockSearch`); or
  3. **CLI**: `opendoc index --workspace <id> --path <dir>` (cron/script friendly).
- Single-shot contract: given workspace + directory, complete indexing such that a
  subsequent search returns real data. Internal vector DB choice (LanceDB/Qdrant) is OD's.
- The current MCP index tool is an HTTP forward to single-file
  `/api/v1/documents/upload` with no batch/directory/change-detection — batch
  capability is the gap this requirement closes.
- Embedding execution is provided through BYOK by default; FastEmbed remains an
  optional feature and belongs outside the future lightweight core. Search results MUST be dynamic — no static
  chunks, empty `Vec::new()`/`hits: []` when nothing matches (#3317).

### 2.4 Workspace Isolation (R4)

- Search MUST return only chunks of the active workspace; index MUST affect only the
  active workspace's collection; no cross-workspace contamination.
- Workspace identified via `X-Workspace` header (or query param), resolved per the
  `workspace-management` spec hierarchy: explicit header → `active_workspace` →
  `default_workspace`.
- No 1:1 graphify mapping assumption; the plugin maps `workspace_key → od_workspace_id`
  on its side and passes the OD id through the header.

### 2.5 TEXT Workspace ID (R5)

- `workspace_id` MUST remain `TEXT` (`opendoc-storage` `DocumentChunk.workspace_id` is
  already TEXT). Values may be OD-generated UUID strings or user-defined slugs
  (e.g. `graphify-handoff`); OD MUST NOT enforce UUID validation on the header value.
  Resolution by name or ID is already supported (`workspace-management` §2.2).

### 2.6 Error Semantics

| HTTP | Meaning | Plugin behavior |
|------|---------|-----------------|
| 200 + `hits: []` | No match | Empty (NoOp) |
| 404 | Workspace not found | Empty + warning log |
| 5xx | OD internal error | Empty + error log; MUST NOT panic |

**Hard constraint**: search failure MUST NOT affect Layer 1 hard-link lookups. Layer 2
is always a best-effort fallback.

---

## 3. Behavior Specifications

```spec
WHEN a Layer 2 search request arrives at `POST /api/v1/search` with query and X-Workspace header
THEN the system MUST resolve the workspace (header → active_workspace → default_workspace), run retrieval over that workspace's indexed chunks, and return HTTP 200 with ranked hits.

WHEN the index pipeline is triggered via REST/MCP/CLI for a workspace + directory
THEN the system MUST parse, chunk, embed, and store chunks such that a subsequent search in that workspace returns real matches.

WHEN a search request carries a TEXT workspace slug (e.g. `graphify-handoff`)
THEN the system MUST accept it without UUID validation and scope the query to the resolved workspace row.

WHEN the search request hits a missing route, workspace, or internal error
THEN the system MUST respond 404/5xx without panicking, and the plugin MUST treat it as an empty result without affecting Layer 1.

WHEN no chunks match the query in the active workspace
THEN the system MUST return HTTP 200 with `hits: []` — never static or mock chunks.
```

---

## 4. Out of Scope

- **Layer 1 hard links**: plugin-owned (`# Symbol:` parsing, registry, drift audit).
- **Graphify graph mutation / TOON serialization**: plugin-side.
- **Vector DB & embedding model selection**: OD decides (LanceDB/Qdrant, engine per
  `task-execution-ai-engines`).
- **SQLite FTS5 implementation**: tracked by `hybrid-rag-retrieval`; it MUST NOT be
  claimed complete based on the current LanceDB FTS implementation.
- **R6 (synchronous Rust `SearchBackend` API)**: P2 — deferred until the MCP route is
  validated; not a blocker.

---

## 5. Definition of Done

1. `POST /api/v1/search` (or equivalent) returns non-empty hits after indexing (R1).
2. Hits carry `doc_path` + `spec_id` (or chunk location mappable to `spec_id`) (R2).
3. A programmatically triggerable index pipeline exists (R3).
4. Search and index are workspace-isolated (R4).
5. `workspace_id` accepts TEXT slugs (R5).
6. Verified per project cycle: `cargo check` (zero warnings) → `cargo build` →
   `cargo install --path crates/opendoc-cli --force` → restart → HTTP contract verify →
   real index→search round-trip returning actual chunks, empty on no match (#3311, #3321).
