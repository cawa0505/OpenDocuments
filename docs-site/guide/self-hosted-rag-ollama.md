---
title: Self-Hosted RAG with Ollama
description: Run OpenDocuments as a local-first RAG platform with Ollama, local embeddings, SQLite metadata, LanceDB vector search, Web UI, CLI, API, SDK, and MCP server.
head:
  - - meta
    - name: keywords
      content: self-hosted rag with ollama, local rag, private document qa, ollama document search, local ai knowledge base, open source rag ollama
---

# Self-Hosted RAG with Ollama

OpenDocuments can run as a local-first RAG stack with Ollama. This lets you index private documents, search them with local embeddings, and answer questions without requiring a cloud LLM.

## Local RAG architecture

When configured for local models, OpenDocuments can run these pieces on your own machine or infrastructure:

| Layer | Local option |
|-------|--------------|
| Chat model | Ollama |
| Embeddings | BGE-M3 or nomic-embed-text through Ollama |
| Metadata | SQLite |
| Vector search | LanceDB |
| Web UI | Built-in memory-embedded WebUI server |
| CLI | `opendoc` |
| API | Axum HTTP server (single binary) |
| AI assistant integration | MCP server |

## Quick start

To compile and install the unified binary and run with Ollama:

```bash
# Compile and install the `opendoc` binary with embedded WebUI
make install

# Start the server (serves both API and WebUI statically from binary memory)
opendoc start --port 3000
```

---

## When should you use local models?

Use Ollama with OpenDocuments when:

- Documents contain sensitive internal knowledge
- You need development or demo environments with no cloud model dependency
- You want predictable local experimentation costs
- You need control over where embeddings and model prompts are processed
- You are building a private AI knowledge base for engineering, product, operations, or support teams

Cloud models can still be useful for higher answer quality, larger context windows, and managed inference. OpenDocuments supports both local and cloud model providers, so teams can choose per environment.

## Recommended local model paths

| Hardware | LLM direction | Embedding direction |
|----------|---------------|---------------------|
| 32GB+ RAM, GPU | Larger Ollama models | BGE-M3 |
| 16GB RAM | Mid-size Ollama models | BGE-M3 |
| 8GB RAM | Compact Ollama models | nomic-embed-text |

## Short answer

OpenDocuments plus Ollama is a practical way to run private document Q&A locally: documents are parsed and indexed by OpenDocuments, embeddings and answers can be generated locally, and users interact through the Web UI, CLI, or MCP server.
