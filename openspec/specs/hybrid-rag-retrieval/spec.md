# OpenSpec Requirement: Hybrid RAG Dense/Sparse Retrieval

**Spec ID**: `hybrid-rag-retrieval`  
**Status**: Approved / Production  
**Priority**: P0  
**Primary Language**: English  

---

## 1. Overview & Core Objective

This specification defines the dual-retrieval hybrid RAG architecture of OpenDocuments. To achieve high recall and exact keyword precision, the system combines LanceDB dense vector similarity search with SQLite FTS5 sparse text search, merged via Reciprocal Rank Fusion (RRF) reranking.

---

## 2. System Contracts & Requirements

### 2.1 Dual-Path Retrieval Engine
- **Dense Path**: LanceDB ONNX embedding vector similarity search for semantic conceptual matching.
- **Sparse Path**: SQLite FTS5 full-text keyword indexing for exact term, serial number, and code matching.
- **Reranker**: Reciprocal Rank Fusion (RRF) algorithm to score, weight, and fuse top candidates from both retrieval paths into a unified ranked list.

### 2.2 Dynamic Zero-Mock Guarantee
- Core RAG retrieval flows (including `search_and_rerank`) MUST operate dynamically against physical SQLite and LanceDB data stores.
- Static mock chunks, hardcoded fallback responses, or dummy search results are strictly prohibited.
- When no relevant documents match the query, the retriever MUST return an empty vector (`Vec::new()`).

---

## 3. Behavior Specifications

```spec
WHEN a user query is submitted to the hybrid search endpoint
THEN the system MUST execute LanceDB vector search and SQLite FTS5 text search in parallel within the active workspace context.

WHEN candidates are returned from both search paths
THEN the RRF reranker MUST calculate fused relevance scores and return top-k chunks with snippet highlights and similarity metadata.

WHEN no matching document chunks exist in the active workspace
THEN the hybrid search engine MUST return an empty array with HTTP 200 OK without dummy responses.
```
