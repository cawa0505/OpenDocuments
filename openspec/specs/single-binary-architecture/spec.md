# OpenSpec Requirement: Single-Binary Architecture & Embedded WebUI

**Spec ID**: `single-binary-architecture`  
**Status**: Approved / Production — Process Boundary Supersession Proposed
**Priority**: P0  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the currently deployed single-binary contract for OpenDocuments. The current backend (Axum HTTP server, SQLite storage, LanceDB vector index, LLM client, and background indexing) and the built-in React WebUI static assets are compiled and packaged into a single standalone native executable (`opendoc`).

The draft `lancedb-engine-sidecar` specification proposes replacing only the LanceDB
process boundary with a private, core-managed child process. Until that draft is
approved and implemented, this specification remains the production contract.

No external Node.js runtime, Python process, Docker daemon, or separate static web server is permitted for deployment.

---

## 2. System Contracts & Architectural Requirements

### 2.1 WebUI Embedded Static Assets
- The React WebUI built output (`apps/webui/dist`) MUST be compiled prior to binary build and embedded into the binary using `rust-embed`.
- The Axum web server MUST serve all static assets directly from binary memory with appropriate MIME headers, supporting single-page application (SPA) client-side routing fallback (`index.html`).

### 2.2 Storage & Process Isolation
- All database connections (SQLite via `sqlx` and LanceDB vector store) MUST share connections safely via Axum's `WithState` pattern within the single Rust process.
- External multi-process access or Node.js bridge servers are strictly prohibited.

### 2.3 Proposed Supersession Boundary

If `lancedb-engine-sidecar` is approved, §2.2 is superseded only for the private
LanceDB child process. The following constraints remain:

- `opendoc` is the only public Axum/API service.
- SQLite remains core-owned and MUST NOT be opened by the engine.
- No Node.js or Python runtime is introduced.
- The engine is spawned and terminated by core rather than installed as a separate
  public daemon.

---

## 3. Behavior Specifications

```spec
WHEN the user executes `opendoc start --port 3000`
THEN a single Rust Axum process MUST start, host REST/SSE API endpoints, and serve the embedded WebUI at `http://localhost:3000`.

WHEN a request hits an unknown non-API route (e.g. `/chat/123`)
THEN the Axum static file fallback handler MUST serve `index.html` from memory to support client-side SPA routing.

WHEN the system operates
THEN zero external Node.js or Python runtime processes MUST be spawned.
```
