---
title: MCP Knowledge Base for Claude Code and Cursor
description: Use OpenDocuments as an MCP knowledge base so Claude Code, Cursor, Windsurf, and other MCP clients can search internal documents while helping with development.
head:
  - - meta
    - name: keywords
      content: mcp knowledge base, claude code document search, cursor knowledge base, mcp server rag, ai coding assistant internal docs
---

# MCP Knowledge Base for Claude Code and Cursor

OpenDocuments includes an MCP server so AI coding assistants can search your indexed document corpus. This turns GitHub docs, Notion pages, Google Drive files, Confluence pages, API specs, and local files into a searchable knowledge base for development workflows.

## Why use OpenDocuments with MCP?

AI coding assistants are most useful when they can see the context behind a codebase: architecture notes, runbooks, API contracts, design docs, incidents, release notes, and decisions. OpenDocuments indexes that context and exposes it through MCP tools.

Use this setup when you want an assistant to answer questions like:

- How does authentication work in this service?
- Which API endpoint handles token refresh?
- What does the migration guide say about v3?
- Where is the deployment checklist?
- What did the product spec decide about billing states?

## Automatic setup for OpenCode / Claude Code / Cursor

OpenDocuments has a built-in installer that configures and integrates your self-hosted RAG as a standardized MCP server inside OpenCode/Cursor/Claude Code settings with a single command:

```bash
# Automatically registers the unified 'opendoc-mcp' tool
opendoc install-opencode
```

This automatic installer:
1. Detects your shell, config paths, and OpenCode environment.
2. Injects the standardized `opendoc-mcp` configuration pointing to your active `opendoc` binary installation.
3. Safely prunes legacy OpenDocuments tools (only if they resolve to an obsolete `opendoc` execution path) to avoid tool duplication.

---

## Manual configuration

If you prefer to manually configure your MCP-compatible client, you can spin up the standalone stdio MCP server:

```bash
opendoc start --mcp-only
```

Then append OpenDocuments to your custom MCP configuration file:

```json
{
  "mcpServers": {
    "opendoc": {
      "command": "opendoc",
      "args": ["start", "--mcp-only"]
    }
  }
}
```

## What can the MCP server do?

The MCP server lets compatible clients:

- Search indexed organizational documents
- Ask natural-language questions over the document corpus
- Inspect document status and connector health
- Index newly created files
- Query useful configuration and admin context

## Best documents to index for AI coding

Start with documents that explain intent and operating context:

- Architecture docs
- API specs
- Runbooks
- ADRs and technical decisions
- Onboarding guides
- Release notes
- Incident reviews
- Product specs
- Database and migration notes

## Short answer

OpenDocuments can act as the retrieval layer for Claude Code, Cursor, Windsurf, and other MCP clients. It gives AI coding assistants access to source-grounded internal knowledge instead of relying only on the files open in an editor.
