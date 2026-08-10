# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased] - Rust rewrite era

Full Rust rewrite of the Node.js backend. Single-binary Axum server with embedded WebUI.

### Added
- **Single-binary Axum architecture**: `opendoc` server embedding the React WebUI assets via `rust-embed` (`single-binary-architecture`).
- **BYOK LLM gateway**: SQLite-stored provider keys (never exposed to the frontend), OpenAI-compatible chat, embedding via `kind`-tagged provider rows (`byok-llm-providers`).
- **Hybrid retrieval pipeline**: `POST /api/v1/search` (R1) with `top_k`/`threshold`; `SearchHit` carrying `doc_path` + slug-based `spec_id` + original `heading` (R2); upload→chunk→embed→index pipeline (R3); workspace isolation (R4); TEXT workspace-id resolution (R5) (`search-index-pipeline`, Approved / Production).
- **LanceDB engine sidecar**: `opendoc-engine-lancedb` stdio JSON-RPC child process owning LanceDB schema/index/search/delete/FTS self-heal; core owns embedding, RRF fusion, threshold, `SearchHit` formatting; bounded crash restart; 503 `engine_unavailable` on engine death; graceful shutdown, no orphans (`lancedb-engine-sidecar`, Approved / Production).
- **Workspace management**: multi-workspace with strict `X-Workspace` header → `active_workspace` → `default_workspace` resolution (`workspace-management`).
- **Tags & complex filtering**: tag CRUD, document status/type filters, multi-field sorting (`tags-and-complex-filtering`).
- **Document parsers**: per-format standalone crates — PDF (`lopdf`), DOCX (`docx-rs`), XLSX (`calamine`), PPTX, HTML, email, markdown/text (`document-parsers`).
- **Cross-platform distribution**: `install.sh` + GitHub Release automation.
- **Version management**: all version strings unified under `CARGO_PKG_VERSION` from `Cargo.toml`.

### Changed
- **Architecture**: Node.js multi-process backend → single Rust process + managed engine sidecar; core binary ~51 MB (strip+LTO), engine ~354 MB (strip+LTO).
- **FTS**: LanceDB FTS active lexical path; SQLite FTS5 is the planned target (`hybrid-rag-retrieval`, Partially Implemented).

## [0.2.0] - 2026-03-31

### Added
- **One-touch Ollama setup**: `init` auto-detects Ollama, checks installed models, offers to pull missing ones automatically
- **Multi-turn conversations**: Chat remembers previous context (last 3 turns) for follow-up questions
- **.env auto-loading**: API keys in `.env` are loaded automatically before config resolution
- **Cloud API key validation**: `init` verifies API keys against provider endpoints before saving
- **Config overwrite guard**: `init` prompts before overwriting existing config files
- **Degraded mode banner**: Clear warning when model plugins fail to load, with step-by-step fix instructions
- **Ollama probe retry**: Bootstrap retries Ollama connection 3 times with 3s intervals
- **Enhanced doctor command**: Ollama connectivity check, model availability verification, root-cause analysis
- **Conversation pagination**: `GET /api/v1/conversations` supports `limit` and `offset` parameters
- **Plugin API documentation**: Comprehensive docs for parser, connector, and model plugin interfaces
- **TypeScript SDK guide**: Full client API reference with examples
- **Architecture documentation**: Package structure, data flow, and design decision records
- **SECURITY.md**: Vulnerability reporting process
- **GitHub Pages docs deployment**: Automated VitePress build and deploy workflow
- **SEO optimization**: GitHub topics, OpenGraph meta tags, sitemap, search-friendly README

### Fixed
- **FTS5 SQL injection**: Strip FTS5 operators (AND, OR, NOT, NEAR) and special characters from search queries
- **File upload security**: Size limit (50MB), path traversal prevention, filename sanitization, absolute path blocking
- **Widget domain spoofing**: Exact origin matching instead of substring includes()
- **OAuth state DoS**: Limit pending states to 1000, return 429 on overflow
- **Workspace isolation**: Conversations, query logs, and chat routes enforce workspace filtering from auth context
- **Stream error handling**: SSE streams send error events to clients; incomplete answers not persisted
- **Race condition in storeChunks**: Compensating transactions for FTS/vector sync failures
- **Embedding validation**: Count and dimension checks across batches prevent silent data corruption
- **Reranker failures**: Error logging and scores array validation instead of silent fallback
- **Context window truncation**: Sentence boundary detection prevents cutting mid-word/CJK character
- **Silent catch blocks**: 12+ empty catch blocks replaced with error logging across all packages
- **Config loader**: Fails loudly when config file exists but is invalid; warns when using defaults
- **Cross-platform paths**: `os.homedir()` replaces `process.env.HOME` for Windows compatibility
- **Stop command**: Validates PID, checks process existence before kill, proper error handling
- **Ask REPL cleanup**: Ctrl+C now properly shuts down context via readline close handler
- **Admin stats N+1**: SQL GROUP BY replaces in-memory iteration over all documents
- **Generator duplicate prompt**: Removed redundant systemPrompt from generate() options
- **Empty query validation**: RAG engine rejects blank/whitespace-only queries
- **Web search type safety**: Filter malformed results, log errors instead of silent catch
- **Parser fallback logging**: Primary and fallback parser failures now logged with error details
- **Upload return type**: `uploadDocument()` SDK method returns typed result instead of `any`
- **SSE JSON parse errors**: Client logs parse failures instead of silently ignoring

### Changed
- RAG profile descriptions in init wizard now show concrete metrics (doc count, response time)
- Stub models report `healthy: false` in health checks
- Rate limiter documents x-forwarded-for spoofing risk

## [0.1.0] - 2026-03-27

### Added
- Initial release
- Core RAG engine with intent classification, query decomposition, cross-lingual search
- 5 model plugins: Ollama, OpenAI, Anthropic, Google, Grok
- 9 parser plugins: MD, TXT, PDF, DOCX, XLSX, HTML, Jupyter, Email, Code
- 7 connectors: GitHub, Notion, Google Drive, S3, Confluence, Web Crawler, Web Search
- CLI with 14 commands
- MCP server with 12 tools
- Web UI with Chat, Documents, Settings, Admin pages
- API key authentication with RBAC
- PII auto-redaction and audit logging
