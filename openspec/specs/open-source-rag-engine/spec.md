# OpenSpec Requirement: Open Source RAG Engine Data Plane

**Spec ID**: `open-source-rag-engine`  
**Status**: Approved / Production  
**Priority**: P0  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the pure Data Plane vector search and SQL-like pre-filtering contracts for LoomCowork's Open Source RAG Engine. The engine provides unauthenticated, high-performance vector retrieval over LanceDB and Apache Arrow datasets with client-controlled filter predicate execution.

---

## 2. System Contracts & Requirements

### 2.1 Pure Data Plane Architecture
- The engine MUST process vector similarity search queries without enforcing embedded identity verification, token checking, or session management.
- Client applications control query filters via explicit SQL-like predicate strings passed in `VectorSearchQuery`.
- The engine serves as the zero-dependency base execution layer upon which enterprise Control Plane features (OAuth/SSO, RBAC/ABAC enforcement) can be added.

### 2.2 Pre-Filtering & Query Execution
- Vector search MUST support pre-filtering against Arrow metadata columns: `department` (String), `min_security_level` (Integer/u8), and `score` (Float/f32).
- Supported operators MUST include standard equality (`=`), numeric comparisons (`<=`, `>`, `<`), and logical connectors (`AND`, `OR`).
- Pre-filtering MUST execute during or prior to vector index traversal to eliminate non-matching candidates before top-k ranking.

---

## 3. Behavior Specifications

```spec
WHEN a client submits open_source_rag_search with query_vector, top_k, and optional filter
THEN the engine MUST execute pre-filtered vector similarity search against the LanceDB dataset.

WHEN filter is provided as "department = 'HR' AND min_security_level <= 2"
THEN the engine MUST evaluate the SQL predicate against metadata columns and return only records satisfying all constraints.

WHEN no document chunks satisfy both vector distance and filter predicates
THEN the engine MUST return an empty array with HTTP/IPC success status.
```
