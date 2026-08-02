---
title: What is OpenDocuments?
description: OpenDocuments is an open source self-hosted RAG platform for AI document search with source citations across local files and web sources.
head:
  - - meta
    - name: keywords
      content: what is opendocuments, self-hosted rag platform, ai document search, open source rag, document question answering, cited ai answers
  - - script
    - type: application/ld+json
    - '{"@context":"https://schema.org","@type":"FAQPage","mainEntity":[{"@type":"Question","name":"What is OpenDocuments?\",\"acceptedAnswer\":{\"@type\":\"Answer\",\"text\":\"OpenDocuments is an open source self-hosted RAG platform that indexes documents from sources such as local files, and web pages, then answers natural-language questions with source citations.\"}},{\"@type\":\"Question\",\"name\":\"What is OpenDocuments used for?\",\"acceptedAnswer\":{\"@type\":\"Answer\",\"text\":\"OpenDocuments is used to build a private AI search engine over organizational documents, engineering docs, product specs, policies, spreadsheets, API documentation, and knowledge bases.\"}},{\"@type\":\"Question\",\"name\":\"Can OpenDocuments run locally?\",\"acceptedAnswer\":{\"@type\":\"Answer\",\"text\":\"Yes. OpenDocuments can run locally with Ollama for LLM and embedding models, SQLite for metadata, LanceDB for vector search, and the built-in Web UI, CLI, HTTP API, and MCP server.\"}}]}'
---

# What is OpenDocuments?

**OpenDocuments is an open source, self-hosted RAG platform for AI document search.** It connects to local documents, indexes them, and answers natural-language questions with source citations.

OpenDocuments is designed for teams that want a private knowledge base search system without sending every document to a hosted enterprise search vendor. It can run locally with Ollama or connect to cloud model providers such as OpenAI, Anthropic, Google, and xAI.

## What problem does OpenDocuments solve?

Teams often store knowledge in files:

- Local Markdown, PDF, DOCX, XLSX, CSV, PPTX, code, and email files
- Internal or public web pages

OpenDocuments gives those documents one AI-searchable layer. Users can ask questions like "How does authentication work?", "What changed in the v3 product spec?", or "What is our remote work policy?" and get grounded answers with source references.

## How does OpenDocuments work?

OpenDocuments follows a production RAG flow:

1. Connect to document sources.
2. Parse files and preserve useful structure.
3. Chunk documents for retrieval.
4. Generate embeddings.
5. Store metadata in SQLite and vectors in LanceDB.
6. Search with hybrid vector and keyword retrieval.
7. Rerank and expand results based on the selected RAG profile.
8. Generate an answer with citations and confidence signals.

## Who is OpenDocuments for?

OpenDocuments is useful for:

- Engineering teams that need AI search over code, API docs, runbooks, and architecture notes
- Product and operations teams that need answers from specs, spreadsheets, policies, and meeting notes
- AI-assisted development teams that want Claude Code, Cursor, Windsurf, or another MCP client to search internal knowledge
- Organizations that prefer self-hosted infrastructure and local model options

## What does OpenDocuments include?

OpenDocuments includes:

- **Web UI** — beautiful chat interface, workspace manager, and admin stats dashboard
- **CLI (`opendoc`)** — terminal utility for indexing, querying, and configuring your self-hosted instance
- **TUI (Terminal UI)** — fully interactive CLI terminal chatbot interface for local-first execution
- **Unified API & MCP Server** — Axum-powered backend supporting REST queries and local-first Claude/Cursor MCP integration

## Short answer

OpenDocuments is a private AI search engine for your organization's documents. It is best described as a self-hosted RAG platform, an open source enterprise search alternative, and a knowledge base for AI coding assistants.
