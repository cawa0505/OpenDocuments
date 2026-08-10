# OpenSpec Requirement: LanceDB Engine Sidecar Protocol

**Spec ID**: `lancedb-engine-sidecar`
**Status**: Approved / Production
**Priority**: P0
**Primary Language**: English
**Last Updated**: 2026-08-10

---

## 1. Overview & Core Objective

OpenDocuments SHALL separate its public application process from its heavy vector
storage engine. The public `opendoc` process remains the only HTTP/API service. A
private child process, `opendoc-engine-lancedb`, owns LanceDB, Arrow, DataFusion,
vector indexes, and Lance full-text indexes.

This design exists to keep the core executable small, make the vector engine
replaceable, and prevent LanceDB dependency upgrades from forcing core API and WebUI
releases. It does not promise that the combined installation or total runtime RSS is
smaller than the current unified binary.

The first implementation SHALL support one engine and one protocol. It MUST NOT add
a general plugin SDK, third-party plugin loading, discovery marketplace, or remote
engine registry.

### 1.1 Current and Target Retrieval State

| Capability | Current implementation | Target ownership |
|---|---|---|
| Metadata and transactional state | SQLite in `opendoc` | Core only |
| Dense vectors and vector search | LanceDB in-process | LanceDB engine |
| Chunk storage | LanceDB in-process | LanceDB engine |
| Full-text search currently executed | LanceDB FTS | LanceDB engine |
| SQLite FTS5 sparse path | Specified in roadmap, not implemented | Core |
| Hybrid fusion | LanceDB vector + LanceDB FTS + RRF | Core fuses SQLite FTS5 and engine candidates after FTS5 lands |
| SQLite Vector | Not implemented and not planned by this spec | Out of scope |

**Terminology rule:** SQLite FTS5 is a lexical full-text index, not a vector store.
Documents MUST NOT describe SQLite FTS5 as “SQLite Vector.”

---

## 2. Process and Trust Boundaries

### 2.1 Public Core Process

`opendoc` SHALL remain the only public service and SHALL own:

- Axum REST, SSE, MCP, and WebUI endpoints.
- Authentication, authorization, workspace resolution, and configuration.
- SQLite and all metadata/transactional writes.
- Original-file access, parsing, normalization, and chunking.
- BYOK provider selection, API-key access, and embedding requests.
- SQLite FTS5 indexing and search after that path is implemented.
- Engine process startup, handshake, health monitoring, shutdown, and recovery.
- RRF candidate fusion and public search response formatting.

The core executable MUST NOT link LanceDB, Lance, Arrow, or DataFusion crates.

### 2.2 LanceDB Engine Child Process

`opendoc-engine-lancedb` SHALL own:

- Lance file creation and migration.
- Chunk payloads stored for Lance retrieval.
- Dense-vector insertion and vector search.
- LanceDB FTS index creation and search.
- Document deletion, index optimization, and engine-local consistency checks.

The engine MUST NOT:

- Open SQLite or mutate core metadata.
- Read BYOK API keys or call external embedding providers.
- Expose a public TCP port.
- Resolve users, authentication, or workspace names.
- Serve WebUI, REST, SSE, or MCP endpoints.

### 2.3 One Public Service, One Managed Child

The engine is not an independently administered daemon. The core process SHALL spawn
it, own its lifecycle, and terminate it on shutdown. Initial deployment MUST NOT add a
second systemd, launchd, or Windows service.

---

## 3. Storage Ownership and Rebuild Contract

### 3.1 Authoritative State

- SQLite is the authoritative store for workspaces, documents, status, collections,
  tags, conversations, messages, and provider configuration.
- Original source files are the authoritative source for document content in the
  initial sidecar implementation.
- LanceDB is a derived search index and MUST be rebuildable from authoritative state.
- The engine owns Lance files exclusively; the core MUST NOT open them directly.

### 3.2 Current Chunk Limitation

SQLite currently stores document metadata and `chunk_count`, but not normalized chunk
content. Therefore, the initial rebuild contract depends on the original source file.
If that file is unavailable, the core SHALL mark the document `source_missing` and
MUST NOT claim that its index can be rebuilt.

Persisting normalized chunks in SQLite or adding a separate chunk store is
**[待討論]**. It is not required for the first sidecar implementation.

### 3.3 Index State Machine

1. Core records the document as `indexing` in SQLite.
2. Core parses/chunks the source and obtains embeddings without exposing API keys.
3. Core sends an idempotent `index_chunks` request with an `operation_id`.
4. Engine commits Lance data and acknowledges success.
5. Core marks the document `ready` and records `indexed_at`.
6. If the engine or core crashes before acknowledgement, reconciliation resumes from
   SQLite desired state without a cross-process transaction.

LanceDB failure MUST NOT corrupt or roll back SQLite metadata. No distributed
transaction protocol SHALL be introduced.

---

## 4. IPC Protocol

### 4.1 Transport

The first implementation SHALL use newline-delimited JSON request/response messages
over child-process stdin/stdout.

- Core writes requests to engine stdin.
- Engine writes protocol responses only to stdout.
- Engine writes logs and diagnostics only to stderr.
- No listening TCP port, Unix socket, or Windows named pipe is required initially.
- Closing engine stdin SHALL cause the engine to exit promptly.

The transport may later be replaced without changing public HTTP contracts, provided
the protocol semantics remain compatible.

### 4.2 Required Operations

| Operation | Purpose |
|---|---|
| `handshake` | Negotiate protocol, schema, vector dimension, and capabilities. |
| `health` | Confirm engine readiness without touching public HTTP. |
| `index_chunks` | Idempotently insert or replace chunks and vectors for one document. |
| `search` | Run workspace-scoped vector search and optionally Lance FTS. |
| `delete_document` | Remove all derived chunks/index entries for one document. |
| `optimize` | Run engine-local compaction/index maintenance. |
| `shutdown` | Perform an orderly engine shutdown. |

### 4.3 Handshake Fields

The handshake response MUST include:

- `protocol_version`
- `engine_version`
- `schema_version`
- `capabilities`
- `vector_dimension`

Protocol and schema incompatibility MUST fail startup clearly. Silent downgrade or
best-effort interpretation is prohibited.

### 4.4 Request Identity and Isolation

Every index, search, and delete request MUST include `workspace_id`. Index and delete
requests MUST include `document_id`; mutating requests MUST include `operation_id`.
Workspace IDs remain TEXT and MUST NOT be UUID-validated by the engine.

---

## 5. Search Semantics

### 5.1 Current Transition Behavior

Until SQLite FTS5 is implemented, the engine MAY return both Lance vector and Lance
FTS candidates. Core applies thresholding, top-k selection, and RRF as required by the
public search contract.

### 5.2 Target Hybrid Behavior

After SQLite FTS5 lands:

1. Core queries SQLite FTS5 for lexical candidates.
2. Core queries the engine for dense-vector candidates.
3. Core fuses candidates using RRF and returns the public `SearchHit` shape.
4. Lance FTS may remain an engine capability, but it MUST NOT be documented as SQLite
   FTS5 or used to claim the core lexical path is complete.

### 5.3 Engine Unavailable

- Before SQLite FTS5 is implemented, search requiring the engine SHALL return HTTP
  `503` with error code `engine_unavailable`.
- After SQLite FTS5 is implemented, lexical-only fallback is allowed only when the
  public response identifies the degraded retrieval mode.
- The system MUST NOT emit mock chunks or silently return fabricated vector results.
- Graphify Layer 1 hard-link lookup remains independent of engine availability.

---

## 6. Lifecycle and Cross-Platform Requirements

- Core SHALL launch the engine from an explicit configured or bundled executable path.
- Startup SHALL wait for a successful handshake before reporting vector search ready.
- Child crashes SHALL mark engine health unavailable and trigger bounded restart with
  backoff; restart policy values are **[待討論]**.
- Linux/macOS shutdown SHALL terminate and wait for the child process.
- Windows SHALL launch without a console window and use kill-on-parent-close process
  containment where supported.
- Tests MUST confirm that core shutdown leaves no engine process behind.
- Engine updates and schema migrations MUST be atomic and rollback-capable;
  distribution packaging details are **[待討論]**.

---

## 7. Binary and Dependency Boundaries

- Core release builds MUST contain no `lancedb`, `lance-*`, `arrow-*`, or
  `datafusion-*` packages in `cargo tree -p opendoc-core`.
- Engine release builds MAY include those packages and MUST exclude DynamoDB/AWS
  support by default.
- FastEmbed and other local model runtimes belong to optional engine features, not
  core.
- AWS/S3/DynamoDB support requires a separately approved engine feature or future
  engine implementation. It MUST NOT enter the default core or default local engine.
- Core and engine size budgets SHALL be measured independently; combined installation
  size SHALL also be reported and MUST NOT be presented as reduced unless measured.

### 7.1 Repository Boundary Decision

- The first sidecar implementation SHALL remain in the OpenDocuments remote repository
  as a separate workspace crate and executable.
- A process boundary does not imply an immediate Git repository boundary. Keeping core,
  protocol, and engine changes in one repository permits atomic changes while the stdio
  protocol is still evolving.
- Core MUST communicate with the engine only through the sidecar protocol; it MUST NOT
  link the engine crate as a Rust library. This preserves the dependency and binary-size
  boundary even while both executables share one repository.
- Moving `opendoc-engine-lancedb` to a separate remote repository SHALL be reconsidered
  only after the protocol is stable and the engine has a demonstrated need for an
  independent release cycle, ownership boundary, or reuse by another project.
- A standalone protocol crate or repository is deferred until there is a second real
  consumer. It MUST NOT be introduced speculatively.

---

## 8. Superseded and Related Specifications

Approval of this spec supersedes the process/storage clauses in
`single-binary-architecture` that require LanceDB to run inside the Axum process. It
does not change these retained rules:

- one public Axum API service;
- no Node.js or Python runtime dependency;
- native deployment without Docker;
- WebUI assets served by the core process.

Related specifications:

- `binary-size-architecture`
- `search-index-pipeline`
- `hybrid-rag-retrieval`
- `workspace-management`

---

## 9. Out of Scope

- General third-party plugin SDK or dynamic library loading.
- Public engine networking or remote multi-tenant engines.
- AWS, S3, DynamoDB, Qdrant, or GPU engine implementation.
- Distributed transactions between SQLite and LanceDB.
- Persisting normalized chunks in SQLite before the rebuild decision is approved.
- SQLite Vector extensions.

---

## 10. Definition of Done

1. This spec is approved and conflicting single-process clauses are marked superseded.
2. Core builds without LanceDB/Lance/Arrow/DataFusion dependencies.
3. Engine owns all Lance files and exposes the required stdio protocol operations.
4. Core owns SQLite, parsing, BYOK embedding, public APIs, and process lifecycle.
5. Index and search remain workspace-isolated using TEXT workspace IDs.
6. Engine crash/restart and parent shutdown leave SQLite consistent and no orphan child.
7. Current Lance FTS and planned SQLite FTS5 roles are accurately documented.
8. Existing R1–R5 search/index contracts pass end-to-end through the sidecar.

---

**End of Draft**
